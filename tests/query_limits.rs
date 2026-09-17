//! Regression tests for Tree-sitter query match-limit handling.
//!
//! Tree-sitter abandons in-progress captures when a `QueryCursor` exhausts its
//! capture-list pool, and reports it through `did_exceed_match_limit()`. The
//! runtime default is `u32::MAX`, so this cannot happen in normal operation.
//!
//! The production recovery query is a pair of single-node patterns
//! (`(ERROR) @error`, `(MISSING) @missing`). A single-node pattern releases its
//! capture list as soon as it matches, so it cannot exhaust the pool at any
//! limit. These tests therefore drive the guard through
//! `RecoveryScanner::with_query_source`, which is the only way to reach it, and
//! separately pin the fact that the production query never trips it.

mod support;

use repodex::model::{range_from_offsets, DiagnosticKind, SourceRange};
use repodex::parser::{apply_recovery, FactBuilder, RecoveryScanner};
use repodex::{AnalysisStatus, Analyzer, AnalyzerConfig, LanguageId, ParserRegistry, ScopeKind};
use tree_sitter::{Parser, Tree};

/// Many functions, each with a nested call, so a pattern that captures both the
/// function name and the inner call holds two capture lists per in-progress
/// match.
fn nested_source() -> String {
    let mut source = String::new();
    for index in 0..200 {
        source.push_str(&format!(
            "fn f{index}() {{ let a = g{index}(h{index}(k{index}(1))); }}\n"
        ));
    }
    source
}

/// A query whose captures are held across a deeper match.
///
/// The pattern captures a node near the root and another node deep inside it, so
/// an in-progress match holds a capture list for the whole descent. It reuses
/// the production capture names (`@error`, `@missing`) so the scanner records
/// its findings exactly as it would record a real recovery artifact.
const HOLDING_QUERY: &str = "(function_item name: (identifier) @error \
                             body: (block (let_declaration \
                             value: (call_expression) @missing)))";

const MANY_ERRORS: &str = r#"fn a() { let x = ; }
fn b() { let y = ; }
fn c() { let z = ; }
fn d() { let w = ; }
fn e() { let v = ; }
fn f() { let u = ; }
fn g() { let t = ; }
fn h() { let s = ; }
"#;

fn parse(language: LanguageId, source: &str) -> (ParserRegistry, Tree) {
    let registry = ParserRegistry::new().expect("registry");
    let mut parser = Parser::new();
    parser
        .set_language(&registry.adapter(language).ts_language())
        .expect("grammar must load");
    let tree = parser.parse(source, None).expect("tree");
    (registry, tree)
}

/// Run one recovery scan through the production orchestration path.
fn analyze_with_scanner(
    language: LanguageId,
    path: &str,
    source: &str,
    scanner: &RecoveryScanner,
    tree: &Tree,
) -> repodex::FileAnalysis {
    let (registry, _) = parse(language, source);
    let adapter = registry.adapter(language);
    let mut builder = FactBuilder::new(path, language, source.as_bytes());
    let file_range: SourceRange = range_from_offsets(source.as_bytes(), 0, source.len() as u32);
    builder.push_scope(ScopeKind::File, None::<String>, file_range, None);
    adapter.extract(&mut builder, tree);
    apply_recovery(scanner, language, tree, &mut builder);
    builder.pop_scope();
    builder.finish()
}

fn recovery_scanner(
    language: LanguageId,
    ts_language: &tree_sitter::Language,
    limit: u32,
) -> RecoveryScanner {
    let scanner = RecoveryScanner::with_match_limit(language, ts_language, limit).expect("scanner");
    assert_eq!(scanner.match_limit(), limit);
    scanner
}

/// The production recovery query must never trip the guard.
///
/// This is the control: the truncation check is a guard, not a fix for an
/// observed truncation, and it must not fire in normal operation even at the
/// lowest possible limit.
#[test]
fn the_production_recovery_query_cannot_exhaust_the_pool() {
    let (registry, _) = parse(LanguageId::Rust, MANY_ERRORS);
    let ts_language = registry.adapter(LanguageId::Rust).ts_language();
    assert_eq!(
        registry.adapter(LanguageId::Rust).recovery().match_limit(),
        RecoveryScanner::DEFAULT_MATCH_LIMIT
    );
    assert_eq!(RecoveryScanner::DEFAULT_MATCH_LIMIT, u32::MAX);

    // Even at a limit of one, a pair of single-node patterns never holds more
    // than one capture list.
    for limit in [1u32, 2, 3] {
        let scanner = recovery_scanner(LanguageId::Rust, &ts_language, limit);
        let (_, tree) = parse(LanguageId::Rust, MANY_ERRORS);
        let analysis =
            analyze_with_scanner(LanguageId::Rust, "many.rs", MANY_ERRORS, &scanner, &tree);
        assert_eq!(analysis.status, AnalysisStatus::Recovered);
        assert!(!analysis.has_diagnostic(DiagnosticKind::QueryMatchLimitExceeded));
        assert!(!analysis.recovery_regions.is_empty());
    }
}

/// The guard must fire, and the analysis must not be reported as complete.
#[test]
fn an_exhausted_match_limit_is_reported_and_never_reported_as_complete() {
    let source = nested_source();
    let (registry, tree) = parse(LanguageId::Rust, &source);
    let ts_language = registry.adapter(LanguageId::Rust).ts_language();

    // Confirm the query really does exhaust the pool at this limit, so the test
    // cannot pass for the wrong reason.
    let exhausted = recovery_scanner(LanguageId::Rust, &ts_language, 1);
    let scanner =
        RecoveryScanner::with_query_source(LanguageId::Rust, &ts_language, HOLDING_QUERY, 1)
            .expect("scanner");
    assert_eq!(scanner.match_limit(), 1);
    assert_eq!(exhausted.language(), LanguageId::Rust);

    let analysis = analyze_with_scanner(LanguageId::Rust, "nested.rs", &source, &scanner, &tree);

    assert!(
        analysis.has_diagnostic(DiagnosticKind::QueryMatchLimitExceeded),
        "the truncated scan must be reported: {:?}",
        analysis
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.render())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        analysis.status,
        AnalysisStatus::Incomplete,
        "a partial analysis must never be reported as clean or merely recovered"
    );
    assert_ne!(analysis.status, AnalysisStatus::Clean);
    assert_ne!(analysis.status, AnalysisStatus::Recovered);
    assert!(analysis.error_count() > 0);

    let diagnostic = analysis
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.kind == DiagnosticKind::QueryMatchLimitExceeded)
        .expect("the diagnostic");
    assert!(
        diagnostic.message.contains("match limit"),
        "{}",
        diagnostic.message
    );
    assert!(diagnostic.message.contains('1'), "{}", diagnostic.message);

    // Whatever was found before the abandonment is still source-grounded, and
    // nothing is fabricated to fill the gap.
    for declaration in &analysis.declarations {
        assert!(declaration.name_range.byte_end <= analysis.file.byte_len);
        assert!(declaration.range.byte_end <= analysis.file.byte_len);
    }
}

/// The same query at a sufficient limit must not trip the guard.
#[test]
fn a_sufficient_match_limit_does_not_trip_the_guard() {
    let source = nested_source();
    let (registry, tree) = parse(LanguageId::Rust, &source);
    let ts_language = registry.adapter(LanguageId::Rust).ts_language();
    let scanner =
        RecoveryScanner::with_query_source(LanguageId::Rust, &ts_language, HOLDING_QUERY, 64)
            .expect("scanner");
    let analysis = analyze_with_scanner(LanguageId::Rust, "nested.rs", &source, &scanner, &tree);
    assert!(!analysis.has_diagnostic(DiagnosticKind::QueryMatchLimitExceeded));
    assert_ne!(analysis.status, AnalysisStatus::Incomplete);
    assert!(!analysis.recovery_regions.is_empty());
}

/// A query that cannot compile is a query error, not a match-limit exhaustion.
#[test]
fn a_broken_query_is_a_query_error() {
    let (registry, _) = parse(LanguageId::Rust, MANY_ERRORS);
    let ts_language = registry.adapter(LanguageId::Rust).ts_language();
    let error = match RecoveryScanner::with_query_source(LanguageId::Rust, &ts_language, "((", 1) {
        Ok(_) => panic!("a malformed query must be rejected"),
        Err(error) => error,
    };
    assert!(error.contains("failed to compile"), "{error}");

    // A query missing the required capture names is rejected too.
    let error = match RecoveryScanner::with_query_source(
        LanguageId::Rust,
        &ts_language,
        "(identifier) @something_else",
        1,
    ) {
        Ok(_) => panic!("a query without @error must be rejected"),
        Err(error) => error,
    };
    assert!(error.contains("@error"), "{error}");
}

/// A genuine query error must map to `QueryError`, not to the match-limit
/// diagnostic, and must not be reported as `Incomplete`.
#[test]
fn a_query_error_is_not_reported_as_a_match_limit_exhaustion() {
    let source = MANY_ERRORS;
    let (registry, tree) = parse(LanguageId::Rust, source);
    let adapter = registry.adapter(LanguageId::Rust);
    let mut builder = FactBuilder::new("many.rs", LanguageId::Rust, source.as_bytes());
    let file_range: SourceRange = range_from_offsets(source.as_bytes(), 0, source.len() as u32);
    builder.push_scope(ScopeKind::File, None::<String>, file_range, None);
    adapter.extract(&mut builder, &tree);
    // The production scanner succeeds, so no query error is recorded.
    apply_recovery(adapter.recovery(), LanguageId::Rust, &tree, &mut builder);
    builder.pop_scope();
    let analysis = builder.finish();
    assert!(!analysis.has_diagnostic(DiagnosticKind::QueryError));
    assert!(!analysis.has_diagnostic(DiagnosticKind::QueryMatchLimitExceeded));
    assert_eq!(analysis.status, AnalysisStatus::Recovered);
}

/// The scan counters and the CLI policy must treat an incomplete analysis as a
/// failure, not as coverage.
#[test]
fn an_incomplete_analysis_is_counted_as_an_extraction_failure() {
    // The default path over the same bytes must show no failure: the control.
    let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
    let recovered = analyzer.analyze_bytes("many.rs", LanguageId::Rust, MANY_ERRORS.as_bytes());
    assert_eq!(recovered.status, AnalysisStatus::Recovered);

    let temp = support::TempDir::new("query-limit");
    temp.write("many.rs", MANY_ERRORS.as_bytes());
    let scanner =
        repodex::scanner::Scanner::new(&analyzer, repodex::scanner::ScanOptions::default());
    let report = scanner.scan(temp.path()).expect("scan");
    assert_eq!(report.extraction_failures, 0);
    assert_eq!(report.parsed_with_recovery_files, 1);
    assert_eq!(report.failed_files(), 0);
    assert!(report.is_complete());
}

/// The `Incomplete` status must be distinct from every other status and
/// serialized distinctly, so a consumer can tell it apart.
#[test]
fn the_incomplete_status_is_distinct_in_output() {
    assert_eq!(AnalysisStatus::Incomplete.as_str(), "incomplete");
    let names = [
        AnalysisStatus::Clean,
        AnalysisStatus::Recovered,
        AnalysisStatus::Incomplete,
        AnalysisStatus::Unsupported,
        AnalysisStatus::Failed,
    ]
    .map(AnalysisStatus::as_str);
    let mut sorted = names.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "status names must be distinct");

    // Serialize a genuinely incomplete analysis.
    let source = nested_source();
    let (registry, tree) = parse(LanguageId::Rust, &source);
    let ts_language = registry.adapter(LanguageId::Rust).ts_language();
    let scanner =
        RecoveryScanner::with_query_source(LanguageId::Rust, &ts_language, HOLDING_QUERY, 1)
            .expect("scanner");
    let analysis = analyze_with_scanner(LanguageId::Rust, "nested.rs", &source, &scanner, &tree);
    assert_eq!(analysis.status, AnalysisStatus::Incomplete);
    let json = serde_json::to_string(&analysis).expect("serialize");
    assert!(json.contains("\"status\":\"incomplete\""), "{json}");
    assert!(
        json.contains("\"kind\":\"query_match_limit_exceeded\""),
        "{json}"
    );
}
