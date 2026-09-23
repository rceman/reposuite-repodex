//! Derived layer contract tests (§73-§80): multi-session investigation,
//! late events, repeated reads, search-vs-read, mention extraction,
//! incremental-vs-rebuild equivalence, determinism, corrupt checkpoint.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use repodex::agent_event::{
    AgentEvent, AgentEventSink, AgentEventStore, SourceProvenance, StoreSink, AGENT_EVENT_SCHEMA,
};
use repodex::derived::*;

static N: AtomicU32 = AtomicU32::new(0);
fn dir(tag: &str) -> PathBuf {
    let i = N.fetch_add(1, Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("rdv-{tag}-{}-{i}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn ev(sid: &str, inv: &str, seq: u64, ty: &str, data: serde_json::Value) -> AgentEvent {
    AgentEvent {
        schema: AGENT_EVENT_SCHEMA.into(),
        event_id: AgentEvent::make_event_id(sid, seq),
        session_id: sid.into(),
        investigation_id: Some(inv.into()),
        project_id: None,
        repository_id: Some("repo".into()),
        repo_head: Some("head1".into()),
        sequence: seq,
        timestamp: format!("2026-01-01T00:00:{seq:02}Z"),
        source: SourceProvenance {
            runtime: "t".into(),
            adapter: "t".into(),
            adapter_version: "1".into(),
            native_session_id: None,
        },
        event_type: ty.into(),
        data,
        content_digest: None,
    }
}
fn so(sid: &str, inv: &str, seq: u64, path: &str, kind: &str, bytes: u64) -> AgentEvent {
    ev(
        sid,
        inv,
        seq,
        "source_observed",
        serde_json::json!({"path":path,"observation_kind":kind,"bytes":bytes}),
    )
}
fn mc(sid: &str, inv: &str, seq: u64, input: u64, output: u64) -> AgentEvent {
    ev(
        sid,
        inv,
        seq,
        "model_call_completed",
        serde_json::json!({"model_call_id":format!("mc-{seq}"),"input_tokens":input,"output_tokens":output}),
    )
}

fn store_with(dir: &Path, events: Vec<AgentEvent>) -> AgentEventStore {
    let mut s = AgentEventStore::open(dir).unwrap();
    {
        let mut sink = StoreSink { store: &mut s };
        sink.ingest_batch(&events).unwrap();
    }
    s.update_manifest().unwrap();
    s
}
fn known() -> BTreeSet<String> {
    ["a/b.go", "src/x.go", "pkg/y.go"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// §73 multi-session investigation: one investigation, two sessions, aggregates
/// without double-counting.
#[test]
fn multi_session_investigation() {
    let sd = dir("ms");
    let dd = dir("msd");
    let events = vec![
        ev("sA", "invX", 0, "session_started", serde_json::json!({})),
        mc("sA", "invX", 1, 100, 10),
        so("sA", "invX", 2, "a/b.go", "explicit_read", 50),
        ev("sA", "invX", 3, "session_completed", serde_json::json!({})),
        ev("sB", "invX", 0, "session_started", serde_json::json!({})),
        mc("sB", "invX", 1, 200, 20),
        so("sB", "invX", 2, "a/b.go", "search_snippet", 20),
        ev("sB", "invX", 3, "session_completed", serde_json::json!({})),
    ];
    let st = store_with(&sd, events);
    let (ds, _r) = derive(&st, &dd, true, &known(), None).unwrap();
    let ep = ds.episodes.get("invX").unwrap();
    assert_eq!(ep.session_ids.len(), 2);
    assert_eq!(ep.input_tokens, Some(300));
    assert_eq!(ep.output_tokens, Some(30));
    assert_eq!(ep.unique_source_files_observed, 1); // a/b.go across both
                                                    // activity: same path, 2 sessions, 1 investigation
    let act = ds
        .activity
        .paths
        .get(&("repo".into(), "a/b.go".into()))
        .unwrap();
    assert_eq!(act.sessions_observed_count, 2);
    assert_eq!(act.investigations_observed_count, 1);
    assert_eq!(act.observation_event_count_total, 2);
    let _ = std::fs::remove_dir_all(&sd);
    let _ = std::fs::remove_dir_all(&dd);
}

/// §74 late event: build after session_completed, then append task_outcome and
/// incrementally update — investigation reflects the outcome.
#[test]
fn late_event_incremental() {
    let sd = dir("late");
    let dd = dir("lated");
    let st = store_with(
        &sd,
        vec![
            ev("sL", "invL", 0, "session_started", serde_json::json!({})),
            ev("sL", "invL", 1, "session_completed", serde_json::json!({})),
        ],
    );
    let (ds, _) = derive(&st, &dd, false, &known(), None).unwrap();
    ds.save(&dd).unwrap();
    assert_eq!(ds.episodes["invL"].state, InvestigationState::Completed);
    // append a late task_outcome to the same session
    let mut st = AgentEventStore::open(&sd).unwrap();
    {
        let mut sink = StoreSink { store: &mut st };
        sink.ingest_batch(&[ev(
            "sL",
            "invL",
            2,
            "task_outcome",
            serde_json::json!({"outcome":"review_accepted"}),
        )])
        .unwrap();
    }
    st.update_manifest().unwrap();
    let (ds2, rep) = derive(&st, &dd, false, &known(), None).unwrap();
    assert!(rep.sessions_changed >= 1, "late event triggers reprocess");
    let ep = &ds2.episodes["invL"];
    assert!(ep.task_outcomes.iter().any(|o| o == "review_accepted"));
    let _ = std::fs::remove_dir_all(&sd);
    let _ = std::fs::remove_dir_all(&dd);
}

/// §75 repeated read: same path observed many times -> event count, distinct
/// session/investigation counts, first/last correct.
#[test]
fn repeated_read_counts() {
    let sd = dir("rep");
    let dd = dir("repd");
    let events = vec![
        so("s1", "inv", 1, "a/b.go", "explicit_read", 10),
        so("s1", "inv", 5, "a/b.go", "explicit_read", 10),
        so("s1", "inv", 9, "a/b.go", "explicit_read", 10),
        so("s2", "inv", 1, "a/b.go", "explicit_read", 10),
    ];
    let st = store_with(&sd, events);
    let (ds, _) = derive(&st, &dd, true, &known(), None).unwrap();
    let a = &ds.activity.paths[&("repo".into(), "a/b.go".into())];
    assert_eq!(a.observation_event_count_total, 4);
    assert_eq!(a.explicit_read_count_total, 4);
    assert_eq!(a.sessions_observed_count, 2);
    assert_eq!(a.investigations_observed_count, 1);
    let ex = &ds.episodes["inv"].source_exposures[0];
    assert_eq!(ex.observation_event_count, 4);
    assert_eq!(ex.first_observed_sequence, 1);
    assert_eq!(ex.last_observed_sequence, 9);
    let _ = std::fs::remove_dir_all(&sd);
    let _ = std::fs::remove_dir_all(&dd);
}

/// §76 search-vs-read: same path via snippet then explicit read -> separate
/// counters.
#[test]
fn search_vs_read() {
    let sd = dir("svr");
    let dd = dir("svrd");
    let st = store_with(
        &sd,
        vec![
            so("s1", "inv", 1, "a/b.go", "search_snippet", 8),
            so("s1", "inv", 2, "a/b.go", "explicit_read", 40),
        ],
    );
    let (ds, _) = derive(&st, &dd, true, &known(), None).unwrap();
    let a = &ds.activity.paths[&("repo".into(), "a/b.go".into())];
    assert_eq!(a.search_snippet_count_total, 1);
    assert_eq!(a.explicit_read_count_total, 1);
    let ex = &ds.episodes["inv"].source_exposures[0];
    assert_eq!(ex.search_snippet_count, 1);
    assert_eq!(ex.explicit_read_count, 1);
    let _ = std::fs::remove_dir_all(&sd);
    let _ = std::fs::remove_dir_all(&dd);
}

/// §77 mention: valid canonical path matched; invalid path-like + basename go
/// to unresolved, not claimed.
#[test]
fn mention_extraction_conservative() {
    let text = "Fix is in a/b.go and src/x.go. Maybe bogus/deep/nothere.go or x.go alone.";
    let (m, _e, un) = extract_path_mentions(text, &known(), Some("/repo"));
    assert!(m.contains(&"a/b.go".to_string()));
    assert!(m.contains(&"src/x.go".to_string()));
    assert!(!m.iter().any(|p| p.contains("nothere")));
    // bogus/deep/nothere.go is path-like but not a known path -> unresolved
    assert!(un.iter().any(|p| p.contains("bogus")));
}

/// §78 incremental == rebuild: partial build + increments must equal full.
#[test]
fn incremental_equiv_rebuild() {
    let sd = dir("eq");
    let dd_full = dir("eqf");
    let dd_inc = dir("eqi");
    let base = vec![
        ev("s1", "inv", 0, "session_started", serde_json::json!({})),
        mc("s1", "inv", 1, 100, 10),
        so("s1", "inv", 2, "a/b.go", "explicit_read", 30),
    ];
    let st = store_with(&sd, base);
    // full build
    let (dsf, _) = derive(&st, &dd_full, true, &known(), None).unwrap();
    // incremental: build only base, then append + update
    let (dsi, _) = derive(&st, &dd_inc, false, &known(), None).unwrap();
    dsi.save(&dd_inc).unwrap();
    // append more events to session + a new session
    let mut st = AgentEventStore::open(&sd).unwrap();
    {
        let mut sink = StoreSink { store: &mut st };
        sink.ingest_batch(&[
            so("s1", "inv", 3, "src/x.go", "search_snippet", 9),
            ev("s1", "inv", 4, "session_completed", serde_json::json!({})),
            ev("s2", "inv", 0, "session_started", serde_json::json!({})),
            mc("s2", "inv", 1, 50, 5),
            so("s2", "inv", 2, "pkg/y.go", "explicit_read", 12),
            ev("s2", "inv", 3, "session_completed", serde_json::json!({})),
        ])
        .unwrap();
    }
    st.update_manifest().unwrap();
    // incremental update on the partial store
    let (dsi2, _) = derive(&st, &dd_inc, false, &known(), None).unwrap();
    dsi2.save(&dd_inc).unwrap();
    // full rebuild on the SAME final store for ground truth
    let (dsf2, _) = derive(&st, &dd_full, true, &known(), None).unwrap();
    let a = serde_json::to_value(&dsi2.episodes["inv"]).unwrap();
    let b = serde_json::to_value(&dsf2.episodes["inv"]).unwrap();
    assert_eq!(a, b, "incremental != full rebuild");
    // compare activity too
    let ja = serde_json::to_value(dsi2.activity.paths.values().collect::<Vec<_>>()).unwrap();
    let jb = serde_json::to_value(dsf2.activity.paths.values().collect::<Vec<_>>()).unwrap();
    assert_eq!(ja, jb, "activity incremental != full");
    let _ = (dsf, dsf2);
    let _ = std::fs::remove_dir_all(&sd);
    let _ = std::fs::remove_dir_all(&dd_full);
    let _ = std::fs::remove_dir_all(&dd_inc);
}

/// §79 determinism: same store -> identical derived output.
#[test]
fn determinism() {
    let sd = dir("det");
    let d1 = dir("det1");
    let d2 = dir("det2");
    let st = store_with(
        &sd,
        vec![
            mc("s1", "inv", 0, 100, 10),
            so("s1", "inv", 1, "a/b.go", "explicit_read", 30),
            ev(
                "s1",
                "inv",
                2,
                "final_answer",
                serde_json::json!({"content":"see a/b.go"}),
            ),
        ],
    );
    let (a, _) = derive(&st, &d1, true, &known(), None).unwrap();
    let (b, _) = derive(&st, &d2, true, &known(), None).unwrap();
    assert_eq!(
        serde_json::to_value(&a.episodes["inv"]).unwrap(),
        serde_json::to_value(&b.episodes["inv"]).unwrap()
    );
    let _ = std::fs::remove_dir_all(&sd);
    let _ = std::fs::remove_dir_all(&d1);
    let _ = std::fs::remove_dir_all(&d2);
}

/// §80 corrupt checkpoint -> explicit verification failure (no silent rebuild).
#[test]
fn corrupt_checkpoint_detected() {
    let sd = dir("cor");
    let dd = dir("cord");
    let st = store_with(&sd, vec![mc("s1", "inv", 0, 1, 1)]);
    let (ds, _) = derive(&st, &dd, false, &known(), None).unwrap();
    ds.save(&dd).unwrap();
    // tamper: corrupt the checkpoint file
    std::fs::write(dd.join("checkpoint.json"), "not json").unwrap();
    let r = derive(&st, &dd, false, &known(), None);
    assert!(
        r.is_err(),
        "corrupt checkpoint must fail, not silently rebuild"
    );
    let _ = std::fs::remove_dir_all(&sd);
    let _ = std::fs::remove_dir_all(&dd);
}
