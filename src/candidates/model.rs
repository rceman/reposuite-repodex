//! The derived call-candidate model.
//!
//! Everything here is **derived from** a TASK 3A `RepositoryFactSnapshot` plus
//! the TASK 3B link artifact it depends on. A call candidate is evidence that a
//! declaration *could* be relevant under a bounded syntactic rule. It is never
//! proof that a call resolves to that declaration.
//!
//! The single most important invariant:
//!
//! ```text
//! SingleCandidate != resolved call target
//! ```
//!
//! One candidate only means "under this rule there is exactly one syntactic
//! candidate". It is deliberately *not* encoded as `Exact`, and not as
//! `Ambiguous` with one element, because a candidate record is not a resolved
//! link at all — it is the start of a question, not its answer.
//!
//! The model uses its own cardinality vocabulary rather than reusing
//! [`crate::links::model::LinkOutcome`]. `LinkOutcome` describes bounded
//! *structural* relationships (import/module), where `Exact` means "one
//! structural candidate". Call candidates need a different promise — "one
//! *possible* target" — so a separate type keeps the two meanings from
//! collapsing into each other.

use serde::{Deserialize, Serialize};

use crate::links::model::FactLocator;
use crate::repository::digest;

/// Version of the derived candidate record schema.
pub const CANDIDATE_SCHEMA_VERSION: u32 = 1;
/// Version of the candidate artifact manifest format.
pub const CANDIDATE_MANIFEST_VERSION: u32 = 1;

/// Version of the call-candidate *rule* semantics.
///
/// **Increment whenever a change to any candidate rule could change the record
/// set produced from an unchanged snapshot** — a new rule, a changed in-scope
/// call definition, a changed candidate-declaration definition, or a changed
/// lexical/module selection rule. It does **not** need to change for
/// documentation, tests, CLI formatting or performance work.
///
/// This version participates in [`CandidateFingerprint`], so bumping it
/// invalidates previously derived candidate artifacts without touching the
/// TASK 3A snapshot or the TASK 3B link artifact.
pub const CANDIDATE_RULE_ABI_VERSION: u32 = 1;

/// Per-language candidate-policy versions.
///
/// Separate from the ABI version so that a change confined to one language's
/// candidate policy is visible as such in the fingerprint text.
pub const POLICY_VERSION_RUST_CALL: u32 = 1;

/// Stable, machine-readable candidate rule identifiers.
///
/// These strings appear in every candidate record and in the rule registry, so
/// they must never be renamed once shipped.
pub mod candidate_rule {
    /// The bounded Rust local plain-name function-candidate rule.
    pub const RUST_CALL_LOCAL_FUNCTION_CANDIDATE: &str = "rust.call.local_function_candidate";

    pub const ALL: &[&str] = &[RUST_CALL_LOCAL_FUNCTION_CANDIDATE];
}

/// What a call candidate points at.
///
/// A candidate is a snapshot-local reference to one source-written declaration
/// that satisfies the rule. It carries enough descriptive fields to be read
/// without re-opening the snapshot, but it is not a permanent symbol id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CandidateTarget {
    /// Path of the file holding the declaration, relative to the repository.
    pub relative_path: String,
    /// Snapshot-local declaration id.
    pub declaration_id: u32,
    /// The declaration kind, always `function` for the only implemented rule.
    pub declaration_kind: String,
    /// The written declaration name.
    pub name: String,
    /// The lexical scope this declaration lives in, for auditability.
    pub scope_path: String,
}

impl CandidateTarget {
    pub fn new(
        relative_path: impl Into<String>,
        declaration_id: u32,
        declaration_kind: impl Into<String>,
        name: impl Into<String>,
        scope_path: impl Into<String>,
    ) -> Self {
        Self {
            relative_path: relative_path.into(),
            declaration_id,
            declaration_kind: declaration_kind.into(),
            name: name.into(),
            scope_path: scope_path.into(),
        }
    }

    /// Deterministic canonical rendering, used by the candidate digest.
    pub fn render(&self) -> String {
        format!(
            "cand:{0}#{1}:{2}:{3}@{4}",
            self.relative_path,
            self.declaration_id,
            self.declaration_kind,
            self.name,
            self.scope_path
        )
    }

    /// Sort key for deterministic candidate ordering, by the meaningful key —
    /// path then declaration id — never by a derived id.
    pub fn canonical_key(&self) -> (String, u32) {
        (self.relative_path.clone(), self.declaration_id)
    }
}

/// Put a candidate list into canonical order and drop exact duplicates.
///
/// This is the single implementation of deterministic candidate ordering, so
/// every outcome that carries candidates shares it.
pub fn sort_candidates(candidates: &mut Vec<CandidateTarget>) {
    candidates.sort_by_key(CandidateTarget::canonical_key);
    candidates.dedup_by(|left, right| left.canonical_key() == right.canonical_key());
}

/// The discrete cardinality of one call's candidate set.
///
/// This is deliberately *not* `LinkOutcome`. There is no `Exact` here and no
/// numeric confidence: a candidate is a possibility, never a resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum CandidateOutcome {
    /// The rule applied and produced no candidate.
    ///
    /// This means "no candidate under the TASK 3C rule", never "this call has
    /// no runtime target". A real target may still exist through an import, a
    /// re-export, a local variable or a mechanism the rule does not model.
    NoCandidate { reason: String },
    /// Exactly one syntactic candidate. Still only a candidate.
    SingleCandidate { candidate: CandidateTarget },
    /// More than one syntactic candidate. Ambiguity is preserved, never
    /// collapsed to the first candidate.
    MultipleCandidates { candidates: Vec<CandidateTarget> },
    /// The call shape is deliberately outside this task's scope.
    OutOfScope { reason: String },
}

impl CandidateOutcome {
    pub fn no_candidate(reason: impl Into<String>) -> Self {
        CandidateOutcome::NoCandidate {
            reason: reason.into(),
        }
    }

    pub fn out_of_scope(reason: impl Into<String>) -> Self {
        CandidateOutcome::OutOfScope {
            reason: reason.into(),
        }
    }

    /// Build an outcome from a candidate list.
    ///
    /// This is the **only** place a candidate list becomes an outcome, so the
    /// "never collapse ambiguity" rule has a single implementation: zero
    /// candidates is `NoCandidate`, one is `SingleCandidate`, and two or more
    /// is `MultipleCandidates`. It never produces `Exact` and never produces a
    /// one-element `Ambiguous`.
    pub fn from_candidates(
        mut candidates: Vec<CandidateTarget>,
        empty_reason: impl Into<String>,
    ) -> Self {
        sort_candidates(&mut candidates);
        match candidates.len() {
            0 => CandidateOutcome::no_candidate(empty_reason),
            1 => CandidateOutcome::SingleCandidate {
                candidate: candidates.remove(0),
            },
            _ => CandidateOutcome::MultipleCandidates { candidates },
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            CandidateOutcome::NoCandidate { .. } => "no_candidate",
            CandidateOutcome::SingleCandidate { .. } => "single_candidate",
            CandidateOutcome::MultipleCandidates { .. } => "multiple_candidates",
            CandidateOutcome::OutOfScope { .. } => "out_of_scope",
        }
    }

    /// The single candidate of a `SingleCandidate` outcome.
    pub fn single(&self) -> Option<&CandidateTarget> {
        match self {
            CandidateOutcome::SingleCandidate { candidate } => Some(candidate),
            _ => None,
        }
    }

    /// Every candidate this outcome names.
    pub fn candidates(&self) -> &[CandidateTarget] {
        match self {
            CandidateOutcome::SingleCandidate { candidate } => std::slice::from_ref(candidate),
            CandidateOutcome::MultipleCandidates { candidates } => candidates,
            CandidateOutcome::NoCandidate { .. } | CandidateOutcome::OutOfScope { .. } => &[],
        }
    }

    /// Deterministic canonical rendering.
    pub fn render(&self) -> String {
        match self {
            CandidateOutcome::NoCandidate { reason } => format!("no_candidate({reason})"),
            CandidateOutcome::SingleCandidate { candidate } => {
                format!("single_candidate({})", candidate.render())
            }
            CandidateOutcome::MultipleCandidates { candidates } => {
                let rendered = candidates
                    .iter()
                    .map(CandidateTarget::render)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("multiple_candidates([{rendered}])")
            }
            CandidateOutcome::OutOfScope { reason } => format!("out_of_scope({reason})"),
        }
    }
}

/// Per-call provenance.
///
/// The rule's documented conditions and assumptions live once in the rule
/// registry (see [`CandidateRuleDocumentation`]). This record carries only the
/// *occurrence-specific* evidence — the lexical scope path, how far the search
/// ascended, the module that bounded it and the rule-specific facts — because
/// the call's identity already lives once on [`CallCandidateRecord`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateProvenance {
    /// Readable lexical scope path of the call, outermost scope first, e.g.
    /// `(file)::a::run`.
    pub scope_path: String,
    /// Scope levels the search ascended before it stopped. Absent for
    /// out-of-scope calls, where no lexical search ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_levels: Option<u32>,
    /// The enclosing module scope that bounded the search. Absent for
    /// out-of-scope calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enclosing_module: Option<String>,
    /// Rule-specific evidence, deterministic order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
}

impl CandidateProvenance {
    fn render(&self) -> String {
        let evidence = self.evidence.join("|");
        format!(
            "prov scope_path={} search_levels={} enclosing={} evidence={}",
            escape(&self.scope_path),
            self.search_levels
                .map(|level| level.to_string())
                .unwrap_or_else(|| "<none>".to_string()),
            escape(self.enclosing_module.as_deref().unwrap_or("<none>")),
            escape(&evidence),
        )
    }
}

/// One derived call-candidate record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallCandidateRecord {
    /// Content-derived id, stable for a given snapshot and rule set.
    pub record_id: String,
    pub rule_id: String,
    pub language: String,
    /// The call occurrence locator.
    pub source: FactLocator,
    /// The written callee.
    pub written: String,
    pub outcome: CandidateOutcome,
    pub provenance: CandidateProvenance,
}

impl CallCandidateRecord {
    /// Build a record, deriving its id from the record's own content.
    pub fn new(
        language: impl Into<String>,
        rule_id: impl Into<String>,
        source: FactLocator,
        written: impl Into<String>,
        outcome: CandidateOutcome,
        provenance: CandidateProvenance,
    ) -> Self {
        let rule_id = rule_id.into();
        let written = written.into();
        let record_id = candidate_record_id_for(&rule_id, &source, &written);
        Self {
            record_id,
            rule_id,
            language: language.into(),
            source,
            written,
            outcome,
            provenance,
        }
    }

    /// Canonical one-line rendering, used by the candidate digest.
    fn render(&self) -> String {
        format!(
            "record id={} rule={} lang={} source={} written={} outcome={} {}",
            self.record_id,
            self.rule_id,
            self.language,
            self.source.key(),
            escape(&self.written),
            self.outcome.render(),
            self.provenance.render(),
        )
    }
}

/// Derive a candidate record id from the record's own content.
///
/// Including the source locator keeps two identical records from two different
/// occurrences distinct, so a record id is unique per source call occurrence
/// rather than per written form.
pub fn candidate_record_id_for(rule_id: &str, source: &FactLocator, written: &str) -> String {
    let seed = format!(
        "repodex-candidate-v1\n{rule_id}\n{}\n{written}",
        source.key()
    );
    let digest = digest::sha256_text(&seed);
    let hex = digest.strip_prefix("sha256:").unwrap_or(&digest);
    format!("cand-{}", &hex[..16])
}

/// The derived-candidate compatibility fingerprint.
///
/// Covers only what changes *candidate semantics*: the manifest and schema
/// versions, the rule ABI version and the per-language candidate-policy
/// versions. Changing it invalidates the candidate artifact without touching
/// the TASK 3A snapshot or the TASK 3B link artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateFingerprint {
    pub text: String,
    pub digest: String,
}

impl CandidateFingerprint {
    pub fn current() -> Self {
        let text = format!(
            "manifest={} schema={} rule_abi={} rust={}",
            CANDIDATE_MANIFEST_VERSION,
            CANDIDATE_SCHEMA_VERSION,
            CANDIDATE_RULE_ABI_VERSION,
            POLICY_VERSION_RUST_CALL,
        );
        Self {
            digest: digest::sha256_text(&format!("repodex-candidate-fingerprint\n{text}")),
            text,
        }
    }
}

/// Counts of candidate cardinality.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardinalityCounts {
    pub no_candidate: u64,
    pub single_candidate: u64,
    pub multiple_candidates: u64,
    pub out_of_scope: u64,
}

impl CardinalityCounts {
    pub fn record(&mut self, outcome: &CandidateOutcome) {
        match outcome {
            CandidateOutcome::NoCandidate { .. } => self.no_candidate += 1,
            CandidateOutcome::SingleCandidate { .. } => self.single_candidate += 1,
            CandidateOutcome::MultipleCandidates { .. } => self.multiple_candidates += 1,
            CandidateOutcome::OutOfScope { .. } => self.out_of_scope += 1,
        }
    }
}

/// One documented candidate rule, embedded in the manifest so an artifact is
/// self-describing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateRuleDocumentation {
    pub rule_id: String,
    pub language: String,
    pub summary: String,
    /// The exact call shapes the rule accepts.
    pub in_scope_calls: String,
    /// The exact declarations that may be candidates.
    pub candidate_declarations: String,
    /// The lexical/module selection algorithm.
    pub selection_rule: String,
    /// Why one candidate is still only a candidate.
    pub single_candidate_meaning: String,
    /// What `NoCandidate` does and does not mean.
    pub no_candidate_meaning: String,
    /// Constructs the rule deliberately does not touch.
    pub known_exclusions: Vec<String>,
}

/// The complete candidate-rule registry.
pub fn candidate_rule_registry() -> Vec<CandidateRuleDocumentation> {
    vec![CandidateRuleDocumentation {
        rule_id: candidate_rule::RUST_CALL_LOCAL_FUNCTION_CANDIDATE.to_string(),
        language: "rust".to_string(),
        summary: "Bounded lexical/module-local candidate search for Rust \
                  plain-name calls."
            .to_string(),
        in_scope_calls: "a Rust call-like occurrence with form `plain_name`, a \
                         callee that is exactly one written identifier, and a \
                         non-dynamic callee."
            .to_string(),
        candidate_declarations: "source-written Rust `function` declarations \
                                 only; never methods, associated functions, \
                                 closures, locals, constructors, macros, \
                                 imports or re-exports."
            .to_string(),
        selection_rule: "start at the call's containing scope; at each lexical \
                         level collect eligible `function` declarations; stop \
                         at the first level that contains a match; never cross \
                         a `module` or `file` boundary, so the search is \
                         confined to the innermost enclosing module."
            .to_string(),
        single_candidate_meaning: "one syntactic candidate under this bounded \
                                   rule. A candidate is evidence a declaration \
                                   could be relevant, NOT proof the call \
                                   resolves to it."
            .to_string(),
        no_candidate_meaning: "no candidate under the TASK 3C rule. It does not \
                               mean the call has no runtime target."
            .to_string(),
        known_exclusions: vec![
            "imports and re-exports (use / pub use)".to_string(),
            "methods, associated functions and trait methods".to_string(),
            "closures and local callable variables".to_string(),
            "tuple-struct and enum-variant constructors".to_string(),
            "qualified-path, member-selector, static-scoped and indirect calls".to_string(),
            "macro invocations and calls inside macro token trees".to_string(),
            "function declarations inside extern blocks".to_string(),
        ],
    }]
}

/// The manifest of a derived candidate artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateManifest {
    pub candidate_manifest_version: u32,
    pub candidate_schema_version: u32,
    pub candidate_rule_abi_version: u32,

    /// The exact TASK 3A snapshot this artifact was derived from.
    pub snapshot_digest: String,
    pub snapshot_schema_version: u32,
    pub snapshot_analyzer_fingerprint: String,

    /// The exact TASK 3B link artifact this build depends on. The candidate
    /// rule is file-local, but it still depends on the structural model the
    /// link artifact validates.
    pub link_digest: String,
    pub link_fingerprint: String,
    pub link_rule_abi_version: u32,

    /// Identity of the candidate rule set.
    pub candidate_fingerprint: String,
    #[serde(default)]
    pub candidate_fingerprint_text: String,

    /// Digest over canonical candidate content.
    pub candidate_digest: String,

    pub records: u64,
    pub cardinalities: CardinalityCounts,
    /// Candidate-rule registry: every rule, documented once.
    pub rules: Vec<CandidateRuleDocumentation>,
}

impl CandidateManifest {
    /// Canonical text over everything the candidate digest covers.
    ///
    /// Excludes the free-text fingerprint, the candidate digest itself and all
    /// operational counters, so the digest is a pure function of the dependency
    /// identity plus the record content.
    pub fn canonical_text(&self) -> String {
        format!(
            "candidates manifest_version={} schema={} rule_abi={} snapshot={} \
             snapshot_schema={} snapshot_fingerprint={} link_digest={} \
             link_fingerprint={} link_rule_abi={} candidate_fingerprint={}\n",
            self.candidate_manifest_version,
            self.candidate_schema_version,
            self.candidate_rule_abi_version,
            self.snapshot_digest,
            self.snapshot_schema_version,
            self.snapshot_analyzer_fingerprint,
            self.link_digest,
            self.link_fingerprint,
            self.link_rule_abi_version,
            self.candidate_fingerprint,
        )
    }

    /// Recompute the candidate digest from canonical content plus the records.
    pub fn compute_candidate_digest(&self, records: &[CallCandidateRecord]) -> String {
        let mut text = self.canonical_text();
        for record in records {
            text.push_str(&record.render());
            text.push('\n');
        }
        digest::sha256_text(&text)
    }

    pub fn refresh_candidate_digest(&mut self, records: &[CallCandidateRecord]) {
        self.candidate_digest = self.compute_candidate_digest(records);
    }

    /// The documentation of one rule.
    pub fn rule(&self, rule_id: &str) -> Option<&CandidateRuleDocumentation> {
        self.rules.iter().find(|rule| rule.rule_id == rule_id)
    }
}

/// Escape one free-text field for canonical rendering.
fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::links::model::FactKind;

    fn locator(path: &str, fact_id: u32) -> FactLocator {
        FactLocator {
            relative_path: path.to_string(),
            fact_kind: FactKind::Call,
            fact_id,
            item_index: None,
        }
    }

    #[test]
    fn a_single_candidate_is_never_exact_and_never_one_element_ambiguous() {
        let target = CandidateTarget::new("src/lib.rs", 0, "function", "helper", "file");
        let outcome = CandidateOutcome::from_candidates(vec![target], "none");
        // There is no Exact variant at all, and one candidate is its own
        // cardinality rather than a degenerate ambiguity.
        assert!(matches!(outcome, CandidateOutcome::SingleCandidate { .. }));
        assert_eq!(outcome.as_str(), "single_candidate");
    }

    #[test]
    fn zero_candidates_is_no_candidate_many_is_multiple() {
        assert!(matches!(
            CandidateOutcome::from_candidates(Vec::new(), "none"),
            CandidateOutcome::NoCandidate { .. }
        ));
        let two = vec![
            CandidateTarget::new("src/lib.rs", 0, "function", "dup", "file"),
            CandidateTarget::new("src/lib.rs", 1, "function", "dup", "file"),
        ];
        assert!(matches!(
            CandidateOutcome::from_candidates(two, "none"),
            CandidateOutcome::MultipleCandidates { .. }
        ));
    }

    #[test]
    fn record_id_is_derived_from_content() {
        let first = locator("a.rs", 0);
        let second = locator("a.rs", 1);
        assert_eq!(
            candidate_record_id_for("r", &first, "x"),
            candidate_record_id_for("r", &first, "x")
        );
        assert_ne!(
            candidate_record_id_for("r", &first, "x"),
            candidate_record_id_for("r", &second, "x")
        );
        assert_ne!(
            candidate_record_id_for("r1", &first, "x"),
            candidate_record_id_for("r2", &first, "x")
        );
    }
}
