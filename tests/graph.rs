//! TASK 5A: investigation-graph projection.
//!
//! Builds a mixed Rust+Go repo -> snapshot -> links -> candidates -> graph,
//! then asserts the projected nodes/edges, the FACT/CANDIDATE split, candidate
//! sets, dispositions and bidirectional traversal.

mod support;

use repodex::candidates::build_candidates;
use repodex::graph::{build_graph, EvidenceClass, GraphIndex, NodeKind};
use repodex::links::build_links;
use repodex::repository::{build_snapshot, BuildOptions};
use support::TempDir;

/// Build the full pipeline for a mixed repo. Returns (tempdir, GraphIndex).
fn graph_of(files: &[(&str, &str)]) -> (TempDir, GraphIndex) {
    let t = TempDir::new("t5a");
    let repo = t.path().join("repo");
    for (rel, src) in files {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    let snap = t.path().join("snap");
    build_snapshot(&support::analyzer(), &repo, &snap, BuildOptions::default()).expect("snapshot");
    let links = t.path().join("links");
    build_links(&snap, Some(&repo), &links).expect("links");
    let cand = t.path().join("cand");
    build_candidates(&snap, &links, &cand).expect("candidates");
    let g = t.path().join("graph");
    build_graph(&snap, &links, &cand, &g).expect("graph");
    (t, GraphIndex::load(&g).expect("load"))
}

/// The graph node for a call site identified by file + written callee.
fn call_node<'a>(g: &'a GraphIndex, path: &str, written: &str) -> &'a repodex::graph::GraphNode {
    g.nodes()
        .iter()
        .find(|n| n.kind == NodeKind::Call && n.path == path && n.label == written)
        .unwrap_or_else(|| panic!("no call node `{written}` in {path}"))
}

fn fixture() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Cargo.toml", "[package]\nname = \"m\"\n"),
        // Rust: local function call -> single candidate.
        (
            "src/lib.rs",
            "fn helper() {}\nfn caller() { helper() }\nfn dead() { missing() }\n",
        ),
        ("go.mod", "module example.com/m\n\ngo 1.22\n"),
        (
            "auth/auth.go",
            "package auth\nfunc Validate() {}\nfunc validate() {}\n",
        ),
        // Go: two same-name funcs -> multiple; pkg.Func() imported selector;
        // shadowed root; external import selector.
        (
            "app/run.go",
            "package app\nimport \"example.com/m/auth\"\nimport \"example.com/ext/x\"\nfunc run() {\n\tauth.Validate()\n\tx.Do()\n}\n",
        ),
        ("dup/a.go", "package dup\nfunc helper() {}\n"),
        (
            "dup/b.go",
            "package dup\nfunc helper() {}\nfunc run() { helper() }\n",
        ),
    ]
}

#[test]
fn file_contains_declarations_and_calls() {
    let (_t, g) = graph_of(&fixture());
    let file = g.node("file:src/lib.rs").expect("file node");
    let out = g.outgoing(&file.node_id);
    let contains: Vec<_> = out.iter().filter(|e| e.kind == "contains").collect();
    assert!(contains.len() >= 5); // 3 decls + calls
                                  // Every contains edge is FACT and points at a real node.
    for e in contains {
        assert_eq!(e.evidence_class, EvidenceClass::Fact);
        assert!(g.node(&e.target).is_some());
    }
    // A declaration node exists for `helper`.
    let decl = g
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::Declaration && n.label == "helper" && n.path == "src/lib.rs")
        .expect("helper decl");
    assert_eq!(decl.language, "rust");
}

#[test]
fn single_candidate_emits_one_candidate_edge() {
    let (_t, g) = graph_of(&fixture());
    // Go package-local `auth.Validate()` imported selector -> single candidate.
    let call = call_node(&g, "app/run.go", "auth.Validate");
    assert_eq!(call.disposition.as_deref(), Some("single_candidate"));
    let out = g.outgoing(&call.node_id);
    let cand: Vec<_> = out.iter().filter(|e| e.kind == "call_candidate").collect();
    assert_eq!(cand.len(), 1);
    let e = cand[0];
    assert_eq!(e.evidence_class, EvidenceClass::Candidate);
    assert!(e.candidate_set_id.is_some());
    let target = g.node(&e.target).expect("target");
    assert_eq!(target.kind, NodeKind::Declaration);
    assert_eq!(target.label, "Validate");
    assert_eq!(target.path, "auth/auth.go");
}

#[test]
fn multiple_candidates_share_a_candidate_set() {
    let (_t, g) = graph_of(&fixture());
    let call = call_node(&g, "dup/b.go", "helper");
    assert_eq!(call.disposition.as_deref(), Some("multiple_candidates"));
    let out = g.outgoing(&call.node_id);
    let cand: Vec<_> = out.iter().filter(|e| e.kind == "call_candidate").collect();
    assert_eq!(cand.len(), 2);
    let set: std::collections::BTreeSet<_> =
        cand.iter().map(|e| e.candidate_set_id.clone()).collect();
    assert_eq!(set.len(), 1, "both edges share one candidate_set_id");
    for e in cand {
        assert_eq!(e.evidence_class, EvidenceClass::Candidate);
    }
}

#[test]
fn no_candidate_call_has_no_target_edge_but_keeps_disposition() {
    let (_t, g) = graph_of(&fixture());
    // Rust `missing()` has no local function -> no_candidate.
    let call = call_node(&g, "src/lib.rs", "missing");
    assert!(call
        .disposition
        .as_deref()
        .unwrap_or("")
        .starts_with("no_candidate"));
    let out = g.outgoing(&call.node_id);
    assert!(out.iter().all(|e| e.kind != "call_candidate"));
}

#[test]
fn external_import_selector_is_out_of_scope() {
    let (_t, g) = graph_of(&fixture());
    let call = call_node(&g, "app/run.go", "x.Do");
    assert_eq!(
        call.disposition.as_deref(),
        Some("out_of_scope:external_import")
    );
    let out = g.outgoing(&call.node_id);
    assert!(out.iter().all(|e| e.kind != "call_candidate"));
}

#[test]
fn structural_import_edge_is_fact() {
    let (_t, g) = graph_of(&fixture());
    // The `import "example.com/m/auth"` produces an import_path FACT edge to the
    // auth go_package entity.
    let auth_edges: Vec<_> = g
        .edges()
        .iter()
        .filter(|e| e.kind == "import_path")
        .collect();
    assert!(!auth_edges.is_empty());
    for e in auth_edges {
        assert_eq!(e.evidence_class, EvidenceClass::Fact);
        let tgt = g.node(&e.target).expect("target");
        assert_eq!(tgt.kind, NodeKind::Entity);
    }
}

#[test]
fn reverse_traversal_finds_all_callers() {
    let (_t, g) = graph_of(&fixture());
    // `auth.Validate` and any other caller of Validate produce incoming edges on
    // the Validate declaration node.
    let validate = g
        .nodes()
        .iter()
        .find(|n| {
            n.kind == NodeKind::Declaration && n.label == "Validate" && n.path == "auth/auth.go"
        })
        .expect("Validate decl");
    let inc = g.incoming(&validate.node_id);
    let callers: Vec<_> = inc.iter().filter(|e| e.kind == "call_candidate").collect();
    assert_eq!(callers.len(), 1); // one call site targets Validate
    for e in callers {
        let src = g.node(&e.source).expect("caller");
        assert_eq!(src.kind, NodeKind::Call);
        assert_eq!(src.label, "auth.Validate");
    }
}

#[test]
fn forward_traversal_is_bounded_and_deterministic() {
    let (_t, g) = graph_of(&fixture());
    let file = g.node("file:dup/b.go").expect("file");
    // Traverse file -> call -> decl with a depth cap.
    let opts = repodex::graph::TraverseOptions {
        max_depth: 3,
        max_nodes: 64,
        ..Default::default()
    };
    let a = g.traverse(&file.node_id, &opts);
    let b = g.traverse(&file.node_id, &opts);
    let ak: Vec<_> = a.iter().map(|n| n.node_id.clone()).collect();
    let bk: Vec<_> = b.iter().map(|n| n.node_id.clone()).collect();
    assert_eq!(ak, bk, "traversal must be deterministic");
    assert!(a.len() > 1);
}

#[test]
fn neighborhood_separates_fact_and_candidate() {
    let (_t, g) = graph_of(&fixture());
    let call = call_node(&g, "app/run.go", "auth.Validate");
    let nb = g.neighborhood(&call.node_id, &repodex::graph::NeighborFilter::both());
    let has_fact = nb
        .iter()
        .any(|n| n.edge.evidence_class == EvidenceClass::Fact);
    let has_cand = nb
        .iter()
        .any(|n| n.edge.evidence_class == EvidenceClass::Candidate);
    assert!(has_fact && has_cand);
    // Filter to candidates only.
    let mut f = repodex::graph::NeighborFilter::both();
    f.evidence = Some(EvidenceClass::Candidate);
    let only = g.neighborhood(&call.node_id, &f);
    assert!(only
        .iter()
        .all(|n| n.edge.evidence_class == EvidenceClass::Candidate));
}

#[test]
fn graph_is_deterministic_across_rebuilds() {
    let (_t, g) = graph_of(&fixture());
    let d1 = g.manifest().graph_digest.clone();
    // Rebuild into a different output dir -> identical digest.
    let t2 = TempDir::new("t5a-b");
    let repo = _t.path().join("repo");
    let snap = t2.path().join("snap");
    build_snapshot(&support::analyzer(), &repo, &snap, BuildOptions::default()).expect("snapshot");
    let links = t2.path().join("links");
    build_links(&snap, Some(&repo), &links).expect("links");
    let cand = t2.path().join("cand");
    build_candidates(&snap, &links, &cand).expect("candidates");
    let g2 = t2.path().join("graph");
    let out2 = build_graph(&snap, &links, &cand, &g2).expect("graph");
    assert_eq!(
        d1, out2.manifest.graph_digest,
        "graph digest must be identical"
    );
}

// ---------------------------------------------------------------------------
// §50 update-vs-fresh + §51 stale dependency invalidation
// ---------------------------------------------------------------------------

/// A pure-Go repo used for update-vs-fresh/stale tests.
fn go_repo(extra_fn: bool) -> Vec<(&'static str, String)> {
    let auth = if extra_fn {
        "package auth\nfunc Validate() {}\nfunc Extra() {}\n"
    } else {
        "package auth\nfunc Validate() {}\n"
    };
    vec![
        ("go.mod", "module example.com/m\n\ngo 1.22\n".into()),
        ("auth/auth.go", auth.into()),
        (
            "app/run.go",
            "package app\nimport \"example.com/m/auth\"\nfunc run() { auth.Validate() }\n".into(),
        ),
    ]
}

fn pipeline(repo: &std::path::Path, base: &std::path::Path) -> repodex::graph::GraphManifest {
    let snap = base.join("snap");
    build_snapshot(&support::analyzer(), repo, &snap, BuildOptions::default()).expect("snapshot");
    let links = base.join("links");
    build_links(&snap, Some(repo), &links).expect("links");
    let cand = base.join("cand");
    build_candidates(&snap, &links, &cand).expect("candidates");
    let g = base.join("graph");
    build_graph(&snap, &links, &cand, &g)
        .expect("graph")
        .manifest
}

#[test]
fn update_vs_fresh_graph_is_identical() {
    let t = TempDir::new("t5a-uvf");
    // fresh build of the full repo
    let fresh_repo = t.path().join("fresh-repo");
    for (rel, src) in go_repo(true) {
        let p = fresh_repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    let fresh = pipeline(&fresh_repo, &t.path().join("fresh"));

    // incremental: build v0 repo, then update it to add Extra() like fresh.
    let inc_repo = t.path().join("inc-repo");
    for (rel, src) in go_repo(false) {
        let p = inc_repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    let base_snap = t.path().join("inc-base-snap");
    build_snapshot(
        &support::analyzer(),
        &inc_repo,
        &base_snap,
        BuildOptions::default(),
    )
    .expect("base snapshot");
    // add the function, then update the snapshot incrementally
    std::fs::write(
        inc_repo.join("auth/auth.go"),
        "package auth\nfunc Validate() {}\nfunc Extra() {}\n",
    )
    .unwrap();
    let inc_snap = t.path().join("inc-snap");
    repodex::repository::update_snapshot(
        &support::analyzer(),
        &inc_repo,
        &base_snap,
        &inc_snap,
        BuildOptions::default(),
    )
    .expect("update");
    let links = t.path().join("inc-links");
    build_links(&inc_snap, Some(&inc_repo), &links).expect("links");
    let cand = t.path().join("inc-cand");
    build_candidates(&inc_snap, &links, &cand).expect("candidates");
    let g = t.path().join("inc-graph");
    let inc = build_graph(&inc_snap, &links, &cand, &g)
        .expect("graph")
        .manifest;

    assert_eq!(
        fresh.graph_digest, inc.graph_digest,
        "incremental graph must equal fresh for identical bytes"
    );
}

#[test]
fn stale_candidate_artifact_is_rejected() {
    let t = TempDir::new("t5a-stale");
    let repo = t.path().join("repo");
    for (rel, src) in go_repo(true) {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    let snap = t.path().join("snap");
    build_snapshot(&support::analyzer(), &repo, &snap, BuildOptions::default()).expect("snapshot");
    let links = t.path().join("links");
    build_links(&snap, Some(&repo), &links).expect("links");
    let cand = t.path().join("cand");
    build_candidates(&snap, &links, &cand).expect("candidates");
    let g = t.path().join("graph");
    build_graph(&snap, &links, &cand, &g).expect("graph");

    // Rebuild candidates over a *different* snapshot (repo changed) -> the old
    // graph's recorded candidate_digest no longer matches.
    let repo2 = t.path().join("repo2");
    for (rel, mut src) in go_repo(true) {
        if rel == "auth/auth.go" {
            src.push_str("func Another() {}\n");
        }
        let p = repo2.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    let snap2 = t.path().join("snap2");
    build_snapshot(
        &support::analyzer(),
        &repo2,
        &snap2,
        BuildOptions::default(),
    )
    .expect("s2");
    let links2 = t.path().join("links2");
    build_links(&snap2, Some(&repo2), &links2).expect("l2");
    let cand2 = t.path().join("cand2");
    build_candidates(&snap2, &links2, &cand2).expect("c2");

    // Verifying the first graph against the *new* candidate artifact must fail.
    let err = repodex::graph::verify(&g, &snap2, &links2, &cand2)
        .expect_err("stale upstream must be rejected");
    assert!(err.to_string().contains("mismatch") || err.to_string().contains("stale"));
}

// ---------------------------------------------------------------------------
// Phase A — deterministic investigation primitives
// ---------------------------------------------------------------------------

use repodex::graph::{LookupDomain, PathOptions};

#[test]
fn find_by_exact_name_and_path() {
    let (_t, g) = graph_of(&fixture());
    assert_eq!(g.find("Validate", LookupDomain::Name).len(), 1);
    assert!(!g.find("file:app/run.go", LookupDomain::Id).is_empty());
    // Path-suffix lookup.
    let by_suffix = g.find("run.go", LookupDomain::Path);
    assert_eq!(by_suffix.len(), 1);
}

#[test]
fn find_ambiguous_name_returns_all_matches() {
    let (_t, g) = graph_of(&fixture());
    // `helper` exists in src/lib.rs, dup/a.go and dup/b.go — all returned.
    let m = g.find("helper", LookupDomain::Name);
    assert_eq!(m.len(), 3);
}

#[test]
fn callers_returns_candidate_edges() {
    let (_t, g) = graph_of(&fixture());
    let validate = g.find("Validate", LookupDomain::Name)[0];
    let callers = g.callers(&validate.node_id);
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0].kind, "call_candidate");
    assert_eq!(callers[0].evidence_class, EvidenceClass::Candidate);
}

#[test]
fn callees_reaches_call_site_then_candidate() {
    let (_t, g) = graph_of(&fixture());
    let run = g
        .nodes()
        .iter()
        .find(|n| n.kind == NodeKind::Declaration && n.label == "run" && n.path == "app/run.go")
        .expect("run decl");
    let callees = g.callees(&run.node_id);
    // `run` calls auth.Validate and x.Do; only auth.Validate has a candidate.
    let targets: Vec<_> = callees.iter().map(|(_, _, d)| d.label.clone()).collect();
    assert!(targets.contains(&"Validate".to_string()));
    // every hop preserves the call-site -> candidate chain
    for (contains, cand, _) in &callees {
        assert_eq!(contains.kind, "contains");
        assert_eq!(cand.kind, "call_candidate");
        assert_eq!(cand.evidence_class, EvidenceClass::Candidate);
    }
}

#[test]
fn paths_find_mixed_evidence_route() {
    let (_t, g) = graph_of(&fixture());
    let file = g.node("file:app/run.go").unwrap();
    let validate = g.find("Validate", LookupDomain::Name)[0];
    let opts = PathOptions {
        max_depth: 4,
        max_paths: 8,
        ..Default::default()
    };
    let paths = g.paths(&file.node_id, &validate.node_id, &opts);
    assert!(!paths.is_empty());
    // path goes file -> call -> decl and contains a candidate edge.
    let p = &paths[0];
    assert!(p.has_candidate);
    assert_eq!(p.evidence_label(), "candidate path");
    // deterministic: same call twice
    assert_eq!(
        paths.len(),
        g.paths(&file.node_id, &validate.node_id, &opts).len()
    );
}

#[test]
fn paths_respect_depth_and_node_limits() {
    let (_t, g) = graph_of(&fixture());
    let file = g.node("file:app/run.go").unwrap();
    let validate = g.find("Validate", LookupDomain::Name)[0];
    // depth 1 can't reach a decl 2 hops away.
    let shallow = g.paths(
        &file.node_id,
        &validate.node_id,
        &PathOptions {
            max_depth: 1,
            ..Default::default()
        },
    );
    assert!(shallow.is_empty());
    let ok = g.paths(
        &file.node_id,
        &validate.node_id,
        &PathOptions {
            max_depth: 3,
            max_paths: 4,
            ..Default::default()
        },
    );
    assert!(!ok.is_empty());
}

#[test]
fn paths_on_cyclic_graph_terminate() {
    // A recursive/self-referential repo must still terminate.
    let files = vec![
        ("go.mod", "module example.com/m\n\ngo 1.22\n"),
        (
            "p/p.go",
            "package p\nfunc a() { b() }\nfunc b() { a() }\nfunc c() { c() }\n",
        ),
    ];
    let (_t, g) = graph_of(&files);
    let a = g.find("a", LookupDomain::Name)[0];
    let b = g.find("b", LookupDomain::Name)[0];
    let paths = g.paths(&a.node_id, &b.node_id, &PathOptions::default());
    assert!(!paths.is_empty()); // terminates despite the a<->b cycle
}
