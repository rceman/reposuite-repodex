//! Native safety gates for continuous project learning (§40,§47).

use repodex::graph::{GraphIndex, GraphManifest, GraphNode, NodeKind};
use repodex::learning::*;

fn empty_index() -> GraphIndex {
    GraphIndex::from_parts(mani(), vec![], vec![])
}
fn index_with(nodes: Vec<GraphNode>) -> GraphIndex {
    GraphIndex::from_parts(mani(), nodes, vec![])
}
fn mani() -> GraphManifest {
    GraphManifest {
        graph_manifest_version: 1,
        graph_schema_version: 1,
        graph_policy_version: 1,
        snapshot_digest: String::new(),
        link_digest: String::new(),
        candidate_digest: String::new(),
        graph_fingerprint: String::new(),
        graph_fingerprint_text: String::new(),
        graph_digest: String::new(),
        nodes: 0,
        edges: 0,
        node_counts: vec![],
        edge_counts: vec![],
        languages: vec![],
    }
}
fn node(key: &str, label: &str) -> GraphNode {
    GraphNode {
        node_id: repodex::graph::model::node_id_for(key),
        key: key.into(),
        kind: NodeKind::Declaration,
        language: "go".into(),
        path: key
            .trim_start_matches("decl:")
            .split('#')
            .next()
            .unwrap_or("")
            .into(),
        label: label.into(),
        disposition: None,
    }
}

fn mem(anchors: &[&str], deps: &[&str], view: &str) -> MemoryArtifact {
    MemoryArtifact {
        memory_id: "m1".into(),
        family: QuestionFamily::CallPath,
        anchor_names: anchors.iter().map(|s| s.to_string()).collect(),
        route: vec!["call_candidate".into()],
        evidence_classes: vec!["candidate".into()],
        source_dependencies: deps.iter().map(|s| s.to_string()).collect(),
        origin_view: view.into(),
        last_validated_view: view.into(),
        state: MemoryState::Candidate,
        reuse_count: 0,
        useful_reuse_count: 0,
        failed_reuse_count: 0,
    }
}

#[test]
fn coverage_states_bounded_and_na_has_reason() {
    let idx = empty_index();
    let inv = ProjectInventory::default();
    let led = coverage(&idx, &inv, "v1");
    for it in &led.items {
        if it.state == CoverageState::NotApplicable {
            assert!(it.reason.is_some(), "NOT_APPLICABLE needs a reason");
        }
    }
    // every item is view-bound
    assert!(led.items.iter().all(|i| i.view_digest == "v1"));
}

#[test]
fn questions_only_from_gaps_not_curiosity() {
    let idx = empty_index();
    let mut inv = ProjectInventory::default();
    inv.entrypoints.push("cmd/main.go:main".into());
    inv.manifests.push("go.mod".into());
    let led = coverage(&idx, &inv, "v1");
    let qs = generate_questions(&led, &inv);
    // every generated question carries a concrete gap origin
    assert!(qs.iter().all(|q| !q.origin.is_empty()));
    // and a deterministic priority ordering (descending)
    for w in qs.windows(2) {
        assert!(w[0].priority >= w[1].priority);
    }
}

#[test]
fn missing_anchor_is_missing_not_reused() {
    let idx = empty_index();
    let m = mem(&["GoneSymbol"], &[], "v1");
    assert_eq!(rebind_memory(&m, &idx, "v1"), MemoryRebind::Missing);
}

#[test]
fn ambiguous_anchor_is_ambiguous() {
    let idx = index_with(vec![node("decl:a.go#1", "Dup"), node("decl:b.go#1", "Dup")]);
    let m = mem(&["Dup"], &[], "v1");
    assert_eq!(rebind_memory(&m, &idx, "v1"), MemoryRebind::Ambiguous);
}

#[test]
fn moved_same_anchor_rebinds_valid() {
    // same label present once in current view -> CurrentValid (rebind safe).
    let idx = index_with(vec![node("decl:new.go#1", "Get")]);
    let m = mem(&["Get"], &[], "v1");
    assert_eq!(rebind_memory(&m, &idx, "v1"), MemoryRebind::CurrentValid);
}

#[test]
fn changed_view_stale_not_current() {
    let idx = empty_index();
    let mut m = mem(&["X"], &["dep-digest"], "v1");
    m.last_validated_view = "v1".into();
    // same view digest but deps present + different view -> revalidate => Stale
    let r = rebind_memory(&m, &idx, "v2");
    assert!(matches!(r, MemoryRebind::Missing | MemoryRebind::Stale));
}

#[test]
fn rebind_never_upgrades_candidate_to_fact() {
    // rebind returns a state, never a FACT upgrade; usable only if CurrentValid.
    assert!(!MemoryRebind::Stale.usable_current());
    assert!(!MemoryRebind::Ambiguous.usable_current());
    assert!(!MemoryRebind::Missing.usable_current());
    assert!(MemoryRebind::CurrentValid.usable_current());
}

#[test]
fn promotion_is_conservative() {
    let mut m = mem(&["X"], &[], "v1");
    assert_eq!(m.state, MemoryState::Candidate); // never auto-active
    promote(&mut m, false);
    assert_eq!(m.state, MemoryState::Candidate); // no reuse -> stays candidate
    promote(&mut m, true);
    assert_eq!(m.state, MemoryState::Validated); // successful reuse -> validated
}

#[test]
fn demotion_on_failed_rebind() {
    let mut m = mem(&["X"], &[], "v1");
    m.state = MemoryState::Validated;
    demote(&mut m, MemoryRebind::Missing, "v2");
    assert_eq!(m.state, MemoryState::Stale);
    demote(&mut m, MemoryRebind::Missing, "v2");
    demote(&mut m, MemoryRebind::Missing, "v2");
    assert_eq!(m.state, MemoryState::Demoted); // repeated failures -> demoted
}

#[test]
fn priority_is_deterministic() {
    assert_eq!(
        question_priority(4, 3, 2, 1, 0),
        question_priority(4, 3, 2, 1, 0)
    );
    assert!(question_priority(5, 3, 2, 1, 0) > question_priority(1, 3, 2, 1, 0));
}

// --- unique-anchor recipe enforcement (precursor defect B) ---
// covered by the exec.rs change: multiple same-label declaration anchors now
// yield Ambiguous, not an arbitrary first-match execution.

// ---- correction-V1 gates (dependency-aware rebind + reproducibility) ----

#[test]
fn dep_aware_rebind_unrelated_change_stays_valid() {
    // anchor + dep both still resolve, view digest differs -> CurrentPartial
    // (not Stale) since the actual dependency identities are intact.
    let idx = index_with(vec![node("decl:a.go#1", "Get")]);
    let mut m = mem(&["Get"], &["decl:a.go#1"], "v1");
    m.last_validated_view = "v1".into();
    assert_eq!(rebind_memory(&m, &idx, "v2"), MemoryRebind::CurrentPartial);
    // same view digest -> fully current.
    assert_eq!(rebind_memory(&m, &idx, "v1"), MemoryRebind::CurrentValid);
}

#[test]
fn dep_aware_rebind_missing_dep_is_stale() {
    // anchor resolves but a stored dependency node is gone -> Stale.
    let idx = index_with(vec![node("decl:a.go#1", "Get")]);
    let m = mem(&["Get"], &["decl:gone.go#9"], "v1");
    assert_eq!(rebind_memory(&m, &idx, "v1"), MemoryRebind::Stale);
}

#[test]
fn memory_index_bounded_lookup() {
    // 10k records; candidates() must be bounded by family/anchor, not a scan.
    let arts: Vec<MemoryArtifact> = (0..10_000)
        .map(|i| {
            let mut m = mem(
                &[Box::leak(format!("a{}", i % 97).into_boxed_str())],
                &[],
                "v1",
            );
            m.memory_id = format!("m{i}");
            m
        })
        .collect();
    let idx = MemoryIndex::build(arts);
    let c = idx.candidates(QuestionFamily::CallPath, &["a0".to_string()]);
    // bounded by the anchor/family fan-out, not the full 10k.
    assert!(c.len() < 10_000);
    assert_eq!(idx.len(), 10_000);
}

#[test]
fn curriculum_generation_is_reproducible() {
    let idx = empty_index();
    let mut inv = ProjectInventory::default();
    inv.entrypoints.push("cmd/main.go:main".into());
    inv.manifests.push("go.mod".into());
    inv.modules.push("engine".into());
    let led = coverage(&idx, &inv, "v1");
    let a = generate_questions(&led, &inv);
    let b = generate_questions(&led, &inv);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}

#[test]
fn internal_route_only_bias_current_valid() {
    let idx = index_with(vec![node("decl:a.go#1", "Get")]);
    let arts = vec![{
        let mut m = mem(&["Get"], &[], "v1");
        m.route = vec!["call_candidate".into()];
        m.state = MemoryState::Validated;
        m
    }];
    let mi = MemoryIndex::build(arts);
    let d = internal_route(
        &mi,
        &idx,
        repodex::query::adaptive::NavIntent::Callers,
        &["Get".to_string()],
        "v1",
    );
    assert!(d.applied);
    assert_eq!(d.rebind, Some(MemoryRebind::CurrentValid));
    assert!(d.bias.prefer_kinds.contains(&"call_candidate".to_string()));
}
