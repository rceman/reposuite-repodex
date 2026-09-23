//! `context_artifact_presented` — canonical supplied-context evidence (§3-§9).
//! Harness-neutral event type + a parser for the RepoDex RDX1 retrieval-packet
//! text format (RDX1 is RepoDex's own output format, not a harness protocol).

use super::model::*;

/// Build a `context_artifact_presented` event for a session. `seq` is the
/// assigned sequence (append position); `presented_at` should be the session
/// start timestamp (the artifact precedes the work semantically).
pub fn context_artifact_event(
    session_id: &str,
    investigation_id: Option<&str>,
    repository_id: Option<&str>,
    repo_head: Option<&str>,
    seq: u64,
    timestamp: &str,
    ca: ContextArtifactPresented,
) -> AgentEvent {
    AgentEvent {
        schema: AGENT_EVENT_SCHEMA.to_string(),
        event_id: AgentEvent::make_event_id(session_id, seq),
        session_id: session_id.to_string(),
        investigation_id: investigation_id.map(|s| s.to_string()),
        project_id: None,
        repository_id: repository_id.map(|s| s.to_string()),
        repo_head: repo_head.map(|s| s.to_string()),
        sequence: seq,
        timestamp: timestamp.to_string(),
        source: SourceProvenance {
            runtime: "repodex".into(),
            adapter: "rdx1-packet".into(),
            adapter_version: "1".into(),
            native_session_id: None,
        },
        event_type: "context_artifact_presented".to_string(),
        data: serde_json::to_value(&ca).unwrap_or_default(),
        content_digest: None,
    }
}

/// Parse an RDX1 `<RETRIEVAL_PACKET>` block into ranked artifact references.
/// Format: `#RDX1 v1`, `Q "..."`, then `F <rank> <kind> <key> <label>` lines
/// where `<key>` is like `decl:internal/x.go#3` or `call:internal/x.go#0`.
/// Returns (references, packet_text).
pub fn parse_rdx1_packet(text: &str) -> Option<(Vec<ArtifactReference>, String)> {
    let start = text.find("<RETRIEVAL_PACKET>")?;
    let end = text.find("</RETRIEVAL_PACKET>").unwrap_or(text.len());
    let block = &text[start..end];
    let mut refs = Vec::new();
    for line in block.lines() {
        let l = line.trim();
        // `F <rank> <kind> <key> <label>`
        if !l.starts_with("F ") {
            continue;
        }
        let mut it = l[2..].splitn(4, ' ');
        let rank: Option<u64> = it.next().and_then(|s| s.parse().ok());
        let kind = it.next().unwrap_or("");
        let key = it.next().unwrap_or("");
        let label = it.next().unwrap_or("");
        // key = "<kind>:<path>#<line>" or "call:path#n"
        let (path, entity) = match key.split_once(':') {
            Some((_k, rest)) => {
                let p = rest.split('#').next().unwrap_or("").to_string();
                let ent = if label.is_empty() {
                    None
                } else {
                    Some(label.trim_matches('"').to_string())
                };
                (p, ent)
            }
            None => (key.to_string(), None),
        };
        if path.is_empty() {
            continue;
        }
        refs.push(ArtifactReference {
            reference_kind: kind.to_string(),
            path: Some(path),
            entity,
            rank,
            relation: Some("presented".into()),
        });
    }
    if refs.is_empty() {
        return None;
    }
    Some((refs, block.to_string()))
}
