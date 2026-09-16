//! Complete canonical fact-set expectations for every committed fixture.
//!
//! Each fixture is compared against `fixtures/expected/<path>.txt`, which holds
//! every normalized fact in canonical order. Regenerate deliberately with:
//!
//! ```text
//! REPODEX_BLESS=1 cargo test --test expected_facts
//! ```
//!
//! and review the resulting diff. Range, containment and determinism properties
//! are asserted independently in the per-language test files, so a blessed file
//! can never be the only evidence that extraction is correct.

mod support;

#[test]
fn every_fixture_matches_its_expected_canonical_facts() {
    let fixtures = support::all_fixture_files();
    assert!(
        fixtures.len() >= 20,
        "expected a substantial fixture corpus, found {fixtures:?}"
    );
    let mut failures = Vec::new();
    for relative in &fixtures {
        let analysis = support::analyze_fixture(relative);
        let actual = analysis.canonical_text();
        let path = support::expected_path(relative);
        if std::env::var_os("REPODEX_BLESS").is_some() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create expected directory");
            }
            std::fs::write(&path, &actual).expect("write expected facts");
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(expected) if expected == actual => {}
            Ok(_) => failures.push(format!(
                "{relative}: canonical facts differ from expectation"
            )),
            Err(error) => failures.push(format!("{relative}: {error}")),
        }
    }
    if std::env::var_os("REPODEX_BLESS").is_some() {
        eprintln!("blessed {} expected fact files", fixtures.len());
        return;
    }
    assert!(
        failures.is_empty(),
        "{} fixture(s) diverged:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
