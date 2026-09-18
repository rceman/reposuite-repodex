//! Deterministic repository fact snapshot and incremental file-level index.
//!
//! TASK 3A is the first repository-level layer on top of the validated
//! single-file syntax foundation:
//!
//! ```text
//! repository
//!     -> deterministic scanner
//!     -> per-file normalized analysis
//!     -> RepositoryFactSnapshot
//!     -> persistent deterministic snapshot artifact
//!     -> incremental file-level rebuild using a prior snapshot
//! ```
//!
//! This is **not semantic resolution**. There is no cross-file symbol
//! resolution, no resolved reference, no call graph, no repository map and no
//! navigation. A repository snapshot is a deterministic collection of
//! source-grounded file analyses, and nothing more.
//!
//! ## Boundaries this module keeps
//!
//! * No Tree-sitter `Tree` is ever retained or serialized. A persisted artifact
//!   is exactly the normalized facts.
//! * No source buffer is persisted. The source is re-read when a file changes.
//! * The unit of work is the whole file. Unchanged files reuse their persisted
//!   analysis; changed and added files are parsed and extracted in full. There
//!   is no fine-grained fact patching.
//! * Canonical repository content never depends on the absolute checkout root,
//!   the output directory, the wall clock, the process id or discovery order.
//!
//! ## Known extraction limitations remain visible
//!
//! A missing normalized occurrence does **not** always mean the source construct
//! does not exist. In particular, TASK 2's F002 boundary still holds: a Rust
//! call written inside a macro token tree can be absent from the normalized
//! call-like facts, because Tree-sitter represents macro arguments as a flat
//! `token_tree` with no expression subtrees. Recovered analysis is likewise
//! distinct from clean analysis, and the snapshot preserves that distinction
//! rather than flattening it. No heuristic facts are invented to hide either
//! boundary.

pub mod artifact;
pub mod build;
pub mod digest;
pub mod fingerprint;
pub mod index;
pub mod manifest;

pub use artifact::{
    object_key, staging_dir, verify, SnapshotError, VerificationReport, FILES_DIR, MANIFEST_FILE,
};
pub use build::{
    build_snapshot, load_manifest, update_snapshot, verify_snapshot, BuildOptions, BuildOutcome,
    BuildStats,
};
pub use digest::{analysis_digest, content_digest, sha256, sha256_text};
pub use fingerprint::{
    AnalyzerFingerprint, SnapshotConfig, ANALYSIS_ABI_VERSION, IGNORE_POLICY_VERSION,
    MANIFEST_VERSION,
};
pub use index::{CallHit, DeclarationHit, ImportHit, RepositoryFactIndex};
pub use manifest::{CoverageSummary, FactTotals, IndexedFile, RepositoryManifest};
