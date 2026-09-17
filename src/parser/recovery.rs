use tree_sitter::{Language, Query, QueryCursor, StreamingIterator, Tree};

use crate::model::{Diagnostic, DiagnosticKind, LanguageId};

use super::builder::FactBuilder;

/// Why a recovery scan could not be trusted.
///
/// The two cases are deliberately distinct: a query that could not run at all
/// is a defect in the query or the runtime, while a query that ran and was
/// truncated means the *result* is incomplete. Only the second one means the
/// analysis is partial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryError {
    /// The query could not be executed.
    ///
    /// **This variant is currently not produced by any code path.**
    /// [`RecoveryScanner::scan`] can only return
    /// [`RecoveryError::MatchLimitExceeded`]: a query that fails to compile is
    /// rejected earlier, by [`RecoveryScanner::with_query_source`], which
    /// returns a `String` because it runs while an adapter is being built
    /// rather than while a file is being analyzed. Tree-sitter's
    /// `QueryCursor::matches` iteration has no failure signal to report.
    ///
    /// The variant and the [`DiagnosticKind::QueryError`] mapping in
    /// `parser::apply_recovery` are therefore retained as a typed guard for a
    /// runtime that can report an execution failure, and are not exercised
    /// today. `tests/query_limits.rs` asserts the behaviour that is reachable
    /// instead of asserting the absence of a diagnostic that cannot occur.
    Query(String),
    /// Tree-sitter abandoned in-progress captures at the configured match
    /// limit, so the reported recovery artifacts are known to be incomplete.
    MatchLimitExceeded { limit: u32 },
}

impl std::fmt::Display for RecoveryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoveryError::Query(message) => write!(formatter, "{message}"),
            RecoveryError::MatchLimitExceeded { limit } => write!(
                formatter,
                "the recovery query hit tree-sitter's match limit of {limit}; captures were \
                 abandoned, so the recovery scan is incomplete and the file may contain \
                 unreported ERROR or MISSING artifacts"
            ),
        }
    }
}

impl std::error::Error for RecoveryError {}

/// Compiled recovery query for one language.
///
/// Tree-sitter returns a tree even for damaged source. The recovery query is
/// the single place where the analyzer asks the runtime for the artifacts it
/// inserted: `ERROR` nodes (skipped input) and `MISSING` nodes (zero-width
/// tokens inserted to keep the parse going).
pub struct RecoveryScanner {
    language: LanguageId,
    query: Query,
    error_capture: u32,
    missing_capture: u32,
    /// Tree-sitter's per-cursor match limit. The runtime default is
    /// `u32::MAX`, so exhaustion cannot happen in normal operation; the limit
    /// exists so the truncation path is reachable and testable.
    match_limit: u32,
}

impl RecoveryScanner {
    /// Tree-sitter's own default: no practical limit.
    pub const DEFAULT_MATCH_LIMIT: u32 = u32::MAX;

    pub fn new(language: LanguageId, ts_language: &Language) -> Result<Self, String> {
        Self::with_match_limit(language, ts_language, Self::DEFAULT_MATCH_LIMIT)
    }

    /// Build a scanner with an explicit match limit.
    ///
    /// A low limit makes Tree-sitter abandon in-progress captures, which is
    /// exactly the silent-truncation failure mode [`RecoveryScanner::scan`]
    /// must detect rather than hide.
    pub fn with_match_limit(
        language: LanguageId,
        ts_language: &Language,
        match_limit: u32,
    ) -> Result<Self, String> {
        Self::with_query_source(
            language,
            ts_language,
            recovery_query_source(language),
            match_limit,
        )
    }

    /// Build a scanner over an explicit query.
    ///
    /// The production recovery query is a pair of single-node patterns. A
    /// single-node pattern completes as soon as it matches, so it releases its
    /// capture list immediately and cannot exhaust Tree-sitter's capture-list
    /// pool at any limit — which is why the truncation check in
    /// [`RecoveryScanner::scan`] is a guard rather than a fix for an observed
    /// truncation.
    ///
    /// This constructor exists so that guard is reachable and testable with a
    /// query whose captures are held across a deeper match. The analyzer never
    /// calls it; production always uses [`RecoveryScanner::new`].
    ///
    /// The query must declare `@error` and `@missing` captures, exactly like the
    /// production queries, so a capture found by an injected query is recorded
    /// the same way.
    pub fn with_query_source(
        language: LanguageId,
        ts_language: &Language,
        query_source: &str,
        match_limit: u32,
    ) -> Result<Self, String> {
        let query = Query::new(ts_language, query_source).map_err(|error| {
            format!(
                "recovery query failed to compile for {}: {error}",
                language.as_str()
            )
        })?;
        let names = query.capture_names();
        let error_capture = names
            .iter()
            .position(|name| *name == "error")
            .ok_or_else(|| "recovery query is missing the @error capture".to_string())?
            as u32;
        let missing_capture = names
            .iter()
            .position(|name| *name == "missing")
            .ok_or_else(|| "recovery query is missing the @missing capture".to_string())?
            as u32;
        Ok(Self {
            language,
            query,
            error_capture,
            missing_capture,
            match_limit,
        })
    }

    pub fn language(&self) -> LanguageId {
        self.language
    }

    pub fn match_limit(&self) -> u32 {
        self.match_limit
    }

    /// Run the query and record recovery diagnostics and regions.
    ///
    /// Returns the number of recovery artifacts found.
    ///
    /// The match limit is set explicitly and then checked. If Tree-sitter
    /// abandoned in-progress captures, the result is **not** a complete scan:
    /// the scanner returns [`RecoveryError::MatchLimitExceeded`] so the caller
    /// can mark the analysis incomplete instead of reporting a clean file.
    /// Query iteration errors are never silently ignored either.
    pub fn scan(&self, tree: &Tree, builder: &mut FactBuilder<'_>) -> Result<usize, RecoveryError> {
        let source = builder.source();
        let mut cursor = QueryCursor::new();
        cursor.set_match_limit(self.match_limit);
        let mut artifacts = 0usize;
        {
            let mut matches = cursor.matches(&self.query, tree.root_node(), source);
            while let Some(query_match) = matches.next() {
                for capture in query_match.captures() {
                    let range = builder.range(capture.node);
                    if capture.index == self.error_capture {
                        artifacts += 1;
                        builder.mark_recovered();
                        builder.push_recovery_region(range);
                        builder.push_diagnostic(
                            Diagnostic::error(
                                DiagnosticKind::SyntaxError,
                                format!(
                                    "tree-sitter recovered from an ERROR node covering {} byte(s)",
                                    range.byte_len()
                                ),
                            )
                            .with_range(range),
                        );
                    } else if capture.index == self.missing_capture {
                        artifacts += 1;
                        builder.mark_recovered();
                        // A `MISSING` node is zero-width: Tree-sitter inserted
                        // no bytes, so the recorded range keeps that zero width
                        // and says "a token is absent here" rather than pointing
                        // at unrelated source text.
                        builder.push_recovery_region(range);
                        builder.push_diagnostic(
                            Diagnostic::error(
                                DiagnosticKind::MissingSyntax,
                                format!(
                                    "tree-sitter inserted a MISSING {} token",
                                    capture.node.kind()
                                ),
                            )
                            .with_range(range),
                        );
                    }
                }
            }
        }
        if cursor.did_exceed_match_limit() {
            return Err(RecoveryError::MatchLimitExceeded {
                limit: self.match_limit,
            });
        }
        Ok(artifacts)
    }
}

fn recovery_query_source(language: LanguageId) -> &'static str {
    match language {
        LanguageId::Rust => include_str!("../../queries/rust/recovery.scm"),
        LanguageId::Go => include_str!("../../queries/go/recovery.scm"),
        LanguageId::Python => include_str!("../../queries/python/recovery.scm"),
        LanguageId::Php => include_str!("../../queries/php/recovery.scm"),
    }
}
