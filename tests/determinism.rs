//! Determinism tests.
//!
//! Canonical facts must be a pure function of the source bytes. Filesystem
//! discovery order, hash-map iteration order, absolute checkout paths, random
//! identifiers and timestamps must never leak into them.

mod support;

use repodex::scanner::{ScanOptions, Scanner};
use support::{all_fixture_files, analyze_fixture, analyzer, TempDir};

#[test]
fn repeated_analysis_is_byte_identical() {
    for relative in all_fixture_files() {
        let first = analyze_fixture(&relative).canonical_text();
        for _ in 0..4 {
            let again = analyze_fixture(&relative).canonical_text();
            assert_eq!(first, again, "{relative} is not deterministic");
        }
    }
}

#[test]
fn identical_content_under_a_different_root_produces_identical_facts() {
    let source = std::fs::read(support::fixture("rust/impls.rs")).expect("fixture");
    let first_root = TempDir::new("determinism-root-a");
    let second_root = TempDir::new("determinism-root-b");
    let first_path = first_root.write("nested/deep/impls.rs", &source);
    let second_path = second_root.write("nested/deep/impls.rs", &source);

    let analyzer = analyzer();
    let first = analyzer.analyze_path(first_root.path(), &first_path);
    let second = analyzer.analyze_path(second_root.path(), &second_path);

    assert_eq!(first.canonical_text(), second.canonical_text());
    assert_eq!(first.file.snapshot_id, second.file.snapshot_id);
    assert_eq!(first.file.relative_path, "nested/deep/impls.rs");
    // Absolute checkout paths must never appear in canonical facts.
    let text = first.canonical_text();
    assert!(!text.contains(&first_root.path().display().to_string()));
    assert!(!text.contains(&second_root.path().display().to_string()));
    assert!(!text.contains("/tmp/"));
}

#[test]
fn snapshot_id_depends_only_on_bytes() {
    let source = b"pub fn same() {}\n";
    let first_root = TempDir::new("determinism-snapshot-a");
    let second_root = TempDir::new("determinism-snapshot-b");
    let first_path = first_root.write("one.rs", source);
    let second_path = second_root.write("two.rs", source);
    let analyzer = analyzer();
    let first = analyzer.analyze_path(first_root.path(), &first_path);
    let second = analyzer.analyze_path(second_root.path(), &second_path);
    assert_eq!(first.file.snapshot_id, second.file.snapshot_id);
    // A different byte produces a different snapshot id.
    let third_path = second_root.write("three.rs", b"pub fn other() {}\n");
    let third = analyzer.analyze_path(second_root.path(), &third_path);
    assert_ne!(first.file.snapshot_id, third.file.snapshot_id);
}

#[test]
fn creation_order_does_not_affect_scan_results() {
    let first = TempDir::new("determinism-order-a");
    for name in ["c.rs", "a.rs", "b.rs", "z.rs"] {
        first.write(name, b"pub fn f() {}\n");
    }
    let second = TempDir::new("determinism-order-b");
    for name in ["z.rs", "b.rs", "a.rs", "c.rs"] {
        second.write(name, b"pub fn f() {}\n");
    }
    let analyzer = analyzer();
    let scanner = Scanner::new(&analyzer, ScanOptions::default());
    let first_report = scanner.scan(first.path()).expect("scan");
    let second_report = scanner.scan(second.path()).expect("scan");
    assert_eq!(
        first_report
            .files
            .iter()
            .map(|file| file.relative_path.clone())
            .collect::<Vec<_>>(),
        second_report
            .files
            .iter()
            .map(|file| file.relative_path.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        repodex::canonical::scan_digest(&first_report),
        repodex::canonical::scan_digest(&second_report)
    );
}

#[test]
fn scan_digest_covers_complete_facts_not_just_counters() {
    let temp = TempDir::new("determinism-digest");
    temp.write("a.rs", b"pub fn one() {}\n");
    let analyzer = analyzer();
    let scanner = Scanner::new(&analyzer, ScanOptions::default());
    let before = scanner.scan(temp.path()).expect("scan");

    // Same counters, different facts: one declaration replaced by another.
    temp.write("a.rs", b"pub fn two() {}\n");
    let after = scanner.scan(temp.path()).expect("scan");
    assert_eq!(before.declarations, after.declarations);
    assert_eq!(before.visited_files, after.visited_files);
    assert_ne!(
        repodex::canonical::scan_digest(&before),
        repodex::canonical::scan_digest(&after),
        "the digest must change when the facts change, even if counters do not"
    );
}

#[test]
fn canonical_facts_contain_no_timing_or_machine_metadata() {
    let analysis = analyze_fixture("rust/declarations.rs");
    let text = analysis.canonical_text();
    for forbidden in [
        "elapsed",
        "duration",
        "timestamp",
        "started_at",
        "hostname",
        "user",
        "pid",
    ] {
        assert!(
            !text.contains(forbidden),
            "canonical facts must not contain `{forbidden}`"
        );
    }
    // Every canonical line is self-describing and newline free.
    for line in analysis.canonical_lines() {
        assert!(!line.contains('\n'));
        assert!(!line.is_empty());
    }
}

#[test]
fn fact_identifiers_are_dense_and_order_independent() {
    let analysis = analyze_fixture("go/declarations.go");
    let ids = analysis
        .declarations
        .iter()
        .map(|declaration| declaration.declaration_id)
        .collect::<Vec<_>>();
    assert_eq!(ids, (0..ids.len() as u32).collect::<Vec<_>>());
    let scope_ids = analysis
        .scopes
        .iter()
        .map(|scope| scope.scope_id)
        .collect::<Vec<_>>();
    assert_eq!(scope_ids, (0..scope_ids.len() as u32).collect::<Vec<_>>());
    let call_ids = analysis
        .calls
        .iter()
        .map(|call| call.call_id)
        .collect::<Vec<_>>();
    assert_eq!(call_ids, (0..call_ids.len() as u32).collect::<Vec<_>>());
}

#[test]
fn declaration_order_is_source_order() {
    for relative in all_fixture_files() {
        let analysis = analyze_fixture(&relative);
        let mut previous = 0u32;
        for declaration in &analysis.declarations {
            assert!(
                declaration.range.byte_start >= previous,
                "{relative}: declarations must be emitted in source order"
            );
            previous = declaration.range.byte_start;
        }
    }
}
