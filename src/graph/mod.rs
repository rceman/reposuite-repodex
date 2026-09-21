//! The repository investigation graph — a deterministic, source-grounded
//! projection of RepoDex facts, structural links and bounded candidates.
//!
//! It is the retrieval foundation for a future semantic query layer. It keeps
//! uncertainty explicit: `FACT` edges (containment, membership, structural
//! links) are never merged with `CANDIDATE` edges (bounded call candidates),
//! and a call site stays distinct from its candidate targets.

pub mod artifact;
pub mod build;
pub mod index;
pub mod model;

use std::path::PathBuf;

pub use artifact::{artifact_size, read_edges, read_manifest, read_nodes, verify};
pub use build::{build_graph, GraphBuildOutcome, GraphBuildStats, GraphPhaseTimings};
pub use index::{GraphIndex, NeighborFilter, TraverseOptions};
pub use model::{
    EvidenceClass, GraphEdge, GraphFingerprint, GraphManifest, GraphNode, NodeKind,
    GRAPH_MANIFEST_VERSION, GRAPH_POLICY_VERSION, GRAPH_SCHEMA_VERSION,
};

/// A graph-build/verify error.
#[derive(Debug)]
pub enum GraphError {
    /// An upstream artifact could not be read.
    Upstream {
        reason: String,
    },
    /// The graph's recorded upstream digests do not match the artifacts given.
    UpstreamMismatch {
        reason: String,
    },
    /// The graph schema/policy fingerprint does not match this build.
    FingerprintMismatch {
        expected: String,
        found: String,
    },
    /// An edge references a node that does not exist.
    DanglingReference {
        reason: String,
    },
    /// The artifact is internally inconsistent (dup ids, ordering, ...).
    Corrupt {
        reason: String,
    },
    /// Recomputed content digest differs from the manifest.
    DigestMismatch {
        expected: String,
        found: String,
    },
    Io {
        path: PathBuf,
        reason: String,
    },
    Serialize {
        reason: String,
    },
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::Upstream { reason } => write!(f, "upstream artifact: {reason}"),
            GraphError::UpstreamMismatch { reason } => write!(f, "stale upstream: {reason}"),
            GraphError::FingerprintMismatch { expected, found } => {
                write!(
                    f,
                    "graph fingerprint mismatch: expected {expected}, found {found}"
                )
            }
            GraphError::DanglingReference { reason } => write!(f, "dangling reference: {reason}"),
            GraphError::Corrupt { reason } => write!(f, "corrupt graph: {reason}"),
            GraphError::DigestMismatch { expected, found } => {
                write!(
                    f,
                    "graph digest mismatch: expected {expected}, found {found}"
                )
            }
            GraphError::Io { path, reason } => write!(f, "io {}: {reason}", path.display()),
            GraphError::Serialize { reason } => write!(f, "serialize: {reason}"),
        }
    }
}

impl std::error::Error for GraphError {}
