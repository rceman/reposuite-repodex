//! Derived call-candidate records over a TASK 3A snapshot (TASK 3C).
//!
//! TASK 3C is the first call-candidate layer on top of the validated repository
//! fact snapshot and the TASK 3B link artifact:
//!
//! ```text
//! RepositoryFactSnapshot + link artifact
//!     -> Rust plain-name call-like occurrence
//!     -> lexical/module-local syntactic candidate search
//!     -> zero / one / many candidate declarations
//!     -> provenance-bearing candidate record
//! ```
//!
//! ## A candidate is not a resolved call
//!
//! The central rule of this layer:
//!
//! ```text
//! SingleCandidate != resolved call target
//! ```
//!
//! A candidate is evidence that a declaration could be relevant under a bounded
//! syntactic rule. It is never proof that a call resolves to that declaration.
//! The model uses [`CandidateOutcome`], a dedicated cardinality — `NoCandidate`,
//! `SingleCandidate`, `MultipleCandidates`, `OutOfScope` — rather than reusing
//! [`crate::links::model::LinkOutcome`], so one candidate is never represented
//! as `Exact` and never as a one-element `Ambiguous`.
//!
//! ## Scope is Rust only
//!
//! Only `rust.call.local_function_candidate` is implemented, and only for Rust
//! `plain_name` calls whose callee is exactly one written identifier. Candidate
//! declarations are source-written Rust `function` declarations reachable in the
//! innermost enclosing module. Imports, re-exports, methods, associated
//! functions, closures, locals, constructors, macros and every non-plain-name
//! call shape are out of scope.
//!
//! ## Boundaries this module keeps
//!
//! * **No general Rust name resolution.** Imports and re-exports are not
//!   resolved, so `use crate::x::helper; helper();` yields `NoCandidate`, which
//!   is a bounded-rule statement, not "no runtime target".
//! * **No incremental candidate patching.** The whole artifact is rebuilt from
//!   the whole snapshot; the measured cost does not justify mutation.
//! * **No fuzzy, full-text or semantic search**, and no ranking.
//! * **No reparsing.** The layer consumes persisted normalized facts only.

pub mod artifact;
pub mod build;
pub mod model;
pub mod query;
pub mod rule_rust;

pub use artifact::{
    artifact_size, order_records, read_manifest, read_records, verify, CandidateError,
    CandidateVerificationReport, MANIFEST_FILE, RECORDS_FILE,
};
pub use build::{
    build_candidates, CandidateBuildOutcome, CandidateBuildStats, CandidatePhaseTimings,
};
pub use model::{
    candidate_rule, candidate_rule_registry, sort_candidates, CallCandidateRecord,
    CandidateFingerprint, CandidateManifest, CandidateOutcome, CandidateProvenance,
    CandidateRuleDocumentation, CandidateTarget, CardinalityCounts, CANDIDATE_MANIFEST_VERSION,
    CANDIDATE_RULE_ABI_VERSION, CANDIDATE_SCHEMA_VERSION,
};
pub use query::CandidateIndex;
