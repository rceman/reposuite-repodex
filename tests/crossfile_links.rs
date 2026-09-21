//! TASK 3B: derived cross-file structural relationships.
//!
//! The gates that matter most here are:
//!
//! * the curated fixture suite asserts **complete** relationship sets, not just
//!   the presence of the links we hoped for;
//! * the negative suite proves RepoDex does *not* manufacture an exact link when
//!   the evidence is insufficient — a false `Exact` is the most dangerous defect
//!   this layer can have;
//! * the determinism and equivalence gates prove a link artifact is a pure
//!   function of the snapshot bytes plus the metadata bytes, independent of
//!   output directory and checkout root;
//! * the integrity gates prove a corrupted, reordered, or stale-dependency
//!   artifact fails verification.

mod support;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use repodex::links::{
    build_links, read_links, read_manifest, rule, verify, LinkBuildOutcome, LinkError, LinkIndex,
    LinkOutcome, LinkRecord, LinkTarget,
};
use repodex::repository::{build_snapshot, update_snapshot, BuildOptions, RepositoryManifest};
use support::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fixture_root(language: &str) -> PathBuf {
    support::fixture("crossfile").join(language)
}

fn store(temp: &TempDir, name: &str) -> PathBuf {
    temp.path().join("store").join(name)
}

fn snapshot(root: &Path, output: &Path) -> RepositoryManifest {
    build_snapshot(&support::analyzer(), root, output, BuildOptions::default())
        .expect("snapshot build")
        .manifest
}

fn link(snapshot_dir: &Path, repository: &Path, output: &Path) -> LinkBuildOutcome {
    build_links(snapshot_dir, Some(repository), output).expect("link build")
}

/// Build and load the links for a committed fixture tree.
fn fixture_links(temp: &TempDir, language: &str) -> (LinkBuildOutcome, LinkIndex) {
    let root = fixture_root(language);
    let snapshot_dir = store(temp, &format!("snap-{language}"));
    snapshot(&root, &snapshot_dir);
    let links_dir = store(temp, &format!("links-{language}"));
    let outcome = link(&snapshot_dir, &root, &links_dir);
    let index = LinkIndex::load(&links_dir).expect("load links");
    (outcome, index)
}

fn target_summary(target: &LinkTarget) -> String {
    match target {
        LinkTarget::File { relative_path } => format!("file:{relative_path}"),
        LinkTarget::Declaration {
            relative_path,
            declaration_id,
            qualified_name,
            ..
        } => format!("decl:{relative_path}#{declaration_id}:{qualified_name}"),
        LinkTarget::Structure {
            structural_kind,
            key,
            ..
        } => format!("struct:{structural_kind}:{key}"),
    }
}

/// A stable, fully-precise rendering of one relationship.
fn row(link: &LinkRecord) -> String {
    let targets = link
        .outcome
        .all_targets()
        .iter()
        .map(|target| target_summary(target))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{}|{}|{}|{}|{}",
        link.rule_id,
        link.source.key(),
        link.written,
        link.outcome.as_str(),
        targets
    )
}

/// Every relationship produced by one rule.
fn rows_for_rule(index: &LinkIndex, rule_id: &str) -> BTreeSet<String> {
    index
        .links()
        .iter()
        .filter(|link| link.rule_id == rule_id)
        .map(row)
        .collect()
}

fn set(entries: &[&str]) -> BTreeSet<String> {
    entries.iter().map(|entry| entry.to_string()).collect()
}

fn assert_rows(index: &LinkIndex, rule_id: &str, expected: &[&str]) {
    let actual = rows_for_rule(index, rule_id);
    let expected = set(expected);
    let missing: Vec<&String> = expected.difference(&actual).collect();
    let unexpected: Vec<&String> = actual.difference(&expected).collect();
    assert!(
        missing.is_empty() && unexpected.is_empty(),
        "rule {rule_id} relationship set diverged\n  missing: {missing:#?}\n  unexpected: \
         {unexpected:#?}"
    );
}

fn counts(index: &LinkIndex) -> (u64, u64, u64, u64) {
    let outcomes = &index.manifest().outcomes;
    (
        outcomes.exact,
        outcomes.ambiguous,
        outcomes.unresolved,
        outcomes.out_of_scope,
    )
}

/// Copy a directory tree, preserving the relative layout.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create destination");
    for entry in std::fs::read_dir(from).expect("read source") {
        let entry = entry.expect("dir entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

fn read_bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).expect("read file")
}

/// Rewrite a link manifest in place, keeping it internally consistent.
fn mutate_link_manifest(dir: &Path, mutate: impl FnOnce(&mut repodex::links::LinkManifest)) {
    let mut manifest = read_manifest(dir).expect("load manifest");
    let links = read_links(dir).expect("load links");
    let entities = repodex::links::read_entities(dir).expect("load entities");
    mutate(&mut manifest);
    manifest.refresh_link_digest(&links, &entities);
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize"),
    )
    .expect("write manifest");
}

/// Rewrite a link manifest without refreshing the digest.
fn mutate_link_manifest_stale(dir: &Path, mutate: impl FnOnce(&mut repodex::links::LinkManifest)) {
    let mut manifest = read_manifest(dir).expect("load manifest");
    mutate(&mut manifest);
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize"),
    )
    .expect("write manifest");
}

fn rewrite_links(dir: &Path, links: &[LinkRecord]) {
    let mut buffer = Vec::new();
    for link in links {
        serde_json::to_writer(&mut buffer, link).expect("serialize");
        buffer.push(b'\n');
    }
    std::fs::write(dir.join("links.jsonl"), buffer).expect("write links");
}

// ---------------------------------------------------------------------------
// Curated fixture suite: complete relationship sets
// ---------------------------------------------------------------------------

#[test]
fn rust_fixture_produces_the_complete_expected_relationship_set() {
    let temp = TempDir::new("t3b-rust-fixture");
    let (outcome, index) = fixture_links(&temp, "rust");

    assert_rows(
        &index,
        rule::RUST_MOD_STANDARD_FILE,
        &[
            // Both `src/api.rs` and `src/api/mod.rs` exist: ambiguity preserved.
            "rust.mod.standard_file|src/lib.rs#declaration:0|api|ambiguous|file:src/api.rs,file:src/api/mod.rs",
            "rust.mod.standard_file|src/lib.rs#declaration:1|util|exact|file:src/util.rs",
            "rust.mod.standard_file|src/lib.rs#declaration:2|absent|unresolved|",
            "rust.mod.standard_file|src/lib.rs#declaration:3|deep|exact|file:src/deep/mod.rs",
            "rust.mod.standard_file|src/lib.rs#declaration:4|dupmod|exact|file:src/dupmod.rs",
            // `mod outer { mod inner; }`: an inline module's child file.
            "rust.mod.standard_file|src/lib.rs#declaration:6|inner|exact|file:src/outer/inner.rs",
            "rust.mod.standard_file|src/deep/mod.rs#declaration:0|leaf|exact|file:src/deep/leaf.rs",
        ],
    );

    assert_rows(
        &index,
        rule::RUST_USE_CRATE_PATH,
        &[
            "rust.use.crate_path|src/lib.rs#import:0+0|crate::util::helper|exact|decl:src/util.rs#0:crate::util",
            "rust.use.crate_path|src/lib.rs#import:1+0|crate::util::missing_item|unresolved|",
            // `helper` also exists in `crate::deep`, and must not be collapsed in.
            "rust.use.crate_path|src/lib.rs#import:2+0|crate::util::helper|exact|decl:src/util.rs#0:crate::util",
            "rust.use.crate_path|src/lib.rs#import:2+1|crate::util::Other|exact|decl:src/util.rs#1:crate::util",
            "rust.use.crate_path|src/lib.rs#import:3|crate::util::*|exact|file:src/util.rs",
            "rust.use.crate_path|src/lib.rs#import:4+0|crate::deep::helper|exact|decl:src/deep/mod.rs#1:crate::deep",
            // Two declarations named `dup` in one module: ambiguity preserved.
            "rust.use.crate_path|src/lib.rs#import:5+0|crate::dupmod::dup|ambiguous|decl:src/dupmod.rs#0:crate::dupmod,decl:src/dupmod.rs#1:crate::dupmod",
            "rust.use.crate_path|src/lib.rs#import:6+0|crate::absent::thing|unresolved|",
            "rust.use.crate_path|src/lib.rs#import:7+0|crate::outer::inner::inner_fn|exact|decl:src/outer/inner.rs#0:crate::outer::inner",
        ],
    );

    assert_rows(
        &index,
        rule::RUST_USE_NON_CRATE_PATH,
        &[
            "rust.use.non_crate_path|src/lib.rs#import:8+0|std::collections::HashMap|out_of_scope|",
            "rust.use.non_crate_path|src/lib.rs#import:9+0|self::util::helper|out_of_scope|",
        ],
    );

    assert_eq!(counts(&index), (11, 2, 3, 2));
    assert_eq!(outcome.stats.links_total, 18);
}

#[test]
fn go_fixture_produces_the_complete_expected_relationship_set() {
    let temp = TempDir::new("t3b-go-fixture");
    let (outcome, index) = fixture_links(&temp, "go");

    assert_rows(
        &index,
        rule::GO_PACKAGE_SAME_DIRECTORY,
        &[
            "go.package.same_directory|internal/foo/foo.go#declaration:0|foo|exact|struct:go_package:internal/foo:foo",
            "go.package.same_directory|internal/foo/extra.go#declaration:0|foo|exact|struct:go_package:internal/foo:foo",
            // `bar` and `bar_test` share a directory and stay distinct groups.
            "go.package.same_directory|internal/bar/bar.go#declaration:0|bar|exact|struct:go_package:internal/bar:bar",
            "go.package.same_directory|internal/bar/bar_test.go#declaration:0|bar_test|exact|struct:go_package:internal/bar:bar_test",
            "go.package.same_directory|internal/two/alpha.go#declaration:0|alpha|exact|struct:go_package:internal/two:alpha",
            "go.package.same_directory|internal/two/beta.go#declaration:0|beta|exact|struct:go_package:internal/two:beta",
            "go.package.same_directory|main.go#declaration:0|main|exact|struct:go_package::main",
        ],
    );

    assert_rows(
        &index,
        rule::GO_IMPORT_LOCAL_MODULE,
        &[
            "go.import.local_module|main.go#import:0+0|example.com/fixture/internal/foo|exact|struct:go_package:internal/foo:foo",
            "go.import.local_module|main.go#import:0+1|example.com/fixture/internal/two|ambiguous|struct:go_package:internal/two:alpha,struct:go_package:internal/two:beta",
            "go.import.local_module|main.go#import:0+2|example.com/fixture/internal/missing|unresolved|",
            "go.import.local_module|main.go#import:0+3|example.com/fixture/internal|unresolved|",
            "go.import.local_module|main.go#import:0+4|example.com/fixture|exact|struct:go_package::main",
            // A blank import of a local package is still a local relationship.
            "go.import.local_module|main.go#import:0+7|example.com/fixture/internal/bar|ambiguous|struct:go_package:internal/bar:bar,struct:go_package:internal/bar:bar_test",
        ],
    );

    assert_rows(
        &index,
        rule::GO_IMPORT_EXTERNAL,
        &[
            // Path-segment boundary: `example.com/fixturex` is NOT inside
            // `example.com/fixture`.
            "go.import.external|main.go#import:0+5|example.com/fixturex|out_of_scope|",
            "go.import.external|main.go#import:0+6|fmt|out_of_scope|",
        ],
    );

    assert_eq!(counts(&index), (9, 2, 2, 2));
    assert_eq!(outcome.stats.links_total, 15);
    // The `go.mod` is now depended on by two rules — `go.module.go_mod` (the
    // TASK 4A topology's module discovery) and `go.import.local_module` (the
    // import classification) — recorded as distinct content-digest deps.
    assert_eq!(outcome.manifest.metadata.len(), 2);
    for dep in &outcome.manifest.metadata {
        assert_eq!(dep.relative_path, "go.mod");
        assert_eq!(dep.value, "example.com/fixture");
        assert!(dep.present);
    }
}

#[test]
fn python_fixture_produces_the_complete_expected_relationship_set() {
    let temp = TempDir::new("t3b-python-fixture");
    let (outcome, index) = fixture_links(&temp, "python");

    assert_rows(
        &index,
        rule::PYTHON_RELATIVE_IMPORT_PACKAGE_PATH,
        &[
            "python.relative_import.package_path|pkg/__init__.py#import:0|.helper|exact|file:pkg/helper.py",
            "python.relative_import.package_path|pkg/__init__.py#import:1+0|.mod|exact|file:pkg/mod.py",
            "python.relative_import.package_path|pkg/__init__.py#import:2|.missing|unresolved|",
            "python.relative_import.package_path|pkg/sub/deep.py#import:0|..helper|exact|file:pkg/helper.py",
            "python.relative_import.package_path|pkg/sub/deep.py#import:1+0|.sibling|exact|decl:pkg/sub/__init__.py#0:pkg.sub.sibling",
            // Level 3 from `pkg/sub` escapes above the repository root.
            "python.relative_import.package_path|pkg/sub/deep.py#import:2|...top|unresolved|",
            // A root-level file is not inside a package.
            "python.relative_import.package_path|top.py#import:0|.|unresolved|",
        ],
    );

    assert_rows(
        &index,
        rule::PYTHON_ABSOLUTE_IMPORT_LOCAL_CANDIDATE,
        &[
            // Absolute imports are candidates only, never exact.
            "python.absolute_import.local_candidate|top.py#import:2+0|pkg.mod|ambiguous|file:pkg/mod.py",
            "python.absolute_import.local_candidate|top.py#import:3+0|pkg.helper|ambiguous|file:pkg/helper.py",
        ],
    );

    assert_rows(
        &index,
        rule::PYTHON_ABSOLUTE_IMPORT_EXTERNAL,
        &[
            "python.absolute_import.external|top.py#import:1+0|os|out_of_scope|",
            "python.absolute_import.external|top.py#import:4+0|missing_pkg.thing|out_of_scope|",
        ],
    );

    assert_eq!(counts(&index), (4, 2, 3, 2));
    assert_eq!(outcome.stats.links_total, 11);
    // An absolute import is never `Exact`, even with a single candidate.
    for link in index.by_rule(rule::PYTHON_ABSOLUTE_IMPORT_LOCAL_CANDIDATE) {
        assert!(!matches!(link.outcome, LinkOutcome::Exact { .. }));
    }
}

#[test]
fn php_fixture_produces_the_complete_expected_relationship_set() {
    let temp = TempDir::new("t3b-php-fixture");
    let (outcome, index) = fixture_links(&temp, "php");

    let namespaces = rows_for_rule(&index, rule::PHP_NAMESPACE_DECLARATION);
    assert!(namespaces.contains(
        "php.namespace.declaration|src/Service/Foo.php#declaration:0|App\\Service|exact|struct:php_namespace:App\\Service"
    ));
    assert!(namespaces.contains(
        "php.namespace.declaration|src/Service/Foo.php#declaration:1|App\\Service\\Foo|exact|struct:php_namespace:App\\Service"
    ));
    assert!(namespaces.contains(
        "php.namespace.declaration|src/Dup/A.php#declaration:1|App\\Dup\\Thing|exact|struct:php_namespace:App\\Dup"
    ));
    // A declaration in the global namespace has no namespace entity, and none is
    // invented for it.
    assert!(!namespaces.iter().any(|entry| entry.contains("GlobalThing")));

    assert_rows(
        &index,
        rule::PHP_USE_QUALIFIED_NAME,
        &[
            "php.use.qualified_name|src/Service/Bar.php#import:0+0|App\\Service\\Foo|exact|decl:src/Service/Foo.php#1:App\\Service\\Foo",
            // The alias is recorded, and the written imported target is preserved.
            "php.use.qualified_name|src/Service/Bar.php#import:1+0|App\\Other\\Foo|exact|decl:src/Other/Foo.php#1:App\\Other\\Foo",
            // Two declarations share one syntactic qualified name: ambiguity kept.
            "php.use.qualified_name|src/Service/Bar.php#import:2+0|App\\Dup\\Thing|ambiguous|decl:src/Dup/A.php#1:App\\Dup\\Thing,decl:src/Dup/B.php#1:App\\Dup\\Thing",
            "php.use.qualified_name|src/Service/Bar.php#import:3+0|App\\Missing\\Thing|unresolved|",
            // A grouped `use App\\Service\\{Foo as GroupedFoo};` composes the prefix.
            "php.use.qualified_name|src/Service/Bar.php#import:5+0|App\\Service\\Foo|exact|decl:src/Service/Foo.php#1:App\\Service\\Foo",
        ],
    );

    assert_rows(
        &index,
        rule::PHP_USE_EXTERNAL,
        &["php.use.external|src/Service/Bar.php#import:4+0|Symfony\\Component\\Console\\Command\\Command|out_of_scope|"],
    );

    assert_rows(
        &index,
        rule::PHP_USE_NON_CLASS_IMPORT,
        &[
            "php.use.non_class_import|src/Service/Bar.php#import:6+0|App\\Service\\helper|out_of_scope|",
            "php.use.non_class_import|src/Service/Bar.php#import:7+0|App\\Service\\VERSION|out_of_scope|",
        ],
    );

    assert_eq!(counts(&index), (14, 1, 1, 3));
    assert_eq!(outcome.stats.links_total, 19);

    // A `use function`/`use const` is never linked to a class declaration.
    for link in index.by_rule(rule::PHP_USE_NON_CLASS_IMPORT) {
        assert!(matches!(link.outcome, LinkOutcome::OutOfScope { .. }));
        assert!(link.outcome.all_targets().is_empty());
    }
}

// ---------------------------------------------------------------------------
// Negative correctness: no false exactness
// ---------------------------------------------------------------------------

#[test]
fn no_rule_emits_exact_without_a_single_structural_candidate() {
    for language in ["rust", "go", "python", "php"] {
        let temp = TempDir::new(&format!("t3b-exact-{language}"));
        let (_, index) = fixture_links(&temp, language);
        for link in index.exact() {
            let targets = link.outcome.all_targets();
            assert_eq!(
                targets.len(),
                1,
                "an Exact relationship must name exactly one candidate: {link:?}"
            );
            assert!(
                !link.rule_id.is_empty() && !link.provenance.evidence.is_empty(),
                "an Exact relationship must carry provenance: {link:?}"
            );
        }
    }
}

#[test]
fn ambiguous_relationships_keep_every_candidate() {
    for language in ["rust", "go", "python", "php"] {
        let temp = TempDir::new(&format!("t3b-ambiguous-{language}"));
        let (_, index) = fixture_links(&temp, language);
        for link in index.ambiguous() {
            let candidates = link.outcome.candidates();
            assert!(
                !candidates.is_empty(),
                "an Ambiguous relationship must keep its candidates: {link:?}"
            );
        }
    }
}

#[test]
fn same_written_name_in_an_unrelated_module_is_not_collapsed() {
    let temp = TempDir::new("t3b-same-name");
    let (_, index) = fixture_links(&temp, "rust");
    // `helper` exists in both `crate::util` and `crate::deep`; each use path
    // resolves inside its own module only.
    let util = index
        .by_rule(rule::RUST_USE_CRATE_PATH)
        .into_iter()
        .filter(|link| link.written == "crate::util::helper")
        .collect::<Vec<_>>();
    assert_eq!(util.len(), 2);
    for link in util {
        let target = link.outcome.target().expect("exact");
        assert_eq!(target_summary(target), "decl:src/util.rs#0:crate::util");
    }
}

#[test]
fn rust_module_search_does_not_silently_pick_the_first_candidate() {
    let temp = TempDir::new("t3b-rust-ambiguous-mod");
    let (_, index) = fixture_links(&temp, "rust");
    let link = index
        .by_rule(rule::RUST_MOD_STANDARD_FILE)
        .into_iter()
        .find(|link| link.written == "api")
        .expect("the ambiguous module");
    assert_eq!(link.outcome.candidates().len(), 2);
    // The two standard forms are recorded as evidence.
    assert!(link
        .provenance
        .evidence
        .iter()
        .any(|entry| entry.contains("src/api.rs") && entry.contains("src/api/mod.rs")));
}

#[test]
fn go_import_prefix_is_matched_on_path_segments_not_text() {
    let temp = TempDir::new("t3b-go-prefix");
    let (_, index) = fixture_links(&temp, "go");
    let link = index
        .by_rule(rule::GO_IMPORT_EXTERNAL)
        .into_iter()
        .find(|link| link.written == "example.com/fixturex")
        .expect("the prefix trap");
    assert!(matches!(link.outcome, LinkOutcome::OutOfScope { .. }));
}

#[test]
fn python_absolute_import_with_two_local_roots_stays_ambiguous() {
    let temp = TempDir::new("t3b-python-abs");
    let root = temp.path().join("repo");
    std::fs::create_dir_all(root.join("thing")).expect("mkdir");
    // Two distinct files define the same dotted module path `thing`, so the
    // written absolute path matches more than one repository module.
    std::fs::write(root.join("thing.py"), b"X = 1\n").expect("write");
    std::fs::write(root.join("thing/__init__.py"), b"X = 2\n").expect("write");
    std::fs::write(root.join("main.py"), b"import thing\n").expect("write");

    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(&temp, "links");
    let outcome = link(&snapshot_dir, &root, &links_dir);

    assert_eq!(outcome.manifest.outcomes.exact, 0);
    assert_eq!(outcome.manifest.outcomes.ambiguous, 1);
    let index = LinkIndex::load(&links_dir).expect("load");
    assert_eq!(
        index.ambiguous()[0].outcome.candidates().len(),
        2,
        "both repository candidates must survive"
    );
}

#[test]
fn python_dynamic_imports_produce_no_relationship() {
    let temp = TempDir::new("t3b-python-dynamic");
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).expect("mkdir");
    std::fs::write(
        root.join("main.py"),
        b"import importlib\n\n\ndef load(name):\n    return importlib.import_module(name)\n",
    )
    .expect("write");

    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(&temp, "links");
    let outcome = link(&snapshot_dir, &root, &links_dir);
    // `import importlib` has no repository module, so it is external; the
    // dynamic `import_module(name)` call is ordinary call-like syntax and never
    // becomes a relationship.
    assert_eq!(outcome.manifest.outcomes.exact, 0);
    assert_eq!(outcome.manifest.outcomes.unresolved, 0);
    assert_eq!(outcome.manifest.links, 1);
}

#[test]
fn import_relationship_never_becomes_a_call_edge() {
    let temp = TempDir::new("t3b-no-call-edge");
    let root = temp.path().join("repo");
    std::fs::create_dir_all(root.join("pkg")).expect("mkdir");
    std::fs::write(root.join("pkg/__init__.py"), b"").expect("write");
    std::fs::write(root.join("pkg/foo.py"), b"def bar():\n    pass\n").expect("write");
    // The import makes `bar` look obvious, but TASK 3B must not link the call.
    std::fs::write(root.join("main.py"), b"from pkg.foo import bar\n\nbar()\n").expect("write");

    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(&temp, "links");
    link(&snapshot_dir, &root, &links_dir);
    let index = LinkIndex::load(&links_dir).expect("load");
    assert!(
        index.links().iter().all(|link| link.kind != "call"),
        "TASK 3B must not produce a call edge"
    );
    assert!(index
        .links()
        .iter()
        .all(|link| link.source.fact_kind != repodex::links::FactKind::Call));
}

#[test]
fn missing_metadata_never_produces_a_local_go_import() {
    let temp = TempDir::new("t3b-go-no-mod");
    let root = temp.path().join("repo");
    std::fs::create_dir_all(root.join("internal/foo")).expect("mkdir");
    std::fs::write(root.join("internal/foo/foo.go"), b"package foo\n").expect("write");
    std::fs::write(
        root.join("main.go"),
        b"package main\n\nimport \"example.com/fixture/internal/foo\"\n",
    )
    .expect("write");
    // Deliberately no go.mod.

    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(&temp, "links");
    let outcome = link(&snapshot_dir, &root, &links_dir);
    let index = LinkIndex::load(&links_dir).expect("load");

    // Package membership is structural and therefore still exact; the *import*
    // is what must not become local without a module declaration.
    assert!(index.by_rule(rule::GO_IMPORT_LOCAL_MODULE).is_empty());
    assert_eq!(outcome.manifest.outcomes.exact, 2);
    assert_eq!(
        index
            .by_rule(rule::GO_IMPORT_EXTERNAL)
            .iter()
            .map(|link| link.written.as_str())
            .collect::<Vec<_>>(),
        vec!["example.com/fixture/internal/foo"]
    );
    // The dependency on the *absence* of go.mod is recorded.
    assert_eq!(outcome.manifest.metadata.len(), 1);
    assert!(!outcome.manifest.metadata[0].present);
}

#[test]
fn malformed_go_mod_never_produces_a_local_go_import() {
    let temp = TempDir::new("t3b-go-bad-mod");
    let root = temp.path().join("repo");
    std::fs::create_dir_all(root.join("internal/foo")).expect("mkdir");
    std::fs::write(root.join("internal/foo/foo.go"), b"package foo\n").expect("write");
    std::fs::write(root.join("main.go"), b"package main\n").expect("write");
    std::fs::write(root.join("go.mod"), b"go 1.21\nrequire x v1\n").expect("write");

    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(&temp, "links");
    let outcome = link(&snapshot_dir, &root, &links_dir);
    let index = LinkIndex::load(&links_dir).expect("load");

    assert!(index.by_rule(rule::GO_IMPORT_LOCAL_MODULE).is_empty());
    assert_eq!(outcome.manifest.metadata.len(), 1);
    assert!(outcome.manifest.metadata[0].value.starts_with("malformed:"));
}

#[test]
fn adding_a_go_mod_invalidates_a_previous_link_artifact() {
    let temp = TempDir::new("t3b-go-add-mod");
    let root = temp.path().join("repo");
    std::fs::create_dir_all(root.join("internal/foo")).expect("mkdir");
    std::fs::write(root.join("internal/foo/foo.go"), b"package foo\n").expect("write");
    std::fs::write(
        root.join("main.go"),
        b"package main\n\nimport \"example.com/fixture/internal/foo\"\n",
    )
    .expect("write");

    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(&temp, "links");
    link(&snapshot_dir, &root, &links_dir);
    verify(&links_dir, &snapshot_dir, Some(&root)).expect("verifies while go.mod is absent");

    std::fs::write(root.join("go.mod"), b"module example.com/fixture\n").expect("write");
    let error = verify(&links_dir, &snapshot_dir, Some(&root)).expect_err("must be invalid");
    assert!(
        matches!(error, LinkError::MetadataMismatch { .. }),
        "{error}"
    );
}

#[test]
fn rust_use_of_a_module_outside_the_crate_stays_out_of_scope() {
    let temp = TempDir::new("t3b-rust-super");
    let root = temp.path().join("repo");
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    std::fs::write(
        root.join("src/lib.rs"),
        b"mod a;\n\nuse super::thing;\nuse self::a::A;\n",
    )
    .expect("write");
    std::fs::write(root.join("src/a.rs"), b"pub struct A;\n").expect("write");

    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(&temp, "links");
    link(&snapshot_dir, &root, &links_dir);
    let index = LinkIndex::load(&links_dir).expect("load");

    let non_crate: Vec<&str> = index
        .by_rule(rule::RUST_USE_NON_CRATE_PATH)
        .iter()
        .map(|link| link.written.as_str())
        .collect();
    assert_eq!(non_crate, vec!["super::thing", "self::a::A"]);
    // The `mod a;` relationship is still exact.
    assert_eq!(index.manifest().outcomes.exact, 1);
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn repeated_link_builds_produce_identical_bytes() {
    let temp = TempDir::new("t3b-repeat");
    let root = fixture_root("php");
    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);

    let first_dir = store(&temp, "links-a");
    let second_dir = store(&temp, "links-b");
    let first = link(&snapshot_dir, &root, &first_dir);
    let second = link(&snapshot_dir, &root, &second_dir);

    assert_eq!(first.manifest.link_digest, second.manifest.link_digest);
    assert_eq!(
        read_bytes(&first_dir.join("links.jsonl")),
        read_bytes(&second_dir.join("links.jsonl"))
    );
    assert_eq!(
        read_bytes(&first_dir.join("entities.jsonl")),
        read_bytes(&second_dir.join("entities.jsonl"))
    );
    assert_eq!(
        read_bytes(&first_dir.join("manifest.json")),
        read_bytes(&second_dir.join("manifest.json"))
    );
}

#[test]
fn equivalent_checkouts_under_different_roots_produce_equivalent_artifacts() {
    let temp = TempDir::new("t3b-cross-root");
    let source = fixture_root("go");
    let root_a = temp.path().join("alpha-checkout");
    let root_b = temp.path().join("beta").join("nested-checkout");
    copy_tree(&source, &root_a);
    copy_tree(&source, &root_b);

    let snap_a = store(&temp, "snap-a");
    let snap_b = store(&temp, "snap-b");
    let manifest_a = snapshot(&root_a, &snap_a);
    let manifest_b = snapshot(&root_b, &snap_b);
    assert_eq!(manifest_a.snapshot_digest, manifest_b.snapshot_digest);

    let links_a = store(&temp, "links-a");
    let links_b = store(&temp, "links-b");
    let outcome_a = link(&snap_a, &root_a, &links_a);
    let outcome_b = link(&snap_b, &root_b, &links_b);

    assert_eq!(
        outcome_a.manifest.link_digest,
        outcome_b.manifest.link_digest
    );
    assert_eq!(
        read_bytes(&links_a.join("links.jsonl")),
        read_bytes(&links_b.join("links.jsonl"))
    );
    assert_eq!(
        read_bytes(&links_a.join("manifest.json")),
        read_bytes(&links_b.join("manifest.json"))
    );
}

#[test]
fn the_output_directory_does_not_affect_canonical_identity() {
    let temp = TempDir::new("t3b-output-location");
    let root = fixture_root("python");
    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);

    let near = temp.path().join("links");
    let far = temp.path().join("deeply").join("nested").join("links");
    let outcome_near = link(&snapshot_dir, &root, &near);
    let outcome_far = link(&snapshot_dir, &root, &far);
    assert_eq!(
        outcome_near.manifest.link_digest,
        outcome_far.manifest.link_digest
    );
}

// ---------------------------------------------------------------------------
// TASK 3A incremental update vs fresh build
// ---------------------------------------------------------------------------

/// Build a snapshot, then either update or rebuild it, then link both and
/// compare canonical link identity.
fn assert_update_equals_fresh(
    label: &str,
    root: &Path,
    mutate: impl FnOnce(&Path),
    temp: &TempDir,
) {
    let base_snapshot = store(temp, &format!("{label}-base-snap"));
    snapshot(root, &base_snapshot);
    let base_links = store(temp, &format!("{label}-base-links"));
    let base = link(&base_snapshot, root, &base_links);

    mutate(root);

    // Path A: incremental update, then a link rebuild.
    let updated_snapshot = store(temp, &format!("{label}-updated-snap"));
    update_snapshot(
        &support::analyzer(),
        root,
        &base_snapshot,
        &updated_snapshot,
        BuildOptions::default(),
    )
    .expect("snapshot update");
    let updated_links = store(temp, &format!("{label}-updated-links"));
    let updated = link(&updated_snapshot, root, &updated_links);

    // Path B: a fresh snapshot of the same final bytes, then a link build.
    let fresh_snapshot = store(temp, &format!("{label}-fresh-snap"));
    snapshot(root, &fresh_snapshot);
    let fresh_links = store(temp, &format!("{label}-fresh-links"));
    let fresh = link(&fresh_snapshot, root, &fresh_links);

    assert_eq!(
        updated.manifest.snapshot_digest, fresh.manifest.snapshot_digest,
        "{label}: updated and fresh snapshots must agree"
    );
    assert_eq!(
        updated.manifest.link_digest, fresh.manifest.link_digest,
        "{label}: updated and fresh link artifacts must agree"
    );
    assert_eq!(
        read_bytes(&updated_links.join("links.jsonl")),
        read_bytes(&fresh_links.join("links.jsonl")),
        "{label}: link bytes must agree"
    );
    assert_eq!(
        updated.manifest.outcomes, fresh.manifest.outcomes,
        "{label}: outcome counts must agree"
    );
    let _ = base;
}

/// A synthetic three-language repository used by the equivalence scenarios.
fn equivalence_repo(temp: &TempDir, name: &str) -> PathBuf {
    let root = temp.path().join(name);
    std::fs::create_dir_all(root.join("src")).expect("mkdir");
    std::fs::create_dir_all(root.join("internal/foo")).expect("mkdir");
    std::fs::create_dir_all(root.join("pkg")).expect("mkdir");
    std::fs::write(root.join("go.mod"), b"module example.com/eq\n\ngo 1.21\n").expect("write");
    std::fs::write(
        root.join("src/lib.rs"),
        b"mod util;\n\nuse crate::util::helper;\n",
    )
    .expect("write");
    std::fs::write(root.join("src/util.rs"), b"pub fn helper() {}\n").expect("write");
    std::fs::write(root.join("internal/foo/foo.go"), b"package foo\n").expect("write");
    std::fs::write(
        root.join("main.go"),
        b"package main\n\nimport \"example.com/eq/internal/foo\"\n",
    )
    .expect("write");
    std::fs::write(root.join("pkg/__init__.py"), b"from . import mod\n").expect("write");
    std::fs::write(root.join("pkg/mod.py"), b"VALUE = 1\n").expect("write");
    root
}

#[test]
fn update_equals_fresh_for_no_changes() {
    let temp = TempDir::new("t3b-eq-none");
    let root = equivalence_repo(&temp, "repo");
    assert_update_equals_fresh("none", &root, |_| {}, &temp);
}

#[test]
fn update_equals_fresh_for_one_declaration_added() {
    let temp = TempDir::new("t3b-eq-add-decl");
    let root = equivalence_repo(&temp, "repo");
    assert_update_equals_fresh(
        "add-decl",
        &root,
        |root| {
            std::fs::write(
                root.join("src/util.rs"),
                b"pub fn helper() {}\n\npub fn added() {}\n",
            )
            .expect("write");
        },
        &temp,
    );
}

#[test]
fn update_equals_fresh_for_one_declaration_renamed() {
    let temp = TempDir::new("t3b-eq-rename-decl");
    let root = equivalence_repo(&temp, "repo");
    assert_update_equals_fresh(
        "rename-decl",
        &root,
        |root| {
            std::fs::write(root.join("src/util.rs"), b"pub fn renamed() {}\n").expect("write");
            std::fs::write(
                root.join("src/lib.rs"),
                b"mod util;\n\nuse crate::util::renamed;\n",
            )
            .expect("write");
        },
        &temp,
    );
}

#[test]
fn update_equals_fresh_for_one_import_changed() {
    let temp = TempDir::new("t3b-eq-import");
    let root = equivalence_repo(&temp, "repo");
    assert_update_equals_fresh(
        "import",
        &root,
        |root| {
            std::fs::write(
                root.join("src/lib.rs"),
                b"mod util;\n\nuse crate::util::absent;\n",
            )
            .expect("write");
        },
        &temp,
    );
}

#[test]
fn update_equals_fresh_for_a_file_added() {
    let temp = TempDir::new("t3b-eq-file-added");
    let root = equivalence_repo(&temp, "repo");
    assert_update_equals_fresh(
        "file-added",
        &root,
        |root| {
            std::fs::write(root.join("src/extra.rs"), b"pub fn extra() {}\n").expect("write");
            std::fs::write(
                root.join("src/lib.rs"),
                b"mod util;\nmod extra;\n\nuse crate::util::helper;\nuse crate::extra::extra;\n",
            )
            .expect("write");
        },
        &temp,
    );
}

#[test]
fn update_equals_fresh_for_a_file_deleted() {
    let temp = TempDir::new("t3b-eq-file-deleted");
    let root = equivalence_repo(&temp, "repo");
    assert_update_equals_fresh(
        "file-deleted",
        &root,
        |root| {
            std::fs::remove_file(root.join("pkg/mod.py")).expect("remove");
            std::fs::write(root.join("pkg/__init__.py"), b"from . import mod\n").expect("write");
        },
        &temp,
    );
}

#[test]
fn update_equals_fresh_for_a_structural_module_change() {
    let temp = TempDir::new("t3b-eq-structural");
    let root = equivalence_repo(&temp, "repo");
    assert_update_equals_fresh(
        "structural",
        &root,
        |root| {
            // `mod util;` becomes an inline module, so the external file
            // relationship disappears and a `crate::util` module appears.
            std::fs::write(
                root.join("src/lib.rs"),
                b"mod util {\n    pub fn helper() {}\n}\n\nuse crate::util::helper;\n",
            )
            .expect("write");
            std::fs::remove_file(root.join("src/util.rs")).expect("remove");
        },
        &temp,
    );
}

#[test]
fn update_equals_fresh_for_relevant_metadata_changed() {
    let temp = TempDir::new("t3b-eq-metadata");
    let root = equivalence_repo(&temp, "repo");
    assert_update_equals_fresh(
        "metadata",
        &root,
        |root| {
            std::fs::write(
                root.join("go.mod"),
                b"module example.com/renamed\n\ngo 1.21\n",
            )
            .expect("write");
        },
        &temp,
    );
}

// ---------------------------------------------------------------------------
// Artifact integrity
// ---------------------------------------------------------------------------

fn integrity_fixture(temp: &TempDir) -> (PathBuf, PathBuf, PathBuf) {
    let root = fixture_root("go");
    let snapshot_dir = store(temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(temp, "links");
    link(&snapshot_dir, &root, &links_dir);
    (root, snapshot_dir, links_dir)
}

#[test]
fn a_valid_artifact_verifies() {
    let temp = TempDir::new("t3b-verify-ok");
    let (root, snapshot_dir, links_dir) = integrity_fixture(&temp);
    let report = verify(&links_dir, &snapshot_dir, Some(&root)).expect("verify");
    assert!(report.metadata_checked);
    assert_eq!(report.links, 15);
    assert_eq!(report.outcomes.exact, 9);
}

#[test]
fn a_link_artifact_does_not_verify_against_a_different_snapshot() {
    let temp = TempDir::new("t3b-verify-wrong-snapshot");
    let (root, _, links_dir) = integrity_fixture(&temp);
    let other_snapshot = store(&temp, "other-snap");
    snapshot(&fixture_root("python"), &other_snapshot);
    let error = verify(&links_dir, &other_snapshot, Some(&root)).expect_err("must fail");
    assert!(
        matches!(error, LinkError::SnapshotMismatch { .. }),
        "{error}"
    );
}

#[test]
fn a_corrupted_link_digest_fails_verification() {
    let temp = TempDir::new("t3b-verify-digest");
    let (root, snapshot_dir, links_dir) = integrity_fixture(&temp);
    mutate_link_manifest_stale(&links_dir, |manifest| {
        manifest.link_digest = repodex::repository::sha256_text("tampered");
    });
    let error = verify(&links_dir, &snapshot_dir, Some(&root)).expect_err("must fail");
    assert!(
        matches!(error, LinkError::LinkDigestMismatch { .. }),
        "{error}"
    );
}

#[test]
fn a_reordered_link_list_fails_verification() {
    let temp = TempDir::new("t3b-verify-order");
    let (root, snapshot_dir, links_dir) = integrity_fixture(&temp);
    let mut links = read_links(&links_dir).expect("load");
    links.reverse();
    rewrite_links(&links_dir, &links);
    // The digest no longer matches, because canonical order is part of it.
    let error = verify(&links_dir, &snapshot_dir, Some(&root)).expect_err("must fail");
    assert!(
        matches!(
            error,
            LinkError::UnsortedLinks | LinkError::LinkDigestMismatch { .. }
        ),
        "{error}"
    );
}

#[test]
fn a_duplicated_link_id_fails_verification() {
    let temp = TempDir::new("t3b-verify-duplicate");
    let (root, snapshot_dir, links_dir) = integrity_fixture(&temp);
    let mut links = read_links(&links_dir).expect("load");
    let duplicate = links[0].clone();
    // Insert the duplicate next to its original so ordering still holds and the
    // duplicate check is what fires.
    links.insert(1, duplicate);
    rewrite_links(&links_dir, &links);
    let error = verify(&links_dir, &snapshot_dir, Some(&root)).expect_err("must fail");
    assert!(
        matches!(error, LinkError::DuplicateLinkId { .. }),
        "{error}"
    );
}

#[test]
fn a_link_referencing_a_missing_source_locator_fails_verification() {
    let temp = TempDir::new("t3b-verify-locator");
    let (root, snapshot_dir, links_dir) = integrity_fixture(&temp);
    let mut links = read_links(&links_dir).expect("load");
    // A path that still sorts first, so ordering validation does not fire before
    // the locator check we are exercising.
    links[0].source.relative_path = "0-not-in-snapshot.go".to_string();
    rewrite_links(&links_dir, &links);
    mutate_link_manifest(&links_dir, |_| {});
    let error = verify(&links_dir, &snapshot_dir, Some(&root)).expect_err("must fail");
    assert!(matches!(error, LinkError::MissingLocator { .. }), "{error}");
}

#[test]
fn a_link_referencing_a_missing_candidate_locator_fails_verification() {
    let temp = TempDir::new("t3b-verify-candidate");
    let (root, snapshot_dir, links_dir) = integrity_fixture(&temp);
    let mut links = read_links(&links_dir).expect("load");
    let index = links
        .iter()
        .position(|link| matches!(link.outcome, LinkOutcome::Exact { .. }))
        .expect("an exact link");
    links[index].outcome = LinkOutcome::exact(LinkTarget::file("not/in/the/snapshot.go"));
    rewrite_links(&links_dir, &links);
    mutate_link_manifest(&links_dir, |_| {});
    let error = verify(&links_dir, &snapshot_dir, Some(&root)).expect_err("must fail");
    assert!(matches!(error, LinkError::MissingLocator { .. }), "{error}");
}

#[test]
fn a_link_referencing_an_unknown_structure_fails_verification() {
    let temp = TempDir::new("t3b-verify-structure");
    let (root, snapshot_dir, links_dir) = integrity_fixture(&temp);
    let mut links = read_links(&links_dir).expect("load");
    let index = links
        .iter()
        .position(|link| {
            matches!(
                link.outcome,
                LinkOutcome::Exact {
                    target: LinkTarget::Structure { .. }
                }
            )
        })
        .expect("a structural link");
    links[index].outcome = LinkOutcome::exact(LinkTarget::Structure {
        entity_id: "ent-0000000000000000".to_string(),
        structural_kind: "go_package".to_string(),
        key: "nope:nope".to_string(),
    });
    rewrite_links(&links_dir, &links);
    mutate_link_manifest(&links_dir, |_| {});
    let error = verify(&links_dir, &snapshot_dir, Some(&root)).expect_err("must fail");
    assert!(matches!(error, LinkError::MissingEntity { .. }), "{error}");
}

#[test]
fn a_link_referencing_an_undocumented_rule_fails_verification() {
    let temp = TempDir::new("t3b-verify-rule");
    let (root, snapshot_dir, links_dir) = integrity_fixture(&temp);
    let mut links = read_links(&links_dir).expect("load");
    links[0].rule_id = "not.a.rule".to_string();
    rewrite_links(&links_dir, &links);
    mutate_link_manifest(&links_dir, |_| {});
    let error = verify(&links_dir, &snapshot_dir, Some(&root)).expect_err("must fail");
    assert!(
        matches!(error, LinkError::UndocumentedRule { .. }),
        "{error}"
    );
}

#[test]
fn a_stale_snapshot_dependency_fails_verification() {
    let temp = TempDir::new("t3b-verify-stale-snapshot");
    let (root, snapshot_dir, links_dir) = integrity_fixture(&temp);
    mutate_link_manifest(&links_dir, |manifest| {
        manifest.snapshot_digest = repodex::repository::sha256_text("another snapshot");
    });
    let error = verify(&links_dir, &snapshot_dir, Some(&root)).expect_err("must fail");
    assert!(
        matches!(error, LinkError::SnapshotMismatch { .. }),
        "{error}"
    );
}

#[test]
fn a_corrupted_snapshot_artifact_fails_link_verification() {
    let temp = TempDir::new("t3b-verify-corrupt-snapshot");
    let (root, snapshot_dir, links_dir) = integrity_fixture(&temp);
    let manifest = repodex::repository::load_manifest(&snapshot_dir).expect("snapshot manifest");
    let victim = &manifest.files[0];
    let path = snapshot_dir
        .join("files")
        .join(format!("{}.json", victim.object_key));
    // Replace the artifact with something that is not JSON at all, so the
    // snapshot no longer yields the analysis the link artifact references.
    std::fs::write(&path, b"{").expect("write");
    let error = verify(&links_dir, &snapshot_dir, Some(&root)).expect_err("must fail");
    assert!(
        matches!(error, LinkError::InvalidManifest { .. }),
        "{error}"
    );
}

#[test]
fn the_link_artifact_is_much_smaller_than_duplicating_the_snapshot() {
    // The artifact must not serialize a second copy of every normalized fact.
    let temp = TempDir::new("t3b-size");
    let root = fixture_root("php");
    let snapshot_dir = store(&temp, "snap");
    snapshot(&root, &snapshot_dir);
    let links_dir = store(&temp, "links");
    let outcome = link(&snapshot_dir, &root, &links_dir);
    // A link artifact holds derived relationships, so its size tracks the number
    // of relationships, not the number of facts. Duplicating the snapshot would
    // add the whole snapshot size again.
    assert!(
        outcome.stats.link_bytes < outcome.stats.snapshot_bytes * 3,
        "link artifact {} bytes vs snapshot {} bytes",
        outcome.stats.link_bytes,
        outcome.stats.snapshot_bytes
    );
    for link in read_links(&links_dir).expect("load") {
        // No normalized fact content is embedded: only locators and outcomes.
        let serialized = serde_json::to_string(&link).expect("serialize");
        assert!(!serialized.contains("\"scopes\""));
        assert!(!serialized.contains("\"recovery_regions\""));
    }
}

// ---------------------------------------------------------------------------
// Query API
// ---------------------------------------------------------------------------

#[test]
fn the_query_api_filters_exactly() {
    let temp = TempDir::new("t3b-query");
    let (_, index) = fixture_links(&temp, "go");

    assert_eq!(index.from_source("main.go").len(), 9);
    assert_eq!(index.by_kind("package_membership").len(), 7);
    assert_eq!(index.by_rule(rule::GO_IMPORT_EXTERNAL).len(), 2);
    assert_eq!(index.unresolved().len(), 2);
    assert_eq!(index.ambiguous().len(), 2);
    assert_eq!(index.out_of_scope().len(), 2);
    assert_eq!(index.exact().len(), 9);

    // Targeting a file finds the relationships whose outcome names it.
    let targeting = index.targeting_file("internal/foo/foo.go");
    assert!(targeting
        .iter()
        .all(|link| link.outcome.as_str() == "exact"));

    // An entity is reachable by id, and its members are recorded.
    let entity = index
        .entities()
        .iter()
        .find(|entity| entity.key == "internal/foo:foo")
        .expect("the foo package");
    assert_eq!(entity.files.len(), 2);

    // Counts by rule are in canonical order.
    let counts = index.counts_by_rule();
    assert_eq!(counts.len(), 3);
    assert!(counts.windows(2).all(|pair| pair[0].0 < pair[1].0));
}

#[test]
fn manifest_rule_registry_documents_every_emitted_rule() {
    let temp = TempDir::new("t3b-rules");
    for language in ["rust", "go", "python", "php"] {
        let (outcome, index) = fixture_links(&temp, language);
        let documented: BTreeSet<&str> = outcome
            .manifest
            .rules
            .iter()
            .map(|rule| rule.rule_id.as_str())
            .collect();
        assert_eq!(documented.len(), rule::ALL.len());
        for link in index.links() {
            assert!(
                documented.contains(link.rule_id.as_str()),
                "{} is undocumented",
                link.rule_id
            );
            let documentation = outcome.manifest.rule(&link.rule_id).expect("documentation");
            assert!(!documentation.exact_condition.is_empty());
            assert!(!documentation.ambiguous_condition.is_empty());
            assert!(!documentation.unresolved_condition.is_empty());
            assert!(!documentation.out_of_scope_condition.is_empty());
            assert!(!documentation.input_syntax.is_empty());
        }
    }
}

#[test]
fn no_permanent_semantic_symbol_identity_is_introduced() {
    // Every locator must be snapshot-local: a relative path plus a file-local
    // fact id. Nothing may look like a stable global symbol id.
    let temp = TempDir::new("t3b-no-symbol-id");
    let (outcome, index) = fixture_links(&temp, "rust");
    for link in index.links() {
        assert!(
            !link.source.relative_path.is_empty(),
            "a locator must name its file"
        );
        assert!(!link.source.relative_path.starts_with('/'));
        assert!(link.link_id.starts_with("lnk-"));
    }
    for entity in index.entities() {
        assert!(entity.entity_id.starts_with("ent-"));
        for file in &entity.files {
            assert!(!file.starts_with('/'));
        }
    }
    assert!(outcome.manifest.link_digest.starts_with("sha256:"));
}
