//! `RepoQueryObservation` — the structured, machine-facing counterpart of one
//! query execution (PRODUCTION-GAP-TELEMETRY-V1 §12-§13).
//!
//! One source of truth: the observation is derived from the same
//! [`EvidenceProjection`] the Agent-facing RDX is rendered from — it is never
//! parsed back out of the rendered packet text.
//!
//! The observation is **not** Agent-facing. It is compact JSON telemetry
//! metadata a producer (Gateway today; Relay integration later) can attach to
//! a canonical `context_artifact_presented` AgentEvent without Relay ever
//! parsing RDX.

use serde::Serialize;

use super::adaptive::{AdaptiveRendered, NavIntent};
use super::projection::EvidenceProjection;
use crate::agent_event::model::GapSignature;

/// Observation schema identifier — machine telemetry, not Agent-facing.
pub const QUERY_OBSERVATION_SCHEMA: &str = "reposuite.repodex.query-observation.v1";

/// Structured observation of one RepoDex query execution.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RepoQueryObservation {
    pub schema: String,
    /// sha256 digest of the normalized query text — the query text itself is
    /// NOT stored by default (§11).
    pub query_digest: String,
    /// Classified navigation intent, e.g. `callers`, `definition`.
    pub intent: String,
    /// Navigation profile actually rendered (`adaptive` / `full`).
    pub nav_profile: String,
    /// Repository identity when the caller supplied it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_head: Option<String>,
    // --- evidence bookkeeping ---
    pub emitted_seed_count: u64,
    pub eligible_seed_count: u64,
    pub seed_selection_complete: bool,
    pub relationship_limit_reached: bool,
    pub seed_count: u64,
    pub relation_count: u64,
    /// Nodes carrying a candidate/evidence disposition in the packet.
    pub candidate_count: u64,
    /// Disposition histogram (e.g. `single_candidate`: 3) — bounded by the
    /// closed disposition vocabulary.
    pub dispositions: std::collections::BTreeMap<String, u64>,
    /// Normalized gap signatures, machine-stable and bounded.
    pub gap_signatures: Vec<GapSignature>,
    /// The Agent-facing packet — digest and size only, never the content.
    pub packet_bytes: u64,
    pub packet_digest: String,
    /// Query execution wall time inside RepoDex, when the caller measured it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

impl RepoQueryObservation {
    /// The allowlisted metadata object a producer should attach to a
    /// `context_artifact_presented` AgentEvent (§9). Compact by construction.
    pub fn event_metadata(&self) -> serde_json::Value {
        serde_json::json!({
            "query_intent": self.intent,
            "navigation_profile": self.nav_profile,
            "evidence_complete": self.seed_selection_complete
                && !self.relationship_limit_reached
                && !self.gap_signatures.iter().any(|g| g.family == "evidence_truncated"),
            "gap_signatures": self.gap_signatures,
            "seed_count": self.seed_count,
            "candidate_count": self.candidate_count,
            "fact_count": self.seed_count + self.relation_count,
            "relation_count": self.relation_count,
            "packet_bytes": self.packet_bytes,
            "query_digest": self.query_digest,
        })
    }
}

/// Stable reason-code head: the identifier-ish prefix of a reason string,
/// bounded by the producer's own emitted vocabulary. `receiver_type_unavailable`
/// stays itself; `no_direct_method_in_lexical_class(...)` normalizes to its
/// head. Unknown/unrecognized codes pass through unchanged — never collapsed.
fn reason_head(reason: &str) -> String {
    reason
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()
        .unwrap_or(reason)
        .to_string()
}

/// Map a candidate-level `out_of_scope` / `no_candidate` disposition into a
/// gap signature. PHP-specific codes stay inside `reason_code` as opaque
/// stable values — the core model is language-neutral (§18).
///
/// `kind`/`reason` come from the structured graph-schema-3 fields. The
/// legacy compact `disposition` string is used ONLY when the structured
/// fields are absent (pre-schema-3 graphs): splitting at the first `:` is
/// recovery of our own normalized producer vocabulary, NOT RDX parsing.
fn disposition_gap(kind: &str, reason: Option<&str>) -> GapSignature {
    let family = match kind {
        "no_candidate" | "no_candidate_rule" => "no_candidate",
        "out_of_scope" => "out_of_scope",
        _ => "unsupported_evidence_class",
    };
    GapSignature {
        family: family.to_string(),
        reason_code: reason.map(reason_head),
    }
}

/// The structured (kind, reason) pair of a disposition — schema-3 fields
/// first, legacy `kind:reason` compact string only as the fallback.
fn disposition_parts(
    disposition: Option<&str>,
    kind: Option<&str>,
    reason: Option<&str>,
) -> Option<(String, Option<String>)> {
    if let Some(k) = kind {
        return Some((k.to_string(), reason.map(String::from)));
    }
    let d = disposition?;
    let (k, r) = match d.split_once(':') {
        Some((k, r)) => (k.to_string(), Some(r.to_string())),
        None => (d.to_string(), None),
    };
    Some((k, r))
}

/// Derive gap signatures from the projection's structured state (§16):
/// packet-level gaps first, then per-node candidate gaps. Never from text.
fn gap_signatures(
    proj: &EvidenceProjection,
    intent: NavIntent,
    rendered: Option<&AdaptiveRendered>,
) -> Vec<GapSignature> {
    let mut out: Vec<GapSignature> = Vec::new();
    if proj.emitted_seed_count == 0 && proj.eligible_seed_count == 0 {
        out.push(GapSignature {
            family: "no_result".to_string(),
            reason_code: None,
        });
    }
    if intent == NavIntent::Ambiguous {
        out.push(GapSignature {
            family: "ambiguous_result".to_string(),
            reason_code: Some("no_confident_intent".to_string()),
        });
    }
    if proj.emitted_seed_count < proj.eligible_seed_count {
        out.push(GapSignature {
            family: "result_limit".to_string(),
            reason_code: Some("seed_bound".to_string()),
        });
    }
    if proj.relationship_limit_reached {
        out.push(GapSignature {
            family: "result_limit".to_string(),
            reason_code: Some("relationship_limit".to_string()),
        });
    }
    if let Some(r) = &proj.truncated_reason {
        out.push(GapSignature {
            family: "evidence_truncated".to_string(),
            reason_code: Some(reason_head(r)),
        });
    }
    if let Some(re) = rendered {
        if re.relevant_total > re.emitted_related || re.budget_dropped > 0 {
            out.push(GapSignature {
                family: "evidence_truncated".to_string(),
                reason_code: Some("truncated_continuation".to_string()),
            });
        }
    }
    if let Some(c) = &proj.connector {
        if !c.found {
            out.push(GapSignature {
                family: "bounded_no_route".to_string(),
                reason_code: Some("no_route".to_string()),
            });
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut push = |kind: &str, reason: Option<&str>| {
        let g = disposition_gap(kind, reason);
        if seen.insert((g.family.clone(), g.reason_code.clone())) {
            out.push(g);
        }
    };
    let candidate_family =
        |kind: &str| matches!(kind, "no_candidate" | "no_candidate_rule" | "out_of_scope");
    for s in &proj.seeds {
        if let Some((k, r)) = disposition_parts(
            s.disposition.as_deref(),
            s.disposition_kind.as_deref(),
            s.disposition_reason.as_deref(),
        ) {
            if candidate_family(&k) {
                push(&k, r.as_deref());
            }
        }
    }
    for r in &proj.related {
        if let Some((k, rsn)) = disposition_parts(
            r.disposition.as_deref(),
            r.disposition_kind.as_deref(),
            r.disposition_reason.as_deref(),
        ) {
            if candidate_family(&k) {
                push(&k, rsn.as_deref());
            }
        }
    }
    out
}

/// Build the observation from the SAME projection that produced `packet`.
/// `query_ms` is the measured query wall when the caller timed it.
pub fn observe(
    proj: &EvidenceProjection,
    intent: NavIntent,
    nav_profile: &str,
    packet: &str,
    rendered: Option<&AdaptiveRendered>,
    query_ms: Option<u64>,
) -> RepoQueryObservation {
    let mut dispositions: std::collections::BTreeMap<String, u64> =
        std::collections::BTreeMap::new();
    let mut candidate_count = 0u64;
    for s in &proj.seeds {
        if let Some((k, _)) = disposition_parts(
            s.disposition.as_deref(),
            s.disposition_kind.as_deref(),
            s.disposition_reason.as_deref(),
        ) {
            *dispositions.entry(k).or_default() += 1;
            candidate_count += 1;
        }
    }
    for r in &proj.related {
        if let Some((k, _)) = disposition_parts(
            r.disposition.as_deref(),
            r.disposition_kind.as_deref(),
            r.disposition_reason.as_deref(),
        ) {
            *dispositions.entry(k).or_default() += 1;
            candidate_count += 1;
        }
    }
    RepoQueryObservation {
        schema: QUERY_OBSERVATION_SCHEMA.to_string(),
        query_digest: crate::repository::digest::sha256_text(&proj.query),
        intent: format!("{:?}", intent).to_lowercase(),
        nav_profile: nav_profile.to_string(),
        repository_id: None,
        repo_head: None,
        emitted_seed_count: proj.emitted_seed_count as u64,
        eligible_seed_count: proj.eligible_seed_count as u64,
        seed_selection_complete: proj.seed_selection_complete,
        relationship_limit_reached: proj.relationship_limit_reached,
        seed_count: proj.seeds.len() as u64,
        relation_count: proj.related.len() as u64,
        candidate_count,
        dispositions,
        gap_signatures: gap_signatures(proj, intent, rendered),
        packet_bytes: packet.len() as u64,
        packet_digest: crate::repository::digest::sha256_text(packet),
        duration_ms: query_ms,
    }
}
