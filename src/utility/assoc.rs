//! Non-neural evidence-utility association model (Part VI-VII, §17-§23).
//!
//! Auditable counts/rates conditioned on (shape, repr, role), with shrinkage
//! toward a global prior for sparse support. Associations are OUTCOME_ASSOCIATED
//! — never causal truth (§1, §21-§23).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::model::{ContinuationMetrics, EvidenceUnit};

/// Minimum observations before a conditional estimate is trusted (§18).
pub const MIN_SUPPORT: u32 = 5;
/// Shrinkage prior strength — pull sparse cells toward the global rate (§18).
pub const SHRINKAGE: f64 = 3.0;

/// Per-cell association counters (§19 — separate dimensions, not one scalar).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssocCell {
    /// Times this unit was delivered.
    pub delivered: u64,
    /// A search/read happened after delivery (insufficient-evidence signal).
    pub search_followups: u64,
    pub read_followups: u64,
    /// The representation was expanded (e.g. signature -> body read).
    pub expansions: u64,
    /// The unit was referenced in the final answer (outcome-associated).
    pub final_mentions: u64,
    /// Cumulative subsequent tool calls (work proxy).
    pub subsequent_work: u64,
}

/// The conditioning key: scope + shape + repr + role (bounded cardinality).
pub type AssocKey = String;

fn key(scope: &str, shape: &str, u: &EvidenceUnit) -> AssocKey {
    format!("{scope}|{shape}|{:?}|{:?}", u.repr, u.role)
}

/// Project-scoped association store — incremental, replay-equivalent (§44).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct UtilityStats {
    pub cells: BTreeMap<AssocKey, AssocCell>,
    /// Global prior (all cells pooled) for shrinkage.
    pub global: AssocCell,
}

impl UtilityStats {
    pub fn load(dir: &Path) -> Self {
        let f = dir.join("utility.json");
        std::fs::read_to_string(&f)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(".utility.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, dir.join("utility.json"))
    }
    /// Fold one delivered unit + its continuation metrics into the stats.
    /// Ignored/unconsumed evidence is NOT automatically negative (§23) — only
    /// measured followups/mentions are counted; absence of a followup is just
    /// absence of a counted event, not a penalty.
    pub fn observe(&mut self, scope: &str, shape: &str, u: &EvidenceUnit, m: &ContinuationMetrics) {
        let k = key(scope, shape, u);
        let c = self.cells.entry(k).or_default();
        c.delivered += 1;
        c.search_followups += m.searches_before_primary.unwrap_or(0).min(1);
        c.read_followups += m.reads_before_primary.unwrap_or(0).min(1);
        c.expansions += m.expansions;
        c.final_mentions += m.final_mentioned as u64;
        c.subsequent_work += m.subsequent_tool_calls;
        self.global.delivered += 1;
        self.global.search_followups += c.search_followups.min(1) - c.search_followups.min(1);
        // no double count
    }
    /// Shrunken search-followup rate for a cell: empirical-Bayes toward global.
    /// Below MIN_SUPPORT the result stays the global prior (§18, §28).
    pub fn followup_rate(&self, scope: &str, shape: &str, u: &EvidenceUnit) -> f64 {
        let k = key(scope, shape, u);
        let g = if self.global.delivered == 0 {
            0.5
        } else {
            self.global.search_followups as f64 / self.global.delivered as f64
        };
        match self.cells.get(&k) {
            Some(c) if c.delivered >= MIN_SUPPORT as u64 => {
                (c.search_followups as f64 + SHRINKAGE * g) / (c.delivered as f64 + SHRINKAGE)
            }
            _ => g, // low support -> global prior
        }
    }
}
