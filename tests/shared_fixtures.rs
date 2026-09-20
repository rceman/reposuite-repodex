//! Shared adversarial fixtures: encodings, line endings and awkward source.

mod support;

use repodex::{AnalysisStatus, DiagnosticKind, LanguageId};
use support::{analyze_source, TempDir};

#[test]
fn empty_files_are_clean_and_factless() {
    for (language, path) in [
        (LanguageId::Rust, "empty.rs"),
        (LanguageId::Go, "empty.go"),
        (LanguageId::Python, "empty.py"),
        (LanguageId::Php, "empty.php"),
    ] {
        let analysis = analyze_source(language, path, b"");
        assert_eq!(analysis.status, AnalysisStatus::Clean, "{path}");
        assert!(analysis.declarations.is_empty());
        assert!(analysis.imports.is_empty());
        assert!(analysis.calls.is_empty());
        assert!(analysis.recovery_regions.is_empty());
        assert_eq!(analysis.file.byte_len, 0);
        assert_eq!(analysis.file.line_count, 0);
        assert!(!analysis.file.has_final_newline);
        assert_eq!(analysis.scopes.len(), 1);
    }
}

#[test]
fn comment_only_files_are_clean_and_factless() {
    let rust = analyze_source(
        LanguageId::Rust,
        "comments.rs",
        b"// only a comment\n// and another\n",
    );
    assert_eq!(rust.status, AnalysisStatus::Clean);
    assert!(rust.declarations.is_empty());
    assert_eq!(rust.file.line_count, 2);

    let python = analyze_source(
        LanguageId::Python,
        "comments.py",
        b"# comment\n\"\"\"docstring only\"\"\"\n",
    );
    assert_eq!(python.status, AnalysisStatus::Clean);
    assert!(python.declarations.is_empty());

    let php = analyze_source(LanguageId::Php, "comments.php", b"<?php\n// comment\n");
    assert_eq!(php.status, AnalysisStatus::Clean);
    assert!(php.declarations.is_empty());
}

#[test]
fn utf8_multibyte_text_shifts_ranges_correctly() {
    let source = "\u{3b1}\u{3b2}\u{3b3} = 1\n";
    // A multibyte identifier must not shift byte offsets relative to the source.
    let analysis = analyze_source(LanguageId::Python, "multibyte.py", source.as_bytes());
    assert_eq!(analysis.status, AnalysisStatus::Clean);
    assert_eq!(analysis.file.byte_len, source.len() as u32);
    let multibyte = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "\u{3b1}\u{3b2}\u{3b3}")
        .expect("multibyte identifier");
    assert_eq!(multibyte.name_range.byte_start, 0);
    assert_eq!(multibyte.name_range.byte_end, 6);

    // Now a real declaration after multibyte text.
    let mut rust = String::from("// \u{3b1}\u{3b2}\u{3b3} \u{65e5}\u{672c}\u{8a9e}\n");
    let prefix_len = rust.len();
    rust.push_str("pub fn after_multibyte() {}\n");
    let analysis = analyze_source(LanguageId::Rust, "multibyte.rs", rust.as_bytes());
    assert_eq!(analysis.status, AnalysisStatus::Clean);
    let declaration = &analysis.declarations[0];
    assert_eq!(declaration.name, "after_multibyte");
    assert_eq!(declaration.name_range.byte_start as usize, prefix_len + 7);
    let source_text = std::str::from_utf8(rust.as_bytes()).expect("utf-8");
    assert_eq!(
        &source_text
            [declaration.name_range.byte_start as usize..declaration.name_range.byte_end as usize],
        "after_multibyte"
    );
    // Columns are UTF-8 byte columns, so a multibyte prefix still shifts them.
    assert_eq!(declaration.name_range.row_start, 1);
    assert_eq!(declaration.name_range.column_start, 7);
}

#[test]
fn crlf_line_endings_are_preserved_and_parsed() {
    let rust = "pub fn one() {}\r\npub fn two() {}\r\n";
    let analysis = analyze_source(LanguageId::Rust, "crlf.rs", rust.as_bytes());
    assert_eq!(analysis.status, AnalysisStatus::Clean);
    assert_eq!(analysis.declarations.len(), 2);
    assert_eq!(analysis.file.byte_len, rust.len() as u32);
    assert_eq!(analysis.file.line_count, 2);
    // The second declaration starts on row 1, after the CRLF.
    assert_eq!(analysis.declarations[1].range.row_start, 1);
    assert_eq!(analysis.declarations[1].range.column_start, 0);
    assert_eq!(
        &rust[..analysis.declarations[0].range.byte_end as usize],
        "pub fn one() {}"
    );

    let go = "package p\r\n\r\nfunc One() {}\r\n";
    let analysis = analyze_source(LanguageId::Go, "crlf.go", go.as_bytes());
    assert_eq!(analysis.status, AnalysisStatus::Clean);
    assert_eq!(analysis.file.line_count, 3);

    let php = "<?php\r\nfunction one(): void\r\n{\r\n}\r\n";
    let analysis = analyze_source(LanguageId::Php, "crlf.php", php.as_bytes());
    assert_eq!(analysis.status, AnalysisStatus::Clean);
    assert_eq!(analysis.declarations.len(), 1);
}

#[test]
fn missing_final_newline_is_reported_and_parsed() {
    let rust = "pub fn last() {}";
    let analysis = analyze_source(LanguageId::Rust, "no_newline.rs", rust.as_bytes());
    assert_eq!(analysis.status, AnalysisStatus::Clean);
    assert!(!analysis.file.has_final_newline);
    assert_eq!(analysis.file.line_count, 1);
    assert_eq!(analysis.declarations.len(), 1);
    assert_eq!(analysis.declarations[0].range.byte_end, rust.len() as u32);

    let go = "package p\n\nfunc Last() {}";
    let analysis = analyze_source(LanguageId::Go, "no_newline.go", go.as_bytes());
    assert_eq!(analysis.status, AnalysisStatus::Clean);
    assert!(!analysis.file.has_final_newline);
    assert_eq!(analysis.file.line_count, 3);
}

#[test]
fn a_bom_is_part_of_the_snapshot() {
    let mut source = vec![0xef, 0xbb, 0xbf];
    source.extend_from_slice(b"pub fn with_bom() {}\n");
    let analysis = analyze_source(LanguageId::Rust, "bom.rs", &source);
    // The BOM is preserved in the snapshot and counted in the byte length, and
    // the Rust grammar tolerates it without recovery.
    assert_eq!(analysis.status, AnalysisStatus::Clean);
    assert_eq!(analysis.file.byte_len, source.len() as u32);
    assert!(source.starts_with(&[0xef, 0xbb, 0xbf]));
    // Byte offsets stay relative to the unmodified source bytes, so the
    // declaration shifts by exactly the three BOM bytes.
    let declaration = analysis
        .declarations
        .iter()
        .find(|declaration| declaration.name == "with_bom")
        .expect("declaration after BOM");
    assert_eq!(declaration.name_range.byte_start, 3 + 7);
}

#[test]
fn php_shebang_and_mixed_markup_are_supported() {
    let source = b"#!/usr/bin/env php\n<?php\nfunction cli(): void {}\n";
    let analysis = analyze_source(LanguageId::Php, "cli.php", source);
    assert_eq!(analysis.status, AnalysisStatus::Clean);
    assert!(analysis
        .declarations
        .iter()
        .any(|declaration| declaration.name == "cli"));

    let markup = b"<html><body>\n<?php\nclass Page {}\n?>\n</body></html>\n";
    let analysis = analyze_source(LanguageId::Php, "page.php", markup);
    assert_eq!(analysis.status, AnalysisStatus::Clean);
    assert_eq!(analysis.declarations.len(), 1);
    let expected_start = std::str::from_utf8(markup)
        .expect("utf-8")
        .find("class Page")
        .expect("class position");
    assert_eq!(
        analysis.declarations[0].range.byte_start as usize,
        expected_start
    );
}

#[test]
fn invalid_utf8_never_panics() {
    for language in [
        LanguageId::Rust,
        LanguageId::Go,
        LanguageId::Python,
        LanguageId::Php,
    ] {
        let analysis = analyze_source(language, "bad.rs", &[0x70, 0x75, 0xff, 0xfe]);
        assert_eq!(analysis.status, AnalysisStatus::Unsupported);
        assert!(analysis.declarations.is_empty());
        assert!(analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.kind == DiagnosticKind::UnsupportedEncoding));
    }
}

#[test]
fn file_size_limit_is_enforced_while_reading() {
    let temp = TempDir::new("input-size");
    let mut body = Vec::new();
    body.extend_from_slice(b"pub fn a() {}\n");
    while body.len() < 300 {
        body.extend_from_slice(b"// filler filler filler\n");
    }
    let path = temp.write("big.rs", &body);

    // Metadata says the file is small enough, but the limit still applies.
    let error = repodex::input::read_source(&path, 100).expect_err("must be rejected");
    match error {
        repodex::input::InputError::TooLarge { limit, observed } => {
            assert_eq!(limit, 100);
            // The read stops as soon as the limit is exceeded.
            assert_eq!(observed, 101);
        }
        other => panic!("expected TooLarge, got {other:?}"),
    }
    // The default limit accepts it.
    let input = repodex::input::read_source(&path, repodex::input::DEFAULT_MAX_FILE_SIZE)
        .expect("default limit");
    assert_eq!(input.bytes.len(), body.len());
}

#[test]
fn disappearing_files_are_reported_not_panicked() {
    let temp = TempDir::new("input-missing");
    let path = temp.write("gone.rs", b"pub fn gone() {}\n");
    std::fs::remove_file(&path).expect("remove");
    let error = repodex::input::read_source(&path, repodex::input::DEFAULT_MAX_FILE_SIZE)
        .expect_err("must fail");
    assert_eq!(error.status(), AnalysisStatus::Failed);
    assert_eq!(
        error.diagnostic().kind,
        DiagnosticKind::DisappearedDuringScan
    );
    // Analyzing a missing path through the analyzer reports it too.
    let analysis = support::analyzer().analyze_path(temp.path(), &path);
    assert_eq!(analysis.status, AnalysisStatus::Failed);
    assert!(analysis.declarations.is_empty());
}

#[test]
fn generated_looking_and_semantically_invalid_source_stays_clean() {
    for (relative, language) in [
        ("shared/adversarial.rs", LanguageId::Rust),
        ("shared/adversarial.go", LanguageId::Go),
        ("shared/adversarial.py", LanguageId::Python),
        ("shared/adversarial.php", LanguageId::Php),
    ] {
        let analysis = support::analyze_fixture(relative);
        assert_eq!(analysis.status, AnalysisStatus::Clean, "{relative}");
        assert_eq!(analysis.file.language, language);
        // Deep nesting and duplicate names are extracted, not merged.
        let mut counts = std::collections::BTreeMap::new();
        for declaration in &analysis.declarations {
            *counts.entry(declaration.name.clone()).or_insert(0usize) += 1;
        }
        assert!(
            counts.values().any(|count| *count >= 2),
            "{relative}: the fixture repeats a written name, so at least one name must \
             appear twice, found {counts:?}"
        );
        // Duplicate written names never collapse into one declaration identity.
        let mut identities = analysis
            .declarations
            .iter()
            .map(|declaration| support::declaration_identity(&analysis, declaration))
            .collect::<Vec<_>>();
        let total = identities.len();
        identities.sort();
        identities.dedup();
        assert_eq!(
            identities.len(),
            total,
            "{relative}: duplicate declaration identities in {identities:?}"
        );
    }
}

#[test]
fn json_serialization_is_stable_and_lossless() {
    let analysis = support::analyze_fixture("python/test_sample.py");
    let first = serde_json::to_string(&analysis).expect("serialize");
    let second = serde_json::to_string(&analysis).expect("serialize");
    assert_eq!(first, second);
    let parsed: serde_json::Value = serde_json::from_str(&first).expect("parse");
    assert_eq!(
        parsed["declarations"]
            .as_array()
            .expect("declarations")
            .len(),
        analysis.declarations.len()
    );
    assert_eq!(
        parsed["calls"].as_array().expect("calls").len(),
        analysis.calls.len()
    );
    assert_eq!(
        parsed["schema_version"],
        serde_json::json!(repodex::model::SCHEMA_VERSION)
    );
    // The binding collection round-trips losslessly too.
    assert_eq!(
        parsed["bindings"].as_array().expect("bindings").len(),
        analysis.bindings.len()
    );
}
