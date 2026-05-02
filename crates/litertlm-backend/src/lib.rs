//! Safe Rust wrapper around LiteRT-LM for the Aegis-Node runtime.
//!
//! Per [ADR-023](../../docs/adrs/023-litertlm-as-second-inference-backend.md)
//! and the LiteRT-LM umbrella ([#98](https://github.com/tosin2013/aegis-node/issues/98)).
//! The FFI binding ships under the same strict safety wrapper described
//! in [ADR-014](../../docs/adrs/014-cpu-first-gguf-inference-via-llama-cpp.md)'s
//! "FFI safety posture" — mirrored here so the two backends are
//! reviewable side-by-side.
//!
//! ## Public surface
//!
//! Two types, in dependency order:
//!
//! - [`Engine`] — a loaded model + the process-level LiteRT-LM
//!   runtime, fused into one handle (the upstream C ABI does not
//!   separate them). Caller owns the lifetime; the wrapper does not
//!   retain any global state of its own.
//! - [`Session`] — a single inference session, borrows `&Engine`.
//!
//! On `Drop`, every `Session` releases its conversation/session
//! handles and every `Engine` releases the model and engine — these
//! are guaranteed by the matching `litert_lm_*_delete` calls inside
//! the `Drop` impl.
//!
//! ## Safety posture
//!
//! Per the [LiteRT-A acceptance criteria](https://github.com/tosin2013/aegis-node/issues/95):
//!
//! - **No `unwrap` / `expect` on FFI returns.** Every FFI call is
//!   wrapped in a typed [`LiteRtError`] variant.
//! - **Pinned upstream.** The LiteRT-LM release tag is pinned by
//!   SHA-256 (header + `.so`) in `aegis-litertlm-sys/build.rs`.
//!   Bumping is an explicit, reviewed change to two constants.
//! - **Documented `unsafe`.** Every `unsafe` block has a comment
//!   describing the invariant the caller must hold.
//! - **Defined panic behavior.** Internal panics trigger
//!   [`std::process::abort`] via [`abort_on_internal_panic`]. We never
//!   let unwinding cross the FFI boundary back into C++.
//! - **No global state.** The `Engine` is caller-owned. Multiple
//!   `Engine::load` calls in the same process work as long as caller
//!   memory permits — the upstream API does not require a
//!   process-global init step.

#![warn(missing_docs)]
#![allow(clippy::module_name_repetitions)]

use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr::NonNull;

pub mod chat;
pub use chat::{LiteRtLmBackend, LiteRtLmLoadedModel};

use aegis_litertlm_sys as sys;
use thiserror::Error;

/// All errors the wrapper surfaces. Each variant maps to one phase of
/// `Engine::load` / `Session::infer` so an operator can see exactly
/// which step refused.
#[derive(Debug, Error)]
pub enum LiteRtError {
    /// The path to the `.litertlm` model file does not exist or could
    /// not be read.
    #[error("model file not found or unreadable: {path:?}: {detail}")]
    ModelFileUnreadable {
        /// Path the caller supplied.
        path: String,
        /// Underlying I/O reason.
        detail: String,
    },

    /// LiteRT-LM refused to construct an engine for this model. The C
    /// ABI returns `NULL` on failure without an out-parameter for the
    /// reason; we surface the path so the operator can correlate with
    /// upstream logs (controlled via [`set_min_log_level`]).
    #[error(
        "LiteRT-LM engine creation failed for {path:?} (set min_log_level=0 for upstream logs)"
    )]
    EngineCreationFailed {
        /// Path the caller supplied.
        path: String,
    },

    /// LiteRT-LM refused to allocate engine settings — typically an
    /// out-of-memory condition on the host. Distinct from
    /// `EngineCreationFailed` so the failure mode is obvious from a
    /// truncated stack trace alone.
    #[error("LiteRT-LM engine settings allocation failed (likely OOM)")]
    EngineSettingsAllocFailed,

    /// LiteRT-LM refused to allocate a session config. Same OOM-like
    /// failure mode as [`Self::EngineSettingsAllocFailed`].
    #[error("LiteRT-LM session config allocation failed (likely OOM)")]
    SessionConfigAllocFailed,

    /// `litert_lm_engine_create_session` returned NULL.
    #[error("LiteRT-LM session creation failed (set min_log_level=0 for upstream logs)")]
    SessionInitFailed,

    /// `litert_lm_conversation_config_create` returned NULL.
    /// Same OOM-like failure as the other `*_config_create`
    /// branches; surfaced separately so a truncated stack trace
    /// names the exact phase.
    #[error("LiteRT-LM conversation config allocation failed (likely OOM)")]
    ConversationConfigAllocFailed,

    /// `litert_lm_conversation_create` returned NULL.
    #[error(
        "LiteRT-LM conversation creation failed (set AEGIS_LITERTLM_DEBUG=1 for upstream logs)"
    )]
    ConversationCreateFailed,

    /// `litert_lm_conversation_send_message` returned NULL or its
    /// `_get_string` accessor returned NULL.
    #[error(
        "LiteRT-LM conversation send_message failed (set AEGIS_LITERTLM_DEBUG=1 for upstream logs)"
    )]
    ConversationSendFailed,

    /// `litert_lm_session_generate_content` returned NULL.
    #[error("LiteRT-LM generate_content failed (set min_log_level=0 for upstream logs)")]
    InferenceFailed,

    /// LiteRT-LM produced zero candidates for this prompt. Distinct
    /// from the FFI returning `NULL` — the call succeeded, but the
    /// model emitted no completions, which the caller almost
    /// certainly didn't expect.
    #[error("LiteRT-LM returned 0 candidates for the prompt")]
    NoCandidates,

    /// LiteRT-LM's response text contained invalid UTF-8. Per the
    /// safety posture we never return invalid UTF-8 to the caller.
    #[error("LiteRT-LM response contained invalid UTF-8: {0}")]
    InvalidUtf8(String),

    /// The caller-supplied prompt or path contained an interior NUL,
    /// so we couldn't pass it across the FFI as a C string.
    #[error("input contains interior NUL byte: {0}")]
    InteriorNul(String),

    /// The caller asked for an impossible configuration (e.g. zero
    /// `max_tokens`). Distinct from underlying LiteRT-LM errors so
    /// the fix is obvious.
    #[error("invalid configuration: {0}")]
    InvalidConfig(&'static str),
}

/// Sampling determinism knobs. Mirrors
/// [`aegis_llama_backend::DeterminismKnobs`] field-for-field so a
/// manifest's `inference.determinism` block flows into either backend
/// unchanged.
///
/// Setting `seed = Some(N)` and `temperature = Some(0.0)` together
/// yields byte-identical output across runs *on the CPU backend*. GPU
/// determinism is broken upstream
/// (google-ai-edge/LiteRT-LM#2080 / #2081) and Phase 1 only ships the
/// CPU sampler — see ADR-023 §"Phase 1 scope".
#[derive(Debug, Clone, Default)]
pub struct DeterminismKnobs {
    /// Sampler seed (uint32 range). Without it, LiteRT-LM picks a
    /// random seed per call and outputs vary run-to-run.
    pub seed: Option<u32>,
    /// Logit softmax temperature. `0.0` selects [`Sampler::Greedy`]
    /// regardless of other knobs (Phase 1 / CPU only).
    pub temperature: Option<f32>,
    /// Nucleus sampling — keep tokens whose cumulative probability
    /// mass is within `top_p`. `1.0` = no filter.
    pub top_p: Option<f32>,
    /// Keep only the top-`k` highest-probability tokens before
    /// sampling. `0` = no filter.
    pub top_k: Option<u32>,
    /// Repetition penalty — accepted by the manifest type but not
    /// surfaced through the LiteRT-LM C ABI today
    /// (`LiteRtLmSamplerParams` carries only `top_k`, `top_p`,
    /// `temperature`, `seed`). Recorded in the knobs for parity with
    /// [`aegis_llama_backend::DeterminismKnobs`]; ignored at the FFI
    /// boundary until upstream exposes it.
    pub repeat_penalty: Option<f32>,
}

impl From<&aegis_policy::manifest::DeterminismKnobs> for DeterminismKnobs {
    /// Mirror a manifest's
    /// [`aegis_policy::manifest::DeterminismKnobs`] into the
    /// litertlm-backend shape. Field-by-field copy — the two types
    /// share semantics; the manifest one is the persistence surface,
    /// this one is the FFI-facing input.
    fn from(m: &aegis_policy::manifest::DeterminismKnobs) -> Self {
        Self {
            seed: m.seed,
            temperature: m.temperature,
            top_p: m.top_p,
            top_k: m.top_k,
            repeat_penalty: m.repeat_penalty,
        }
    }
}

/// Inference-time configuration.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    /// Maximum number of tokens to generate per `infer` call. Bounded
    /// to keep tests deterministic and to make a runaway sampler
    /// cheap to interrupt.
    pub max_tokens: u32,
    /// Determinism knobs (parity with llama-backend / LLM-C). Default:
    /// all `None` → greedy sampling at temperature 0.
    pub determinism: DeterminismKnobs,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            determinism: DeterminismKnobs::default(),
        }
    }
}

// `Sampler` enum + `pick_sampler` were the original C-ABI sampler
// projection. Removed because LiteRT-LM v0.10.2's CPU executor
// returns UNIMPLEMENTED for every named sampler type (1, 2, 3),
// leaving `kTypeUnspecified` (0) — "use the model's baked default"
// — as the only working CPU choice. Restore both when upstream's
// CPU sampler ships (LiteRT-LM #2080 / PR #2081); the
// `DeterminismKnobs` → `Sampler` projection logic is the natural
// shape we'll want once the fix is in.

/// Build a `LiteRtLmSamplerParams` from the determinism knobs.
///
/// **Sampler upstream gap:** LiteRT-LM v0.10.2's CPU executor
/// returns `UNIMPLEMENTED` for `kGreedy` (type 3), `kTopK`
/// (type 1), and `kTopP` (type 2) — empirically verified via
/// `engine.cc:445` "Sampler type: N not implemented yet". The
/// only CPU-implemented option in this release is
/// `kTypeUnspecified` (0), which falls back to the model's
/// default sampler baked into the `.litertlm` flatbuffer's
/// `LlmMetadata.sampler_params`. For Gemma 4 the default is
/// top-k=64 / top-p=0.95 / temperature=1.0 (per upstream's
/// model card), which is NOT byte-deterministic — but Phase 1
/// gets us a working session the demo recordings can iterate
/// against.
///
/// Determinism is therefore deferred until upstream lands a
/// CPU sampler (LiteRT-LM #2080 / PR #2081 — the same fix that
/// unblocks GPU determinism). When that lands, this function
/// flips back to emitting `Type_kGreedy` directly and the
/// per-demo seed/temperature contract holds again.
///
/// The `seed` field is still emitted from the manifest knob
/// even though kTypeUnspecified ignores it — keeps the manifest
/// round-trip stable and makes the ledger entry's recorded
/// seed-vs-output correspondence trivial to update once
/// upstream's CPU sampler ships.
fn build_sampler_params(knobs: &DeterminismKnobs) -> sys::LiteRtLmSamplerParams {
    sys::LiteRtLmSamplerParams {
        type_: sys::Type_kTypeUnspecified,
        top_k: knobs.top_k.map(|k| k as i32).unwrap_or(0),
        top_p: knobs.top_p.unwrap_or(1.0),
        temperature: knobs.temperature.unwrap_or(0.0),
        seed: knobs.seed.unwrap_or(0) as i32,
    }
}

/// A loaded LiteRT-LM model + the engine that owns it. Construct one
/// per loaded model; cheap to drop. Multiple `Engine` values may
/// coexist in the same process (the C ABI does not require a
/// process-global init step).
///
/// `Engine` is `Send` but `!Sync`. The LiteRT-LM C ABI's engine
/// handle can move between threads (upstream's Python and JNI
/// bindings rely on this) but concurrent calls on the same handle
/// are not documented as safe. The `Backend` trait in
/// `aegis-inference-engine` requires `LoadedModel: Send`, so the
/// LiteRtLmBackend impl in `chat.rs` needs `Engine: Send`.
pub struct Engine {
    /// Owned engine handle. Non-null for the entire lifetime of the
    /// `Engine` — released by `Drop`.
    handle: NonNull<sys::LiteRtLmEngine>,
}

// SAFETY-INVARIANT: the LiteRT-LM C ABI's engine handle is a thread-
// movable opaque pointer — upstream's Python and JNI bindings move
// the handle across threads as a matter of course. Concurrent calls
// against the same handle are NOT documented as safe; we therefore
// withhold `Sync` (the absence of an `unsafe impl Sync for Engine`
// below). `infer` takes `&mut self` on the wrapping `LoadedModel` so
// the borrow checker enforces single-threaded use.
unsafe impl Send for Engine {}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").finish_non_exhaustive()
    }
}

impl Engine {
    /// Load a `.litertlm` model from disk on the CPU backend.
    ///
    /// Phase 1 / ADR-023 hard-codes `backend_str = "cpu"` and leaves
    /// vision / audio backends unset; multimodal support arrives with
    /// LiteRT-C and the Gemma 4 publish.
    pub fn load(path: &Path) -> Result<Self, LiteRtError> {
        if !path.exists() {
            return Err(LiteRtError::ModelFileUnreadable {
                path: path.display().to_string(),
                detail: "path does not exist".to_string(),
            });
        }

        let path_c = path_to_cstring(path)?;
        // "cpu" is the only sampler-deterministic backend in v0.10.2.
        // Hard-coded so a misconfigured manifest can't accidentally
        // pick a non-deterministic GPU path. See ADR-023 §"Phase 1 scope".
        let backend_c = CString::new("cpu")
            .map_err(|e| LiteRtError::InteriorNul(format!("backend literal contains NUL: {e}")))?;

        // SAFETY-INVARIANT: `litert_lm_engine_settings_create` takes
        // four nul-terminated C strings; we hold both `path_c` and
        // `backend_c` alive until the call returns. The vision/audio
        // backend args are NULL — explicitly allowed by the C ABI
        // ("NULL if not set"). Returns NULL on allocation failure;
        // we map that to a typed error rather than panicking.
        let settings_raw = unsafe {
            sys::litert_lm_engine_settings_create(
                path_c.as_ptr(),
                backend_c.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        let settings = NonNull::new(settings_raw).ok_or(LiteRtError::EngineSettingsAllocFailed)?;

        // RAII guard so we never leak engine settings even on the
        // engine_create error path below.
        struct SettingsGuard(NonNull<sys::LiteRtLmEngineSettings>);
        impl Drop for SettingsGuard {
            fn drop(&mut self) {
                // SAFETY-INVARIANT: `self.0` is a valid pointer
                // returned by `litert_lm_engine_settings_create` and
                // not yet deleted (no other code path has access).
                unsafe { sys::litert_lm_engine_settings_delete(self.0.as_ptr()) };
            }
        }
        let _guard = SettingsGuard(settings);

        // SAFETY-INVARIANT: `settings` is a valid, non-null pointer
        // returned by the matching `_create` call above; ownership
        // remains with the SettingsGuard until the call returns.
        // Returns NULL on failure; we surface that via the typed error
        // and Drop on `_guard` releases the settings.
        let engine_raw = unsafe { sys::litert_lm_engine_create(settings.as_ptr()) };
        let handle = NonNull::new(engine_raw).ok_or_else(|| LiteRtError::EngineCreationFailed {
            path: path.display().to_string(),
        })?;

        Ok(Self { handle })
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        // SAFETY-INVARIANT: `self.handle` is a non-null pointer
        // returned by `litert_lm_engine_create` and not previously
        // deleted (Drop runs at most once per value, and no other
        // code path calls `_delete` on it).
        unsafe { sys::litert_lm_engine_delete(self.handle.as_ptr()) };
    }
}

/// A single-shot inference session. Holds a LiteRT-LM session bound
/// to `&Engine`; on `Drop`, the session is released by
/// `litert_lm_session_delete`.
///
/// `Session` is `!Send + !Sync` for the same reason `Engine` is —
/// see the doc comment on [`Engine`]. The `PhantomData<&'e Engine>`
/// also wires the borrow of the engine, so a session cannot outlive
/// the engine that created it.
pub struct Session<'e> {
    /// Owned session handle. Non-null for the entire lifetime of the
    /// `Session`; released by `Drop`.
    handle: NonNull<sys::LiteRtLmSession>,
    /// Borrow tying the session lifetime to the engine that produced
    /// it. The C ABI does not document the engine-vs-session
    /// teardown order; the conservative reading is that the engine
    /// must outlive the session, which the borrow enforces at
    /// compile time.
    _engine: std::marker::PhantomData<&'e Engine>,
    /// Marker that disables the `Send + Sync` auto-traits.
    _not_thread_safe: std::marker::PhantomData<*const ()>,
    /// Shadow of [`SessionOptions::max_tokens`] — mirrored from
    /// `litert_lm_session_config_set_max_output_tokens` so callers
    /// can re-read what the session was configured with.
    max_tokens: u32,
}

impl<'e> Session<'e> {
    /// Open a fresh inference session against `engine`.
    pub fn new(engine: &'e Engine, options: SessionOptions) -> Result<Self, LiteRtError> {
        if options.max_tokens == 0 {
            return Err(LiteRtError::InvalidConfig("max_tokens must be > 0"));
        }

        // Build the session config: max output tokens + sampler
        // params. Both setters are void-returning, so we only check
        // the create call's return value.
        //
        // SAFETY-INVARIANT: `litert_lm_session_config_create` takes
        // no arguments and returns either a valid pointer or NULL
        // (allocation failure). We check for NULL.
        let config_raw = unsafe { sys::litert_lm_session_config_create() };
        let config = NonNull::new(config_raw).ok_or(LiteRtError::SessionConfigAllocFailed)?;

        struct ConfigGuard(NonNull<sys::LiteRtLmSessionConfig>);
        impl Drop for ConfigGuard {
            fn drop(&mut self) {
                // SAFETY-INVARIANT: matching delete for the create
                // above; runs at most once.
                unsafe { sys::litert_lm_session_config_delete(self.0.as_ptr()) };
            }
        }
        let config_guard = ConfigGuard(config);

        // i32 is the C-ABI type for max_output_tokens; saturating
        // cast keeps requests above i32::MAX as i32::MAX rather than
        // wrapping into a negative value LiteRT-LM would treat as
        // "no cap." Real callers stay in the low thousands; the
        // saturating path exists to make the conversion total.
        let max_tokens_i = i32::try_from(options.max_tokens).unwrap_or(i32::MAX);
        // SAFETY-INVARIANT: `config` is the valid pointer returned by
        // `_create` above; ownership stays with `config_guard`.
        unsafe {
            sys::litert_lm_session_config_set_max_output_tokens(config.as_ptr(), max_tokens_i);
        }

        let sampler_params = build_sampler_params(&options.determinism);
        // SAFETY-INVARIANT: `&sampler_params` is a valid pointer to
        // an `LiteRtLmSamplerParams` we own on the stack; the call
        // copies the struct (per upstream behavior of the setter
        // family) so the pointer does not need to outlive the call.
        unsafe {
            sys::litert_lm_session_config_set_sampler_params(config.as_ptr(), &sampler_params);
        }

        // SAFETY-INVARIANT: `engine.handle` is non-null (Engine
        // invariant). `config.as_ptr()` is non-null (just verified).
        // Returns NULL on failure; surfaced via the typed error.
        let session_raw = unsafe {
            sys::litert_lm_engine_create_session(engine.handle.as_ptr(), config.as_ptr())
        };
        let handle = NonNull::new(session_raw).ok_or(LiteRtError::SessionInitFailed)?;

        // The session created via `_create_session` does NOT take
        // ownership of the config (per upstream's general "caller
        // owns what they created" contract); ConfigGuard's Drop
        // continues to own the config and will release it as we
        // return out of this function.
        drop(config_guard);

        Ok(Self {
            handle,
            _engine: std::marker::PhantomData,
            _not_thread_safe: std::marker::PhantomData,
            max_tokens: options.max_tokens,
        })
    }

    /// `max_tokens` the session was configured with. Surfaced for
    /// tests and debugging — it is otherwise carried into the engine
    /// via the session config.
    #[must_use]
    pub fn max_tokens(&self) -> u32 {
        self.max_tokens
    }

    /// Run a single text prompt through the model and return the
    /// concatenated response text.
    ///
    /// Phase 1 is text-only (single `kInputText` element). Multimodal
    /// inputs (`kInputImage` / `kInputAudio`) ship with the Gemma 4
    /// vision demo (LiteRT-C / Demo 7).
    ///
    /// Stops on whichever fires first:
    /// 1. The model emits its end-of-turn token (LiteRT-LM internal).
    /// 2. The configured `max_tokens` cap kicks in.
    /// 3. The response candidate is not valid UTF-8 — we refuse the
    ///    whole response rather than return broken text.
    pub fn infer(&mut self, prompt: &str) -> Result<String, LiteRtError> {
        let prompt_c = CString::new(prompt)
            .map_err(|e| LiteRtError::InteriorNul(format!("prompt contains interior NUL: {e}")))?;
        let prompt_bytes = prompt_c.as_bytes(); // excludes NUL terminator

        let inputs = [sys::InputData {
            type_: sys::InputDataType_kInputText,
            data: prompt_bytes.as_ptr().cast::<std::ffi::c_void>(),
            size: prompt_bytes.len(),
        }];

        // SAFETY-INVARIANT: `self.handle` is the valid session
        // pointer (Session invariant). `inputs.as_ptr()` is a valid
        // pointer into our local stack array; `inputs.len()` is its
        // exact element count. The InputData entries point at
        // `prompt_c` which we keep alive on the stack until after the
        // call returns. Returns NULL on failure.
        let resp_raw = unsafe {
            sys::litert_lm_session_generate_content(
                self.handle.as_ptr(),
                inputs.as_ptr(),
                inputs.len(),
            )
        };
        let responses = NonNull::new(resp_raw).ok_or(LiteRtError::InferenceFailed)?;

        struct ResponsesGuard(NonNull<sys::LiteRtLmResponses>);
        impl Drop for ResponsesGuard {
            fn drop(&mut self) {
                // SAFETY-INVARIANT: `self.0` is the valid pointer
                // returned by `_generate_content`; runs at most once.
                unsafe { sys::litert_lm_responses_delete(self.0.as_ptr()) };
            }
        }
        let _guard = ResponsesGuard(responses);

        // SAFETY-INVARIANT: `responses` is the valid pointer above.
        let n = unsafe { sys::litert_lm_responses_get_num_candidates(responses.as_ptr()) };
        if n <= 0 {
            return Err(LiteRtError::NoCandidates);
        }

        // SAFETY-INVARIANT: index 0 is in [0, n) since n > 0. The
        // returned C string is owned by the responses object — valid
        // for the lifetime of `_guard`. We copy it out before
        // dropping.
        let text_ptr =
            unsafe { sys::litert_lm_responses_get_response_text_at(responses.as_ptr(), 0) };
        if text_ptr.is_null() {
            return Err(LiteRtError::NoCandidates);
        }
        // SAFETY-INVARIANT: `text_ptr` is a non-null pointer to a
        // null-terminated C string owned by `responses`; valid until
        // `_guard` drops. We materialize an owned `String` here.
        let cstr = unsafe { CStr::from_ptr(text_ptr) };
        let text = cstr
            .to_str()
            .map_err(|e| LiteRtError::InvalidUtf8(e.to_string()))?
            .to_owned();

        Ok(text)
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        // SAFETY-INVARIANT: `self.handle` is a non-null pointer
        // returned by `litert_lm_engine_create_session` and not
        // previously deleted (Drop runs at most once per value).
        unsafe { sys::litert_lm_session_delete(self.handle.as_ptr()) };
    }
}

/// A LiteRT-LM Conversation — the **higher-level** chat surface
/// (vs. [`Session`]'s text-in/text-out path). The Conversation API
/// applies the model's bundled chat template, threads tools through
/// the upstream constrained-decoder, and returns structured JSON
/// responses with pre-parsed tool calls and assistant text.
///
/// Per [ADR-023](../../docs/adrs/023-litertlm-as-second-inference-backend.md)
/// and LiteRT-D ([#119](https://github.com/tosin2013/aegis-node/issues/119)),
/// this is the production path for Gemma 4. The flat-prompt path
/// in [`Session::infer`] was a Phase 1 placeholder that doesn't
/// elicit tool calls from instruct models trained against a
/// specific chat template.
///
/// # Lifetime / threading
///
/// `Conversation` borrows `&'e Engine` so the engine must outlive
/// every conversation derived from it. Same `!Send + !Sync` posture
/// as [`Session`]: thread-movable handles aren't documented as safe,
/// concurrent calls explicitly aren't.
///
/// # Constrained decoding
///
/// `Conversation::open` enables upstream's constrained decoder
/// unconditionally — the Gemma 4 family relies on it for tool-call
/// well-formedness, and it's the default LiteRT-LM advertises. A
/// future option to disable it (for non-tool-call use cases) lands
/// when needed.
pub struct Conversation<'e> {
    /// Owned conversation handle. Non-null for the entire lifetime
    /// of the `Conversation`; released by `Drop`.
    handle: NonNull<sys::LiteRtLmConversation>,
    /// Borrow tying the conversation lifetime to the engine that
    /// produced it.
    _engine: std::marker::PhantomData<&'e Engine>,
    /// `!Send + !Sync` marker (parity with [`Session`]).
    _not_thread_safe: std::marker::PhantomData<*const ()>,
}

impl<'e> Conversation<'e> {
    /// Open a new conversation against `engine`. The session config
    /// (max_output_tokens, sampler params) is built from `options`
    /// the same way [`Session::new`] builds it; the conversation
    /// owns the resulting session-config and conversation-config
    /// for its lifetime.
    ///
    /// `system_message` is the system-role text the model sees
    /// once at the conversation's start (`None` = no system turn).
    /// `tools_json` is the OpenAI-compatible tools array as a JSON
    /// string (`None` or `"[]"` = no tools); the upstream constrained
    /// decoder uses it to constrain tool-call output to declared
    /// names + argument shapes.
    ///
    /// `messages_json` lets the caller pre-load conversation history
    /// (an array of `{role, content}` objects). `None` means start
    /// fresh with just the system turn (if any).
    pub fn open(
        engine: &'e Engine,
        options: SessionOptions,
        system_message: Option<&str>,
        tools_json: Option<&str>,
        messages_json: Option<&str>,
    ) -> Result<Self, LiteRtError> {
        if options.max_tokens == 0 {
            return Err(LiteRtError::InvalidConfig("max_tokens must be > 0"));
        }

        // Session config: max_output_tokens + sampler. Lifetime
        // ends with this function — the conversation_config_create
        // copies what it needs (per the C ABI contract).
        // SAFETY-INVARIANT: `_create` returns NULL on alloc failure;
        // checked.
        let session_cfg_raw = unsafe { sys::litert_lm_session_config_create() };
        let session_cfg =
            NonNull::new(session_cfg_raw).ok_or(LiteRtError::SessionConfigAllocFailed)?;

        struct SessionCfgGuard(NonNull<sys::LiteRtLmSessionConfig>);
        impl Drop for SessionCfgGuard {
            fn drop(&mut self) {
                // SAFETY-INVARIANT: matching delete for the create
                // above; runs at most once.
                unsafe { sys::litert_lm_session_config_delete(self.0.as_ptr()) };
            }
        }
        let session_cfg_guard = SessionCfgGuard(session_cfg);

        let max_tokens_i = i32::try_from(options.max_tokens).unwrap_or(i32::MAX);
        // SAFETY-INVARIANT: `session_cfg` is the valid pointer above;
        // setter copies the value.
        unsafe {
            sys::litert_lm_session_config_set_max_output_tokens(session_cfg.as_ptr(), max_tokens_i);
        }
        let sampler_params = build_sampler_params(&options.determinism);
        // SAFETY-INVARIANT: `&sampler_params` is a valid pointer to
        // a stack value; the setter copies it.
        unsafe {
            sys::litert_lm_session_config_set_sampler_params(session_cfg.as_ptr(), &sampler_params);
        }

        // Convert each optional &str to a CString that lives until
        // after `_create` returns (the C ABI copies the bytes; we
        // hold them across the call to be safe).
        let system_c = optional_cstring(system_message, "system_message")?;
        let tools_c = optional_cstring(tools_json, "tools_json")?;
        let messages_c = optional_cstring(messages_json, "messages_json")?;

        let system_ptr = system_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());
        let tools_ptr = tools_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());
        let messages_ptr = messages_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());

        // SAFETY-INVARIANT: `engine.handle` is non-null (Engine
        // invariant). `session_cfg` is the valid pointer above; the
        // C ABI is documented as accepting NULL for the optional
        // string args. enable_constrained_decoding is `true` per
        // the Conversation docstring above.
        let conv_cfg_raw = unsafe {
            sys::litert_lm_conversation_config_create(
                engine.handle.as_ptr(),
                session_cfg.as_ptr(),
                system_ptr,
                tools_ptr,
                messages_ptr,
                true,
            )
        };
        let conv_cfg =
            NonNull::new(conv_cfg_raw).ok_or(LiteRtError::ConversationConfigAllocFailed)?;

        struct ConvCfgGuard(NonNull<sys::LiteRtLmConversationConfig>);
        impl Drop for ConvCfgGuard {
            fn drop(&mut self) {
                // SAFETY-INVARIANT: matching delete for the create.
                unsafe { sys::litert_lm_conversation_config_delete(self.0.as_ptr()) };
            }
        }
        let conv_cfg_guard = ConvCfgGuard(conv_cfg);

        // SAFETY-INVARIANT: `engine.handle` is non-null; `conv_cfg`
        // is the valid pointer above; returns NULL on failure.
        let conv_raw = unsafe {
            sys::litert_lm_conversation_create(engine.handle.as_ptr(), conv_cfg.as_ptr())
        };
        let handle = NonNull::new(conv_raw).ok_or(LiteRtError::ConversationCreateFailed)?;

        // Per the C ABI's "caller owns what they created" contract,
        // both configs stay owned by their guards and get released
        // here as we return.
        drop(conv_cfg_guard);
        drop(session_cfg_guard);
        // The CStrings are dropped at function return — the
        // Conversation has either copied the bytes or doesn't need
        // them post-create.
        drop(system_c);
        drop(tools_c);
        drop(messages_c);

        Ok(Self {
            handle,
            _engine: std::marker::PhantomData,
            _not_thread_safe: std::marker::PhantomData,
        })
    }

    /// Send one message to the conversation and return the model's
    /// JSON response. Blocks until the model finishes the turn (per
    /// the C ABI doc: "This is a blocking call").
    ///
    /// `message_json` is a JSON object representing the message in
    /// LiteRT-LM's expected shape — typically:
    ///
    /// ```json
    /// {"role": "user", "content": "..."}
    /// ```
    ///
    /// `extra_context_json` is reserved for upstream's RAG-style
    /// "extra context" injection; pass `None` for the typical chat
    /// case.
    ///
    /// The returned [`JsonResponse`] owns the response string for
    /// the duration of the call; copy it out before dropping.
    pub fn send_message(
        &mut self,
        message_json: &str,
        extra_context_json: Option<&str>,
    ) -> Result<JsonResponse, LiteRtError> {
        let message_c = CString::new(message_json).map_err(|e| {
            LiteRtError::InteriorNul(format!("message_json contains interior NUL: {e}"))
        })?;
        let extra_c = optional_cstring(extra_context_json, "extra_context_json")?;
        let extra_ptr = extra_c.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());

        // SAFETY-INVARIANT: `self.handle` is non-null. `message_c`
        // and (when set) `extra_c` are valid C strings that live
        // through the call. Returns NULL on failure.
        let resp_raw = unsafe {
            sys::litert_lm_conversation_send_message(
                self.handle.as_ptr(),
                message_c.as_ptr(),
                extra_ptr,
            )
        };
        let handle = NonNull::new(resp_raw).ok_or(LiteRtError::ConversationSendFailed)?;
        Ok(JsonResponse { handle })
    }
}

impl Drop for Conversation<'_> {
    fn drop(&mut self) {
        // SAFETY-INVARIANT: `self.handle` is non-null; runs at most once.
        unsafe { sys::litert_lm_conversation_delete(self.handle.as_ptr()) };
    }
}

/// A JSON response from [`Conversation::send_message`]. The string
/// is owned by the upstream library; this wrapper releases it on
/// `Drop`.
///
/// Use [`Self::as_str`] to read the response — the underlying byte
/// buffer is valid only for the lifetime of this `JsonResponse`.
pub struct JsonResponse {
    handle: NonNull<sys::LiteRtLmJsonResponse>,
}

impl JsonResponse {
    /// Borrow the response as a `&str`. The slice is valid until
    /// the `JsonResponse` is dropped.
    pub fn as_str(&self) -> Result<&str, LiteRtError> {
        // SAFETY-INVARIANT: `self.handle` is non-null. Returns NULL
        // (per the C ABI) only when `response` itself is NULL,
        // which we already excluded; checked anyway.
        let ptr = unsafe { sys::litert_lm_json_response_get_string(self.handle.as_ptr()) };
        if ptr.is_null() {
            return Err(LiteRtError::ConversationSendFailed);
        }
        // SAFETY-INVARIANT: the C ABI documents the returned string
        // as null-terminated and "valid only for the lifetime of
        // the response object." We bind the borrow to `&self`.
        let cstr = unsafe { CStr::from_ptr(ptr) };
        cstr.to_str()
            .map_err(|e| LiteRtError::InvalidUtf8(e.to_string()))
    }
}

impl Drop for JsonResponse {
    fn drop(&mut self) {
        // SAFETY-INVARIANT: matching delete for the create; runs at
        // most once.
        unsafe { sys::litert_lm_json_response_delete(self.handle.as_ptr()) };
    }
}

/// Helper: turn `Option<&str>` into `Option<CString>`, surfacing
/// interior-NUL errors as a typed `LiteRtError`. The label is
/// echoed in the error message so the operator knows which arg
/// was malformed.
fn optional_cstring(s: Option<&str>, label: &str) -> Result<Option<CString>, LiteRtError> {
    match s {
        None => Ok(None),
        Some(raw) => CString::new(raw)
            .map(Some)
            .map_err(|e| LiteRtError::InteriorNul(format!("{label} contains interior NUL: {e}"))),
    }
}

/// Set the upstream library's minimum log level. `0` = INFO, `1` =
/// WARNING, `2` = ERROR, `3` = FATAL. Defaults are upstream's choice
/// (currently WARNING). Useful for diagnosing
/// [`LiteRtError::EngineCreationFailed`] or
/// [`LiteRtError::SessionInitFailed`], where the C ABI returns NULL
/// without a reason and the upstream log is the only signal.
pub fn set_min_log_level(level: i32) {
    // SAFETY-INVARIANT: the C ABI is documented as "any int from 0..3
    // accepted; out-of-range values are clamped." Calling without
    // any other state is always safe — no handles required.
    unsafe { sys::litert_lm_set_min_log_level(level) };
}

/// Wrap an FFI-callback closure so any panic inside it aborts the
/// process via [`std::process::abort`] instead of unwinding back into
/// C++ code (which is undefined behavior on most ABIs).
///
/// LiteRT-A doesn't currently set any FFI callbacks (the streaming
/// API exposes one — [`sys::LiteRtLmStreamCallback`] — but Phase 1
/// uses the blocking generate_content path). When LiteRT-B wires the
/// streaming callback for token-by-token surfaces, wrap the closure
/// in this guard. Surfacing it now means we document the policy at
/// the same time we add the binding, not after the first crash report.
///
/// # Example
///
/// ```ignore
/// extern "C" fn on_chunk(
///     data: *mut std::ffi::c_void,
///     chunk: *const std::ffi::c_char,
///     _is_final: bool,
///     _err: *const std::ffi::c_char,
/// ) {
///     aegis_litertlm_backend::abort_on_internal_panic(|| {
///         /* parse chunk and forward to a Rust channel */
///     });
/// }
/// ```
pub fn abort_on_internal_panic<F: FnOnce() -> R + std::panic::UnwindSafe, R>(f: F) -> R {
    match std::panic::catch_unwind(f) {
        Ok(r) => r,
        Err(_) => {
            // Deliberately don't try to format the panic info —
            // formatter calls can panic too, and we're already in an
            // FFI-callback context where unwinding is UB. A fast,
            // unformatted abort is the only safe move.
            std::process::abort();
        }
    }
}

fn path_to_cstring(path: &Path) -> Result<CString, LiteRtError> {
    let s = path
        .to_str()
        .ok_or_else(|| LiteRtError::ModelFileUnreadable {
            path: path.display().to_string(),
            detail: "path is not valid UTF-8".to_string(),
        })?;
    CString::new(s)
        .map_err(|e| LiteRtError::InteriorNul(format!("model path contains interior NUL: {e}")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn session_options_default_is_sane() {
        let opts = SessionOptions::default();
        assert!(opts.max_tokens > 0);
    }

    #[test]
    fn invalid_config_max_tokens_zero_surfaces_typed_error_string() {
        let err = LiteRtError::InvalidConfig("max_tokens must be > 0");
        assert!(err.to_string().contains("max_tokens"));
    }

    #[test]
    fn missing_model_file_path_is_categorized_as_unreadable() {
        let err = LiteRtError::ModelFileUnreadable {
            path: "/nope".to_string(),
            detail: "no such file".to_string(),
        };
        assert!(err.to_string().contains("not found or unreadable"));
    }

    // pick_sampler tests removed alongside the function — see the
    // "// `Sampler` enum + `pick_sampler` were the original C-ABI
    // sampler projection..." comment above the function for the
    // restoration plan. The DeterminismKnobs round-trip from the
    // manifest is still tested below.

    #[test]
    fn build_sampler_params_default_emits_k_type_unspecified() {
        // Per the build_sampler_params docstring: LiteRT-LM v0.10.2's
        // CPU executor returns UNIMPLEMENTED for kGreedy (3), kTopK
        // (1), and kTopP (2) — only kTypeUnspecified (0) is
        // implemented on CPU, and that means "use the default sampler
        // baked into the .litertlm flatbuffer." When upstream wires
        // kGreedy on CPU (LiteRT-LM #2080 / #2081), this test flips
        // back to expecting Type_kGreedy.
        let params = build_sampler_params(&DeterminismKnobs::default());
        assert_eq!(params.type_, sys::Type_kTypeUnspecified);
        assert_eq!(params.temperature, 0.0);
        assert_eq!(params.top_p, 1.0);
        assert_eq!(params.top_k, 0);
        assert_eq!(params.seed, 0);
    }

    #[test]
    fn determinism_knobs_round_trip_from_manifest() {
        let m = aegis_policy::manifest::DeterminismKnobs {
            seed: Some(42),
            temperature: Some(0.0),
            top_p: Some(1.0),
            top_k: Some(0),
            repeat_penalty: Some(1.0),
        };
        let k = DeterminismKnobs::from(&m);
        assert_eq!(k.seed, Some(42));
        assert_eq!(k.temperature, Some(0.0));
        assert_eq!(k.top_p, Some(1.0));
        assert_eq!(k.top_k, Some(0));
        assert_eq!(k.repeat_penalty, Some(1.0));
    }

    #[test]
    fn missing_path_surfaces_unreadable_before_ffi_call() {
        // Engine::load short-circuits on missing path so we can
        // exercise its error path without the FFI initialized.
        let err = Engine::load(Path::new("/definitely/does/not/exist.litertlm"))
            .expect_err("missing path must error");
        match err {
            LiteRtError::ModelFileUnreadable { detail, .. } => {
                assert!(detail.contains("does not exist"));
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }
}
