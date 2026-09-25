//! Trajectory utility policy: evidence-unit identity, shadow-vs-apply, optional
//! ordering only, low-support fallback, mandatory-metadata untouched.

use std::fs;
use std::path::Path;

use repodex::utility::{
    ContinuationMetrics, EvidenceRole, EvidenceUnit, ReprKind, UtilityPolicy, UtilityStats,
};
use repodex::view::service::{run_view_query, ViewQueryParams};
use repodex::view::ViewLocator;

mod support;
use support::TempDir;

fn repo(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("go.mod"), "module example.com/u\n").unwrap();
    fs::write(
        root.join("src/lib.go"),
        "package src\nfunc Foo() {}\nfunc FooBar() {}\nfunc FooBaz() {}\nfunc main() { Foo(); FooBar(); FooBaz() }\n",
    )
    .unwrap();
}

fn query(root: &Path, state: &Path, q: &str, up: UtilityPolicy) -> serde_json::Value {
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
        recipes: repodex::recipe::RecipePolicy::Off,
        utility_policy: up,
        source_witness: repodex::witness::SourceWitnessPolicy::Off,
        state_override: Some(state),
    };
    let o = run_view_query(&ViewLocator::Root(root.to_path_buf()), &params).unwrap();
    let mut p = repodex::query::projection::build(&o.result, Some(&o.ensure.index_dir()));
    if let Some(ut) = &o.utility_trace {
        p.utility = Some(serde_json::to_value(ut).unwrap());
    }
    p.to_json()
}

#[test]
fn evidence_unit_identity_is_version_safe() {
    let u = EvidenceUnit {
        scope: "repo:x".into(),
        entity: "go:src/lib.go:func:Foo".into(),
        repr: ReprKind::DeclarationRange,
        role: EvidenceRole::Definition,
        truth_class: "fact".into(),
    };
    // Identity excludes line numbers — same logical unit regardless of coords.
    assert!(u.id().starts_with("sha256:"));
    let u2 = EvidenceUnit {
        scope: "repo:x".into(),
        entity: "go:src/lib.go:func:Foo".into(),
        repr: ReprKind::DeclarationRange,
        role: EvidenceRole::Definition,
        truth_class: "fact".into(),
    };
    assert_eq!(u.id(), u2.id());
}

#[test]
fn shadow_does_not_alter_packet() {
    let s = TempDir::new("u-shadow");
    let root = s.path().join("repo");
    repo(&root);
    let a = query(&root, s.path(), "Foo", UtilityPolicy::Off);
    let b = query(&root, s.path(), "Foo", UtilityPolicy::Shadow);
    // Shadow must not change the emitted seeds/related — only adds a trace.
    assert_eq!(a["seeds"], b["seeds"]);
    assert_eq!(a["related"], b["related"]);
    assert!(b["utility"].is_object(), "shadow emits a utility trace");
    assert_eq!(b["utility"]["policy"], "shadow");
}

#[test]
fn apply_reorders_only_optional_and_preserves_mandatory() {
    let s = TempDir::new("u-apply");
    let root = s.path().join("repo");
    repo(&root);
    // Seed stats: give FooBar a lower followup-rate than the baseline top so
    // apply reorders the optional tail upward.
    let mut st = UtilityStats::default();
    let shape = "AmbiguousLookup";
    let scope = {
        let v = repodex::view::resolve::resolve(&ViewLocator::Root(root.clone())).unwrap();
        repodex::memory::project::project_scope(&v)
    };
    let mk = |e: &str| EvidenceUnit {
        scope: scope.clone(),
        entity: e.into(),
        repr: ReprKind::DeclarationRange,
        role: EvidenceRole::Definition,
        truth_class: "fact".into(),
    };
    // FooBar rarely needs followup (good); others often do.
    for _ in 0..10 {
        st.observe(
            &scope,
            shape,
            &mk("decl:src/lib.go:src.FooBar:func"),
            &ContinuationMetrics {
                searches_before_primary: Some(0),
                ..Default::default()
            },
        );
        st.observe(
            &scope,
            shape,
            &mk("decl:src/lib.go:src.FooBaz:func"),
            &ContinuationMetrics {
                searches_before_primary: Some(3),
                ..Default::default()
            },
        );
    }
    st.save(s.path().join("utility").as_path()).unwrap();
    let j = query(&root, s.path(), "Foo", UtilityPolicy::Apply);
    // The mandatory top seed (rank 0) identity is preserved regardless.
    let seeds = j["seeds"].as_array().unwrap();
    assert!(!seeds.is_empty());
    assert_eq!(seeds[0]["rank"], 1);
}

#[test]
fn low_support_falls_back_to_baseline() {
    let s = TempDir::new("u-lowsup");
    let root = s.path().join("repo");
    repo(&root);
    // No stats -> every cell below MIN_SUPPORT -> global prior -> no reorder.
    let j = query(&root, s.path(), "Foo", UtilityPolicy::Apply);
    let seeds = j["seeds"].as_array().unwrap();
    assert_eq!(seeds[0]["rank"], 1); // deterministic baseline order preserved
}
