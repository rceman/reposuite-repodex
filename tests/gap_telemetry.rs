//! Production Gap Telemetry V1 tests (§33, §39-§41, §61):
//! context-artifact schema, v1 compat, correlation, gap normalization,
//! sequence windows, session/repository isolation, idempotent replay,
//! timestamp robustness, control sessions, RDX ablation, failure isolation.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

mod support;

use repodex::agent_event::fallback::{analyze_session, report, FollowupDisposition};
use repodex::agent_event::ingest::{validate_event, StoreSink};
use repodex::agent_event::model::*;
use repodex::agent_event::store::AgentEventStore;
use repodex::agent_event::AgentEventSink;

static N: AtomicU32 = AtomicU32::new(0);
fn dir(tag: &str) -> PathBuf {
    let i = N.fetch_add(1, Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!("gpt-{tag}-{}-{i}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn ev(sid: &str, seq: u64, ty: &str, data: serde_json::Value) -> AgentEvent {
    AgentEvent {
        schema: AGENT_EVENT_SCHEMA.into(),
        event_id: AgentEvent::make_event_id(sid, seq),
        session_id: sid.into(),
        investigation_id: Some("inv".into()),
        project_id: None,
        repository_id: Some("repo".into()),
        repo_head: Some("h1".into()),
        sequence: seq,
        timestamp: "2026-10-04T00:00:00Z".into(),
        source: SourceProvenance {
            runtime: "test".into(),
            adapter: "t".into(),
            adapter_version: "1".into(),
            native_session_id: None,
        },
        event_type: ty.into(),
        data,
        content_digest: None,
    }
}

fn artifact(seq: u64, tcid: &str, gaps: serde_json::Value) -> AgentEvent {
    ev(
        "s",
        seq,
        "context_artifact_presented",
        serde_json::json!({
            "artifact_id": format!("a-{seq}"),
            "artifact_kind": "rdx1_packet",
            "producer": "repodex",
            "tool_call_id": tcid,
            "presentation": "adaptive",
            "content_digest": "sha256:x",
            "content_bytes": 700,
            "metadata": {
                "query_intent": "callers",
                "navigation_profile": "adaptive",
                "gap_signatures": gaps,
                "seed_count": 3,
                "candidate_count": 1
            }
        }),
    )
}

fn tool(seq: u64, id: &str, name: &str, cat: &str) -> AgentEvent {
    ev(
        "s",
        seq,
        "tool_call_started",
        serde_json::json!({"tool_call_id": id, "tool_name": name, "category": cat}),
    )
}

#[test]
fn artifact_event_is_accepted_and_typed() {
    let e = artifact(1, "tc-9", serde_json::json!([]));
    validate_event(&e).unwrap();
    let ca: ContextArtifactPresented = serde_json::from_value(e.data).unwrap();
    assert_eq!(ca.producer, "repodex");
    assert_eq!(ca.tool_call_id.as_deref(), Some("tc-9"));
    assert_eq!(ca.presentation.as_deref(), Some("adaptive"));
    assert!(ca.metadata.is_some());
}

#[test]
fn old_v1_artifact_without_new_fields_round_trips() {
    // old schema lacked tool_call_id/presentation/metadata — still parses.
    let data = serde_json::json!({
        "artifact_id": "a", "artifact_kind": "rdx1_packet", "producer": "repodex",
        "references": []
    });
    let ca: ContextArtifactPresented = serde_json::from_value(data).unwrap();
    assert!(ca.tool_call_id.is_none() && ca.metadata.is_none());
}

#[test]
fn unknown_future_event_type_still_ingests() {
    let d = dir("unk");
    let mut store = AgentEventStore::open(&d).unwrap();
    let e = ev("s-unk", 0, "future_event_2099", serde_json::json!({"x":1}));
    let mut sink = StoreSink { store: &mut store };
    let r = sink.ingest_batch(&[e]).unwrap();
    assert_eq!(r.accepted, 1);
    let got = &store.session_events("s-unk").unwrap()[0];
    assert_eq!(got.event_type, "future_event_2099");
    assert_eq!(got.data["x"], 1);
}

#[test]
fn malformed_envelope_rejected() {
    let mut e = artifact(0, "t", serde_json::json!([]));
    e.schema = "wrong".into();
    assert!(validate_event(&e).is_err());
}

#[test]
fn verification_only_disposition() {
    let s = vec![
        artifact(0, "tc-a", serde_json::json!([])),
        tool(1, "r1", "read", "file_read"),
        ev(
            "s",
            2,
            "source_observed",
            serde_json::json!({"path":"a.php","observation_kind":"explicit_read","tool_call_id":"r1"}),
        ),
    ];
    let a = analyze_session(&s);
    assert_eq!(a.len(), 1);
    assert_eq!(
        a[0].disposition,
        FollowupDisposition::SourceVerificationOnly
    );
    assert!(a[0].verified_before);
}

#[test]
fn native_search_after_representation_is_fallback() {
    let s = vec![
        artifact(0, "tc-a", serde_json::json!([])),
        tool(1, "r1", "read", "file_read"),
        tool(2, "g1", "grep", "search"),
    ];
    let a = analyze_session(&s);
    assert_eq!(
        a[0].disposition,
        FollowupDisposition::NativeDiscoveryAfterArtifact
    );
    assert!(a[0].verified_before);
}

#[test]
fn producer_tool_call_is_not_native_discovery() {
    // the RepoDex exec call itself is correlated and must not count
    let s = vec![
        tool(0, "tc-a", "exec", "shell"),
        artifact(1, "tc-a", serde_json::json!([])),
        ev("s", 2, "session_completed", serde_json::json!({})),
    ];
    let a = analyze_session(&s);
    assert_eq!(a[0].disposition, FollowupDisposition::NoFollowupDiscovery);
}

#[test]
fn second_representation_is_another_artifact() {
    let s = vec![
        artifact(0, "tc-a", serde_json::json!([])),
        artifact(1, "tc-b", serde_json::json!([])),
        ev("s", 2, "session_completed", serde_json::json!({})),
    ];
    let a = analyze_session(&s);
    assert_eq!(a.len(), 2);
    assert_eq!(
        a[0].disposition,
        FollowupDisposition::AnotherContextArtifact
    );
    assert_eq!(a[1].disposition, FollowupDisposition::NoFollowupDiscovery);
}

#[test]
fn task_action_ends_window() {
    // normalized `test` op on a shell call ends the window as task action
    let s = vec![
        artifact(0, "tc-a", serde_json::json!([])),
        ev(
            "s",
            1,
            "tool_call_started",
            serde_json::json!({"tool_call_id":"e1","tool_name":"exec","category":"shell","repository_operation":"test"}),
        ),
        tool(2, "g1", "grep", "search"), // after window close: must not count
    ];
    let a = analyze_session(&s);
    assert_eq!(a[0].disposition, FollowupDisposition::TaskActionStarted);
}

#[test]
fn legacy_shell_is_unclassified_not_task_action() {
    let s = vec![
        artifact(0, "tc-a", serde_json::json!([])),
        tool(1, "e1", "exec", "shell"),
    ];
    let a = analyze_session(&s);
    assert_eq!(
        a[0].disposition,
        FollowupDisposition::UnclassifiedRepositoryActivity
    );
}

#[test]
fn shell_discovery_op_is_native_fallback() {
    let s = vec![
        artifact(0, "tc-a", serde_json::json!([])),
        ev(
            "s",
            1,
            "tool_call_started",
            serde_json::json!({"tool_call_id":"e1","tool_name":"exec","category":"shell","repository_operation":"discovery_search"}),
        ),
    ];
    let a = analyze_session(&s);
    assert_eq!(
        a[0].disposition,
        FollowupDisposition::NativeDiscoveryAfterArtifact
    );
}

#[test]
fn other_repository_tool_with_discovery_op_is_fallback() {
    let s = vec![
        artifact(0, "tc-a", serde_json::json!([])),
        ev(
            "s",
            1,
            "tool_call_started",
            serde_json::json!({"tool_call_id":"t1","tool_name":"repo_search","category":"other_repository_tool","repository_operation":"discovery_search"}),
        ),
    ];
    let a = analyze_session(&s);
    assert_eq!(
        a[0].disposition,
        FollowupDisposition::NativeDiscoveryAfterArtifact
    );
}

#[test]
fn source_read_op_is_verification() {
    let s = vec![
        artifact(0, "tc-a", serde_json::json!([])),
        ev(
            "s",
            1,
            "tool_call_started",
            serde_json::json!({"tool_call_id":"t1","tool_name":"open","category":"file_read","repository_operation":"source_read"}),
        ),
    ];
    let a = analyze_session(&s);
    assert_eq!(
        a[0].disposition,
        FollowupDisposition::SourceVerificationOnly
    );
}

#[test]
fn report_counts_reconcile() {
    let s = vec![
        artifact(
            0,
            "tc-a",
            serde_json::json!([{"family":"out_of_scope","reason_code":"late_static_binding"},{"family":"no_candidate","reason_code":"receiver_type_unavailable"}]),
        ),
        artifact(1, "tc-b", serde_json::json!([])),
    ];
    let r = report(&[("s".into(), s)]);
    assert_eq!(
        r.presentations_with_gap + r.presentations_without_gap,
        r.artifact_presentations
    );
    // overlapping families: sum(by_gap_family) > presentations is expected
    assert_eq!(r.by_gap_family["out_of_scope"].presentations, 1);
    assert_eq!(r.by_gap_family["no_candidate"].presentations, 1);
    assert_eq!(r.presentations_with_gap, 1);
}

#[test]
fn out_of_order_store_is_sorted_by_sequence() {
    // same/late timestamps don't matter — sequence is authoritative
    let mut s = vec![
        artifact(0, "tc-a", serde_json::json!([])),
        tool(1, "g1", "grep", "search"),
    ];
    s[1].timestamp = "1970-01-01T00:00:00Z".into(); // earlier wall time
    s.reverse();
    let a = analyze_session(&s);
    assert_eq!(
        a[0].disposition,
        FollowupDisposition::NativeDiscoveryAfterArtifact
    );
}

#[test]
fn sessions_are_isolated() {
    // artifact in session A + native search in session B must not associate
    let sa = vec![artifact(0, "tc-a", serde_json::json!([]))];
    let mut b = tool(0, "g1", "grep", "search");
    b.session_id = "s-b".into();
    b.event_id = AgentEvent::make_event_id("s-b", 0);
    let r = report(&[("s".into(), sa), ("s-b".into(), vec![b])]);
    assert_eq!(r.by_disposition.native_discovery_after, 0);
    assert_eq!(r.sessions_without_artifacts, 1);
    assert_eq!(r.false_repodex_associations, 0);
}

#[test]
fn control_session_without_representation_yields_no_association() {
    let s = vec![
        tool(0, "g1", "grep", "search"),
        tool(1, "g2", "grep", "search"),
    ];
    let r = report(&[("s".into(), s)]);
    assert_eq!(r.artifact_presentations, 0);
    assert_eq!(r.sessions_without_artifacts, 1);
    assert_eq!(r.false_repodex_associations, 0);
}

#[test]
fn duplicate_replay_does_not_double_count() {
    let d = dir("dup");
    let mut store = AgentEventStore::open(&d).unwrap();
    let evs = vec![
        artifact(0, "tc-a", serde_json::json!([])),
        tool(1, "g1", "grep", "search"),
    ];
    {
        let mut sink = StoreSink { store: &mut store };
        sink.ingest_batch(&evs).unwrap();
    }
    let s1 = store.session_events("s").unwrap();
    let r1 = report(&[("s".into(), s1.clone())]);
    // replay identical batch: deduped, report identical
    {
        let mut sink = StoreSink { store: &mut store };
        let rep = sink.ingest_batch(&evs).unwrap();
        assert_eq!(rep.duplicates, 2);
        assert_eq!(rep.accepted, 0);
    }
    let s2 = store.session_events("s").unwrap();
    assert_eq!(s1.len(), s2.len());
    let r2 = report(&[("s".into(), s2)]);
    assert_eq!(
        serde_json::to_value(&r1).unwrap(),
        serde_json::to_value(&r2).unwrap()
    );
}

#[test]
fn report_buckets_have_denominators() {
    let s = vec![
        artifact(
            0,
            "tc-a",
            serde_json::json!([{"family":"out_of_scope","reason_code":"late_static_binding"}]),
        ),
        tool(1, "g1", "grep", "search"),
    ];
    let r = report(&[("s".into(), s)]);
    let bucket = &r.by_gap_reason_code["out_of_scope:late_static_binding"];
    assert_eq!(bucket.presentations, 1);
    assert_eq!(bucket.native_discovery_after, 1);
    assert_eq!(r.by_intent["callers"].native_discovery_after, 1);
}

// ---------------------------------------------------------------------------
// Production-path tests (CORRECTION-V1 §28-§30): gap signatures must be
// GENERATED from real fixture query execution — candidate artifact -> graph
// structured disposition -> EvidenceProjection -> RepoQueryObservation.
// ---------------------------------------------------------------------------

fn build_phpnav_observation(question: &str) -> repodex::query::observation::RepoQueryObservation {
    let temp = support::TempDir::new("gpt-phpnav");
    let root = support::fixture("phpnav");
    let snap = temp.path().join("snap");
    repodex::repository::build_snapshot(
        &support::analyzer(),
        &root,
        &snap,
        repodex::repository::BuildOptions::default(),
    )
    .expect("snapshot");
    let links = temp.path().join("links");
    repodex::links::build_links(&snap, Some(&root), &links).expect("links");
    let cand = temp.path().join("cand");
    repodex::candidates::build_candidates(&snap, &links, &cand).expect("candidates");
    let gdir = temp.path().join("graph");
    repodex::graph::build_graph(&snap, &links, &cand, &gdir).expect("graph");
    let index = repodex::graph::GraphIndex::load(&gdir).expect("graph index");
    let p = repodex::query::adaptive::plan(question, None, None, None);
    let plan = repodex::query::QueryPlan::parse(
        question,
        repodex::query::QueryMode::Exhaustive,
        Some(p.query_intent),
        p.target.clone(),
        p.to.clone(),
        50,
        None,
    );
    let engine = repodex::query::QueryEngine::new(&index);
    let result = engine.run(&plan);
    let proj = repodex::query::projection::build(&result, Some(&gdir));
    let (packet, rendered) = repodex::query::adaptive::adaptive_rdx_observed(&proj, p.nav, None);
    repodex::query::observation::observe(&proj, p.nav, "adaptive", &packet, Some(&rendered), None)
}

fn gap_set(
    obs: &repodex::query::observation::RepoQueryObservation,
) -> Vec<(String, Option<String>)> {
    obs.gap_signatures
        .iter()
        .map(|g| (g.family.clone(), g.reason_code.clone()))
        .collect()
}

#[test]
fn production_query_generates_out_of_scope_reason_codes() {
    // `callers method run` on phpnav: real out_of_scope dispositions
    // (receiver_type_unavailable, dynamic_member_name, ...) reach the
    // observation as normalized reason codes — never by hand.
    let obs = build_phpnav_observation("callers method run");
    let gaps = gap_set(&obs);
    assert!(
        gaps.contains(&(
            "out_of_scope".into(),
            Some("receiver_type_unavailable".into())
        )),
        "receiver_type_unavailable missing: {gaps:?}"
    );
    assert!(
        gaps.iter().any(|(f, _)| f == "out_of_scope"
            && gaps.contains(&(f.clone(), Some("dynamic_member_name".into())))),
        "dynamic_member_name missing: {gaps:?}"
    );
}

#[test]
fn production_query_generates_no_candidate_and_late_static() {
    let obs = build_phpnav_observation("callers method save");
    let gaps = gap_set(&obs);
    assert!(
        gaps.contains(&("out_of_scope".into(), Some("late_static_binding".into()))),
        "late_static_binding missing: {gaps:?}"
    );
    assert!(
        gaps.iter().any(|(f, _)| f == "no_candidate"),
        "no_candidate family missing: {gaps:?}"
    );
}

#[test]
fn generated_metadata_flows_into_artifact_and_report() {
    // End-to-end: real observation -> context_artifact_presented.metadata
    // -> fallback report bucket.
    let obs = build_phpnav_observation("callers method save");
    let meta = obs.event_metadata();
    let s = vec![
        tool(0, "tc-a", "exec", "shell"),
        ev(
            "s",
            1,
            "context_artifact_presented",
            serde_json::json!({
                "artifact_id": "a-1", "artifact_kind": "rdx1_packet",
                "producer": "repodex", "tool_call_id": "tc-a",
                "presentation": "adaptive", "metadata": meta,
            }),
        ),
        ev(
            "s",
            2,
            "tool_call_started",
            serde_json::json!({"tool_call_id":"g1","tool_name":"exec","category":"shell","repository_operation":"discovery_search"}),
        ),
    ];
    let r = report(&[("s".into(), s)]);
    let bucket = &r.by_gap_reason_code["out_of_scope:late_static_binding"];
    assert_eq!(bucket.presentations, 1);
    assert_eq!(bucket.native_discovery_after, 1);
    assert_eq!(r.presentations_with_gap, 1);
    assert_eq!(
        r.presentations_with_gap + r.presentations_without_gap,
        r.artifact_presentations
    );
}
