//! Generic Agent Telemetry Ingest boundary (§27).
//!
//! `producer/adapter -> AgentEvent[] -> generic sink`. No Devin/ATIF types
//! cross this boundary. Validation enforces the canonical envelope; unknown
//! future event types are preserved, not rejected (§11).

use super::model::{AgentEvent, AGENT_EVENT_SCHEMA, AGENT_EVENT_VERSION};
use super::store::{AgentEventStore, IngestReport};
use super::AgentEventError;

/// The generic ingest sink boundary.
pub trait AgentEventSink {
    /// Ingest a batch of canonical events; report accepted/duplicates/rejected.
    fn ingest_batch(&mut self, events: &[AgentEvent]) -> Result<IngestReport, AgentEventError>;
}

/// Validate one canonical envelope. Returns a structured error string when the
/// event is malformed (§66). Unknown `event_type` is allowed (forward-compat).
pub fn validate_event(e: &AgentEvent) -> Result<(), AgentEventError> {
    if e.schema != AGENT_EVENT_SCHEMA {
        return Err(AgentEventError::Invalid {
            field: "schema".into(),
            reason: format!("expected {AGENT_EVENT_SCHEMA}, found {}", e.schema),
        });
    }
    if e.event_id.is_empty() {
        return Err(AgentEventError::Invalid {
            field: "event_id".into(),
            reason: "empty".into(),
        });
    }
    if e.session_id.is_empty() {
        return Err(AgentEventError::Invalid {
            field: "session_id".into(),
            reason: "empty".into(),
        });
    }
    if e.event_id != AgentEvent::make_event_id(&e.session_id, e.sequence) {
        return Err(AgentEventError::Invalid {
            field: "event_id".into(),
            reason: format!("event_id {} does not match session:sequence", e.event_id),
        });
    }
    if e.event_type.is_empty() {
        return Err(AgentEventError::Invalid {
            field: "type".into(),
            reason: "empty".into(),
        });
    }
    // RFC3339/ISO-8601 sanity: must contain 'T' and a timezone marker.
    let ts = &e.timestamp;
    if !(ts.contains('T') && (ts.ends_with('Z') || ts.contains('+'))) {
        return Err(AgentEventError::Invalid {
            field: "timestamp".into(),
            reason: format!("not RFC3339 UTC: {ts}"),
        });
    }
    if e.sequence == u64::MAX {
        return Err(AgentEventError::Invalid {
            field: "sequence".into(),
            reason: "invalid".into(),
        });
    }
    Ok(())
}

/// The persisted store as an `AgentEventSink`.
pub struct StoreSink<'a> {
    pub store: &'a mut AgentEventStore,
}

impl AgentEventSink for StoreSink<'_> {
    fn ingest_batch(&mut self, events: &[AgentEvent]) -> Result<IngestReport, AgentEventError> {
        let mut report = IngestReport::default();
        // Validate all first; fill content_digest; reject malformed with errors.
        let mut good = Vec::with_capacity(events.len());
        for e in events {
            if let Err(err) = validate_event(e) {
                report.rejected += 1;
                report.errors.push(err.to_string());
                continue;
            }
            let mut e = e.clone();
            if e.content_digest.is_none() {
                e.content_digest = Some(AgentEventStore::event_digest(&e));
            }
            good.push(e);
        }
        let r = self.store.append(&good)?;
        report.accepted = r.accepted;
        report.duplicates = r.duplicates;
        report.rejected += r.rejected;
        report.errors.extend(r.errors);
        Ok(report)
    }
}

/// Reject non-canonical schema versions explicitly (genuine incompatibility
/// fails; unknown types within v1 pass through).
pub fn schema_version_supported(schema: &str) -> bool {
    schema == AGENT_EVENT_SCHEMA || schema.starts_with("reposuite.agent-event.")
}

pub fn version() -> u32 {
    AGENT_EVENT_VERSION
}
