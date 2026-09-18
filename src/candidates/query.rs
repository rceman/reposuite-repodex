//! Exact query API over a derived call-candidate artifact.
//!
//! This is deliberately not a query language and not a search engine. Every
//! operation is an exact filter over the derived records: by source file, by
//! cardinality, by rule, by candidate file or declaration. There is no fuzzy
//! match and no ranking.

use std::path::Path;

use super::artifact::{self, CandidateError};
use super::model::{CallCandidateRecord, CandidateOutcome};

/// A loaded candidate artifact, indexed for exact lookups.
#[derive(Debug)]
pub struct CandidateIndex {
    pub manifest: super::model::CandidateManifest,
    records: Vec<CallCandidateRecord>,
}

impl CandidateIndex {
    /// Load a candidate artifact directory.
    ///
    /// This reads the records; it does not re-derive them and does not verify
    /// the artifact. Use [`crate::candidates::artifact::verify`] for that.
    pub fn load(dir: &Path) -> Result<Self, CandidateError> {
        let manifest = artifact::read_manifest(dir)?;
        let records = artifact::read_records(dir)?;
        Ok(Self { manifest, records })
    }

    /// Every record, in canonical order.
    pub fn records(&self) -> &[CallCandidateRecord] {
        &self.records
    }

    /// Records whose source call is in `relative_path`.
    pub fn from_source(&self, relative_path: &str) -> Vec<&CallCandidateRecord> {
        self.records
            .iter()
            .filter(|record| record.source.relative_path == relative_path)
            .collect()
    }

    /// Records naming a candidate declaration in `relative_path`.
    pub fn targeting_file(&self, relative_path: &str) -> Vec<&CallCandidateRecord> {
        self.records
            .iter()
            .filter(|record| {
                record
                    .outcome
                    .candidates()
                    .iter()
                    .any(|candidate| candidate.relative_path == relative_path)
            })
            .collect()
    }

    /// Records naming one specific declaration.
    pub fn targeting_declaration(
        &self,
        relative_path: &str,
        declaration_id: u32,
    ) -> Vec<&CallCandidateRecord> {
        self.records
            .iter()
            .filter(|record| {
                record.outcome.candidates().iter().any(|candidate| {
                    candidate.relative_path == relative_path
                        && candidate.declaration_id == declaration_id
                })
            })
            .collect()
    }

    /// Records by cardinality.
    pub fn none(&self) -> Vec<&CallCandidateRecord> {
        self.by_cardinality(|outcome| matches!(outcome, CandidateOutcome::NoCandidate { .. }))
    }

    pub fn single(&self) -> Vec<&CallCandidateRecord> {
        self.by_cardinality(|outcome| matches!(outcome, CandidateOutcome::SingleCandidate { .. }))
    }

    pub fn multiple(&self) -> Vec<&CallCandidateRecord> {
        self.by_cardinality(|outcome| {
            matches!(outcome, CandidateOutcome::MultipleCandidates { .. })
        })
    }

    pub fn out_of_scope(&self) -> Vec<&CallCandidateRecord> {
        self.by_cardinality(|outcome| matches!(outcome, CandidateOutcome::OutOfScope { .. }))
    }

    /// Records of one rule.
    pub fn by_rule(&self, rule_id: &str) -> Vec<&CallCandidateRecord> {
        self.records
            .iter()
            .filter(|record| record.rule_id == rule_id)
            .collect()
    }

    /// Counts by cardinality, in canonical order.
    pub fn cardinality_counts(&self) -> super::model::CardinalityCounts {
        let mut counts = super::model::CardinalityCounts::default();
        for record in &self.records {
            counts.record(&record.outcome);
        }
        counts
    }

    /// Counts by rule id, in canonical order.
    pub fn counts_by_rule(&self) -> std::collections::BTreeMap<String, u64> {
        let mut counts = std::collections::BTreeMap::new();
        for record in &self.records {
            *counts.entry(record.rule_id.clone()).or_default() += 1;
        }
        counts
    }

    fn by_cardinality(
        &self,
        predicate: impl Fn(&CandidateOutcome) -> bool,
    ) -> Vec<&CallCandidateRecord> {
        self.records
            .iter()
            .filter(|record| predicate(&record.outcome))
            .collect()
    }
}
