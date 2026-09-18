//! Derived cross-file structural relationships over a TASK 3A snapshot.
//!
//! TASK 3B is the first cross-file layer on top of the validated repository fact
//! snapshot:
//!
//! ```text
//! RepositoryFactSnapshot
//!     -> repository structural model
//!     -> language-specific module/package/namespace relationships
//!     -> import/module link outcomes
//! ```
//!
//! ## What a link outcome means
//!
//! Every relationship has one of four discrete outcomes and no numeric
//! confidence:
//!
//! * [`LinkOutcome::Exact`] — under this rule's documented structural
//!   assumptions there is exactly one candidate. It never means "the runtime
//!   target".
//! * [`LinkOutcome::Ambiguous`] — more than one candidate. Ambiguity is
//!   preserved; a candidate is never silently chosen.
//! * [`LinkOutcome::Unresolved`] — the rule applied and produced no candidate.
//! * [`LinkOutcome::OutOfScope`] — the relationship is deliberately outside this
//!   task.
//!
//! ## Boundaries this module keeps
//!
//! * **No call-target resolution, no reference resolution, no method dispatch,
//!   no type inference, no inheritance or trait resolution, no overload
//!   resolution and no dynamic dispatch.** A `from foo import bar` followed by
//!   `bar()` yields an *import* relationship here; the call edge belongs to
//!   TASK 3C.
//! * **No framework behavior.** Laravel container resolution, Django framework
//!   semantics, Go interface dispatch and Rust trait dispatch are not modelled.
//! * **No LSP, no gopls/rust-analyzer/Pyright/PHPStan/Psalm, no watcher, no
//!   daemon, no MCP, no HTTP/RPC, no database and no graph database.** A link
//!   artifact is a small deterministic index, not a graph database.
//! * **No persistent Tree-sitter trees and no reparsing.** The linking layer
//!   consumes persisted normalized facts only.
//! * **No fuzzy, full-text or semantic search**, and no ranking. TASK 3A's exact
//!   lookup remains the only other lookup surface, and it is exact too.
//!
//! ## What is deliberately *not* duplicated
//!
//! The link artifact does not contain a second copy of the normalized file
//! facts. A TASK 3A snapshot already measures several times the size of the
//! source it describes; every relationship here references the snapshot through
//! a snapshot-local locator instead.
//!
//! ## Structural identity is not semantic identity
//!
//! A Go package group, a PHP namespace, a Python package path and a Rust module
//! relationship are structural identities derivable from written syntax plus
//! repository organization. They are not universal runtime semantic identities,
//! and no permanent symbol id is introduced. File-local fact ids are reused as
//! locators and are explicitly not promised to be stable across source edits.

pub mod artifact;
pub mod build;
pub mod metadata;
pub mod model;
pub mod query;
pub mod rules_go;
pub mod rules_php;
pub mod rules_python;
pub mod rules_rust;
pub mod structure;

pub use artifact::{
    artifact_size, order_entities, order_links, read_entities, read_links, read_manifest, verify,
    LinkError, LinkVerificationReport, ENTITIES_FILE, LINKS_FILE, MANIFEST_FILE,
};
pub use build::{build_links, LinkBuildOutcome, LinkBuildStats, LinkPhaseTimings};
pub use metadata::{read_go_module, GoModule, GoModuleState, GO_MOD};
pub use model::{
    rule, rule_registry, FactKind, FactLocator, LinkFingerprint, LinkManifest, LinkOutcome,
    LinkProvenance, LinkRecord, LinkTarget, MetadataDependency, OutcomeCounts, RuleDocumentation,
    StructuralEntity, LINK_MANIFEST_VERSION, LINK_RULE_ABI_VERSION, LINK_SCHEMA_VERSION,
};
pub use query::LinkIndex;
pub use structure::Structure;
