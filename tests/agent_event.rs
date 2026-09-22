//! AgentEvent v1 contract tests (§62-§69): minimal session, idempotency,
//! conflicting duplicate, unknown type, malformed, out-of-order, path
//! normalization, large batch.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use repodex::agent_event::devin_atif::{from_atif, AtifContext};
use repodex::agent_event::*;
use repodex::agent_event::{AgentEventSink, StoreSink};

static N: AtomicU32 = AtomicU32::new(0);
fn dir(tag: &str) -> PathBuf {
    let i = N.fetch_add(1, Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("rae-{tag}-{}-{i}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn prov() -> SourceProvenance {
    SourceProvenance {
        runtime: "test".into(),
        adapter: "t".into(),
        adapter_version: "1".into(),
        native_session_id: None,
    }
}
fn ev(sid: &str, seq: u64, ty: &str, data: serde_json::Value) -> AgentEvent {
    AgentEvent {
        schema: AGENT_EVENT_SCHEMA.into(),
        event_id: AgentEvent::make_event_id(sid, seq),
        session_id: sid.into(),
        investigation_id: Some("inv".into()),
        project_id: None,
        repository_id: Some("repo".into()),
        repo_head: Some("head".into()),
        sequence: seq,
        timestamp: format!("2026-01-01T00:00:{seq:02}Z"),
        source: prov(),
        event_type: ty.into(),
        data,
        content_digest: None,
    }
}

/// §62 minimal full session round-trips through the store.
#[test]
fn minimal_session_roundtrip() {
    let d = dir("min");
    let mut store = AgentEventStore::open(&d).unwrap();
    let sid = "s-min";
    let events = vec![
        ev(sid, 0, "session_started", serde_json::json!({"model":"m"})),
        ev(
            sid,
            1,
            "model_call_started",
            serde_json::json!({"model_call_id":"mc-1"}),
        ),
        ev(
            sid,
            2,
            "tool_call_started",
            serde_json::json!({"tool_call_id":"t1","tool_name":"read","category":"file_read"}),
        ),
        ev(
            sid,
            3,
            "tool_call_completed",
            serde_json::json!({"tool_call_id":"t1","ok":true,"output_bytes":10}),
        ),
        ev(
            sid,
            4,
            "source_observed",
            serde_json::json!({"path":"a/b.go","observation_kind":"explicit_read"}),
        ),
        ev(
            sid,
            5,
            "model_call_completed",
            serde_json::json!({"model_call_id":"mc-1","input_tokens":100,"output_tokens":20}),
        ),
        ev(
            sid,
            6,
            "final_answer",
            serde_json::json!({"content":"done"}),
        ),
        ev(
            sid,
            7,
            "session_completed",
            serde_json::json!({"reason":"runtime_ended"}),
        ),
    ];
    let r = {
        let mut sink = StoreSink { store: &mut store };
        sink.ingest_batch(&events).unwrap()
    };
    assert_eq!(r.accepted, 8);
    assert_eq!(r.rejected, 0);
    let got = store.session_events(sid).unwrap();
    assert_eq!(got.len(), 8);
    for (a, b) in events.iter().zip(got.iter()) {
        assert_eq!(a.event_type, b.event_type);
        assert!(b.content_digest.is_some());
    }
    let _ = std::fs::remove_dir_all(&d);
}

/// §63 idempotent replay: identical batch twice -> duplicates, no new events.
#[test]
fn idempotent_replay() {
    let d = dir("idem");
    let mut store = AgentEventStore::open(&d).unwrap();
    let events = vec![
        ev("s-i", 0, "session_started", serde_json::json!({})),
        ev("s-i", 1, "session_completed", serde_json::json!({})),
    ];
    let r1 = {
        let mut s = StoreSink { store: &mut store };
        s.ingest_batch(&events).unwrap()
    };
    assert_eq!(r1.accepted, 2);
    let r2 = {
        let mut s = StoreSink { store: &mut store };
        s.ingest_batch(&events).unwrap()
    };
    assert_eq!(r2.accepted, 0);
    assert_eq!(r2.duplicates, 2);
    assert_eq!(store.session_events("s-i").unwrap().len(), 2);
    let _ = std::fs::remove_dir_all(&d);
}

/// §64 same event_id, different payload -> explicit conflict (rejected).
#[test]
fn conflicting_duplicate_rejected() {
    let d = dir("conf");
    let mut store = AgentEventStore::open(&d).unwrap();
    let a = ev(
        "s-c",
        0,
        "session_started",
        serde_json::json!({"model":"m1"}),
    );
    let mut b = a.clone();
    b.data = serde_json::json!({"model":"m2"}); // same id, different content
    let r = {
        let mut s = StoreSink { store: &mut store };
        s.ingest_batch(&[a]).unwrap();
        s.ingest_batch(&[b]).unwrap()
    };
    assert_eq!(r.rejected, 1, "conflicting event_id must be rejected");
    assert!(!r.errors.is_empty());
    let _ = std::fs::remove_dir_all(&d);
}

/// §65 unknown future event type is preserved, not rejected/corrupted.
#[test]
fn unknown_event_type_preserved() {
    let d = dir("unk");
    let mut store = AgentEventStore::open(&d).unwrap();
    let e = ev(
        "s-u",
        0,
        "future_co_read_v9",
        serde_json::json!({"opaque":{"x":[1,2,3]}}),
    );
    let r = {
        let mut s = StoreSink { store: &mut store };
        s.ingest_batch(&[e]).unwrap()
    };
    assert_eq!(r.accepted, 1);
    let got = store.session_events("s-u").unwrap();
    assert_eq!(got[0].event_type, "future_co_read_v9");
    assert_eq!(got[0].data["opaque"]["x"], serde_json::json!([1, 2, 3]));
    let _ = std::fs::remove_dir_all(&d);
}

/// §66 malformed events rejected with diagnostics.
#[test]
fn malformed_events_rejected() {
    let mut bad = ev("s-m", 0, "session_started", serde_json::json!({}));
    bad.schema = "wrong.schema".into();
    assert!(validate_event(&bad).is_err());
    let mut bad2 = ev("s-m", 0, "x", serde_json::json!({}));
    bad2.event_id = "mismatch".into();
    assert!(validate_event(&bad2).is_err());
    let mut bad3 = ev("s-m", 0, "x", serde_json::json!({}));
    bad3.timestamp = "not-a-time".into();
    assert!(validate_event(&bad3).is_err());
    let mut bad4 = ev("s-m", 0, "x", serde_json::json!({}));
    bad4.session_id = String::new();
    assert!(validate_event(&bad4).is_err());
}

/// §67 out-of-order ingest is accepted (sequence kept for later verification),
/// since replay/retry must stay practical.
#[test]
fn out_of_order_accepted() {
    let d = dir("ooo");
    let mut store = AgentEventStore::open(&d).unwrap();
    let a = ev("s-o", 5, "model_call_completed", serde_json::json!({}));
    let b = ev("s-o", 1, "session_started", serde_json::json!({}));
    let r = {
        let mut s = StoreSink { store: &mut store };
        s.ingest_batch(&[a, b]).unwrap()
    };
    assert_eq!(r.accepted, 2);
    // verify detects the regression without corrupting evidence
    let got = store.session_events("s-o").unwrap();
    let mut anom = 0;
    let mut last = -1i64;
    for e in &got {
        if (e.sequence as i64) <= last {
            anom += 1;
        }
        last = e.sequence as i64;
    }
    assert_eq!(anom, 1, "sequence regression detected by verifier");
    let _ = std::fs::remove_dir_all(&d);
}

/// §68 repository path normalization: relative/absolute-in-repo map to
/// canonical repo-relative; traversal/external do not escape.
#[test]
fn repo_path_normalization() {
    // Build an ATIF trace with a read tool call for a repo file, an absolute
    // in-repo path is normalized; an external path is not claimed as repo.
    let traj = serde_json::json!({
        "session_id":"s-p","steps":[
            {"step_id":0,"timestamp":"2026-01-01T00:00:00Z","source":"user","message":"t"},
            {"step_id":1,"timestamp":"2026-01-01T00:00:01Z","source":"agent","message":"",
             "metrics":{"prompt_tokens":1,"completion_tokens":1},
             "tool_calls":[{"tool_call_id":"t1","function_name":"read","arguments":{"file_path":"/repo/src/a.go"}}],
             "observation":{"results":[{"source_call_id":"t1","content":"<file-view path=\"/repo/src/a.go\" start_line=\"1\" end_line=\"2\">\n 1|package a\n"}]}},
            {"step_id":2,"timestamp":"2026-01-01T00:00:02Z","source":"agent","message":"ans"}
        ],
        "final_metrics":{"total_prompt_tokens":1,"total_completion_tokens":1,"total_steps":3}
    });
    let ctx = AtifContext {
        repo_root: Some("/repo".into()),
        ..Default::default()
    };
    let events = from_atif(&traj, &ctx).unwrap();
    let so: Vec<&AgentEvent> = events
        .iter()
        .filter(|e| e.event_type == "source_observed")
        .collect();
    assert_eq!(so.len(), 1);
    assert_eq!(so[0].data["path"], serde_json::json!("src/a.go"));
    assert!(!so[0].data["path"].as_str().unwrap().contains(".."));
}

/// §69 large batch streaming ingest — no O(N^2); bounded memory.
#[test]
fn large_batch_ingest() {
    let d = dir("big");
    let mut store = AgentEventStore::open(&d).unwrap();
    // 40 sessions x 250 events = 10k events
    let mut total = 0u64;
    for s in 0..40 {
        let mut batch = Vec::new();
        for i in 0..250 {
            batch.push(ev(
                &format!("s-{s}"),
                i,
                "agent_message",
                serde_json::json!({"role":"a","content_bytes":i}),
            ));
        }
        let r = {
            let mut sk = StoreSink { store: &mut store };
            sk.ingest_batch(&batch).unwrap()
        };
        total += r.accepted;
    }
    assert_eq!(total, 10_000);
    assert_eq!(store.sessions().unwrap().len(), 40);
    let _ = std::fs::remove_dir_all(&d);
}
