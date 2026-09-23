//! InvestigationMemory contract tests (§59-§67): context-artifact joins,
//! same/unrelated query, changed/removed staleness, incremental-vs-rebuild,
//! late outcome, multi-session, hot-path bounded scaling.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use repodex::agent_event::{
    AgentEvent, AgentEventSink, AgentEventStore, SourceProvenance, StoreSink, AGENT_EVENT_SCHEMA,
};
use repodex::derived::{derive, DerivedStore};
use repodex::memory::*;

static N: AtomicU32 = AtomicU32::new(0);
fn dir(tag: &str) -> PathBuf {
    let i = N.fetch_add(1, Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("rmem-{tag}-{}-{i}", std::process::id()));
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
        repo_head: Some("h1".into()),
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
fn so(sid: &str, inv: &str, seq: u64, path: &str, kind: &str) -> AgentEvent {
    ev(
        sid,
        inv,
        seq,
        "source_observed",
        serde_json::json!({"path":path,"observation_kind":kind,"bytes":10}),
    )
}
fn q_ev(sid: &str, inv: &str, seq: u64, q: &str) -> AgentEvent {
    ev(
        sid,
        inv,
        seq,
        "agent_message",
        serde_json::json!({"role":"user","content":q}),
    )
}
fn ca(sid: &str, inv: &str, seq: u64, paths: &[(&str, u64)]) -> AgentEvent {
    let refs: Vec<serde_json::Value> = paths
        .iter()
        .map(|(p, r)| serde_json::json!({"reference_kind":"decl","path":p,"rank":r}))
        .collect();
    ev(
        sid,
        inv,
        seq,
        "context_artifact_presented",
        serde_json::json!({"artifact_id":"x","artifact_kind":"rdx1_packet",
            "producer":"repodex","references":refs}),
    )
}
fn fa(sid: &str, inv: &str, seq: u64, text: &str) -> AgentEvent {
    ev(
        sid,
        inv,
        seq,
        "final_answer",
        serde_json::json!({"content":text}),
    )
}

fn build(
    events: Vec<AgentEvent>,
    known: &BTreeSet<String>,
) -> (MemoryStore, PathBuf, PathBuf, PathBuf) {
    let sd = dir("s");
    let dd = dir("d");
    let md = dir("m");
    let mut st = AgentEventStore::open(&sd).unwrap();
    {
        let mut sk = StoreSink { store: &mut st };
        sk.ingest_batch(&events).unwrap();
    }
    st.update_manifest().unwrap();
    let (ds, _) = derive(&st, &dd, true, known, None).unwrap();
    ds.save(&dd).unwrap();
    let (ms, _) = build_memory(&ds, &md, true, known, known).unwrap();
    (ms, sd, dd, md)
}
fn known() -> BTreeSet<String> {
    ["a/b.go", "src/x.go", "pkg/y.go", "src/z.go"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// §59 context-artifact fixture: surfaced A,B,C; read A; observe D; answer
/// mentions A+D. Verify factual joins + no false negative for B/C.
#[test]
fn context_artifact_joins() {
    let (ms, sd, dd, md) = build(
        vec![
            q_ev("s1", "inv", 0, "what does Foo do"),
            ca(
                "s1",
                "inv",
                1,
                &[("a/b.go", 1), ("src/x.go", 2), ("pkg/y.go", 3)],
            ),
            so("s1", "inv", 2, "a/b.go", "explicit_read"),
            so("s1", "inv", 3, "src/z.go", "search_snippet"),
            fa("s1", "inv", 4, "answer uses a/b.go and src/z.go"),
        ],
        &known(),
    );
    let e = &ms.entries["inv"];
    let get = |p: &str| e.evidence.get(p).cloned();
    let a = get("a/b.go").unwrap();
    assert!(a.surfaced && a.observed && a.explicitly_read && a.mentioned_in_final_answer);
    assert_eq!(a.surfaced_rank, Some(1));
    let b = get("src/x.go").unwrap();
    assert!(b.surfaced && !b.observed && !b.mentioned_in_final_answer); // surfaced-only, neutral
    let c = get("pkg/y.go").unwrap();
    assert!(c.surfaced && !c.observed); // surfaced-only, NOT negative
    let d = get("src/z.go").unwrap();
    assert!(!d.surfaced && d.observed && d.mentioned_in_final_answer); // discovered
    for d in [sd, dd, md] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// §60 same-query history: memory finds older investigation from later query.
#[test]
fn same_query_history() {
    let q = "Where is ensureWorkerActionableSlot called";
    let (ms, sd, dd, md) = build(
        vec![
            q_ev("s1", "inv-old", 0, q),
            so("s1", "inv-old", 1, "a/b.go", "explicit_read"),
            fa("s1", "inv-old", 2, "see a/b.go"),
            q_ev(
                "s2",
                "inv-new",
                0,
                "where does ensureWorkerActionableSlot get invoked",
            ),
            so("s2", "inv-new", 1, "pkg/y.go", "explicit_read"),
            fa("s2", "inv-new", 2, "see pkg/y.go"),
        ],
        &known(),
    );
    let m = ms.query(
        "which paths call ensureWorkerActionableSlot",
        &known(),
        10,
        64,
    );
    // both should match (shared identifier); the older one is retrievable
    assert!(m.iter().any(|x| x.investigation_id == "inv-old"));
    for d in [sd, dd, md] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// §61 unrelated query: a globally-popular path must not surface candidates
/// for an unrelated investigation merely by popularity.
#[test]
fn unrelated_query_no_popularity_leak() {
    // inv1 heavily uses a/b.go; inv2 is a different question entirely
    let (ms, sd, dd, md) = build(
        vec![
            q_ev("s1", "inv-pop", 0, "workflow role planner definitions"),
            so("s1", "inv-pop", 1, "a/b.go", "explicit_read"),
            so("s1", "inv-pop", 2, "a/b.go", "explicit_read"),
            q_ev("s2", "inv-other", 0, "database migration ordering"),
            so("s2", "inv-other", 1, "pkg/y.go", "explicit_read"),
        ],
        &known(),
    );
    // query about a completely different topic -> inv-other, not inv-pop
    let m = ms.query("how are database migrations ordered", &known(), 10, 64);
    // a/b.go (popular) should not be a top candidate for the migration query
    let top_paths: Vec<_> = ms
        .query_paths("how are database migrations ordered", &known(), 10)
        .into_iter()
        .map(|(p, _, _, _)| p)
        .collect();
    assert!(
        m.iter().all(|x| x.investigation_id != "inv-pop")
            || !top_paths.iter().any(|p| p == "a/b.go")
    );
    for d in [sd, dd, md] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// §62 changed/§63 removed staleness: remembered path present->Fresh,
/// absent->Removed.
#[test]
fn staleness_fresh_removed() {
    let (ms, sd, dd, md) = build(
        vec![
            q_ev("s1", "inv", 0, "q"),
            so("s1", "inv", 1, "a/b.go", "explicit_read"),
        ],
        &known(),
    );
    let e = &ms.entries["inv"];
    assert_eq!(e.evidence["a/b.go"].freshness, Some(Freshness::Fresh));
    // removed: rebuild memory with a current_paths lacking a/b.go -> Removed
    let current: BTreeSet<String> = ["src/x.go"].iter().map(|s| s.to_string()).collect();
    let ds = repodex::derived::DerivedStore::load(&dd).unwrap();
    let (ms2, _) = build_memory(&ds, &dir("m2"), true, &known(), &current).unwrap();
    assert_eq!(
        ms2.entries["inv"].evidence["a/b.go"].freshness,
        Some(Freshness::Removed)
    );
    for d in [sd, dd, md] {
        let _ = std::fs::remove_dir_all(d);
    }
}
/// §64 incremental == rebuild for memory.
#[test]
fn memory_incremental_equiv() {
    let sd = dir("mi");
    let dd = dir("mid");
    let md_full = dir("mif");
    let md_inc = dir("mii");
    let base = vec![
        q_ev("s1", "inv", 0, "role definitions"),
        so("s1", "inv", 1, "a/b.go", "explicit_read"),
        fa("s1", "inv", 2, "a/b.go"),
    ];
    let mut st = AgentEventStore::open(&sd).unwrap();
    {
        let mut sk = StoreSink { store: &mut st };
        sk.ingest_batch(&base).unwrap();
    }
    st.update_manifest().unwrap();
    let (ds, _) = derive(&st, &dd, true, &known(), None).unwrap();
    ds.save(&dd).unwrap();
    // incremental memory on base
    let (msi, _) = build_memory(&ds, &md_inc, false, &known(), &known()).unwrap();
    msi.save(&md_inc).unwrap();
    // append a new session + late outcome
    {
        let mut sk = StoreSink { store: &mut st };
        sk.ingest_batch(&[
            ev(
                "s1",
                "inv",
                3,
                "task_outcome",
                serde_json::json!({"outcome":"review_accepted"}),
            ),
            q_ev("s2", "inv2", 0, "different question"),
            so("s2", "inv2", 1, "pkg/y.go", "explicit_read"),
        ])
        .unwrap();
    }
    st.update_manifest().unwrap();
    let (ds2, _) = derive(&st, &dd, false, &known(), None).unwrap();
    ds2.save(&dd).unwrap();
    let (msi2, _) = build_memory(&ds2, &md_inc, false, &known(), &known()).unwrap();
    // full rebuild for ground truth
    let (msf, _) = build_memory(&ds2, &md_full, true, &known(), &known()).unwrap();
    for inv in ["inv", "inv2"] {
        assert_eq!(
            serde_json::to_value(&msi2.entries[inv]).unwrap(),
            serde_json::to_value(&msf.entries[inv]).unwrap(),
            "incremental != rebuild for {inv}"
        );
    }
    // late outcome reflected in inv
    assert!(msf.entries["inv"]
        .task_outcomes
        .iter()
        .any(|o| o == "review_accepted"));
    for d in [sd, dd, md_full, md_inc] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// §67 hot-path scaling: many investigations touching one path must NOT grow
/// the hot PathActivity record with all session ids.
#[test]
fn hot_path_bounded() {
    let mut events = Vec::new();
    for i in 0..200 {
        events.push(q_ev(&format!("s{i}"), &format!("inv{i}"), 0, "q"));
        events.push(so(
            &format!("s{i}"),
            &format!("inv{i}"),
            1,
            "a/b.go",
            "explicit_read",
        ));
    }
    let (_ms, sd, dd, md) = build(events, &known());
    let ds = DerivedStore::load(&dd).unwrap();
    let a = &ds.activity.paths[&("repo".into(), "a/b.go".into())];
    assert_eq!(a.sessions_observed_count, 200);
    assert_eq!(a.investigations_observed_count, 200);
    // hot record serialized size stays bounded (no id arrays)
    let bytes = serde_json::to_string(a).unwrap().len();
    assert!(bytes < 8_000, "hot record grew unboundedly: {bytes} bytes");
    // contribution index holds exact membership separately
    let cm = &ds.activity.contrib[&("repo".into(), "a/b.go".into())];
    assert_eq!(cm.session_ids.len(), 200);
    for d in [sd, dd, md] {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// §66 multi-session: two sessions in one investigation don't inflate
/// investigation-level evidence.
#[test]
fn multi_session_no_inflation() {
    let (ms, sd, dd, md) = build(
        vec![
            q_ev("s1", "inv", 0, "q"),
            so("s1", "inv", 1, "a/b.go", "explicit_read"),
            q_ev("s2", "inv", 0, "q"),
            so("s2", "inv", 1, "a/b.go", "explicit_read"),
        ],
        &known(),
    );
    let e = &ms.entries["inv"];
    assert_eq!(e.session_ids.len(), 2);
    // one evidence record per path, not per session
    assert_eq!(e.evidence["a/b.go"].explicit_read_count, 2);
    for d in [sd, dd, md] {
        let _ = std::fs::remove_dir_all(d);
    }
}
