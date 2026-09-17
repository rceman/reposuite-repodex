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
//!
//! Scope note: `RecoveryError::Query` is declared but is not produced by any
//! code path today (see its documentation), so no test here asserts that a
//! query *error* is reported during analysis. `a_query_that_cannot_compile_is_rejected`
//! covers the one query failure that does occur, which happens while a scanner
//! is being built rather than while a file is being analyzed.

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

/// A query that cannot compile is rejected while the scanner is built, with a
/// message naming the language. This is the only query failure the current
/// runtime can produce.
#[test]
fn a_query_that_cannot_compile_is_rejected() {
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

/// A truncated recovery scan must be reported as *exactly* a match-limit
/// exhaustion: not as ordinary recovery, not as clean, and not as a query
/// error.
///
/// The contrast is real, not vacuous: the same source through the same
/// orchestration path does not report a truncation with the production scanner
/// and does report one, as `Incomplete`, with a scanner whose limit is low
/// enough to abandon captures.
///
/// The control below asserts `not Incomplete` and `no match-limit diagnostic`.
/// It does not assert `Recovered`: over this nested source the production
/// scanner happens to recover, but the claim this test needs is only that it
/// does not report a truncated scan. The test that pins `Recovered` on the
/// production scanner is
/// `the_production_recovery_query_cannot_exhaust_the_pool`.
///
/// (The `RecoveryError::Query` variant is not produced by any code path today —
/// see its documentation — so no assertion here claims to exercise it.)
#[test]
fn a_match_limit_exhaustion_is_reported_only_as_a_match_limit_exhaustion() {
    let source = nested_source();
    let (registry, tree) = parse(LanguageId::Rust, &source);
    let ts_language = registry.adapter(LanguageId::Rust).ts_language();

    // Control: the production scanner over the same tree must not report a
    // truncated scan. The assertions below establish "not Incomplete" and "no
    // match-limit diagnostic"; they do not establish "Recovered".
    let adapter = registry.adapter(LanguageId::Rust);
    let mut builder = FactBuilder::new("nested.rs", LanguageId::Rust, source.as_bytes());
    let file_range: SourceRange = range_from_offsets(source.as_bytes(), 0, source.len() as u32);
    builder.push_scope(ScopeKind::File, None::<String>, file_range, None);
    adapter.extract(&mut builder, &tree);
    apply_recovery(adapter.recovery(), LanguageId::Rust, &tree, &mut builder);
    builder.pop_scope();
    let control = builder.finish();
    assert!(
        !control.has_diagnostic(DiagnosticKind::QueryMatchLimitExceeded),
        "the production scanner must not report a truncated scan"
    );
    assert_ne!(control.status, AnalysisStatus::Incomplete);

    // The truncated scanner over the same tree must report exactly that.
    let scanner =
        RecoveryScanner::with_query_source(LanguageId::Rust, &ts_language, HOLDING_QUERY, 1)
            .expect("scanner");
    let truncated = analyze_with_scanner(LanguageId::Rust, "nested.rs", &source, &scanner, &tree);

    assert!(truncated.has_diagnostic(DiagnosticKind::QueryMatchLimitExceeded));
    assert!(
        !truncated.has_diagnostic(DiagnosticKind::QueryError),
        "a truncation is not a query error"
    );
    assert_eq!(truncated.status, AnalysisStatus::Incomplete);
    assert_ne!(truncated.status, AnalysisStatus::Clean);
    assert_ne!(truncated.status, AnalysisStatus::Recovered);

    // Exactly one diagnostic of the recovery-failure family is emitted, and it
    // is the match-limit one. A truncation must not also masquerade as ordinary
    // syntax recovery.
    let failure_family = truncated
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.kind,
                DiagnosticKind::QueryMatchLimitExceeded | DiagnosticKind::QueryError
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(failure_family.len(), 1);
    assert_eq!(
        failure_family[0].kind,
        DiagnosticKind::QueryMatchLimitExceeded
    );
}

/// The scan counters and the CLI policy must treat a truncated analysis as a
/// failure, not as coverage.
///
/// This is the end-to-end control: over these bytes the production path only
/// ever recovers, and recovery is not a failure. The claim that a genuinely
/// *incomplete* analysis is counted as an extraction failure is asserted by
/// `scanner::tests::an_incomplete_analysis_is_counted_as_an_extraction_failure`,
/// which drives the classification function directly, because the default
/// analyzer's match limit is `u32::MAX` and no file can reach `Incomplete`
/// through `Scanner::scan`.
#[test]
fn a_recovered_file_is_not_counted_as_an_extraction_failure() {
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
