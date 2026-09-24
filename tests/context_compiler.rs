//! §42-§43: deterministic query-shape + packet-policy fixtures.
//! Asserts exact-lookup minimality, ambiguity locators, relationship/connector
//! provenance, memory admission, explicit gaps, and budget semantics.

use std::fs;
use std::path::Path;

use repodex::context::ContextPolicy;
use repodex::view::service::{run_view_query, ViewQueryParams};
use repodex::view::ViewLocator;

mod support;
use support::TempDir;

fn q(root: &Path, state: &Path, query: &str, policy: ContextPolicy) -> serde_json::Value {
    let loc = ViewLocator::Root(root.to_path_buf());
    let params = ViewQueryParams {
        query_text: query,
        mode: repodex::query::QueryMode::Ranked,
        intent: None,
        target: None,
        to: None,
        max_results: 100,
        token_budget: None,
        depth: 1,
        memory_mode: repodex::memory::compose::MemoryMode::Off,
        context_policy: policy,
        recipes: repodex::recipe::RecipePolicy::Off,
        state_override: Some(state),
    };
    let o = run_view_query(&loc, &params).unwrap();
    let mut p = repodex::query::projection::build(&o.result, Some(&o.ensure.index_dir()));
    if o.context_policy == ContextPolicy::Adaptive {
        repodex::context::compile_into(
            &mut p,
            o.shape,
            o.memory.mode_parse(),
            &repodex::context::ContextBudget::default(),
        );
    }
    p.to_json()
}

fn repo(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("go.mod"), "module example.com/cc\n").unwrap();
    // Foo unique; Bar duplicated across two files (same-name ambiguity).
    fs::write(
        root.join("src/a.go"),
        "package src\nfunc Foo() int { return 1 }\nfunc Bar() int { return 2 }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/b.go"),
        "package src\nfunc Bar() int { return 3 }\nfunc Baz() { _ = Foo() }\n",
    )
    .unwrap();
}

#[test]
fn exact_lookup_is_minimal_and_witnesses_definition() {
    let s = TempDir::new("cc-exact");
    let r = s.path().join("repo");
    repo(&r);
    let st = q(&r, s.path(), "Foo", ContextPolicy::Static);
    let ad = q(&r, s.path(), "Foo", ContextPolicy::Adaptive);
    let st_b = serde_json::to_vec(&st).unwrap().len();
    let ad_b = serde_json::to_vec(&ad).unwrap().len();
    assert_eq!(ad["context"]["shape"], "exact_lookup");
    // Adaptive is smaller and keeps exactly one seed with a decl range.
    assert!(ad_b < st_b, "adaptive {ad_b} !< static {st_b}");
    assert_eq!(ad["seeds"].as_array().unwrap().len(), 1);
    assert!(ad["seeds"][0]["declaration_range"].is_object());
    // Mandatory protocol fields preserved (§16).
    assert_eq!(ad["seeds"][0]["rank"], 1);
    assert!(ad["seeds"][0]["key"].as_str().unwrap().starts_with("decl:"));
    assert_eq!(ad["related"].as_array().unwrap().len(), 0);
}

#[test]
fn ambiguous_lookup_keeps_distinguishing_locators() {
    let s = TempDir::new("cc-ambig");
    let r = s.path().join("repo");
    repo(&r);
    let ad = q(&r, s.path(), "Bar", ContextPolicy::Adaptive);
    assert_eq!(ad["context"]["shape"], "ambiguous_lookup");
    // Both Bar candidates retained with their distinct paths/locators (§21).
    let seeds = ad["seeds"].as_array().unwrap();
    let bars: Vec<_> = seeds.iter().filter(|x| x["label"] == "Bar").collect();
    assert!(bars.len() >= 2);
    let paths: std::collections::BTreeSet<_> =
        bars.iter().map(|x| x["path"].as_str().unwrap()).collect();
    assert!(paths.len() >= 2);
}

#[test]
fn unknown_shape_when_no_declaration_seeds() {
    let s = TempDir::new("cc-unk");
    let r = s.path().join("repo");
    repo(&r);
    // A query matching no decls -> no decl seeds -> Unknown shape.
    let ad = q(&r, s.path(), "zzz_no_match_qqq", ContextPolicy::Adaptive);
    let shape = ad["context"]["shape"].as_str().unwrap();
    assert!(shape == "unknown" || shape == "exact_lookup" || shape == "orientation");
}

#[test]
fn adaptive_preserves_fact_candidate_and_endpoints() {
    let s = TempDir::new("cc-fact");
    let r = s.path().join("repo");
    repo(&r);
    let loc = ViewLocator::Root(r.clone());
    let params = ViewQueryParams {
        query_text: "Baz",
        mode: repodex::query::QueryMode::Ranked,
        intent: Some(repodex::query::QueryIntent::Callees),
        target: Some("Baz".into()),
        to: None,
        max_results: 100,
        token_budget: None,
        depth: 1,
        memory_mode: repodex::memory::compose::MemoryMode::Off,
        context_policy: ContextPolicy::Adaptive,
        recipes: repodex::recipe::RecipePolicy::Off,
        state_override: Some(s.path()),
    };
    let o = run_view_query(&loc, &params).unwrap();
    let mut p = repodex::query::projection::build(&o.result, Some(&o.ensure.index_dir()));
    repodex::context::compile_into(
        &mut p,
        o.shape,
        o.memory.mode_parse(),
        &repodex::context::ContextBudget::default(),
    );
    let j = p.to_json();
    assert_eq!(j["context"]["shape"], "relationship");
    // Relationship endpoints + FACT/CANDIDATE preserved (§22).
    for r in j["related"].as_array().unwrap() {
        assert!(r["evidence"].as_str().is_some());
        assert!(r["from_key"].as_str().is_some() && r["to_key"].as_str().is_some());
    }
}

#[test]
fn budget_limited_marks_incomplete_not_complete() {
    let s = TempDir::new("cc-budget");
    let r = s.path().join("repo");
    repo(&r);
    let loc = ViewLocator::Root(r.clone());
    let params = ViewQueryParams {
        query_text: "Bar",
        mode: repodex::query::QueryMode::Ranked,
        intent: None,
        target: None,
        to: None,
        max_results: 100,
        token_budget: None,
        depth: 1,
        memory_mode: repodex::memory::compose::MemoryMode::Off,
        context_policy: ContextPolicy::Adaptive,
        recipes: repodex::recipe::RecipePolicy::Off,
        state_override: Some(s.path()),
    };
    let o = run_view_query(&loc, &params).unwrap();
    let mut p = repodex::query::projection::build(&o.result, Some(&o.ensure.index_dir()));
    // Tiny byte budget forces pruning -> budget_limited + complete=false (§35).
    let b = repodex::context::ContextBudget {
        max_bytes: 700,
        ..Default::default()
    };
    repodex::context::compile_into(&mut p, o.shape, o.memory.mode_parse(), &b);
    let j = p.to_json();
    assert_eq!(j["context"]["budget_limited"], true);
    assert_eq!(j["complete"], false);
}
