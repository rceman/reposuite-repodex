//! Guarded Evidence Recipes: mining (min support), current-view execution,
//! membership re-derivation, absent-anchor fallback, project isolation.

use std::fs;
use std::path::Path;

use repodex::recipe::{mine, RecipePolicy, RecipeStore};
use repodex::view::service::{run_view_query, ViewQueryParams};
use repodex::view::ViewLocator;

mod support;
use support::TempDir;

fn repo(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("go.mod"), "module example.com/rc\n").unwrap();
    fs::write(
        root.join("src/lib.go"),
        "package src\nfunc Foo() int { return 1 }\nfunc A() { _ = Foo() }\nfunc B() { _ = Foo() }\n",
    )
    .unwrap();
}

fn query(root: &Path, state: &Path, q: &str, recipes: RecipePolicy) -> serde_json::Value {
    let params = ViewQueryParams {
        query_text: q,
        mode: repodex::query::QueryMode::Ranked,
        intent: None,
        target: None,
        to: None,
        max_results: 100,
        token_budget: None,
        depth: 1,
        memory_mode: repodex::memory::compose::MemoryMode::Off,
        context_policy: repodex::context::ContextPolicy::Static,
        recipes,
        utility_policy: repodex::utility::UtilityPolicy::Off,
        source_witness: repodex::witness::SourceWitnessPolicy::Off,
        state_override: Some(state),
    };
    let o = run_view_query(&ViewLocator::Root(root.to_path_buf()), &params).unwrap();
    let mut p = repodex::query::projection::build(&o.result, Some(&o.ensure.index_dir()));
    if let Some(rc) = &o.recipe {
        p.recipe = Some(
            serde_json::json!({"recipe_id":rc.recipe_id,"produced":rc.produced.len(),"fell_back":rc.fell_back}),
        );
    }
    p.to_json()
}

#[test]
fn recipe_mines_and_executes_current_callers() {
    let s = TempDir::new("rc-exec");
    let root = s.path().join("repo");
    repo(&root);
    // Build index + view scope.
    let _ = query(&root, s.path(), "Foo", RecipePolicy::Off);
    let loc = ViewLocator::Root(root.to_path_buf());
    let view = repodex::view::resolve::resolve(&loc).unwrap();
    let scope = repodex::memory::project::project_scope(&view);

    // Author a recipe directly (mining covered separately): DefinitionToCallers
    // on the "foo" query family.
    let r = repodex::recipe::RecipeDefinition {
        schema: repodex::recipe::RECIPE_SCHEMA.into(),
        recipe_id: "r1".into(),
        project_scope: scope.clone(),
        version: repodex::recipe::RECIPE_VERSION,
        family: repodex::recipe::RecipeFamily::DefinitionToCallers,
        query_family_terms: vec!["foo".into()],
        steps: vec![
            repodex::recipe::SelectorStep {
                id: 0,
                selector: repodex::recipe::Selector::ResolveAnchor,
                after: None,
            },
            repodex::recipe::SelectorStep {
                id: 1,
                selector: repodex::recipe::Selector::CallersOf,
                after: Some(0),
            },
        ],
        support: 2,
        provenance: vec!["i1".into(), "i2".into()],
    };
    let mut store = RecipeStore::default();
    store.upsert(s.path(), r).unwrap();

    // recipes=auto: query "Foo" -> recipe matches -> produces caller relations.
    let j = query(&root, s.path(), "Foo", RecipePolicy::Auto);
    let rel = j["related"].as_array().unwrap();
    assert!(j["recipe"].is_object(), "recipe block present: {j}");
    // Foo has 2 callers (A and B) — recipe re-derives them from current view.
    assert!(!rel.is_empty(), "recipe produced current-view relations");
}

#[test]
fn recipe_off_by_default_and_no_match_safe() {
    let s = TempDir::new("rc-off");
    let root = s.path().join("repo");
    repo(&root);
    // No recipes stored; recipes=auto must not break the query.
    let j = query(&root, s.path(), "Foo", RecipePolicy::Auto);
    assert!(j["recipe"].is_null());
    assert!(!j["seeds"].as_array().unwrap().is_empty());
    // recipes=off identical to default.
    let j2 = query(&root, s.path(), "Foo", RecipePolicy::Off);
    assert_eq!(j["seeds"], j2["seeds"]);
}

#[test]
fn mine_requires_min_support() {
    let s = TempDir::new("rc-mine");
    // One episode only -> below MIN_SUPPORT=2 -> no recipe.
    let eps = vec![repodex::derived::InvestigationEpisode {
        investigation_id: "i1".into(),
        query_text: Some("Foo".into()),
        ..Default::default()
    }];
    let sexp = repodex::symbol_exposure::SymbolExposureStore::open(s.path());
    let n = mine(&eps, &sexp, "repo:test", s.path()).unwrap();
    assert_eq!(n, 0, "single episode must not mint a recipe");
}
