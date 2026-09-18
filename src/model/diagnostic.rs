use serde::{Deserialize, Serialize};

use super::SourceRange;

/// A structured observation about the analysis of one file.
///
/// Diagnostics describe the analyzer's own state; they never assert that the
/// analyzed code is semantically wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub kind: DiagnosticKind,
    pub message: String,
    /// Range the diagnostic refers to, when one is known.
    pub range: Option<SourceRange>,
}

impl Diagnostic {
    pub fn error(kind: DiagnosticKind, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            kind,
            message: message.into(),
            range: None,
        }
    }

    pub fn warning(kind: DiagnosticKind, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            kind,
            message: message.into(),
            range: None,
        }
    }

    pub fn with_range(mut self, range: SourceRange) -> Self {
        self.range = Some(range);
        self
    }

    /// Stable one-line rendering used by canonical fact output and tests.
    pub fn render(&self) -> String {
        let range = match &self.range {
            Some(range) => range.render(),
            None => "-".to_string(),
        };
        format!(
            "{} {} [{}] {}",
            severity_str(self.severity),
            kind_str(self.kind),
            range,
            self.message
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticKind {
    /// Tree-sitter produced an `ERROR` node.
    SyntaxError,
    /// Tree-sitter produced a `MISSING` node: it inserted a zero-width token to
    /// keep the parse going.
    MissingSyntax,
    /// Aggregate description of a recovery region.
    RecoveryRegion,
    /// The file extension is not one of the supported languages.
    UnsupportedLanguage,
    /// The file is not valid UTF-8.
    UnsupportedEncoding,
    /// The file exceeds the configured maximum size.
    FileTooLarge,
    /// The file could not be read.
    IoError,
    /// The file disappeared between discovery and reading.
    DisappearedDuringScan,
    /// A Tree-sitter query failed to compile or run.
    QueryError,
    /// A Tree-sitter query hit its match limit and abandoned captures, so the
    /// query result is known to be truncated.
    QueryMatchLimitExceeded,
    /// The scanner could not descend into a directory or stat an entry, so part
    /// of the tree under the scan root was never visited.
    TraversalFailure,
    /// A grammar could not be loaded by the Tree-sitter runtime.
    GrammarLoadError,
}

impl DiagnosticKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticKind::SyntaxError => "syntax_error",
            DiagnosticKind::MissingSyntax => "missing_syntax",
            DiagnosticKind::RecoveryRegion => "recovery_region",
            DiagnosticKind::UnsupportedLanguage => "unsupported_language",
            DiagnosticKind::UnsupportedEncoding => "unsupported_encoding",
            DiagnosticKind::FileTooLarge => "file_too_large",
            DiagnosticKind::IoError => "io_error",
            DiagnosticKind::DisappearedDuringScan => "disappeared_during_scan",
            DiagnosticKind::QueryError => "query_error",
            DiagnosticKind::QueryMatchLimitExceeded => "query_match_limit_exceeded",
            DiagnosticKind::TraversalFailure => "traversal_failure",
            DiagnosticKind::GrammarLoadError => "grammar_load_error",
        }
    }
}

fn severity_str(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Info => "info",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Error => "error",
    }
}

fn kind_str(kind: DiagnosticKind) -> &'static str {
    kind.as_str()
}
