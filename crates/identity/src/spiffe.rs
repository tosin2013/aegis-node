//! SPIFFE ID parsing and emission for Aegis-Node workload identity.
//!
//! Per ADR-003 (F1) the canonical Aegis SPIFFE ID is:
//!
//! ```text
//! spiffe://<trust-domain>/agent/<workload-name>/<instance>
//! ```
//!
//! This module enforces that exact shape — the strict format means a verifier
//! never has to guess whether an identity belongs to an Aegis-Node agent.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const SCHEME: &str = "spiffe://";
const AGENT_SEGMENT: &str = "agent";

/// SPIFFE workload identity. Always Aegis-shaped:
/// `spiffe://<trust-domain>/agent/<workload>/<instance>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpiffeId {
    trust_domain: String,
    workload_name: String,
    instance: String,
}

impl SpiffeId {
    /// Build a SPIFFE ID from components. Validates trust domain and segments.
    pub fn new(trust_domain: &str, workload_name: &str, instance: &str) -> Result<Self> {
        validate_trust_domain(trust_domain)?;
        validate_segment(workload_name, "workload-name")?;
        validate_segment(instance, "instance")?;
        Ok(Self {
            trust_domain: trust_domain.to_string(),
            workload_name: workload_name.to_string(),
            instance: instance.to_string(),
        })
    }

    /// Parse a SPIFFE URI string. Strict: only the Aegis-shaped form is
    /// accepted. Reject anything else so audit code never has to special-case.
    pub fn parse(input: &str) -> Result<Self> {
        let rest = input.strip_prefix(SCHEME).ok_or(Error::InvalidSpiffeId {
            input: input.to_string(),
            reason: "missing spiffe:// scheme",
        })?;

        let mut parts = rest.splitn(4, '/');
        let trust_domain = parts.next().ok_or(Error::InvalidSpiffeId {
            input: input.to_string(),
            reason: "missing trust domain",
        })?;
        let agent_seg = parts.next().ok_or(Error::InvalidSpiffeId {
            input: input.to_string(),
            reason: "missing path",
        })?;
        let workload = parts.next().ok_or(Error::InvalidSpiffeId {
            input: input.to_string(),
            reason: "missing workload-name segment",
        })?;
        let instance = parts.next().ok_or(Error::InvalidSpiffeId {
            input: input.to_string(),
            reason: "missing instance segment",
        })?;

        if agent_seg != AGENT_SEGMENT {
            return Err(Error::InvalidSpiffeId {
                input: input.to_string(),
                reason: "first path segment must be 'agent'",
            });
        }
        if instance.contains('/') {
            return Err(Error::InvalidSpiffeId {
                input: input.to_string(),
                reason: "trailing path segments not allowed",
            });
        }

        Self::new(trust_domain, workload, instance)
    }

    pub fn trust_domain(&self) -> &str {
        &self.trust_domain
    }

    pub fn workload_name(&self) -> &str {
        &self.workload_name
    }

    pub fn instance(&self) -> &str {
        &self.instance
    }

    /// Render the canonical URI form.
    pub fn uri(&self) -> String {
        format!(
            "{}{}/{}/{}/{}",
            SCHEME, self.trust_domain, AGENT_SEGMENT, self.workload_name, self.instance
        )
    }
}

impl fmt::Display for SpiffeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.uri())
    }
}

fn validate_trust_domain(td: &str) -> Result<()> {
    if td.is_empty() || td.len() > 255 {
        return Err(Error::InvalidTrustDomain(td.to_string()));
    }
    let ok = td
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '_'));
    if !ok {
        return Err(Error::InvalidTrustDomain(td.to_string()));
    }
    Ok(())
}

fn validate_segment(seg: &str, _label: &'static str) -> Result<()> {
    if seg.is_empty() || seg.len() > 63 {
        return Err(Error::InvalidSpiffeId {
            input: seg.to_string(),
            reason: "path segment must be 1..=63 chars",
        });
    }
    let ok = seg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '~'));
    if !ok {
        return Err(Error::InvalidSpiffeId {
            input: seg.to_string(),
            reason: "path segment has disallowed character",
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        let s = "spiffe://aegis-node.local/agent/research/inst-001";
        let id = SpiffeId::parse(s).unwrap();
        assert_eq!(id.trust_domain(), "aegis-node.local");
        assert_eq!(id.workload_name(), "research");
        assert_eq!(id.instance(), "inst-001");
        assert_eq!(id.uri(), s);
    }

    #[test]
    fn parse_rejects_non_agent_path() {
        let err = SpiffeId::parse("spiffe://td/svc/x/y").unwrap_err();
        assert!(matches!(err, Error::InvalidSpiffeId { .. }));
    }

    #[test]
    fn parse_rejects_extra_segments() {
        let err = SpiffeId::parse("spiffe://td/agent/wl/inst/extra").unwrap_err();
        assert!(matches!(err, Error::InvalidSpiffeId { .. }));
    }

    #[test]
    fn parse_rejects_missing_scheme() {
        let err = SpiffeId::parse("td/agent/wl/inst").unwrap_err();
        assert!(matches!(err, Error::InvalidSpiffeId { .. }));
    }

    #[test]
    fn rejects_uppercase_trust_domain() {
        let err = SpiffeId::new("Aegis.Local", "wl", "i").unwrap_err();
        assert!(matches!(err, Error::InvalidTrustDomain(_)));
    }

    #[test]
    fn rejects_empty_segment() {
        let err = SpiffeId::new("td", "", "i").unwrap_err();
        assert!(matches!(err, Error::InvalidSpiffeId { .. }));
    }
}
