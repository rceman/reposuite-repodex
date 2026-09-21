//! Phase B: deterministic textual query engine.
//!
//! Builds a mixed repo -> graph -> QueryEngine, then asserts the §27 corpus:
//! exact/camel/snake/path/package lookup, ranked vs exhaustive, completeness,
//! callers/callees/related intents, zero-result and budget-truncated queries.

mod support;

use repodex::candidates::build_candidates;
use repodex::graph::{build_graph, GraphIndex};
use repodex::links::build_links;
use repodex::query::{QueryEngine, QueryIntent, QueryMode, QueryPlan};
use repodex::repository::{build_snapshot, BuildOptions};
use support::TempDir;

fn engine_of(files: &[(&str, &str)]) -> (TempDir, GraphIndex) {
    let t = TempDir::new("q");
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

fn fixture() -> Vec<(&'static str, &'static str)> {
    vec![
        ("go.mod", "module example.com/m\n\ngo 1.22\n"),
        (
            "auth/token.go",
            "package auth\nfunc ValidateToken() {}\nfunc IssueToken() {}\n",
        ),
        ("auth/handler.go", "package auth\nfunc NewHandler() {}\n"),
        (
            "app/run.go",
            "package app\nimport \"example.com/m/auth\"\nfunc run() { auth.ValidateToken() }\n",
        ),
    ]
}

fn run(
    g: &GraphIndex,
    raw: &str,
    mode: QueryMode,
    intent: Option<QueryIntent>,
    target: Option<String>,
) -> repodex::query::QueryResult {
    let e = QueryEngine::new(g);
    let plan = QueryPlan::parse(raw, mode, intent, target, None, 50, None);
    e.run(&plan)
}

#[test]
fn identifier_terms_split_conventions() {
    use repodex::query::identifier_terms as t;
    assert_eq!(t("authToken"), vec!["auth", "token"]);
    assert_eq!(t("AuthToken"), vec!["auth", "token"]);
    assert_eq!(t("auth_token"), vec!["auth", "token"]);
    assert_eq!(t("auth-token"), vec!["auth", "token"]);
    assert_eq!(t("auth/token"), vec!["auth", "token"]);
    assert_eq!(t("HTTPRequest"), vec!["http", "request"]);
}

#[test]
fn exact_function_name_query() {
    let (_t, g) = engine_of(&fixture());
    let r = run(&g, "ValidateToken", QueryMode::Ranked, None, None);
    assert!(!r.seeds.is_empty());
    assert_eq!(r.seeds[0].node.label, "ValidateToken");
    assert!(r.seeds[0].factors.contains(&"exact".to_string()));
    assert!(r.complete);
}

#[test]
fn camel_and_snake_query_match_same() {
    let (_t, g) = engine_of(&fixture());
    let a = run(&g, "ValidateToken", QueryMode::Ranked, None, None);
    let b = run(&g, "validate token", QueryMode::Ranked, None, None);
    // both reach the ValidateToken declaration as a top seed
    assert!(a.seeds.iter().any(|s| s.node.label == "ValidateToken"));
    assert!(b.seeds.iter().any(|s| s.node.label == "ValidateToken"));
}

#[test]
fn path_and_package_terms_match() {
    let (_t, g) = engine_of(&fixture());
    // `auth` matches the auth package entity + files under auth/.
    let r = run(&g, "auth", QueryMode::Ranked, None, None);
    assert!(r
        .seeds
        .iter()
        .any(|s| s.node.path.starts_with("auth/") || s.node.label == "auth"));
}

#[test]
fn multi_term_query_ranks_full_match_first() {
    let (_t, g) = engine_of(&fixture());
    let r = run(&g, "validate token", QueryMode::Ranked, None, None);
    // ValidateToken matches both terms -> should outrank single-term hits.
    assert_eq!(r.seeds[0].node.label, "ValidateToken");
}

#[test]
fn zero_result_query_is_empty_and_complete() {
    let (_t, g) = engine_of(&fixture());
    let r = run(&g, "zzz_no_such_symbol", QueryMode::Ranked, None, None);
    assert!(r.seeds.is_empty());
    assert!(r.complete);
    assert_eq!(r.total, 0);
}

#[test]
fn callers_intent_finds_candidate_callers() {
    let (_t, g) = engine_of(&fixture());
    let r = run(
        &g,
        "ValidateToken",
        QueryMode::Ranked,
        Some(QueryIntent::Callers),
        Some("ValidateToken".into()),
    );
    assert_eq!(r.seeds.len(), 1);
    assert!(r
        .related
        .iter()
        .any(|h| h.kind == "call_candidate" && h.direction == "incoming"));
}

#[test]
fn callees_intent_reaches_candidate_targets() {
    let (_t, g) = engine_of(&fixture());
    let r = run(
        &g,
        "run",
        QueryMode::Ranked,
        Some(QueryIntent::Callees),
        Some("run".into()),
    );
    assert!(r
        .related
        .iter()
        .any(|h| h.node.label == "ValidateToken" && h.kind == "call_candidate"));
}

#[test]
fn exhaustive_reports_all_matches() {
    let (_t, g) = engine_of(&fixture());
    let r = run(&g, "auth", QueryMode::Exhaustive, None, None);
    assert!(r.complete);
    assert_eq!(r.shown, r.total);
}

#[test]
fn truncated_exhaustive_reports_incomplete() {
    let (_t, g) = engine_of(&fixture());
    let e = QueryEngine::new(&g);
    let plan = QueryPlan::parse("auth", QueryMode::Exhaustive, None, None, None, 1, None);
    let r = e.run(&plan);
    assert!(!r.complete);
    assert_eq!(r.truncated_reason.as_deref(), Some("limit"));
    assert!(r.total > r.shown);
}

#[test]
fn query_is_deterministic() {
    let (_t, g) = engine_of(&fixture());
    let a = run(&g, "auth token", QueryMode::Ranked, None, None);
    let b = run(&g, "auth token", QueryMode::Ranked, None, None);
    let ka: Vec<_> = a
        .seeds
        .iter()
        .map(|s| (s.node.key.clone(), s.score))
        .collect();
    let kb: Vec<_> = b
        .seeds
        .iter()
        .map(|s| (s.node.key.clone(), s.score))
        .collect();
    assert_eq!(ka, kb);
}
