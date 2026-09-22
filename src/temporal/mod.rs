//! Git-derived temporal/activity intelligence foundation.
//!
//! A persisted, incrementally-updated, path-centric index of per-file Git
//! change metadata (TEMPORAL/ACTIVITY evidence family — never semantic code
//! truth). Query-time access reads only the persisted artifact; no `git log`,
//! `git blame` or history scan ever runs during a RepoDex query (§3).

use std::path::PathBuf;

pub mod artifact;
pub mod build;
pub mod collect;
pub mod index;
pub mod model;

pub use artifact::{verify, FILES_FILE, MANIFEST_FILE};
pub use build::{build_temporal, repository_identity, update_temporal, TemporalOutcome};
pub use index::TemporalIndex;
pub use model::{
    FileTemporal, TemporalBuildInfo, TemporalGitSemantics, TemporalManifest, CHANGE_TAIL_CAP,
    TEMPORAL_MANIFEST_VERSION, TEMPORAL_POLICY_VERSION, TEMPORAL_SCHEMA_VERSION,
};

/// Temporal index error.
#[derive(Debug)]
pub enum TemporalError {
    /// A Git subprocess failed or produced unparseable output.
    Git {
        reason: String,
    },
    /// The recorded HEAD is not an ancestor of the current HEAD — history was
    /// rewritten; a full rebuild is required (§17).
    HistoryDiverged {
        old_head: String,
        new_head: String,
    },
    /// The prior artifact belongs to a different repository.
    UpstreamMismatch {
        reason: String,
    },
    /// The artifact's schema/policy fingerprint differs from this build.
    FingerprintMismatch {
        expected: String,
        found: String,
    },
    /// The artifact is internally inconsistent.
    Corrupt {
        reason: String,
    },
    /// Recomputed record digest differs from the manifest.
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

impl std::fmt::Display for TemporalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TemporalError::Git { reason } => write!(f, "git: {reason}"),
            TemporalError::HistoryDiverged { old_head, new_head } => write!(
                f,
                "history_diverged: indexed head {old_head} is not an ancestor of {new_head}; rebuild_required"
            ),
            TemporalError::UpstreamMismatch { reason } => {
                write!(f, "repository mismatch: {reason}")
            }
            TemporalError::FingerprintMismatch { expected, found } => {
                write!(f, "temporal fingerprint mismatch: expected {expected}, found {found}")
            }
            TemporalError::Corrupt { reason } => write!(f, "corrupt temporal index: {reason}"),
            TemporalError::DigestMismatch { expected, found } => {
                write!(f, "temporal digest mismatch: expected {expected}, found {found}")
            }
            TemporalError::Io { path, reason } => write!(f, "io {}: {reason}", path.display()),
            TemporalError::Serialize { reason } => write!(f, "serialize: {reason}"),
        }
    }
}

impl std::error::Error for TemporalError {}
