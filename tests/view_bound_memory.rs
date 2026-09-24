//! RepositoryView-bound symbol memory: target-view rebinding (§15-§28, §51-§53).
//!
//! A historical `SymbolMemoryEvidence` (stable locator + facets) is rebound
//! against a mutated current view. Assertions cover the full binding-state
//! taxonomy and the hard safety rules: old ranges never replay as current,
//! unchanged symbols survive file-level changes, changed/absent symbols never
//! become current FACTs, and cross-project memory never mixes.

use std::fs;
use std::path::Path;

use repodex::memory::project::project_scope;
use repodex::memory::rebind::{
    decl_facets, decl_locator, BindingState, DeclFacets, TargetViewRebinder,
};
use repodex::model::FileAnalysis;
use repodex::query::QueryMode;
use repodex::view::service::{run_view_query, ViewQueryParams};
use repodex::view::ViewLocator;

mod support;
use support::TempDir;

fn q(root: &Path, state: &Path, text: &str) -> repodex::view::service::ViewQueryOutcome {
    let p = ViewQueryParams {
        query_text: text,
        mode: QueryMode::Ranked,
        intent: None,
        target: None,
        to: None,
        max_results: 50,
        token_budget: None,
        depth: 4,
        memory_mode: repodex::memory::compose::MemoryMode::Off,
        state_override: Some(state),
    };
    run_view_query(&ViewLocator::Root(root.to_path_buf()), &p).unwrap()
}

/// Load the snapshot FileAnalysis for `path` from the view's index dir.
fn analysis(index_dir: &Path, path: &str) -> FileAnalysis {
    let m = index_dir.join("snapshot/manifest.json");
    let mj: serde_json::Value = serde_json::from_str(&fs::read_to_string(m).unwrap()).unwrap();
    let key = mj["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["relative_path"] == path)
        .unwrap()["object_key"]
        .as_str()
        .unwrap()
        .to_string();
    let fa = index_dir.join(format!("snapshot/files/{key}.json"));
    serde_json::from_str(&fs::read_to_string(fa).unwrap()).unwrap()
}

/// Build a "historical" record for `name` in `path`: stable locator + facets
/// computed from the CURRENT bytes at record time (as an exposure would).
fn record(index_dir: &Path, view_root: &Path, path: &str, name: &str) -> (String, DeclFacets) {
    let a = analysis(index_dir, path);
    let d = a.declaration_named(name).expect(name);
    let bytes = fs::read(view_root.join(path)).unwrap();
    (decl_locator(&a, d), decl_facets(&bytes, d))
}

fn rebinder(index_dir: &Path, view: &repodex::view::RepositoryView) -> TargetViewRebinder {
    TargetViewRebinder::open(index_dir, &view.canonical_root, &project_scope(view)).unwrap()
}

fn base_repo(root: &Path) {
    fs::write(root.join("go.mod"), "module example.com/m\n").unwrap();
    fs::write(
        root.join("main.go"),
        "package main\n\nfunc Foo() int { return 1 }\n\nfunc Bar() int { return Foo() }\n",
    )
    .unwrap();
}

#[test]
fn unchanged_symbol_rebinds_exact_fresh_despite_file_change() {
    let s = TempDir::new("vbm-fresh");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    base_repo(&root);
    let o1 = q(&root, s.path(), "Foo");
    let (loc, fac) = record(&o1.ensure.index_dir(), &root, "main.go", "Foo");
    // §17: unrelated code added to the same file — whole-file digest changes but
    // Foo is untouched; navigation usefulness + intrinsic equality must survive.
    fs::write(
        root.join("main.go"),
        "package main\n\nfunc Helper() int { return 0 }\n\nfunc Foo() int { return 1 }\n\nfunc Bar() int { return Foo() }\n",
    )
    .unwrap();
    let o2 = q(&root, s.path(), "Foo");
    let mut rb = rebinder(&o2.ensure.index_dir(), &o2.view);
    let ann = rb.rebind(&loc, "Foo", "main.go", Some(&fac), None, None);
    assert_eq!(ann.state, BindingState::ExactFresh, "{ann:?}");
    assert_eq!(ann.current_path.as_deref(), Some("main.go"));
    assert!(ann.declaration_range.is_some());
}

#[test]
fn body_change_is_changed_implementation_not_fresh() {
    let s = TempDir::new("vbm-impl");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    base_repo(&root);
    let o1 = q(&root, s.path(), "Foo");
    let (loc, fac) = record(&o1.ensure.index_dir(), &root, "main.go", "Foo");
    // Body change, same signature (§18).
    fs::write(
        root.join("main.go"),
        "package main\n\nfunc Foo() int { return 42 }\n\nfunc Bar() int { return Foo() }\n",
    )
    .unwrap();
    let o2 = q(&root, s.path(), "Foo");
    let mut rb = rebinder(&o2.ensure.index_dir(), &o2.view);
    let ann = rb.rebind(&loc, "Foo", "main.go", Some(&fac), None, None);
    assert_eq!(ann.state, BindingState::ChangedImplementation, "{ann:?}");
    // body-dependent evidence must NOT be reused as current (candidate, not fact)
    assert_eq!(ann.evidence_class, "candidate");
}

#[test]
fn signature_change_is_changed_interface() {
    let s = TempDir::new("vbm-iface");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    base_repo(&root);
    let o1 = q(&root, s.path(), "Foo");
    let (loc, fac) = record(&o1.ensure.index_dir(), &root, "main.go", "Foo");
    fs::write(
        root.join("main.go"),
        "package main\n\nfunc Foo(n int) int { return n }\n\nfunc Bar() int { return 0 }\n",
    )
    .unwrap();
    let o2 = q(&root, s.path(), "Foo");
    let mut rb = rebinder(&o2.ensure.index_dir(), &o2.view);
    let ann = rb.rebind(&loc, "Foo", "main.go", Some(&fac), None, None);
    assert_eq!(ann.state, BindingState::ChangedInterface, "{ann:?}");
    assert_eq!(ann.evidence_class, "candidate");
}

#[test]
fn move_rebinds_moved_but_same_with_new_coordinates() {
    let s = TempDir::new("vbm-move");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    base_repo(&root);
    let o1 = q(&root, s.path(), "Foo");
    let (loc, fac) = record(&o1.ensure.index_dir(), &root, "main.go", "Foo");
    // §20: Foo moved to foo.go — same qualified name+facets -> MOVED_BUT_SAME,
    // current path/range (never the old ones).
    fs::write(
        root.join("foo.go"),
        "package main\n\nfunc Foo() int { return 1 }\n",
    )
    .unwrap();
    fs::write(
        root.join("main.go"),
        "package main\n\nfunc Bar() int { return Foo() }\n",
    )
    .unwrap();
    let o2 = q(&root, s.path(), "Foo");
    let mut rb = rebinder(&o2.ensure.index_dir(), &o2.view);
    let ann = rb.rebind(&loc, "Foo", "main.go", Some(&fac), None, None);
    assert_eq!(ann.state, BindingState::MovedButSame, "{ann:?}");
    assert_eq!(ann.current_path.as_deref(), Some("foo.go"));
    assert_eq!(ann.declaration_range.unwrap().path, "foo.go");
}

#[test]
fn removed_symbol_is_absent_not_stale_fact() {
    let s = TempDir::new("vbm-absent");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    base_repo(&root);
    let o1 = q(&root, s.path(), "Foo");
    let (loc, fac) = record(&o1.ensure.index_dir(), &root, "main.go", "Foo");
    fs::write(
        root.join("main.go"),
        "package main\n\nfunc Bar() int { return 0 }\n",
    )
    .unwrap();
    let o2 = q(&root, s.path(), "Bar");
    let mut rb = rebinder(&o2.ensure.index_dir(), &o2.view);
    let ann = rb.rebind(&loc, "Foo", "main.go", Some(&fac), None, None);
    assert_eq!(ann.state, BindingState::Absent, "{ann:?}");
    assert!(ann.current_path.is_none());
    assert_eq!(ann.evidence_class, "candidate");
}

#[test]
fn duplicate_same_name_is_ambiguous_without_facets() {
    let s = TempDir::new("vbm-ambig");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    base_repo(&root);
    // A second `Foo` in another file — same qname, different path (§21-§22).
    fs::write(
        root.join("other.go"),
        "package main\n\nfunc Foo() int { return 9 }\n",
    )
    .unwrap();
    let o = q(&root, s.path(), "Foo");
    // real locator for main.go's Foo (scope-qualified); rebind WITHOUT facets.
    let a = analysis(&o.ensure.index_dir(), "main.go");
    let d = a.declaration_named("Foo").unwrap();
    let loc = decl_locator(&a, d);
    let mut rb = rebinder(&o.ensure.index_dir(), &o.view);
    // locator-only (no facets) + two distinct-path candidates -> AMBIGUOUS.
    let ann = rb.rebind(&loc, "Foo", "main.go", None, None, None);
    assert_eq!(ann.state, BindingState::Ambiguous, "{ann:?}");
}

#[test]
fn project_scope_isolates_unrelated_repos() {
    let a = TempDir::new("vbm-scope-a");
    let b = TempDir::new("vbm-scope-b");
    let ra = a.path().join("repo");
    let rb_ = b.path().join("repo");
    fs::create_dir_all(&ra).unwrap();
    fs::create_dir_all(&rb_).unwrap();
    base_repo(&ra);
    base_repo(&rb_);
    let va = q(&ra, a.path(), "Foo");
    let vb = q(&rb_, b.path(), "Foo");
    // Same paths/symbol names but different non-git roots -> different scopes.
    assert_ne!(project_scope(&va.view), project_scope(&vb.view));
}

// --- §31: memory composition into the production query path -----------------

#[test]
fn symbol_mode_rebinds_and_attaches_current_annotation() {
    let s = TempDir::new("vbm-compose");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    base_repo(&root);
    // ensure index + get view/scope
    let o = q(&root, s.path(), "Foo");
    let scope = project_scope(&o.view);
    let (loc, fac) = record(&o.ensure.index_dir(), &root, "main.go", "Foo");

    // Build a memory store with a scoped entry exposing Foo (locator+facets).
    let memdir = s.path().join("memory");
    let mut ms = repodex::memory::MemoryStore::default();
    let mut sym = repodex::memory::SymbolMemoryEvidence {
        symbol_id: "decl:main.go#0".into(),
        locator: Some(loc.clone()),
        facets: Some(fac.clone()),
        symbol_name: "Foo".into(),
        path: "main.go".into(),
        symbol_kind: "function".into(),
        ..Default::default()
    };
    sym.declaration_exposed = true;
    let mut entry = repodex::memory::MemoryEntry {
        schema: "reposuite.memory-entry.v1".into(),
        schema_version: 1,
        investigation_id: "inv-1".into(),
        session_ids: vec!["s1".into()],
        repository_id: Some("root".into()),
        project_scope: Some(scope.clone()),
        signature: repodex::memory::query_signature("Foo", &Default::default(), None),
        ..Default::default()
    };
    entry.symbols.insert("decl:main.go#0".into(), sym);
    let mut ev = repodex::memory::MemoryEvidence {
        path: "main.go".into(),
        ..Default::default()
    };
    ev.observed = true;
    entry.evidence.insert("main.go".into(), ev);
    ms.entries.insert("inv-1".into(), entry);
    ms.rebuild_postings();
    ms.save(&memdir).unwrap();

    // Query with memory_mode=Symbol -> memory block carries a rebound annotation.
    let p = ViewQueryParams {
        query_text: "Foo",
        mode: QueryMode::Ranked,
        intent: None,
        target: None,
        to: None,
        max_results: 50,
        token_budget: None,
        depth: 4,
        memory_mode: repodex::memory::compose::MemoryMode::Symbol,
        state_override: Some(s.path()),
    };
    let o2 = run_view_query(&ViewLocator::Root(root.to_path_buf()), &p).unwrap();
    assert_eq!(o2.memory.project_scope, scope);
    assert_eq!(o2.memory.matched_investigations, 1);
    assert_eq!(o2.memory.symbols.len(), 1, "{:?}", o2.memory.symbols);
    assert_eq!(o2.memory.symbols[0].state, BindingState::ExactFresh);
    assert_eq!(
        o2.memory.symbols[0].current_path.as_deref(),
        Some("main.go")
    );

    // memory_mode=Off -> empty composition, deterministic query still works.
    let p2 = ViewQueryParams {
        query_text: "Foo",
        mode: QueryMode::Ranked,
        intent: None,
        target: None,
        to: None,
        max_results: 50,
        token_budget: None,
        depth: 4,
        memory_mode: repodex::memory::compose::MemoryMode::Off,
        state_override: Some(s.path()),
    };
    let o3 = run_view_query(&ViewLocator::Root(root.to_path_buf()), &p2).unwrap();
    assert_eq!(o3.memory.mode, "off");
    assert!(o3.memory.symbols.is_empty());
    assert!(!o3.result.seeds.is_empty());
}

#[test]
fn cross_project_memory_never_leaks_into_scoped_query() {
    // Memory recorded under a DIFFERENT project scope must not surface (§4).
    let s = TempDir::new("vbm-iso");
    let root = s.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    base_repo(&root);
    let o = q(&root, s.path(), "Foo");
    let (loc, fac) = record(&o.ensure.index_dir(), &root, "main.go", "Foo");
    let memdir = s.path().join("memory");
    let mut ms = repodex::memory::MemoryStore::default();
    let sym = repodex::memory::SymbolMemoryEvidence {
        symbol_id: "decl:main.go#0".into(),
        locator: Some(loc),
        facets: Some(fac),
        symbol_name: "Foo".into(),
        path: "main.go".into(),
        symbol_kind: "function".into(),
        ..Default::default()
    };
    let mut entry = repodex::memory::MemoryEntry {
        schema: "reposuite.memory-entry.v1".into(),
        schema_version: 1,
        investigation_id: "inv-x".into(),
        session_ids: vec!["s1".into()],
        // stamped with a DIFFERENT project's scope -> must not match.
        project_scope: Some("project:other-repo".into()),
        signature: repodex::memory::query_signature("Foo", &Default::default(), None),
        ..Default::default()
    };
    entry.symbols.insert("decl:main.go#0".into(), sym);
    ms.entries.insert("inv-x".into(), entry);
    ms.rebuild_postings();
    ms.save(&memdir).unwrap();

    let comp = repodex::memory::compose::compose(
        &o.view,
        &o.ensure.index_dir(),
        repodex::memory::compose::MemoryMode::Symbol,
        "Foo",
        &Default::default(),
        s.path(),
        false,
    );
    assert_eq!(comp.matched_investigations, 0);
    assert!(comp.symbols.is_empty());
}
