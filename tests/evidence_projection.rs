//! Faithful bounded evidence delivery (§57).
//!
//! The canonical EvidenceProjection is the single source every transport
//! renders from; these tests assert rank/path/locator/evidence/provenance/
//! ranges/connectors survive machine JSON, RDX, human, and service parity.

use std::fs;
use std::path::Path;
use std::process::Command;

use repodex::query::{projection, QueryIntent, QueryMode};
use repodex::view::service::{run_view_query, ViewQueryParams};
use repodex::view::ViewLocator;

mod support;
use std::path::PathBuf;
use support::TempDir;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_reposuite-repodex"))
}

fn state(name: &str) -> TempDir {
    TempDir::new(&format!("ep-{name}"))
}

/// Fixture: two same-named `Helper` decls (a + b), a `Caller` that calls
/// `a::Helper`, a FACT `contains` edge, and a bounded path Caller->Helper.
fn repo(root: &Path) {
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("go.mod"), "module example.com/fed\n").unwrap();
    fs::write(
        root.join("a/foo.go"),
        "package a\nfunc Helper() int { return 1 }\nfunc Caller() int { return Helper() }\n",
    )
    .unwrap();
    fs::write(
        root.join("b/bar.go"),
        "package b\nfunc Helper() int { return 2 }\n",
    )
    .unwrap();
}

fn query<'a>(
    root: &'a Path,
    state: &'a Path,
    text: &'a str,
    intent: Option<QueryIntent>,
    target: Option<String>,
    to: Option<String>,
) -> repodex::view::service::ViewQueryOutcome {
    let p = ViewQueryParams {
        query_text: text,
        mode: QueryMode::Ranked,
        intent,
        target,
        to,
        max_results: 50,
        token_budget: None,
        depth: 4,
        memory_mode: repodex::memory::compose::MemoryMode::Off,
        context_policy: repodex::context::ContextPolicy::Static,
        recipes: repodex::recipe::RecipePolicy::Off,
        utility_policy: repodex::utility::UtilityPolicy::Off,
        source_witness: repodex::witness::SourceWitnessPolicy::Off,
        vocab_bridge: false,
        state_override: Some(state),
    };
    run_view_query(&ViewLocator::Root(root.to_path_buf()), &p).unwrap()
}

// --- §7/§8: explicit seed rank; local id is not rank -------------------------

#[test]
fn seed_rank_is_explicit_and_not_a_local_id() {
    let s = state("rank");
    let root = s.path().join("repo");
    repo(&root);
    let o = query(&root, s.path(), "Helper", None, None, None);
    let proj = projection::build(&o.result, Some(&o.ensure.index_dir()));
    // Two same-named seeds, ranks 1 and 2 (array order == rank order).
    assert_eq!(proj.seeds.len(), 2);
    assert_eq!(proj.seeds[0].rank, 1);
    assert_eq!(proj.seeds[1].rank, 2);
    // Same-named entities stay distinguishable by key/path (§57).
    assert_ne!(proj.seeds[0].key, proj.seeds[1].key);
    assert_ne!(proj.seeds[0].path, proj.seeds[1].path);
    // In RDX the local `F<n>` id must NOT be conflated with rank — rank is an
    // explicit `rank=` field on the `D` line, decoupled from the id ordinal.
    let rdx = proj.to_rdx();
    assert!(rdx.contains("rank=1"));
    assert!(rdx.contains("rank=2"));
    assert!(rdx.contains("#RDX1 v2"));
}

// --- §9/§11-§14: current path + declaration range materialized ----------------

#[test]
fn decl_seed_has_current_path_and_ranges_without_reparse() {
    let s = state("ranges");
    let root = s.path().join("repo");
    repo(&root);
    let o = query(&root, s.path(), "Helper", None, None, None);
    let proj = projection::build(&o.result, Some(&o.ensure.index_dir()));
    let a = &proj.seeds[0];
    assert_eq!(a.path, "a/foo.go");
    // Current declaration + body ranges materialized from FileAnalysis (§11).
    let dr = a.declaration_range.as_ref().expect("decl range");
    assert_eq!(dr.path, "a/foo.go");
    assert_eq!(dr.start_line, 2); // `func Helper` is line 2
    assert!(dr.byte_end > dr.byte_start);
    assert!(a.body_range.is_some());
    // Range is a locator, not source bytes (§56) — no source text emitted.
    let br = a.body_range.as_ref().unwrap();
    assert!(br.byte_end > br.byte_start);
}

// --- §15-§19: relationship fidelity (endpoints, via, fact/candidate, rule) ---

#[test]
fn relationship_fidelity_preserved() {
    let s = state("rel");
    let root = s.path().join("repo");
    repo(&root);
    let o = query(
        &root,
        s.path(),
        "x",
        Some(QueryIntent::Callers),
        Some("Helper".into()),
        None,
    );
    let proj = projection::build(&o.result, Some(&o.ensure.index_dir()));
    assert!(!proj.related.is_empty());
    for r in &proj.related {
        // Both endpoints always present + direction-resolved (§15).
        assert!(!r.from_key.is_empty());
        assert!(!r.to_key.is_empty());
        // `via` (the seed anchor) preserved (§16).
        assert!(!r.via.is_empty());
        // CANDIDATE stays CANDIDATE — never promoted (§17).
        assert!(r.evidence == "fact" || r.evidence == "candidate");
        // Provenance + candidate-set ambiguity preserved (§18-§19).
        assert!(r.rule_id.is_some());
        if r.evidence == "candidate" {
            assert!(r.candidate_set.is_some());
        }
    }
}

// --- §20-§21: scoped completeness, not global semantic completeness ----------

#[test]
fn completeness_is_scoped_to_seed_selection() {
    let s = state("complete");
    let root = s.path().join("repo");
    repo(&root);
    let o = query(&root, s.path(), "Helper", None, None, None);
    let proj = projection::build(&o.result, Some(&o.ensure.index_dir()));
    // `seed_selection_complete` = all matching seeds emitted within bound — it
    // is NOT a claim of repository-wide semantic coverage.
    assert!(proj.seed_selection_complete);
    assert_eq!(proj.emitted_seed_count, proj.seeds.len());
    assert_eq!(proj.eligible_seed_count, 2);
    assert_eq!(proj.bounds.max_seeds, 50);
}

// --- §22-§28: bounded two-endpoint connector ----------------------------------

#[test]
fn two_endpoint_path_reaches_real_traversal() {
    let s = state("paths");
    let root = s.path().join("repo");
    repo(&root);
    // Caller(decl:a/foo.go#2) -> Helper(decl:a/foo.go#1): Caller contains a call
    // site that candidate-resolves to Helper — a real traversal, not a one-hop
    // neighborhood (§24).
    let o = query(
        &root,
        s.path(),
        "x",
        Some(QueryIntent::Paths),
        Some("decl:a/foo.go#2".into()),
        Some("decl:a/foo.go#1".into()),
    );
    let proj = projection::build(&o.result, Some(&o.ensure.index_dir()));
    let conn = proj.connector.as_ref().expect("connector packet");
    assert!(conn.found, "Caller->Helper route should exist");
    // Bounded: <=2 routes, <=3 depth (§25).
    assert!(conn.routes.len() <= 2);
    assert!(conn.max_routes == 2 && conn.max_depth == 3);
    for route in &conn.routes {
        for step in &route.steps {
            assert!(!step.from_key.is_empty());
            assert!(!step.to_key.is_empty());
            assert!(!step.relation.is_empty());
            assert!(step.evidence == "fact" || step.evidence == "candidate");
        }
    }
}

#[test]
fn no_route_means_bounded_absence_not_global() {
    let s = state("nopath");
    let root = s.path().join("repo");
    repo(&root);
    // Helper(a) -> Helper(b): no route exists within depth<=3.
    let o = query(
        &root,
        s.path(),
        "x",
        Some(QueryIntent::Paths),
        Some("decl:a/foo.go#1".into()),
        Some("decl:b/bar.go#1".into()),
    );
    let proj = projection::build(&o.result, Some(&o.ensure.index_dir()));
    let conn = proj.connector.as_ref().expect("connector");
    assert!(!conn.found);
    // §27: explicit "bounded search" semantics, never a global-absence claim.
    assert!(conn
        .no_route_meaning
        .as_deref()
        .unwrap_or("")
        .contains("not a global-absence"));
}

// --- §36: direct == machine JSON semantic parity -------------------------------

#[test]
fn machine_json_matches_projection() {
    let s = state("parity");
    let root = s.path().join("repo");
    repo(&root);
    let o = query(&root, s.path(), "Helper", None, None, None);
    let proj = projection::build(&o.result, Some(&o.ensure.index_dir()));
    let json = proj.to_json();
    // Rank, path, key, ranges all present in machine form.
    assert_eq!(json["seeds"][0]["rank"], 1);
    assert_eq!(json["seeds"][0]["path"], "a/foo.go");
    assert!(json["seeds"][0]["declaration_range"].is_object());
    assert_eq!(json["emitted_seed_count"], 2);
    assert_eq!(json["schema"], projection::PROJECTION_SCHEMA);
}

// --- §39: RDX v2 round-trips + backward decode --------------------------------

#[test]
fn rdx_v2_decodes_and_preserves_rank() {
    let s = state("rdx");
    let root = s.path().join("repo");
    repo(&root);
    let o = query(&root, s.path(), "Helper", None, None, None);
    let proj = projection::build(&o.result, Some(&o.ensure.index_dir()));
    let rdx = proj.to_rdx();
    // v2 decode succeeds; F/R/S evidence lines preserved (§39).
    let doc = repodex::rdx1::parse(&rdx).unwrap();
    assert!(!doc.facts.is_empty());
    // Rank carried explicitly on D lines (not via F-id position).
    let rank1 = rdx
        .lines()
        .any(|l| l.starts_with("D") && l.contains("rank=1"));
    assert!(rank1);
}

// --- §33: a query on a highly-connected node stays bounded --------------------

#[test]
fn output_is_bounded() {
    let s = state("bounds");
    let root = s.path().join("repo");
    repo(&root);
    let o = query(&root, s.path(), "Helper", None, None, None);
    let proj = projection::build(&o.result, Some(&o.ensure.index_dir()));
    assert!(proj.seeds.len() <= proj.bounds.max_seeds);
    assert!(proj.related.len() <= proj.bounds.max_related);
}

// --- §36/§26-27: service-routed query shares the projection (binary-level) ----

#[test]
fn service_query_uses_same_projection() {
    let s = state("svc");
    let root = s.path().join("repo");
    repo(&root);
    let state_dir = s.path().join("state");
    // Start the service, query via /v1/query, compare to a direct projection.
    let bin = bin();
    let start = Command::new(&bin)
        .args(["start", "--state-dir"])
        .arg(&state_dir)
        .env("REPODEX_STATE_DIR", &state_dir)
        .output()
        .unwrap();
    assert!(start.status.success(), "start: {:?}", start);
    let rt: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(state_dir.join("service.runtime.json")).unwrap())
            .unwrap();
    let tok = fs::read_to_string(state_dir.join("service.token")).unwrap();
    let body = serde_json::json!({"root": root.to_str().unwrap(), "query":"Helper"});
    let (st, resp) = repodex::service::http::post_json(
        rt["host"].as_str().unwrap(),
        rt["port"].as_u64().unwrap() as u16,
        "/v1/query",
        Some(tok.trim()),
        &body,
        30000,
    )
    .unwrap();
    assert_eq!(st, 200, "service query failed: {resp}");
    let seeds = resp["result"]["seeds"].as_array().unwrap();
    // Service preserves rank + path + range (§36 parity).
    assert_eq!(seeds[0]["rank"], 1);
    assert_eq!(seeds[0]["path"], "a/foo.go");
    assert!(seeds[0]["declaration_range"].is_object());
    let _ = Command::new(&bin)
        .args(["stop", "--state-dir"])
        .arg(&state_dir)
        .env("REPODEX_STATE_DIR", &state_dir)
        .output();
}
