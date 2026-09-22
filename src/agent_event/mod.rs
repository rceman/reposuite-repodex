//! AgentEvent v1 + Agent Telemetry Ingest foundation.
//!
//! Canonical RepoSuite-level telemetry contract (harness-neutral), a durable
//! append-oriented Agent Event Store, a generic ingest boundary, and the first
//! producer adapter (`devin_atif`). The ingest layer records observations —
//! it never decides useful/irrelevant/correct (§4).

use std::path::PathBuf;

pub mod devin_atif;
pub mod ingest;
pub mod model;
pub mod store;

pub use ingest::{schema_version_supported, validate_event, AgentEventSink, StoreSink};
pub use model::*;
pub use store::{AgentEventStore, IngestReport, StoreManifest};

/// Agent telemetry error.
#[derive(Debug)]
pub enum AgentEventError {
    /// Malformed canonical event (§66).
    Invalid {
        field: String,
        reason: String,
    },
    /// Same event_id with different semantic content (§30).
    Conflict {
        event_id: String,
    },
    /// Corrupt stored record (§34).
    Corrupt {
        reason: String,
    },
    /// ATIF adapter problem.
    Adapter {
        reason: String,
    },
    Io {
        path: PathBuf,
        reason: String,
    },
    Serialize {
        reason: String,
    },
}

impl std::fmt::Display for AgentEventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentEventError::Invalid { field, reason } => {
                write!(f, "invalid event ({field}): {reason}")
            }
            AgentEventError::Conflict { event_id } => {
                write!(f, "conflicting duplicate event_id {event_id}")
            }
            AgentEventError::Corrupt { reason } => write!(f, "corrupt store: {reason}"),
            AgentEventError::Adapter { reason } => write!(f, "adapter: {reason}"),
            AgentEventError::Io { path, reason } => write!(f, "io {}: {reason}", path.display()),
            AgentEventError::Serialize { reason } => write!(f, "serialize: {reason}"),
        }
    }
}

impl std::error::Error for AgentEventError {}
