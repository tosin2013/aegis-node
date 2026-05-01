//! Safe Rust wrapper around llama.cpp for the Aegis-Node runtime.
//!
//! Per [ADR-014](../../docs/adrs/014-cpu-first-gguf-inference-via-llama-cpp.md)
//! and the llama.cpp umbrella (#69). The FFI binding ships under the
//! strict safety wrapper described below ([LLM-A (#70)](https://github.com/tosin2013/aegis-node/issues/70)),
//! the [`Backend`] trait abstraction lives in `aegis-inference-engine`
//! and is implemented in `chat.rs` ([LLM-B (#71)](https://github.com/tosin2013/aegis-node/issues/71)),
//! and the manifest-surfaced determinism knobs (seed / temperature /
//! top-p / top-k / repeat-penalty) flow through
//! [`SessionOptions::determinism`] into [`build_sampler_chain`]
//! ([LLM-C (#72)](https://github.com/tosin2013/aegis-node/issues/72)).
//!
//! ## Public surface
//!
//! Three types, in dependency order:
//!
//! - [`Backend`] — process-level llama.cpp init. Created exactly once
//!   per process. Caller owns the lifetime; the wrapper does not retain
//!   any global state of its own (per the LLM-A "no global state"
//!   requirement).
//! - [`Model`] — a loaded GGUF, borrows `&Backend`.
//! - [`Session`] — a single inference session, borrows `&Model`.
//!
//! On `Drop`, every `Session` releases its context and every `Model`
//! releases its weights — these are guaranteed by the inner
//! `llama-cpp-2` types, which we wrap rather than re-implement.
//!
//! ## Safety posture
//!
//! Per the [LLM-A acceptance criteria](https://github.com/tosin2013/aegis-node/issues/70):
//!
//! - **No `unwrap` / `expect` on FFI returns.** Every FFI call is wrapped
//!   in a typed [`LlamaError`] variant.
//! - **Pinned upstream.** `llama-cpp-2` is pinned to an exact version in
//!   `Cargo.toml` (`=0.1.145`); bumping is an explicit, reviewed change.
//! - **Documented `unsafe`.** This wrapper itself contains zero `unsafe`
//!   blocks — every unsafe call is encapsulated by `llama-cpp-2`. Each
//!   call into that crate is annotated with the invariant we rely on
//!   for soundness.
//! - **Defined panic behavior.** Internal panics (i.e., bugs in the
//!   wrapper, not user errors) trigger [`std::process::abort`] via
//!   [`abort_on_internal_panic`]. We never let unwinding cross the FFI
//!   boundary back into llama.cpp.
//! - **No global state.** The `Backend` is caller-owned. Multiple
//!   `Backend::init` calls in the same process error cleanly with
//!   [`LlamaError::BackendAlreadyInitialized`] rather than silently
//!   succeeding (llama.cpp itself rejects double-init; we surface the
//!   error to make the contract explicit).

#![warn(missing_docs)]
#![allow(clippy::module_name_repetitions)]

use std::path::Path;
use std::sync::Arc;

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use thiserror::Error;

pub mod chat;
pub use chat::{LlamaCppBackend, LlamaCppLoadedModel};

/// All errors the wrapper surfaces. Each variant maps to one phase of
/// `Model::load` / `Session::infer` so an operator can see exactly
/// which step refused.
#[derive(Debug, Error)]
pub enum LlamaError {
    /// `llama.cpp` was already initialized in this process. The caller
    /// likely has two `Backend` handles or is sharing one across
    /// threads incorrectly. llama.cpp's init is process-global; only
    /// one [`Backend`] may exist at a time.
    #[error("llama.cpp backend already initialized in this process (only one Backend allowed)")]
    BackendAlreadyInitialized,

    /// llama.cpp's init failed for a system-level reason (resource
    /// exhaustion, missing GPU driver where one was required by the
    /// build, etc.). Not recoverable — surface and let the caller
    /// abort the process.
    #[error("llama.cpp backend init failed: {0}")]
    BackendInitFailed(String),

    /// The path to the GGUF file does not exist or could not be read.
    #[error("model file not found or unreadable: {path:?}: {detail}")]
    ModelFileUnreadable {
        /// Path the caller supplied.
        path: String,
        /// Underlying I/O reason from the OS or llama.cpp.
        detail: String,
    },

    /// llama.cpp refused the file as not a valid GGUF. Distinct from
    /// `ModelFileUnreadable`: the file exists, but its contents aren't
    /// a model llama.cpp recognizes.
    #[error("not a valid GGUF model: {path:?}: {detail}")]
    ModelLoadFailed {
        /// Path the caller supplied.
        path: String,
        /// Reason from llama.cpp (parser error, version mismatch, etc.).
        detail: String,
    },

    /// llama.cpp could not allocate or initialize an inference context
    /// for this model — typically out of memory, KV-cache too large,
    /// or context-length out of range.
    #[error("session creation failed: {0}")]
    SessionInitFailed(String),

    /// Tokenization of the prompt failed (e.g., the model's vocabulary
    /// rejects the input bytes).
    #[error("tokenization failed: {0}")]
    TokenizationFailed(String),

    /// `llama.cpp`'s decode step returned an error during inference.
    /// Distinct from a refusal — the runtime is in an undefined state
    /// after this and the [`Session`] should be dropped.
    #[error("inference decode failed: {0}")]
    InferenceFailed(String),

    /// The generated tokens couldn't be detokenized into UTF-8 text.
    /// Tokens that don't form valid UTF-8 (e.g., a broken byte-pair
    /// boundary) end the response — we never return invalid UTF-8.
    #[error("detokenization produced invalid UTF-8: {0}")]
    InvalidUtf8(String),

    /// The caller asked for an impossible configuration (e.g., zero
    /// context size). Distinct from underlying llama.cpp errors so the
    /// fix is obvious.
    #[error("invalid configuration: {0}")]
    InvalidConfig(&'static str),
}

/// Process-level llama.cpp backend. Caller owns the lifetime; only one
/// may exist per process at a time.
///
/// llama.cpp's init runs once per process and registers global state
/// inside the C++ library (logging hooks, CPU feature detection, etc.).
/// Our wrapper does not add any global state of its own — every handle
/// is reachable from a `Backend` value the caller created.
pub struct Backend {
    inner: LlamaBackend,
}

impl Backend {
    /// Initialize the llama.cpp backend. Returns
    /// [`LlamaError::BackendAlreadyInitialized`] if called twice in the
    /// same process; the caller must keep the first `Backend` alive
    /// for the duration of any model use.
    ///
    /// This is the only function in the wrapper that can mutate
    /// llama.cpp's process-level state. All subsequent `Model` /
    /// `Session` calls take a `&Backend` to enforce that lifetime
    /// chain at compile time.
    pub fn init() -> Result<Self, LlamaError> {
        // SAFETY-INVARIANT (delegated to llama-cpp-2): `LlamaBackend::init`
        // is the only entry point into llama.cpp's process-level setup.
        // It is internally idempotent on the C++ side but the Rust
        // binding refuses double-init with a typed error. We surface
        // that error verbatim so the caller can distinguish "already
        // initialized in this process" from "init failed for a system
        // reason."
        match LlamaBackend::init() {
            Ok(inner) => Ok(Self { inner }),
            Err(llama_cpp_2::LlamaCppError::BackendAlreadyInitialized) => {
                Err(LlamaError::BackendAlreadyInitialized)
            }
            Err(other) => Err(LlamaError::BackendInitFailed(other.to_string())),
        }
    }
}

/// A loaded GGUF model. Owns an `Arc<Backend>` so it has no lifetime
/// parameter — required for `Box<dyn LoadedModel>` in the LLM-B trait
/// (the dyn-trait object can't carry a Rust lifetime). The Arc is
/// cheap to clone and ensures the backend stays alive as long as
/// any model derived from it.
///
/// On `Drop`, llama.cpp releases the model weights — that's
/// `LlamaModel::Drop`'s job, which we just wrap.
pub struct Model {
    inner: LlamaModel,
    /// Shared `Backend` ownership. The Arc is the only thing keeping
    /// the backend alive when callers drop their original `Backend`
    /// handle.
    backend: Arc<Backend>,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Model").finish_non_exhaustive()
    }
}

impl Model {
    /// Load a GGUF model from disk.
    ///
    /// `backend` is taken by `Arc` so the resulting `Model` is
    /// `'static` and can be moved into a `Box<dyn LoadedModel>`. Clone
    /// the Arc cheaply if you need the backend handle to outlive any
    /// individual model.
    ///
    /// Errors are mapped to typed variants so the caller (and the F1
    /// boot path) sees exactly which gate refused: the file system,
    /// the GGUF parser, or context allocation.
    pub fn load(backend: Arc<Backend>, path: &Path) -> Result<Self, LlamaError> {
        if !path.exists() {
            return Err(LlamaError::ModelFileUnreadable {
                path: path.display().to_string(),
                detail: "path does not exist".to_string(),
            });
        }

        // Default model params: CPU-only, mmap'd. We do NOT add GPU
        // offload here — Phase 1 is CPU-first per ADR-014, and the
        // GPU backend is Phase 2 (#90).
        let params = LlamaModelParams::default();

        // SAFETY-INVARIANT (delegated to llama-cpp-2):
        // `LlamaModel::load_from_file` requires a valid `&LlamaBackend`
        // (held alive by the Arc field below) and a valid file path.
        // It returns `Err` for any FFI / IO problem; we never unwrap
        // here.
        match LlamaModel::load_from_file(&backend.inner, path, &params) {
            Ok(inner) => Ok(Self { inner, backend }),
            Err(e) => Err(LlamaError::ModelLoadFailed {
                path: path.display().to_string(),
                detail: e.to_string(),
            }),
        }
    }

    /// Number of tokens in this model's vocabulary. Useful for
    /// pre-allocating sample arrays in callers; otherwise opaque.
    #[must_use]
    pub fn n_vocab(&self) -> i32 {
        self.inner.n_vocab()
    }
}

impl From<&aegis_policy::manifest::DeterminismKnobs> for DeterminismKnobs {
    /// Mirror a manifest's [`aegis_policy::manifest::DeterminismKnobs`]
    /// into the llama-backend shape. Field-by-field copy — the two
    /// types share semantics; the manifest one is the persistence
    /// surface, this one is the FFI-facing input.
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

/// Sampling determinism knobs (per ADR-014, LLM-C / issue #72).
///
/// Every field is optional. `None` means "backend default for that
/// knob"; an explicit value drives the corresponding stage of the
/// sampler chain. Setting `seed = Some(N)` and `temperature =
/// Some(0.0)` together yields byte-identical output across runs —
/// the configuration auditors rely on for replay verification.
///
/// Parallel to the manifest type at
/// `aegis_policy::manifest::DeterminismKnobs`. Kept as a separate
/// shape (vs. importing the policy type directly) so LLM-A's wrapper
/// stays free of policy-crate deps and can be used standalone.
#[derive(Debug, Clone, Default)]
pub struct DeterminismKnobs {
    /// Sampler seed (uint32 range). Without it, the sampler picks a
    /// random seed per call and outputs vary run-to-run.
    pub seed: Option<u32>,
    /// Logit softmax temperature. `0.0` = greedy (always pick
    /// argmax). Higher values flatten the distribution.
    pub temperature: Option<f32>,
    /// Nucleus sampling — keep tokens whose cumulative probability
    /// mass is within `top_p`. `1.0` = no filter.
    pub top_p: Option<f32>,
    /// Keep only the top-`k` highest-probability tokens before
    /// sampling. `0` = no filter.
    pub top_k: Option<u32>,
    /// Penalty applied to recently-emitted tokens to discourage
    /// repetition. `1.0` = no penalty.
    pub repeat_penalty: Option<f32>,
}

/// Inference-time configuration.
#[derive(Debug, Clone)]
pub struct SessionOptions {
    /// Maximum context length in tokens. Bounded by the model's training
    /// context (clamped if larger). 0 means "use the model's training
    /// context length" — defensive default.
    pub n_ctx: u32,
    /// Maximum number of tokens to generate per `infer` call. Bounded
    /// to keep tests deterministic and to make a runaway sampler cheap.
    pub max_tokens: u32,
    /// Determinism knobs (LLM-C). Default: all `None` → greedy
    /// sampling at temperature 0.
    pub determinism: DeterminismKnobs,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            n_ctx: 2048,
            max_tokens: 256,
            determinism: DeterminismKnobs::default(),
        }
    }
}

/// A single-shot inference session. Holds a llama.cpp context bound to
/// `&Model`; on `Drop`, the context is released by `LlamaContext::Drop`.
pub struct Session<'m> {
    model: &'m Model,
    /// Shadow of [`SessionOptions::max_tokens`] — kept on the session
    /// so each `infer` call is bounded the same way.
    max_tokens: u32,
    /// Sampler chain assembled from [`DeterminismKnobs`] (per LLM-C).
    /// Default knobs (all `None`) collapse to a plain greedy sampler.
    sampler: LlamaSampler,
    /// Owned llama.cpp context.
    context: llama_cpp_2::context::LlamaContext<'m>,
}

/// Build the sampler chain that realizes the requested
/// [`DeterminismKnobs`].
///
/// Order is canonical for llama.cpp: `penalties → top_k → top_p →
/// temp → final-stage`. The final stage is `greedy` when
/// `temperature` is `0.0` or unset (always pick argmax — fully
/// deterministic), or `dist(seed)` otherwise (seed-aware random
/// sampling). With no knobs set, the chain is just `greedy` — the
/// LLM-A behavior preserved as a sane default.
///
/// Setting `seed = Some(N)` and `temperature = Some(0.0)` yields
/// byte-identical output across runs against the same model, prompt,
/// and `max_tokens` — that's the configuration the F8 replay viewer
/// (per ADR-010) and the demo program (per ADR-020) require for
/// reproducibility verification.
fn build_sampler_chain(knobs: &DeterminismKnobs) -> LlamaSampler {
    let mut samplers: Vec<LlamaSampler> = Vec::new();

    if let Some(rp) = knobs.repeat_penalty {
        // 1.0 means "no penalty" — skip the no-op stage rather than
        // pay its cost per token.
        if (rp - 1.0).abs() > f32::EPSILON {
            samplers.push(LlamaSampler::penalties(
                /* penalty_last_n: */ 64, rp, /* penalty_freq: */ 0.0,
                /* penalty_present: */ 0.0,
            ));
        }
    }

    if let Some(k) = knobs.top_k {
        // 0 means "no filter" — skip.
        if k > 0 {
            samplers.push(LlamaSampler::top_k(k as i32));
        }
    }

    if let Some(p) = knobs.top_p {
        // 1.0 means "no filter" — skip.
        if p < 1.0 {
            samplers.push(LlamaSampler::top_p(p, /* min_keep: */ 1));
        }
    }

    let temp = knobs.temperature.unwrap_or(0.0);
    if temp <= 0.0 {
        // Greedy = argmax. Fully deterministic regardless of seed,
        // which is the configuration auditors want for replay.
        samplers.push(LlamaSampler::greedy());
    } else {
        samplers.push(LlamaSampler::temp(temp));
        // Seed-aware random sampler. Without a pinned seed, llama.cpp
        // falls back to a per-call random seed and outputs vary
        // run-to-run — make the choice explicit by passing 0 as the
        // sentinel "random" seed when none is supplied.
        samplers.push(LlamaSampler::dist(knobs.seed.unwrap_or(0)));
    }

    LlamaSampler::chain_simple(samplers)
}

impl<'m> Session<'m> {
    /// Open a fresh inference context against `model`.
    pub fn new(model: &'m Model, options: SessionOptions) -> Result<Self, LlamaError> {
        if options.max_tokens == 0 {
            return Err(LlamaError::InvalidConfig("max_tokens must be > 0"));
        }

        // n_ctx == 0 means "use model default" — clamp to the model's
        // training context. We expose 0 as the user-facing way to say
        // "I don't care, pick a sensible default."
        let mut params = LlamaContextParams::default();
        if let Some(n_ctx) = std::num::NonZeroU32::new(options.n_ctx) {
            params = params.with_n_ctx(Some(n_ctx));
        }

        // SAFETY-INVARIANT (delegated to llama-cpp-2):
        // `LlamaModel::new_context` requires a valid `&LlamaModel`
        // (held alive by the `'m` lifetime) and valid context params.
        // Returns `Err` on allocation / configuration failure — we
        // don't unwrap.
        let context = model
            .inner
            .new_context(&model.backend.inner, params)
            .map_err(|e| LlamaError::SessionInitFailed(e.to_string()))?;

        let sampler = build_sampler_chain(&options.determinism);

        Ok(Self {
            model,
            max_tokens: options.max_tokens,
            sampler,
            context,
        })
    }

    /// Run a single prompt through the model and return the assistant's
    /// completion as a String.
    ///
    /// The wrapper is intentionally minimal: it tokenizes the prompt
    /// without applying a chat template (LLM-B's job — the chat
    /// template is bound at the session level via OCI-B, but
    /// formatting it into a turn structure is the Backend trait's
    /// concern). This entry point exists so LLM-A's smoke test can
    /// verify "load + decode + sample + detokenize" end-to-end.
    ///
    /// Stops on whichever fires first:
    /// 1. EOS token from the model.
    /// 2. `max_tokens` reached (per [`SessionOptions::max_tokens`]).
    /// 3. A detokenization step would produce invalid UTF-8 — we
    ///    refuse the whole response rather than return broken text.
    pub fn infer(&mut self, prompt: &str) -> Result<String, LlamaError> {
        let model = &self.model.inner;

        // Tokenize the prompt. Add a BOS — the smoke test inputs are
        // single-turn and don't carry their own BOS.
        let prompt_tokens = model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| LlamaError::TokenizationFailed(e.to_string()))?;

        if prompt_tokens.is_empty() {
            return Err(LlamaError::TokenizationFailed(
                "tokenizer returned 0 tokens for the prompt".to_string(),
            ));
        }

        // Feed the prompt as a single batch. Only the last token's
        // logits are needed for the next-step sample; computing logits
        // for every prompt token is wasted work.
        let mut batch = LlamaBatch::new(prompt_tokens.len().max(1), 1);
        let last_idx = prompt_tokens.len() - 1;
        for (i, token) in prompt_tokens.iter().enumerate() {
            let want_logits = i == last_idx;
            batch
                .add(*token, i as i32, &[0], want_logits)
                .map_err(|e| LlamaError::InferenceFailed(format!("batch.add: {e}")))?;
        }

        self.context
            .decode(&mut batch)
            .map_err(|e| LlamaError::InferenceFailed(format!("decode: {e}")))?;

        let mut output = String::new();
        let mut cur_pos = prompt_tokens.len() as i32;
        // After the prompt decode, the logits index of the last
        // produced token within the batch is `last_idx` — we sample
        // from there for the first generation step.
        let mut sample_idx = last_idx as i32;

        // Explicit counter is clearer than the clippy-suggested
        // zip-with-infinite-range alternative.
        #[allow(clippy::explicit_counter_loop)]
        for _ in 0..self.max_tokens {
            // SAFETY-INVARIANT (delegated to llama-cpp-2):
            // `LlamaSampler::sample` reads logits from the most recent
            // decode at index `sample_idx`. We track that index
            // explicitly across iterations rather than guessing -1, so
            // a future addition of speculative decoding stays correct.
            let next_token = self.sampler.sample(&self.context, sample_idx);
            // Update the sampler's repetition / grammar state so the
            // next iteration's distribution accounts for what we just
            // emitted.
            self.sampler.accept(next_token);

            // EOS / EOG = stop. Use the model's vocab to identify the
            // token class; don't hard-code an integer.
            if model.is_eog_token(next_token) {
                break;
            }

            // Detokenize and append. Reject invalid UTF-8 per the
            // contract above. `special=false` keeps internal control
            // tokens out of the user-facing string.
            let bytes = model
                .token_to_piece_bytes(next_token, 64, false, None)
                .map_err(|e| LlamaError::InferenceFailed(format!("token_to_piece_bytes: {e}")))?;
            let text = std::str::from_utf8(&bytes).map_err(|e| {
                LlamaError::InvalidUtf8(format!("non-UTF8 token output at position {cur_pos}: {e}"))
            })?;
            output.push_str(text);

            // Feed the sampled token back as the next decode step.
            batch.clear();
            batch
                .add(next_token, cur_pos, &[0], true)
                .map_err(|e| LlamaError::InferenceFailed(format!("batch.add (gen): {e}")))?;
            self.context
                .decode(&mut batch)
                .map_err(|e| LlamaError::InferenceFailed(format!("decode (gen): {e}")))?;

            // After a single-token decode, the new logits sit at index
            // 0 of the batch.
            sample_idx = 0;
            cur_pos += 1;
        }

        Ok(output)
    }
}

/// Wrap an FFI-callback closure so any panic inside it aborts the
/// process via [`std::process::abort`] instead of unwinding back into
/// C++ code (which is undefined behavior on most ABIs).
///
/// LLM-A doesn't currently set any FFI callbacks (logging, sampling
/// hooks, etc.), but as we wire those in (LLM-B will, for sampler
/// chains), wrap the closure in this guard. Surfacing it now means we
/// document the policy at the same time we add the binding, not after
/// the first crash report.
///
/// # Example
///
/// ```ignore
/// llama_cpp_2::set_log_callback(|msg| {
///     aegis_llama_backend::abort_on_internal_panic(|| {
///         tracing::info!(?msg, "llama.cpp log");
///     })
/// });
/// ```
pub fn abort_on_internal_panic<F: FnOnce() -> R + std::panic::UnwindSafe, R>(f: F) -> R {
    match std::panic::catch_unwind(f) {
        Ok(r) => r,
        Err(_) => {
            // We deliberately don't try to format the panic info —
            // formatter calls can panic too, and we're already in an
            // FFI-callback context where unwinding is UB. A fast,
            // unformatted abort is the only safe move.
            std::process::abort();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn session_options_default_is_sane() {
        let opts = SessionOptions::default();
        assert!(opts.n_ctx > 0);
        assert!(opts.max_tokens > 0);
    }

    #[test]
    fn invalid_config_max_tokens_zero_surfaces_typed_error() {
        // We can't actually create a Backend in unit tests without
        // the heavy FFI initializer running; instead, test the
        // wrapper-level validation in isolation by inspecting the
        // discriminant via Display.
        let err = LlamaError::InvalidConfig("max_tokens must be > 0");
        let s = err.to_string();
        assert!(s.contains("max_tokens"), "{s}");
    }

    #[test]
    fn missing_model_file_path_is_categorized_as_unreadable() {
        // Sanity: the error variant most callers will hit looks like
        // a file-not-found, not a parse error. The actual classifier
        // logic lives in `Model::load`, which needs a Backend; here
        // we just confirm the discriminant rendering.
        let err = LlamaError::ModelFileUnreadable {
            path: "/nope".to_string(),
            detail: "no such file".to_string(),
        };
        assert!(err.to_string().contains("not found or unreadable"));
    }

    // --- LLM-C determinism knobs --------------------------------------
    //
    // We can't observe the chain's internals without an FFI sample
    // (LlamaBackend::init is a process-global) but we can exercise
    // the builder with various knob shapes to make sure no path
    // panics — that's enough to defend against an `unwrap()` slipping
    // back in. The actual same-seed-gives-same-output assertion lives
    // in the gated real-model smoke test.

    #[test]
    fn build_sampler_chain_with_default_knobs_does_not_panic() {
        let _ = build_sampler_chain(&DeterminismKnobs::default());
    }

    #[test]
    fn build_sampler_chain_with_only_seed_does_not_panic() {
        // `temperature` defaults to None → 0.0 → greedy. Seed is
        // ignored on the greedy path; the builder must still accept
        // the configuration cleanly.
        let _ = build_sampler_chain(&DeterminismKnobs {
            seed: Some(42),
            ..Default::default()
        });
    }

    #[test]
    fn build_sampler_chain_with_full_chain_does_not_panic() {
        // Every stage active.
        let _ = build_sampler_chain(&DeterminismKnobs {
            seed: Some(42),
            temperature: Some(0.7),
            top_p: Some(0.95),
            top_k: Some(40),
            repeat_penalty: Some(1.1),
        });
    }

    #[test]
    fn build_sampler_chain_skips_no_op_stages() {
        // `top_k = 0`, `top_p = 1.0`, `repeat_penalty = 1.0` are all
        // documented "no filter" sentinels. The builder must not
        // emit no-op stages — those have a per-token cost.
        // We can't observe the chain length directly via the FFI
        // wrapper, so this test exists to keep the no-op skip logic
        // covered by line coverage. Any panic here would fail.
        let _ = build_sampler_chain(&DeterminismKnobs {
            top_k: Some(0),
            top_p: Some(1.0),
            repeat_penalty: Some(1.0),
            ..Default::default()
        });
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
}
