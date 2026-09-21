//! Phase C: RDX1 emit -> parse round-trip and grammar invariants.

mod support;

use repodex::candidates::build_candidates;
use repodex::graph::{build_graph, GraphIndex};
use repodex::links::build_links;
use repodex::query::{QueryEngine, QueryIntent, QueryMode, QueryPlan};
use repodex::rdx1;
use repodex::repository::{build_snapshot, BuildOptions};
use support::TempDir;

fn graph_of(files: &[(&str, &str)]) -> (TempDir, GraphIndex) {
    let t = TempDir::new("rdx1");
    let repo = t.path().join("repo");
    for (rel, src) in files {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    let snap = t.path().join("snap");
    build_snapshot(&support::analyzer(), &repo, &snap, BuildOptions::default()).expect("snap");
    let links = t.path().join("links");
    build_links(&snap, Some(&repo), &links).expect("links");
    let cand = t.path().join("cand");
    build_candidates(&snap, &links, &cand).expect("cand");
    let g = t.path().join("graph");
    build_graph(&snap, &links, &cand, &g).expect("graph");
    (t, GraphIndex::load(&g).expect("load"))
}

fn fixture() -> Vec<(&'static str, &'static str)> {
    vec![
        ("go.mod", "module example.com/m\n\ngo 1.22\n"),
        (
            "auth/token.go",
            "package auth\nfunc ValidateToken() {}\nfunc IssueToken() {}\n",
        ),
        (
            "app/run.go",
            "package app\nimport \"example.com/m/auth\"\nfunc run() { auth.ValidateToken() }\n",
        ),
    ]
}

fn result_of(
    g: &GraphIndex,
    raw: &str,
    intent: QueryIntent,
    target: &str,
) -> repodex::query::QueryResult {
    let e = QueryEngine::new(g);
    e.run(&QueryPlan::parse(
        raw,
        QueryMode::Ranked,
        Some(intent),
        Some(target.into()),
        None,
        50,
        None,
    ))
}

#[test]
fn rdx1_emit_parse_round_trip() {
    let (_t, g) = graph_of(&fixture());
    let r = result_of(&g, "ValidateToken", QueryIntent::Callers, "ValidateToken");
    let text = rdx1::render(&r);
    assert!(text.starts_with("#RDX1 v1\n"));
    let doc = rdx1::parse(&text).expect("parse");
    assert_eq!(doc.query.as_deref(), Some("ValidateToken"));
    assert!(!doc.facts.is_empty());
    assert!(!doc.relations.is_empty());
    // every relation references defined facts
    let lids: std::collections::BTreeSet<u32> = doc.facts.iter().map(|f| f.lid).collect();
    for rel in &doc.relations {
        assert!(lids.contains(&rel.src) && lids.contains(&rel.tgt));
        assert!(rel.evidence == 'f' || rel.evidence == 'c');
    }
    // completeness summary present
    assert!(doc.summary.contains_key("complete"));
}

#[test]
fn rdx1_candidate_callers_use_candidate_evidence() {
    let (_t, g) = graph_of(&fixture());
    let r = result_of(&g, "ValidateToken", QueryIntent::Callers, "ValidateToken");
    let doc = rdx1::parse(&rdx1::render(&r)).unwrap();
    let cands: Vec<_> = doc
        .relations
        .iter()
        .filter(|x| x.rel == "call_candidate")
        .collect();
    assert!(!cands.is_empty());
    for c in &cands {
        assert_eq!(c.evidence, 'c', "candidate calls must be evidence `c`");
        assert!(c.candidate_set.is_some(), "candidate sets preserved");
    }
}

#[test]
fn rdx1_is_deterministic() {
    let (_t, g) = graph_of(&fixture());
    let a = rdx1::render(&result_of(&g, "auth", QueryIntent::Find, "auth"));
    let b = rdx1::render(&result_of(&g, "auth", QueryIntent::Find, "auth"));
    assert_eq!(a, b);
}

#[test]
fn rdx1_truncation_reports_incomplete() {
    let (_t, g) = graph_of(&fixture());
    let e = QueryEngine::new(&g);
    let r = e.run(&QueryPlan::parse(
        "auth",
        QueryMode::Exhaustive,
        None,
        None,
        None,
        1,
        None,
    ));
    let doc = rdx1::parse(&rdx1::render(&r)).unwrap();
    assert_eq!(doc.summary.get("complete").map(String::as_str), Some("0"));
    assert_eq!(doc.summary.get("truncated").map(String::as_str), Some("1"));
}
