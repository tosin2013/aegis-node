//! Aegis-Node session lifecycle (F0-A — issue #24).
//!
//! `Session` is the runtime's top-level integration object. `boot` reads
//! a manifest + model + config, computes their SHA-256 digests, gets an
//! SVID with those digests bound in, opens the Trajectory Ledger, and
//! emits the `EntryType::SessionStart` entry. `shutdown` writes
//! `SessionEnd` and returns the chain root hash.
//!
//! The mediator (F0-B, #25) sits on top of `Session` and owns the
//! per-tool-call sequence: rebind → policy → gate → access entry. This
//! module deliberately does not implement that — boot is its own slice.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use aegis_approval_gate::{ApprovalChannel, SessionGrantTable};
use aegis_identity::{
    verify_chat_template_binding, verify_digest_binding, Digest, DigestField, DigestTriple,
    LocalCa, SpiffeId, X509Svid,
};
use aegis_ledger_writer::{Entry, EntryType, LedgerSchemaVersion, LedgerWriter};
use aegis_mcp_client::McpClient;
use aegis_policy::{Policy, SessionAggregateState};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

use crate::error::{Error, Result};

/// Inputs to [`Session::boot`].
#[derive(Debug, Clone)]
pub struct BootConfig {
    /// Caller-supplied session identifier (UUIDv7 in production; tests
    /// pin it for golden output).
    pub session_id: String,
    pub manifest_path: PathBuf,
    pub model_path: PathBuf,
    /// Optional runtime config; absent → empty-bytes digest.
    pub config_path: Option<PathBuf>,
    /// Optional chat-template digest sidecar produced by `aegis pull`
    /// (per ADR-022 / OCI-B). When `Some`, the file's hex contents are
    /// parsed into a 32-byte SHA-256 and bound into the SVID via the
    /// `CHAT_TEMPLATE_BINDING_OID` extension. When `None`, no
    /// chat-template binding is set (back-compat for legacy callers and
    /// for non-GGUF models that don't carry a chat template).
    pub chat_template_sidecar: Option<PathBuf>,
    pub identity_dir: PathBuf,
    pub workload_name: String,
    pub instance: String,
    pub ledger_path: PathBuf,
    /// Ledger schema version (ADR-026). `None` means the default
    /// ([`LedgerSchemaVersion::V1`]) — existing callers stay on v1
    /// without churn. New callers opt in to v2 by setting
    /// `Some(LedgerSchemaVersion::V2)`.
    pub ledger_schema: Option<LedgerSchemaVersion>,
}

/// Live agent session: compiled policy, open ledger, issued SVID, the
/// digest triple bound at boot, and the agent identity hash that flows
/// into every ledger entry. Paths are retained so the F0-B mediator
/// can re-hash live bytes on every per-tool-call rebind check.
pub struct Session {
    policy: Policy,
    ledger: LedgerWriter,
    svid_cert_pem: String,
    svid_key_pem: String,
    bound_digests: DigestTriple,
    /// Bound chat-template digest (per ADR-022 / OCI-B). `None` when the
    /// session was booted without a chat-template sidecar (e.g., legacy
    /// callers, non-GGUF models). `Some` when the SVID's
    /// `CHAT_TEMPLATE_BINDING_OID` extension was issued.
    bound_chat_template: Option<Digest>,
    spiffe_id: SpiffeId,
    agent_identity_hash: [u8; 32],
    session_id: String,
    /// Wall-clock timestamp captured at boot. Used as the anchor for
    /// time-bounded write_grants (`duration: PT1H` means valid for the
    /// first hour of THIS session). Per ADR-009 / issue #38.
    pub(crate) session_start: DateTime<Utc>,
    pub(crate) manifest_path: PathBuf,
    pub(crate) model_path: PathBuf,
    pub(crate) config_path: Option<PathBuf>,
    /// Local CA directory — retained so the multi-turn driver can
    /// re-load the CA each turn to mint a fresh per-turn SVID
    /// (ADR-030). The CA is cheap to load (couple of file reads); we
    /// don't cache it on Session because the load path also surfaces
    /// any post-boot CA tampering as a typed error.
    pub(crate) identity_dir: PathBuf,
    pub(crate) workload_name: String,
    pub(crate) instance: String,
    /// F3 approval channel — routes `Decision::RequireApproval`. None
    /// means the legacy halt-on-RequireApproval behavior; set via
    /// [`Session::with_approval_channel`] after boot.
    pub(crate) approval_channel: Option<Box<dyn ApprovalChannel>>,
    /// F6 end-of-session network attestation accumulator (issue #37).
    /// Every `mediate_network_connect` call appends one entry, regardless
    /// of outcome. `shutdown` summarizes + signs + emits a
    /// `NetworkAttestation` ledger entry before `SessionEnd`.
    pub(crate) network_log: Vec<NetworkConnectionMeta>,
    /// MCP client used by `mediate_mcp_tool_call` (per ADR-018 / F2-MCP-B
    /// / issue #44). None means MCP tool calls are unsupported in this
    /// session — the mediator returns `Error::Denied` rather than panic.
    /// Set via [`Session::with_mcp_client`] after boot.
    pub(crate) mcp_client: Option<Box<dyn McpClient>>,
    /// LLM-B inference backend. None means `run_turn` is unavailable
    /// (the legacy fixed-script `run` path keeps working). Set via
    /// [`Session::with_loaded_model`] after boot. Per ADR-014.
    pub(crate) loaded_model: Option<Box<dyn crate::backend::LoadedModel>>,
    /// Adversarial pre-filter classifier (ADR-028). Always populated
    /// after boot — defaults to [`crate::adversarial::default_classifier`]
    /// (the always-on `RegexHeuristicClassifier`). Operators can swap
    /// in a model-backed classifier via
    /// [`Session::with_adversarial_classifier`].
    pub(crate) adversarial_classifier: crate::adversarial::SharedClassifier,
    /// Per-session aggregate quota accumulator (ADR-027). Zero-cost
    /// when the manifest declares no quotas; tracks call counts per
    /// tool class otherwise. Reset on each [`Self::boot`] — there is
    /// no cross-session quota state by design.
    pub(crate) aggregate_state: SessionAggregateState,
    /// Per-turn SVID (ADR-030). Issued at `turn_start` by the
    /// multi-turn driver; dropped at `turn_end`. `None` outside a turn
    /// — the session SVID
    /// ([`Self::svid_cert_pem`] / [`Self::svid_key_pem`]) takes over
    /// for single-turn paths and mediator calls invoked directly.
    pub(crate) current_turn_svid: Option<X509Svid>,
    /// Task-scoped ephemeral approval-grant table (ADR-029). Lookup
    /// key is `(tool_name, sha256(canonical_args))`. Reset at boot;
    /// in-memory only — grants vaporize at session end by design.
    pub(crate) grant_table: SessionGrantTable,
}

/// One observed network-connection attempt + the gate's decision.
/// Kept narrow: host + port + protocol + outcome + when. The full
/// reasoning step lives in F5 entries already.
#[derive(Debug, Clone)]
pub struct NetworkConnectionMeta {
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub decision: NetworkConnectionDecision,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkConnectionDecision {
    /// Policy returned Allow without invoking the approval gate.
    Allowed,
    /// Policy returned RequireApproval and the channel granted.
    Approved,
    /// Denied — by policy, by approval rejection, or by approval timeout.
    Denied,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // LedgerWriter holds a BufWriter<File> that isn't Debug; surface
        // only the operator-meaningful fields.
        f.debug_struct("Session")
            .field("session_id", &self.session_id)
            .field("spiffe_id", &self.spiffe_id)
            .field("bound_digests", &self.bound_digests)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// Run the boot sequence end-to-end. On any failure the partial
    /// ledger is dropped (LedgerWriter cleans up via close-on-drop).
    pub fn boot(cfg: BootConfig) -> Result<Self> {
        let session_start = Utc::now();
        let policy = Policy::from_yaml_file(&cfg.manifest_path)?;

        // Refuse a manifest that claims an MCP server name reserved
        // for native dispatch (per [#92](https://github.com/tosin2013/aegis-node/issues/92)).
        // Catching it here means the conflict is loud at boot, not
        // silent at the first tool call.
        for server in &policy.manifest().tools.mcp {
            if crate::turn::RESERVED_NATIVE_NAMESPACES
                .iter()
                .any(|r| *r == server.server_name)
            {
                return Err(Error::ReservedMcpServerName {
                    name: server.server_name.clone(),
                });
            }
        }

        let model_digest = sha256_file(&cfg.model_path)?;
        let manifest_digest = sha256_file(&cfg.manifest_path)?;
        let config_digest = match &cfg.config_path {
            Some(p) => sha256_file(p)?,
            None => sha256_bytes(&[]),
        };
        let bound_digests = DigestTriple {
            model: Digest(model_digest),
            manifest: Digest(manifest_digest),
            config: Digest(config_digest),
        };

        // Read the chat-template sidecar if the caller supplied one.
        // Per ADR-022 the sidecar carries a hex SHA-256 of the GGUF's
        // `tokenizer.chat_template` bytes; we parse it but do NOT
        // re-derive it here (the runtime trust boundary doesn't parse
        // GGUFs). The sidecar is itself the product of a cosign-covered
        // manifest annotation; if it's been tampered with on disk, the
        // SVID-self-check below catches it indirectly via the issuer.
        let bound_chat_template = match &cfg.chat_template_sidecar {
            Some(path) => Some(read_chat_template_sidecar(path)?),
            None => None,
        };

        let ca = LocalCa::load(&cfg.identity_dir)?;
        let svid = ca.issue_svid_with_chat_template(
            &cfg.workload_name,
            &cfg.instance,
            bound_digests,
            bound_chat_template,
        )?;

        // Self-check: the cert we just got back MUST encode the digests
        // we passed in. If not, aegis-identity has a bug — fail loud.
        if let Some(mismatch) = verify_digest_binding(&svid.cert_pem, &bound_digests)? {
            return Err(Error::SvidSelfCheck {
                field: digest_field_name(mismatch.field).to_string(),
            });
        }
        if let Some(mismatch) =
            verify_chat_template_binding(&svid.cert_pem, bound_chat_template.as_ref())?
        {
            return Err(Error::SvidSelfCheck {
                field: digest_field_name(mismatch.field).to_string(),
            });
        }

        let agent_identity_hash = sha256_bytes(svid.spiffe_id.uri().as_bytes());

        let schema_version = cfg.ledger_schema.unwrap_or_default();
        let mut ledger = LedgerWriter::create_with_version(
            &cfg.ledger_path,
            cfg.session_id.clone(),
            schema_version,
        )?;

        let mut payload = Map::new();
        payload.insert("spiffeId".to_string(), Value::String(svid.spiffe_id.uri()));
        payload.insert(
            "modelDigestHex".to_string(),
            Value::String(hex::encode(bound_digests.model.0)),
        );
        payload.insert(
            "manifestDigestHex".to_string(),
            Value::String(hex::encode(bound_digests.manifest.0)),
        );
        payload.insert(
            "configDigestHex".to_string(),
            Value::String(hex::encode(bound_digests.config.0)),
        );
        if let Some(template) = bound_chat_template {
            payload.insert(
                "chatTemplateDigestHex".to_string(),
                Value::String(hex::encode(template.0)),
            );
        }
        ledger.append(Entry {
            session_id: cfg.session_id.clone(),
            entry_type: EntryType::SessionStart,
            agent_identity_hash,
            timestamp: Utc::now(),
            payload,
        })?;

        Ok(Self {
            policy,
            ledger,
            svid_cert_pem: svid.cert_pem,
            svid_key_pem: svid.key_pem,
            bound_digests,
            bound_chat_template,
            spiffe_id: svid.spiffe_id,
            agent_identity_hash,
            session_id: cfg.session_id,
            session_start,
            manifest_path: cfg.manifest_path,
            model_path: cfg.model_path,
            config_path: cfg.config_path,
            identity_dir: cfg.identity_dir,
            workload_name: cfg.workload_name,
            instance: cfg.instance,
            approval_channel: None,
            network_log: Vec::new(),
            mcp_client: None,
            loaded_model: None,
            adversarial_classifier: crate::adversarial::default_classifier(),
            aggregate_state: SessionAggregateState::new(),
            current_turn_svid: None,
            grant_table: SessionGrantTable::new(),
        })
    }

    /// Attach an F3 approval channel. When set, `Decision::RequireApproval`
    /// is routed through `channel` (TTY prompt, file poll, etc.) before
    /// the mediator dispatches the operation. Without it, the mediator
    /// preserves the pre-#27 halt-on-RequireApproval behavior.
    pub fn with_approval_channel(mut self, channel: Box<dyn ApprovalChannel>) -> Self {
        self.approval_channel = Some(channel);
        self
    }

    /// Attach an MCP client. Required to invoke `mediate_mcp_tool_call`;
    /// without it MCP tool calls are denied (the mediator emits a
    /// Violation citing "no MCP client configured").
    pub fn with_mcp_client(mut self, client: Box<dyn McpClient>) -> Self {
        self.mcp_client = Some(client);
        self
    }

    /// Attach an LLM-B inference backend. Required to invoke
    /// [`Self::run_turn`]; without it `run_turn` returns
    /// [`Error::NoBackendConfigured`]. Per ADR-014 / LLM-B.
    pub fn with_loaded_model(mut self, model: Box<dyn crate::backend::LoadedModel>) -> Self {
        self.loaded_model = Some(model);
        self
    }

    /// `&mut self` form of [`Self::with_loaded_model`]. Useful when
    /// the session was already booted (e.g., by the CLI's `aegis run`
    /// path) and the caller wants to plug in a backend without
    /// consuming the session value. Replaces any previously-attached
    /// model.
    pub fn set_loaded_model(&mut self, model: Box<dyn crate::backend::LoadedModel>) {
        self.loaded_model = Some(model);
    }

    /// Swap the [`AdversarialClassifier`](crate::adversarial::AdversarialClassifier)
    /// used by the multi-turn loop's pre-filter gate (ADR-028). The
    /// default (set at boot) is the always-on
    /// [`RegexHeuristicClassifier`](crate::adversarial::RegexHeuristicClassifier).
    /// Operators wanting the model-backed `LiteRtLmGuardClassifier`
    /// (opt-in per ADR-028 §"Classifier interface") swap it in here.
    pub fn with_adversarial_classifier(
        mut self,
        classifier: crate::adversarial::SharedClassifier,
    ) -> Self {
        self.adversarial_classifier = classifier;
        self
    }

    /// Wall-clock anchor for time-bounded write_grants — set once at boot.
    pub fn session_start(&self) -> DateTime<Utc> {
        self.session_start
    }

    /// Emit a `NetworkAttestation` then a `SessionEnd`, close the
    /// ledger, and return the chain root hash. The attestation MUST be
    /// emitted even for zero-connection runs (per issue #37 / F6) —
    /// "no attestation entry" is not equivalent to "no connections".
    pub fn shutdown(mut self) -> Result<[u8; 32]> {
        crate::attestation::emit_network_attestation(&mut self)?;

        let mut payload = Map::new();
        payload.insert("spiffeId".to_string(), Value::String(self.spiffe_id.uri()));
        self.ledger.append(Entry {
            session_id: self.session_id.clone(),
            entry_type: EntryType::SessionEnd,
            agent_identity_hash: self.agent_identity_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(self.ledger.close()?)
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// Ledger schema version this session is writing under (ADR-026).
    /// Determines whether the multi-turn driver emits v2 `turn_start` /
    /// `turn_end` / `tool_call` / `tool_result` entries.
    pub fn schema_version(&self) -> LedgerSchemaVersion {
        self.ledger.schema_version()
    }

    /// Write a `turn_start` entry (v2, ADR-026 §"Per-turn entry
    /// sequence" + ADR-030 §"Interaction with the F9 ledger"). Records
    /// the turn number, the bound model digest, the SHA-256 of the
    /// canonical-serialized input context, the per-turn SVID's
    /// thumbprint, and the SVID's audience claim. F8 replay reads
    /// `contextDigestHex` to detect mid-session context tampering;
    /// auditors cross-check `svidThumbprintHex` against access entries
    /// from this turn (cross-check itself is a deferred follow-up).
    ///
    /// Caller is responsible for only invoking this on v2 ledgers —
    /// emitting it on a v1 file would taint the chain with an entry
    /// type v1 consumers don't expect.
    pub(crate) fn write_turn_start(
        &mut self,
        turn_number: u32,
        context_digest_hex: &str,
        svid_thumbprint_hex: &str,
        spiffe_id_aud: &str,
    ) -> Result<()> {
        let mut payload = Map::new();
        payload.insert("turnNumber".to_string(), Value::Number(turn_number.into()));
        payload.insert(
            "modelDigestHex".to_string(),
            Value::String(hex::encode(self.bound_digests.model.0)),
        );
        payload.insert(
            "contextDigestHex".to_string(),
            Value::String(context_digest_hex.to_string()),
        );
        payload.insert(
            "svidThumbprintHex".to_string(),
            Value::String(svid_thumbprint_hex.to_string()),
        );
        payload.insert(
            "spiffeIdAud".to_string(),
            Value::String(spiffe_id_aud.to_string()),
        );
        self.ledger.append(Entry {
            session_id: self.session_id.clone(),
            entry_type: EntryType::TurnStart,
            agent_identity_hash: self.agent_identity_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }

    /// Drop the active per-turn SVID. Called by the multi-turn driver
    /// at `turn_end` (ADR-030 §"Per-turn rebinding lifecycle"). Memory
    /// containing the key material falls out of scope; Rust's drop
    /// semantics handle the zero-out at the `Drop` impl of the
    /// underlying buffer types. The session-long SVID is unaffected.
    pub(crate) fn drop_turn_svid(&mut self) {
        self.current_turn_svid = None;
    }

    /// Issue a per-turn SVID (ADR-030). Loads the local CA from disk,
    /// hashes the live digest triple, mints a short-lived SVID with a
    /// `TURN_BINDING` extension carrying `audience`, stashes it as
    /// `self.current_turn_svid`, and returns the cert's SHA-256
    /// thumbprint so the caller can record it in `turn_start` without
    /// re-borrowing `self`. The session-long SVID
    /// ([`Self::cert_pem`]) remains in place for dispatch paths
    /// outside the multi-turn loop.
    ///
    /// TTL caps at 60s + the remaining wallclock budget for the
    /// session so the per-turn SVID can never outlive the loop that
    /// issued it. Bounded to ≥60s so a near-empty budget still mints
    /// a usable cert.
    pub(crate) fn issue_turn_svid(
        &mut self,
        audience: &str,
        wallclock_remaining_seconds: u64,
    ) -> Result<String> {
        let ttl_secs = wallclock_remaining_seconds.saturating_add(60).max(60);
        let ttl = time::Duration::seconds(i64::try_from(ttl_secs).unwrap_or(i64::MAX));

        let live_digests = self.compute_live_digests()?;
        let ca = LocalCa::load(&self.identity_dir)?;
        let svid = ca.issue_turn_svid(
            &self.workload_name,
            &self.instance,
            live_digests,
            self.bound_chat_template,
            audience,
            ttl,
        )?;
        let thumbprint = aegis_identity::cert_thumbprint_hex(&svid.cert_pem)?;
        self.current_turn_svid = Some(svid);
        Ok(thumbprint)
    }

    /// Write a `turn_end` entry (v2). Closes a turn with cumulative
    /// usage counters and the per-tool-class aggregate-quota snapshot
    /// (ADR-027). `quotaSnapshots[]` carries one entry per declared
    /// or dispatched class — auditors can chart budget burn-down
    /// across the session.
    pub(crate) fn write_turn_end(
        &mut self,
        turn_number: u32,
        tokens_in: Option<u64>,
        tokens_out: Option<u64>,
        tokens_cumulative: u64,
        wallclock_ms_cumulative: u64,
    ) -> Result<()> {
        let mut payload = Map::new();
        payload.insert("turnNumber".to_string(), Value::Number(turn_number.into()));
        if let Some(t) = tokens_in {
            payload.insert("tokensIn".to_string(), Value::Number(t.into()));
        }
        if let Some(t) = tokens_out {
            payload.insert("tokensOut".to_string(), Value::Number(t.into()));
        }
        payload.insert(
            "tokensCumulative".to_string(),
            Value::Number(tokens_cumulative.into()),
        );
        payload.insert(
            "wallclockMsCumulative".to_string(),
            Value::Number(wallclock_ms_cumulative.into()),
        );
        let snapshots = self.aggregate_state.snapshots(self.policy.manifest());
        let snapshots_json = serde_json::to_value(&snapshots).unwrap_or(Value::Array(Vec::new()));
        payload.insert("quotaSnapshots".to_string(), snapshots_json);
        self.ledger.append(Entry {
            session_id: self.session_id.clone(),
            entry_type: EntryType::TurnEnd,
            agent_identity_hash: self.agent_identity_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }

    /// Write a `tool_call` entry (v2). Records that the model elected
    /// to invoke `tool_name` with `arguments` during the given turn.
    /// `request_args_hex` is the SHA-256 of the canonical-serialized
    /// arguments — F8 replay matches calls to results without storing
    /// the args twice in the chain.
    pub(crate) fn write_tool_call_entry(
        &mut self,
        turn_number: u32,
        tool_call_id: &str,
        tool_name: &str,
        tool_origin: &crate::adversarial::ToolOrigin,
        request_args_hex: &str,
    ) -> Result<()> {
        let mut payload = Map::new();
        payload.insert("turnNumber".to_string(), Value::Number(turn_number.into()));
        payload.insert(
            "toolCallId".to_string(),
            Value::String(tool_call_id.to_string()),
        );
        payload.insert("toolName".to_string(), Value::String(tool_name.to_string()));
        payload.insert(
            "toolOrigin".to_string(),
            Value::String(tool_origin.to_string()),
        );
        payload.insert(
            "requestArgsHex".to_string(),
            Value::String(request_args_hex.to_string()),
        );
        self.ledger.append(Entry {
            session_id: self.session_id.clone(),
            entry_type: EntryType::ToolCall,
            agent_identity_hash: self.agent_identity_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }

    /// Write a `tool_result` entry (v2). Pairs with a prior `tool_call`
    /// via `tool_call_id`. Stores the result inline when ≤32 KB,
    /// otherwise stores only the hash + a sidecar `resultPayloadRef`.
    /// Sidecar emission itself is deferred (ADR-026 follow-up); the
    /// 32 KB threshold is enforced here so existing tests don't bloat
    /// ledger files.
    pub(crate) fn write_tool_result_entry(
        &mut self,
        turn_number: u32,
        tool_call_id: &str,
        result_hash_hex: &str,
        result_payload: serde_json::Value,
    ) -> Result<()> {
        const INLINE_THRESHOLD_BYTES: usize = 32 * 1024;
        let mut payload = Map::new();
        payload.insert("turnNumber".to_string(), Value::Number(turn_number.into()));
        payload.insert(
            "toolCallId".to_string(),
            Value::String(tool_call_id.to_string()),
        );
        payload.insert(
            "resultHashHex".to_string(),
            Value::String(result_hash_hex.to_string()),
        );
        let approx_size = serde_json::to_string(&result_payload)
            .map(|s| s.len())
            .unwrap_or(usize::MAX);
        if approx_size <= INLINE_THRESHOLD_BYTES {
            payload.insert("resultPayload".to_string(), result_payload);
        } else {
            // Sidecar mechanism is deferred (ADR-026 follow-up). For
            // now drop the inline payload and record the hash + a
            // marker that downstream tooling can detect. This keeps
            // chain integrity while signalling "blob too large to
            // inline; sidecar emission TODO".
            payload.insert(
                "resultPayloadRef".to_string(),
                Value::String(format!("pending-sidecar:{result_hash_hex}.{approx_size}b")),
            );
        }
        self.ledger.append(Entry {
            session_id: self.session_id.clone(),
            entry_type: EntryType::ToolResult,
            agent_identity_hash: self.agent_identity_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }

    /// Append an `AdversarialContent` Violation to the F9 ledger
    /// (ADR-028). Called by the multi-turn driver when the
    /// pre-filter gate flags a tool result. Like
    /// `TurnCapExceeded`, this is namespaced under
    /// `violationKind: "AdversarialContent"` for v1-schema
    /// compatibility — ADR-026's schema v2 will move it into
    /// `tool_result.adversarialClassifier`.
    pub(crate) fn write_adversarial_violation(
        &mut self,
        verdict: &crate::adversarial::ClassifierVerdict,
        classifier_name: &str,
        origin: &crate::adversarial::ToolOrigin,
    ) -> Result<()> {
        use crate::adversarial::ClassifierVerdict as V;
        let (reason, score) = match verdict {
            V::Clean => return Ok(()), // shouldn't be called; defend in depth
            V::Suspicious { reason, score } => (reason.clone(), *score),
            V::Malicious { reason, score } => (reason.clone(), *score),
        };

        let mut payload = Map::new();
        payload.insert(
            "violationKind".to_string(),
            Value::String("AdversarialContent".to_string()),
        );
        payload.insert(
            "violationReason".to_string(),
            Value::String(format!(
                "adversarial pre-filter flagged tool result: {} (reason={reason}, score={score:.2})",
                verdict.as_str()
            )),
        );
        payload.insert(
            "classifierVerdict".to_string(),
            Value::String(verdict.as_str().to_string()),
        );
        payload.insert(
            "classifierName".to_string(),
            Value::String(classifier_name.to_string()),
        );
        if let Some(n) = serde_json::Number::from_f64(score.into()) {
            payload.insert("classifierScore".to_string(), Value::Number(n));
        }
        payload.insert("classifierReason".to_string(), Value::String(reason));
        payload.insert("toolOrigin".to_string(), Value::String(origin.to_string()));

        self.ledger.append(Entry {
            session_id: self.session_id.clone(),
            entry_type: EntryType::Violation,
            agent_identity_hash: self.agent_identity_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }

    /// Append an `AggregateCapExceeded` Violation to the F9 ledger
    /// (ADR-027). Called by the per-tool-class mediators when the
    /// aggregate accumulator refuses one more dispatch. Like
    /// `TurnCapExceeded` and `AdversarialContent`, namespaced under
    /// `violationKind: "AggregateCapExceeded"` so v1 ledger readers
    /// can ignore it; ADR-026's schema v2 wires it into
    /// `turn_end.quotaSnapshots[]` as well.
    pub(crate) fn write_aggregate_cap_violation(
        &mut self,
        err: &aegis_policy::AggregateCapExceeded,
        resource_uri: &str,
        access_kind: &str,
    ) -> Result<()> {
        let mut payload = Map::new();
        payload.insert(
            "violationKind".to_string(),
            Value::String("AggregateCapExceeded".to_string()),
        );
        payload.insert(
            "violationReason".to_string(),
            Value::String(format!(
                "aggregate cap exceeded: class={} bound={} observed={} cap={}",
                err.class.label(),
                err.bound,
                err.observed,
                err.cap,
            )),
        );
        payload.insert(
            "resourceUri".to_string(),
            Value::String(resource_uri.to_string()),
        );
        payload.insert(
            "accessType".to_string(),
            Value::String(access_kind.to_string()),
        );
        payload.insert("toolClass".to_string(), Value::String(err.class.label()));
        payload.insert("capBound".to_string(), Value::String(err.bound.to_string()));
        payload.insert("observed".to_string(), Value::Number(err.observed.into()));
        payload.insert("cap".to_string(), Value::Number(err.cap.into()));

        self.ledger.append(Entry {
            session_id: self.session_id.clone(),
            entry_type: EntryType::Violation,
            agent_identity_hash: self.agent_identity_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }

    /// Run the ADR-027 aggregate-quota gate for `class` and convert any
    /// cap breach into an `Error::Denied` with a synthesised reason.
    /// Emits the `AggregateCapExceeded` ledger entry as a side-effect
    /// on breach. On pass: increments the accumulator and returns
    /// `Ok(())`; the caller proceeds to dispatch the syscall.
    pub(crate) fn enforce_aggregate_quota(
        &mut self,
        class: aegis_policy::ToolClass,
        resource_uri: &str,
        access_kind: &str,
    ) -> Result<()> {
        let quota = aegis_policy::quota_for(self.policy.manifest(), &class).cloned();
        match self
            .aggregate_state
            .check_and_increment(class, quota.as_ref())
        {
            Ok(_) => Ok(()),
            Err(err) => {
                let reason = format!(
                    "aggregate cap exceeded for {}: {}/{} {}",
                    err.class.label(),
                    err.observed,
                    err.cap,
                    err.bound,
                );
                self.write_aggregate_cap_violation(&err, resource_uri, access_kind)?;
                Err(Error::Denied { reason })
            }
        }
    }

    /// Append a `TurnCapExceeded` Violation to the F9 ledger.
    /// Called by the multi-turn driver ([`Self::run`]) when the
    /// Triple-Bound Circuit Breaker trips (per ADR-025). The
    /// payload shape is namespaced under
    /// `violationKind: "TurnCapExceeded"` so existing v1 ledger
    /// readers can ignore it while ADR-026's schema v2 work can
    /// upgrade it to a first-class entry kind.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn write_turn_cap_violation(
        &mut self,
        bound: crate::turn::TurnCapKind,
        at_turn: u32,
        max_turns: u32,
        tokens_consumed: u64,
        max_tokens: u64,
        wallclock_seconds: f64,
        max_seconds: u64,
    ) -> Result<()> {
        let mut payload = Map::new();
        payload.insert(
            "violationKind".to_string(),
            Value::String("TurnCapExceeded".to_string()),
        );
        payload.insert(
            "violationReason".to_string(),
            Value::String(format!(
                "turn cap exceeded: bound={bound:?}, turn {at_turn}/{max_turns}, \
                 tokens {tokens_consumed}/{max_tokens}, \
                 wallclock {wallclock_seconds:.1}s/{max_seconds}s"
            )),
        );
        payload.insert(
            "capBound".to_string(),
            Value::String(format!("{bound:?}").to_lowercase()),
        );
        payload.insert("atTurn".to_string(), Value::Number(at_turn.into()));
        payload.insert("maxTurns".to_string(), Value::Number(max_turns.into()));
        payload.insert(
            "tokensConsumed".to_string(),
            Value::Number(tokens_consumed.into()),
        );
        payload.insert("maxTokens".to_string(), Value::Number(max_tokens.into()));
        // f64 — Number::from_f64 can fail on NaN/inf which we never produce.
        if let Some(n) = serde_json::Number::from_f64(wallclock_seconds) {
            payload.insert("wallclockSeconds".to_string(), Value::Number(n));
        }
        payload.insert("maxSeconds".to_string(), Value::Number(max_seconds.into()));

        self.ledger.append(Entry {
            session_id: self.session_id.clone(),
            entry_type: EntryType::Violation,
            agent_identity_hash: self.agent_identity_hash,
            timestamp: Utc::now(),
            payload,
        })?;
        Ok(())
    }

    pub fn spiffe_id(&self) -> &SpiffeId {
        &self.spiffe_id
    }

    pub fn agent_identity_hash(&self) -> [u8; 32] {
        self.agent_identity_hash
    }

    pub fn bound_digests(&self) -> &DigestTriple {
        &self.bound_digests
    }

    /// Bound chat-template digest, if the session was booted with a
    /// chat-template sidecar. `None` for sessions booted without one
    /// (legacy callers, non-GGUF models). Per ADR-022 / OCI-B.
    pub fn bound_chat_template(&self) -> Option<&Digest> {
        self.bound_chat_template.as_ref()
    }

    pub fn cert_pem(&self) -> &str {
        &self.svid_cert_pem
    }

    pub fn key_pem(&self) -> &str {
        &self.svid_key_pem
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Mutable access for the F0-B mediator and downstream emitters that
    /// need to append entries.
    pub fn ledger_writer_mut(&mut self) -> &mut LedgerWriter {
        &mut self.ledger
    }

    /// Re-hash the manifest + model + (optional) config files from disk
    /// and return the live digest triple. Used by the F0-B mediator's
    /// per-tool-call rebind step. Naive implementation re-reads on
    /// every call; Phase 2 will cache + invalidate via mtime.
    pub(crate) fn compute_live_digests(&self) -> Result<DigestTriple> {
        let model = sha256_file(&self.model_path)?;
        let manifest = sha256_file(&self.manifest_path)?;
        let config = match &self.config_path {
            Some(p) => sha256_file(p)?,
            None => sha256_bytes(&[]),
        };
        Ok(DigestTriple {
            model: Digest(model),
            manifest: Digest(manifest),
            config: Digest(config),
        })
    }
}

fn sha256_file(path: &Path) -> Result<[u8; 32]> {
    let f = File::open(path)?;
    let mut reader = BufReader::new(f);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    Ok(out)
}

fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&hasher.finalize());
    out
}

/// Read a `chat_template.sha256.txt` sidecar file (lowercase 64-char hex)
/// into a [`Digest`]. Returns a typed error if the file is missing,
/// unreadable, or doesn't carry a 64-char hex SHA-256.
fn read_chat_template_sidecar(path: &Path) -> Result<Digest> {
    let raw = std::fs::read_to_string(path).map_err(|e| Error::ChatTemplateSidecar {
        path: path.display().to_string(),
        detail: format!("read failed: {e}"),
    })?;
    let trimmed = raw.trim();
    if trimmed.len() != 64 || !trimmed.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::ChatTemplateSidecar {
            path: path.display().to_string(),
            detail: format!("expected 64-char hex SHA-256, got {trimmed:?}"),
        });
    }
    Digest::from_hex(trimmed).map_err(|e| Error::ChatTemplateSidecar {
        path: path.display().to_string(),
        detail: e.to_string(),
    })
}

fn digest_field_name(f: DigestField) -> &'static str {
    f.name()
}
