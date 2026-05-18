//! `aegis evidence cmmc` — turn an F9 Trajectory Ledger into a
//! CMMC 2.0 / NIST SP 800-171 evidence pack.
//!
//! Per ADR-011 (F9) the ledger is the canonical, hash-chained,
//! tamper-evident record of every runtime decision. Per the
//! [Compliance Traceability Matrix](../../../docs/COMPLIANCE_MATRIX.md)
//! every architectural feature (F1–F10) maps to one or more NIST
//! SP 800-171 controls. This tool walks the ledger, applies the
//! embedded mapping, and emits two artifacts a C3PAO can ingest:
//!
//! - `evidence-pack.json` — schema-validated machine-readable bundle.
//!   Schema at `schemas/evidence/v1/evidence-pack.schema.json`.
//! - `evidence-pack.md` — operator-facing summary.
//!
//! ## Scope (foundation PR)
//!
//! - **NIST SP 800-171 Rev. 3** controls only (AC, AU, CM, IA, SC, SI).
//! - **v2 ledgers** (the schema introduced by ADR-026). v1 ledgers
//!   produced by pre-multi-turn sessions are a compat shim and a
//!   deferred follow-up.
//! - **Single ledger** input. Multi-ledger aggregation is deferred.
//! - **Unsigned output.** Cosign `--sign` integration is deferred —
//!   the produced JSON is already cryptographically anchored via the
//!   ledger root hash it embeds.
//!
//! See [issue #187](https://github.com/tosin2013/aegis-node/issues/187)
//! and `docs/COMPLIANCE_MATRIX.md` §"Evidence artifact generation".

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use aegis_ledger_writer::{verify_file, LedgerSchemaVersion};
use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `aegis evidence cmmc` argument shape.
#[derive(Debug, Args)]
pub struct CmmcArgs {
    /// Path to the JSONL trajectory ledger to ingest.
    #[arg(long)]
    pub ledger: PathBuf,
    /// Directory the evidence pack is written to. Must exist; both
    /// `evidence-pack.json` and `evidence-pack.md` are placed there.
    #[arg(long)]
    pub out: PathBuf,
    /// Drop entries whose `timestamp` is strictly before this UTC
    /// instant. Optional — when omitted, the pack covers from the
    /// first entry on disk.
    #[arg(long)]
    pub since: Option<DateTime<Utc>>,
    /// Drop entries whose `timestamp` is strictly after this UTC
    /// instant. Optional.
    #[arg(long)]
    pub until: Option<DateTime<Utc>>,
}

/// Schema version stamped on every generated pack. Bumped when the
/// JSON shape changes incompatibly.
pub const PACK_SCHEMA_VERSION: &str = "1";

/// One NIST SP 800-171 control as cited in the matrix. Internal
/// (static) form — see [`SerializedControl`] for the JSON-facing
/// version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Control {
    pub id: &'static str,
    pub family: &'static str,
    pub title: &'static str,
}

/// JSON-facing copy of a [`Control`] — owned strings so the
/// `EvidencePack` can derive `Serialize` + `Deserialize` cleanly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SerializedControl {
    pub id: String,
    pub family: String,
    pub title: String,
}

impl From<Control> for SerializedControl {
    fn from(c: Control) -> Self {
        Self {
            id: c.id.to_string(),
            family: c.family.to_string(),
            title: c.title.to_string(),
        }
    }
}

/// (entryType, optional violationKind) → list of control IDs the
/// event provides evidence for. Sourced from
/// `docs/COMPLIANCE_MATRIX.md` §"NIST SP 800-171 control mapping" —
/// keep the two in sync; the CI lint that enforces parity is a
/// deferred follow-up.
const CONTROL_CATALOG: &[Control] = &[
    Control {
        id: "3.1.1",
        family: "AC",
        title: "Limit system access to authorized users",
    },
    Control {
        id: "3.1.2",
        family: "AC",
        title: "Limit system access to authorized transactions",
    },
    Control {
        id: "3.1.3",
        family: "AC",
        title: "Control flow of CUI per approved authorizations",
    },
    Control {
        id: "3.1.7",
        family: "AC",
        title: "Prevent non-privileged users from executing privileged functions",
    },
    Control {
        id: "3.1.8",
        family: "AC",
        title: "Limit unsuccessful logon attempts (aggregate quota)",
    },
    Control {
        id: "3.1.20",
        family: "AC",
        title: "Verify and control connections to external systems",
    },
    Control {
        id: "3.3.1",
        family: "AU",
        title: "Create and retain audit logs",
    },
    Control {
        id: "3.3.2",
        family: "AU",
        title: "Ensure individual users uniquely identifiable in audit",
    },
    Control {
        id: "3.3.4",
        family: "AU",
        title: "Alert in event of audit logging process failure",
    },
    Control {
        id: "3.3.5",
        family: "AU",
        title: "Correlate audit records for investigation",
    },
    Control {
        id: "3.3.8",
        family: "AU",
        title: "Protect audit info from unauthorized access",
    },
    Control {
        id: "3.4.1",
        family: "CM",
        title: "Establish baseline configurations",
    },
    Control {
        id: "3.4.3",
        family: "CM",
        title: "Track changes to organizational systems",
    },
    Control {
        id: "3.5.4",
        family: "IA",
        title: "Employ replay-resistant authentication",
    },
    Control {
        id: "3.5.5",
        family: "IA",
        title: "Prevent reuse of identifiers for a defined period",
    },
    Control {
        id: "3.13.1",
        family: "SC",
        title: "Monitor and control communications at external boundaries",
    },
    Control {
        id: "3.13.4",
        family: "SC",
        title: "Prevent unauthorized information transfer via shared resources",
    },
    Control {
        id: "3.13.6",
        family: "SC",
        title: "Deny network communications by default + permit by exception",
    },
    Control {
        id: "3.14.1",
        family: "SI",
        title: "Identify, report, and correct information system flaws",
    },
    Control {
        id: "3.14.6",
        family: "SI",
        title: "Monitor system to detect attacks and indicators of potential attacks",
    },
    Control {
        id: "3.14.7",
        family: "SI",
        title: "Identify unauthorized use of organizational systems",
    },
];

/// Map an entry to the controls it provides evidence for.
///
/// For most entry kinds the mapping is purely by `entryType`. For
/// `violation`, the dispatch additionally inspects `violationKind`
/// (the v1-schema-namespaced field used since #193) so an
/// `AdversarialContent` violation tags 3.14.6 (attack detection) while
/// a `TurnCapExceeded` violation tags 3.13.4 (resource-exhaustion
/// prevention).
fn controls_for(entry: &Value) -> Vec<&'static str> {
    let entry_type = entry["entryType"].as_str().unwrap_or("");
    match entry_type {
        "session_start" => vec!["3.4.1", "3.4.3", "3.3.2"],
        "session_end" => vec!["3.3.1"],
        "reasoning_step" => vec!["3.3.1", "3.3.5"],
        "access" => vec!["3.1.1", "3.1.2", "3.3.1"],
        "violation" => {
            let kind = entry["violationKind"].as_str().unwrap_or("");
            match kind {
                "AdversarialContent" => vec!["3.14.1", "3.14.6"],
                "TurnCapExceeded" => vec!["3.13.4", "3.14.1"],
                "AggregateCapExceeded" => vec!["3.1.8", "3.13.4", "3.14.1"],
                _ => vec!["3.14.1"],
            }
        }
        "approval_request" | "approval_granted" | "approval_rejected" | "approval_timed_out" => {
            vec!["3.1.7", "3.3.1"]
        }
        "approval_decision" => vec!["3.1.7", "3.3.5"],
        "network_attestation" => vec!["3.1.3", "3.13.1", "3.13.6"],
        "turn_start" => vec!["3.3.1", "3.3.5", "3.5.4", "3.5.5"],
        "turn_end" => vec!["3.3.1", "3.13.4"],
        "tool_call" | "tool_result" => vec!["3.3.1", "3.3.5"],
        _ => Vec::new(),
    }
}

/// One control's evidence summary for the pack JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlEvidence {
    pub control: SerializedControl,
    /// Total entries (across the ledger window) that tagged this control.
    pub evidence_count: u64,
    /// Sequence numbers of the first and last contributing entries.
    /// `aegis verify` cites these so an auditor can spot-check.
    pub first_sequence_number: u64,
    pub last_sequence_number: u64,
    /// EntryIds of up to the first 10 contributing entries. Operators
    /// asking "show me a sample" get a copy-paste-ready list; the full
    /// chain remains the canonical record.
    pub sample_entry_ids: Vec<String>,
}

/// Top-level shape of `evidence-pack.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidencePack {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    /// Session ID extracted from the ledger (uniform across all entries).
    pub session_id: Option<String>,
    /// Hex-encoded F9 ledger root hash. The cryptographic anchor —
    /// `aegis verify <ledger>` re-derives this from disk.
    #[serde(rename = "ledgerRootHex")]
    pub ledger_root_hex: String,
    /// Detected schema version of the input ledger (v1 or v2).
    #[serde(rename = "ledgerSchemaVersion")]
    pub ledger_schema_version: String,
    /// Window applied to the walk, if any.
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    /// Wallclock when the pack was generated.
    #[serde(rename = "generatedAt")]
    pub generated_at: DateTime<Utc>,
    /// Total entries the walk inspected (after date filtering).
    pub entry_count: u64,
    /// Per-control evidence. Keyed by control ID; sorted lex for
    /// byte-stable JSON output (so a regenerate without ledger changes
    /// is a stable artifact).
    pub controls: BTreeMap<String, ControlEvidence>,
    /// Total controls covered by the ledger window (length of the
    /// `controls` map). Exposed as its own field so a glance at the
    /// JSON without iterating the map suffices.
    pub controls_covered: u64,
}

/// Execute the `aegis evidence cmmc` command end-to-end.
pub fn execute(args: CmmcArgs) -> Result<()> {
    let pack = build_pack(&args)?;
    write_outputs(&pack, &args.out)?;
    println!(
        "wrote evidence pack: {} (entries={}, controls_covered={})",
        args.out.display(),
        pack.entry_count,
        pack.controls_covered,
    );
    Ok(())
}

fn build_pack(args: &CmmcArgs) -> Result<EvidencePack> {
    if !args.ledger.exists() {
        return Err(anyhow!("ledger file not found: {}", args.ledger.display()));
    }
    // Verify chain integrity first — emitting an evidence pack from a
    // broken ledger would be worse than no pack at all. The summary
    // also gives us session_id, root hash, and schema version.
    let summary = verify_file(&args.ledger)
        .with_context(|| format!("verifying ledger at {}", args.ledger.display()))?;
    let ledger_schema_version = match summary.schema_version {
        Some(LedgerSchemaVersion::V1) => "v1".to_string(),
        Some(LedgerSchemaVersion::V2) => "v2".to_string(),
        None => "unknown".to_string(),
    };

    let raw = fs::read_to_string(&args.ledger)
        .with_context(|| format!("reading ledger at {}", args.ledger.display()))?;

    let mut entry_count: u64 = 0;
    let mut accum: BTreeMap<&'static str, ControlAccum> = BTreeMap::new();

    for line in raw.lines().filter(|l| !l.is_empty()) {
        let entry: Value = serde_json::from_str(line)
            .with_context(|| format!("parsing ledger line {entry_count}"))?;
        let ts = entry["timestamp"]
            .as_str()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok());
        if let (Some(t), Some(since)) = (ts, args.since) {
            if t < since {
                continue;
            }
        }
        if let (Some(t), Some(until)) = (ts, args.until) {
            if t > until {
                continue;
            }
        }
        entry_count += 1;

        let sequence_number = entry["sequenceNumber"].as_u64().unwrap_or(0);
        let entry_id = entry["entryId"].as_str().unwrap_or("").to_string();

        for cid in controls_for(&entry) {
            let a = accum.entry(cid).or_insert_with(|| ControlAccum {
                evidence_count: 0,
                first_sequence_number: sequence_number,
                last_sequence_number: sequence_number,
                sample_entry_ids: Vec::new(),
            });
            a.evidence_count += 1;
            a.last_sequence_number = sequence_number;
            if a.sample_entry_ids.len() < 10 && !entry_id.is_empty() {
                a.sample_entry_ids.push(entry_id.clone());
            }
        }
    }

    let mut controls: BTreeMap<String, ControlEvidence> = BTreeMap::new();
    for (cid, a) in accum.into_iter() {
        let control = CONTROL_CATALOG
            .iter()
            .find(|c| c.id == cid)
            .copied()
            .ok_or_else(|| anyhow!("internal: control id {cid} not in catalog"))?;
        controls.insert(
            cid.to_string(),
            ControlEvidence {
                control: control.into(),
                evidence_count: a.evidence_count,
                first_sequence_number: a.first_sequence_number,
                last_sequence_number: a.last_sequence_number,
                sample_entry_ids: a.sample_entry_ids,
            },
        );
    }
    let controls_covered = controls.len() as u64;

    Ok(EvidencePack {
        schema_version: PACK_SCHEMA_VERSION.to_string(),
        session_id: summary.session_id,
        ledger_root_hex: summary.root_hash_hex,
        ledger_schema_version,
        since: args.since,
        until: args.until,
        generated_at: Utc::now(),
        entry_count,
        controls,
        controls_covered,
    })
}

/// Per-control accumulator used during the ledger walk.
struct ControlAccum {
    evidence_count: u64,
    first_sequence_number: u64,
    last_sequence_number: u64,
    sample_entry_ids: Vec<String>,
}

fn write_outputs(pack: &EvidencePack, out_dir: &Path) -> Result<()> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("creating output dir {}", out_dir.display()))?;

    let json_path = out_dir.join("evidence-pack.json");
    let md_path = out_dir.join("evidence-pack.md");

    let json = serde_json::to_string_pretty(pack)?;
    let mut f = fs::File::create(&json_path)
        .with_context(|| format!("creating {}", json_path.display()))?;
    f.write_all(json.as_bytes())?;
    f.write_all(b"\n")?;
    f.sync_all()?;

    let md = render_markdown(pack);
    let mut f =
        fs::File::create(&md_path).with_context(|| format!("creating {}", md_path.display()))?;
    f.write_all(md.as_bytes())?;
    f.sync_all()?;

    Ok(())
}

/// Human-readable summary. Mirrors the JSON's per-control fields but
/// renders as a Markdown table grouped by control family so an
/// auditor scanning a single page sees coverage at a glance.
pub fn render_markdown(pack: &EvidencePack) -> String {
    let mut s = String::new();
    s.push_str("# Aegis-Node CMMC 2.0 / NIST SP 800-171 Evidence Pack\n\n");
    s.push_str(&format!(
        "- **Session**: `{}`\n",
        pack.session_id.as_deref().unwrap_or("(none)")
    ));
    s.push_str(&format!("- **Ledger root**: `{}`\n", pack.ledger_root_hex));
    s.push_str(&format!(
        "- **Ledger schema**: {}\n",
        pack.ledger_schema_version
    ));
    if let Some(s2) = pack.since {
        s.push_str(&format!("- **Since**: {s2}\n"));
    }
    if let Some(u) = pack.until {
        s.push_str(&format!("- **Until**: {u}\n"));
    }
    s.push_str(&format!(
        "- **Generated at**: {}\n",
        pack.generated_at.to_rfc3339()
    ));
    s.push_str(&format!("- **Entries inspected**: {}\n", pack.entry_count));
    s.push_str(&format!(
        "- **Controls covered**: {} of {}\n\n",
        pack.controls_covered,
        CONTROL_CATALOG.len()
    ));

    // Group by family for readability.
    let mut by_family: BTreeMap<&str, Vec<&ControlEvidence>> = BTreeMap::new();
    for ev in pack.controls.values() {
        by_family
            .entry(ev.control.family.as_str())
            .or_default()
            .push(ev);
    }

    for (family, evs) in &by_family {
        s.push_str(&format!("## {family}\n\n"));
        s.push_str("| Control | Title | Evidence count | First seq | Last seq |\n");
        s.push_str("|---|---|---|---|---|\n");
        for ev in evs {
            s.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                ev.control.id,
                ev.control.title,
                ev.evidence_count,
                ev.first_sequence_number,
                ev.last_sequence_number,
            ));
        }
        s.push('\n');
    }

    s.push_str("---\n");
    s.push_str(&format!(
        "Generated by `aegis evidence cmmc` (pack schema {})\n",
        PACK_SCHEMA_VERSION
    ));
    s
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn entry_type_to_controls_baseline() {
        let entry = serde_json::json!({"entryType": "session_start"});
        let controls = controls_for(&entry);
        assert!(controls.contains(&"3.4.1"));
    }

    #[test]
    fn violation_dispatches_on_kind() {
        let adversarial = serde_json::json!({
            "entryType": "violation",
            "violationKind": "AdversarialContent",
        });
        assert!(controls_for(&adversarial).contains(&"3.14.6"));

        let turn_cap = serde_json::json!({
            "entryType": "violation",
            "violationKind": "TurnCapExceeded",
        });
        assert!(controls_for(&turn_cap).contains(&"3.13.4"));

        let aggregate = serde_json::json!({
            "entryType": "violation",
            "violationKind": "AggregateCapExceeded",
        });
        let cs = controls_for(&aggregate);
        assert!(cs.contains(&"3.1.8") && cs.contains(&"3.13.4"));
    }

    #[test]
    fn unknown_entry_type_maps_to_empty() {
        let entry = serde_json::json!({"entryType": "unknown_kind"});
        assert!(controls_for(&entry).is_empty());
    }

    #[test]
    fn control_catalog_contains_every_id_used_in_mapping() {
        // The dispatch in controls_for() should only return IDs that
        // appear in CONTROL_CATALOG. Walk every reachable entry type
        // and confirm.
        let kinds = [
            "session_start",
            "session_end",
            "reasoning_step",
            "access",
            "approval_request",
            "approval_granted",
            "approval_rejected",
            "approval_timed_out",
            "approval_decision",
            "network_attestation",
            "turn_start",
            "turn_end",
            "tool_call",
            "tool_result",
        ];
        let violation_kinds = [
            "AdversarialContent",
            "TurnCapExceeded",
            "AggregateCapExceeded",
            "",
        ];
        for k in kinds {
            let entry = serde_json::json!({"entryType": k});
            for cid in controls_for(&entry) {
                assert!(
                    CONTROL_CATALOG.iter().any(|c| c.id == cid),
                    "control {cid} from entryType {k} missing from catalog",
                );
            }
        }
        for vk in violation_kinds {
            let entry = serde_json::json!({
                "entryType": "violation",
                "violationKind": vk,
            });
            for cid in controls_for(&entry) {
                assert!(
                    CONTROL_CATALOG.iter().any(|c| c.id == cid),
                    "control {cid} from violationKind {vk} missing from catalog",
                );
            }
        }
    }
}
