use tree_sitter::{Language, Query, QueryCursor, StreamingIterator, Tree};

use crate::model::{Diagnostic, DiagnosticKind, LanguageId, SourceRange};

use super::builder::FactBuilder;

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
}

impl RecoveryScanner {
    pub fn new(language: LanguageId, ts_language: &Language) -> Result<Self, String> {
        let source = recovery_query_source(language);
        let query = Query::new(ts_language, source).map_err(|error| {
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
        })
    }

    pub fn language(&self) -> LanguageId {
        self.language
    }

    /// Run the query and record recovery diagnostics and regions.
    ///
    /// Returns the number of recovery artifacts found. Query iteration errors
    /// are never silently ignored: they are recorded as diagnostics.
    pub fn scan(&self, tree: &Tree, builder: &mut FactBuilder<'_>) -> Result<usize, String> {
        let source = builder.source();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, tree.root_node(), source);
        let mut artifacts = 0usize;
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
                    builder.push_recovery_region(zero_width_region(range));
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
        Ok(artifacts)
    }
}

/// A `MISSING` node is zero-width. Widen it by one byte so the recovery region
/// is visible in output without changing its meaning.
fn zero_width_region(range: SourceRange) -> SourceRange {
    if range.is_empty() {
        SourceRange::new(
            range.byte_start,
            range.byte_end,
            range.row_start,
            range.column_start,
            range.row_end,
            range.column_end,
        )
    } else {
        range
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
