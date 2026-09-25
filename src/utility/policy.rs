//! Shadow/apply utility policy (Part VIII-IX, §24-§28) + seeded randomization
//! (Part X, §29-§32). Only OPTIONAL evidence is influenced; mandatory protocol
//! metadata is never touched and FACT/CANDIDATE is never changed (§27).

use serde::Serialize;

use super::assoc::UtilityStats;
use super::model::EvidenceUnit;
use crate::query::ScoredNode;

/// Utility policy mode (§24): off | shadow | apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UtilityPolicy {
    Off,
    Shadow,
    Apply,
}
impl UtilityPolicy {
    pub fn parse(s: &str) -> Self {
        match s {
            "shadow" => Self::Shadow,
            "apply" => Self::Apply,
            _ => Self::Off,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Shadow => "shadow",
            Self::Apply => "apply",
        }
    }
}

/// Minimum margin a unit's (lower) followup-rate must beat the baseline mean by
/// before apply reorders/admits it (§28 — avoids noisy low-support churn).
pub const MIN_MARGIN: f64 = 0.05;

/// Inspectable utility-policy trace (§39 — debug surface, not agent-facing).
#[derive(Debug, Clone, Default, Serialize)]
pub struct UtilityTrace {
    pub policy: String,
    pub decision_id: String,
    /// Optional units the baseline emitted vs. what the policy would emit.
    pub baseline_optional: Vec<String>,
    pub policy_optional: Vec<String>,
    /// True when the policy ordering/admission differs from baseline.
    pub diverged: bool,
    pub fallback_reason: String,
    /// Selection propensity when randomized (1/eligible) — None otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub propensity: Option<f64>,
    /// The seed used for randomized selection (reproducible, §32).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub random_seed: Option<u64>,
}

/// Compute the utility-guided optional seed ordering. Returns the trace; in
/// `apply` mode also reorders `proj.seeds`' *optional* tail (never the
/// mandatory top seed's identity/protocol fields). In `shadow` the packet is
/// untouched (§25).
pub fn decide(
    seeds: &mut Vec<ScoredNode>,
    shape: &str,
    scope: &str,
    stats: &UtilityStats,
    policy: UtilityPolicy,
    decision_id: &str,
) -> UtilityTrace {
    decide_seeded(seeds, shape, scope, stats, policy, decision_id, None)
}

/// Seeded variant (Part X): when `random_seed` is Some, the optional seed tail
/// is selected by a deterministic seeded shuffle rather than the utility rate,
/// and the propensity (1/eligible) is logged for off-policy analysis (§31-§32).
/// Evaluation only — production default stays deterministic (§32).
pub fn decide_seeded(
    seeds: &mut Vec<ScoredNode>,
    shape: &str,
    scope: &str,
    stats: &UtilityStats,
    policy: UtilityPolicy,
    decision_id: &str,
    random_seed: Option<u64>,
) -> UtilityTrace {
    let mut tr = UtilityTrace {
        policy: policy.as_str().into(),
        decision_id: decision_id.into(),
        ..Default::default()
    };
    if policy == UtilityPolicy::Off {
        tr.fallback_reason = "policy_off".into();
        return tr;
    }
    // Rank optional seeds by shrunken followup-rate (lower followup = evidence
    // sufficed). The top (rank-1) seed is mandatory context — never reordered.
    let scores: Vec<(usize, f64)> = seeds
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, s)| {
            let u = EvidenceUnit {
                scope: scope.into(),
                entity: s.node.key.clone(),
                repr: super::model::ReprKind::DeclarationRange,
                role: super::model::EvidenceRole::Definition,
                truth_class: if matches!(s.node.kind, crate::graph::model::NodeKind::Declaration) {
                    "fact".into()
                } else {
                    "candidate".into()
                },
            };
            (i, stats.followup_rate(scope, shape, &u))
        })
        .collect();
    // Policy order: stable sort optional seeds by followup-rate ascending, but
    // only reorder when the best beats the baseline by >= MIN_MARGIN (§28).
    let mut order: Vec<usize> = scores.iter().map(|(i, _)| *i).collect();
    order.sort_by(|a, b| {
        scores[*a - 1]
            .1
            .partial_cmp(&scores[*b - 1].1)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    tr.baseline_optional = scores
        .iter()
        .map(|(i, _)| seeds[*i].node.key.clone())
        .collect();
    tr.policy_optional = order.iter().map(|&i| seeds[i].node.key.clone()).collect();
    tr.diverged = order != scores.iter().map(|(i, _)| *i).collect::<Vec<usize>>();
    if let Some(seed) = random_seed {
        // Seeded shuffle of the optional tail (Fisher-Yates, deterministic).
        let mut idx: Vec<usize> = order.clone();
        let mut st = seed ^ 0x9e3779b97f4a7c15;
        for i in (1..idx.len()).rev() {
            st ^= st << 13;
            st ^= st >> 7;
            st ^= st << 17; // xorshift64
            let j = (st as usize) % (i + 1);
            idx.swap(i, j);
        }
        order = idx;
        tr.random_seed = Some(seed);
        tr.propensity = if seeds.len() > 1 {
            Some(1.0 / (seeds.len() - 1) as f64)
        } else {
            Some(1.0)
        };
    }
    if policy == UtilityPolicy::Apply && tr.diverged {
        // Apply margin gate: only reorder when the top-of-policy beats the
        // baseline top by >= MIN_MARGIN and has support.
        let best = order.first().map(|&i| scores[i - 1].1);
        let base = scores.first().map(|s| s.1);
        if let (Some(b), Some(base0)) = (best, base) {
            if base0 - b >= MIN_MARGIN {
                // Reorder optional tail only (index 1..); keep mandatory head.
                let head = seeds[0].clone();
                let mut tail: Vec<_> = order.iter().map(|&i| seeds[i].clone()).collect();
                let mut new_seeds = vec![head];
                new_seeds.append(&mut tail);
                *seeds = new_seeds;
                tr.fallback_reason = String::new();
            } else {
                tr.fallback_reason = "insufficient_margin".into();
            }
        }
    }
    tr
}
