//! Recovery policy tests.
//!
//! Policy:
//!
//! * a valid fixture must never contain an `ERROR` or `MISSING` node, and must
//!   never carry a recovery diagnostic;
//! * a malformed fixture must report recovery, keep neighboring recoverable
//!   declarations, and never fabricate a name that is not written in the source;
//! * a recovered parse is never classified as clean.

mod support;

use repodex::{AnalysisStatus, DiagnosticKind, LanguageId};
use support::{all_fixture_files, analyze_fixture};

/// Fixtures that are intentionally damaged.
const MALFORMED: [&str; 4] = [
    "rust/malformed.rs",
    "go/malformed.go",
    "python/malformed.py",
    "php/malformed.php",
];

/// Fixtures that are intentionally recovered but are *not* canonical malformed
/// fixtures. They are exempt from the "must be clean" invariant, but they do
/// not carry the `valid_before`/`valid_after`/MISSING+ERROR shape the
/// malformed-specific assertions below require.
const RECOVERED: [&str; 1] = ["callcandidates/rust/src/broken.rs"];

#[test]
fn valid_fixtures_contain_no_recovery_artifacts() {
    for relative in all_fixture_files() {
        if MALFORMED.contains(&relative.as_str()) || RECOVERED.contains(&relative.as_str()) {
            continue;
        }
        let analysis = analyze_fixture(&relative);
        assert_eq!(
            analysis.status,
            AnalysisStatus::Clean,
            "{relative} must parse without recovery"
        );
        assert!(
            analysis.recovery_regions.is_empty(),
            "{relative} must not contain ERROR/MISSING regions"
        );
        assert!(
            !analysis.diagnostics.iter().any(|diagnostic| matches!(
                diagnostic.kind,
                DiagnosticKind::SyntaxError | DiagnosticKind::MissingSyntax
            )),
            "{relative} must not report recovery diagnostics"
        );
    }
}

#[test]
fn malformed_fixtures_report_recovery_and_are_not_clean() {
    for relative in MALFORMED {
        let analysis = analyze_fixture(relative);
        assert_eq!(
            analysis.status,
            AnalysisStatus::Recovered,
            "{relative} must be classified as recovered"
        );
        assert_ne!(analysis.status, AnalysisStatus::Clean);
        assert!(
            !analysis.recovery_regions.is_empty(),
            "{relative} must expose recovery regions"
        );
        assert!(
            analysis.diagnostics.iter().any(|diagnostic| matches!(
                diagnostic.kind,
                DiagnosticKind::SyntaxError | DiagnosticKind::MissingSyntax
            )),
            "{relative} must report a recovery diagnostic"
        );
    }
}

#[test]
fn recovery_diagnostics_and_regions_agree() {
    for relative in MALFORMED {
        let analysis = analyze_fixture(relative);
        let recovery_diagnostics = analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                matches!(
                    diagnostic.kind,
                    DiagnosticKind::SyntaxError | DiagnosticKind::MissingSyntax
                )
            })
            .count();
        assert_eq!(
            recovery_diagnostics,
            analysis.recovery_regions.len(),
            "{relative}: one diagnostic per recovery region"
        );
        for (diagnostic, region) in analysis.diagnostics.iter().zip(&analysis.recovery_regions) {
            let range = diagnostic
                .range
                .expect("recovery diagnostics carry a range");
            assert_eq!(range.byte_start, region.byte_start, "{relative}");
            assert_eq!(range.byte_end, region.byte_end, "{relative}");
        }
    }
}

#[test]
fn malformed_fixtures_keep_neighbouring_declarations() {
    for relative in MALFORMED {
        let analysis = analyze_fixture(relative);
        let names: Vec<&str> = analysis
            .declarations
            .iter()
            .map(|declaration| declaration.name.as_str())
            .collect();
        assert!(
            names.contains(&"valid_before") || names.contains(&"validBefore"),
            "{relative}: the declaration before the damage must survive, found {names:?}"
        );
        assert!(
            names.contains(&"valid_after") || names.contains(&"validAfter"),
            "{relative}: the declaration after the damage must survive, found {names:?}"
        );
    }
}

#[test]
fn missing_names_are_never_fabricated() {
    // Every declaration name must appear verbatim in the source bytes.
    for relative in all_fixture_files() {
        let analysis = analyze_fixture(&relative);
        let bytes = support::fixture_source(&relative);
        let text = String::from_utf8_lossy(&bytes);
        for declaration in &analysis.declarations {
            let written = &text[declaration.name_range.byte_start as usize
                ..declaration.name_range.byte_end as usize];
            assert_eq!(
                written, declaration.name,
                "{relative}: the recorded name must be exactly what the source writes"
            );
        }
    }
}

#[test]
fn recovery_regions_are_zero_width_or_positive_width_but_well_ordered() {
    for relative in MALFORMED {
        let analysis = analyze_fixture(relative);
        for region in &analysis.recovery_regions {
            assert!(region.byte_start <= region.byte_end, "{relative}");
            assert!(region.byte_end <= analysis.file.byte_len, "{relative}");
        }
        // MISSING nodes are zero width; ERROR nodes are not.
        assert!(
            analysis
                .recovery_regions
                .iter()
                .any(|region| region.byte_start == region.byte_end),
            "{relative}: this fixture inserts at least one MISSING token"
        );
        assert!(
            analysis
                .recovery_regions
                .iter()
                .any(|region| region.byte_start < region.byte_end),
            "{relative}: this fixture contains at least one ERROR node"
        );
    }
}

#[test]
fn a_valid_fixture_does_not_gain_recovery_after_an_edit_is_repaired() {
    let broken = analyze_fixture("rust/malformed.rs");
    assert_eq!(broken.status, AnalysisStatus::Recovered);
    // The same content with the damage repaired is clean.
    let repaired = support::analyze_source(
        LanguageId::Rust,
        "repaired.rs",
        b"fn valid_before() {}\n\nstruct Broken {\n    field: u32,\n}\n\nfn valid_after() {}\n",
    );
    assert_eq!(repaired.status, AnalysisStatus::Clean);
    assert!(repaired.recovery_regions.is_empty());
    assert_eq!(repaired.declarations.len(), 4);
}

#[test]
fn facts_inside_recovery_regions_are_still_source_grounded() {
    // `field: ,` inserts a MISSING type but the written field name survives.
    let analysis = analyze_fixture("rust/malformed.rs");
    let field = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.kind == repodex::DeclarationKind::Field)
        .expect("field");
    assert_eq!(field.name, "field");
    assert_eq!(
        support::slice(
            "rust/malformed.rs",
            field.name_range.byte_start,
            field.name_range.byte_end
        ),
        "field"
    );
    // The declaration range still ends inside the file.
    assert!(field.range.byte_end <= analysis.file.byte_len);
}
