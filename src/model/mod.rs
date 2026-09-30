//! Normalized, source-grounded syntax facts.
//!
//! Everything in this module describes *syntax occurrences* observed in a single
//! source snapshot. Nothing here is a resolved semantic entity: there are no
//! resolved symbols, no resolved references and no call edges.

mod analysis;
mod diagnostic;
mod facts;
mod range;

pub use analysis::{snapshot_id, AnalysisStatus, FileAnalysis, SourceFile, SCHEMA_VERSION};
pub use diagnostic::{Diagnostic, DiagnosticKind, DiagnosticSeverity};
pub use facts::{
    BindingKind, CallLikeForm, CallLikeOccurrence, Declaration, DeclarationFlag, DeclarationKind,
    ImportCategory, ImportForm, ImportItem, ImportOccurrence, LocalBindingOccurrence,
    ReceiverEvidenceKind, ReceiverTypeEvidence, ReferenceKind, ReferenceOccurrence, Scope,
    ScopeKind, TestEvidence, TestEvidenceKind,
};
pub use range::{point_at, range_from_offsets, LineIndex, SourceRange};

use serde::{Deserialize, Serialize};

/// Language identifiers supported by TASK 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguageId {
    Rust,
    Go,
    Python,
    Php,
    /// Manifest/config artifact (go.mod, Cargo.toml). Detected by file NAME,
    /// not extension — intentionally outside `ALL` so it is never scanned as a
    /// source language or included in the benchmark corpus.
    Manifest,
}

impl LanguageId {
    /// All supported languages, in the fixed reporting order of TASK 1.
    /// `Manifest` is excluded: it is an artifact kind, not a scannable source
    /// language, so it must not appear in `--language all` corpus generation
    /// or per-language indexing fingerprints.
    pub const ALL: [LanguageId; 4] = [
        LanguageId::Rust,
        LanguageId::Go,
        LanguageId::Python,
        LanguageId::Php,
    ];

    /// Stable lower-case identifier used in JSON output and canonical facts.
    pub fn as_str(self) -> &'static str {
        match self {
            LanguageId::Rust => "rust",
            LanguageId::Go => "go",
            LanguageId::Python => "python",
            LanguageId::Php => "php",
            LanguageId::Manifest => "manifest",
        }
    }

    /// Human readable display name.
    pub fn display_name(self) -> &'static str {
        match self {
            LanguageId::Rust => "Rust",
            LanguageId::Go => "Go",
            LanguageId::Python => "Python",
            LanguageId::Php => "PHP",
            LanguageId::Manifest => "Manifest",
        }
    }

    /// Source file extensions owned by this language (without the leading dot).
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            LanguageId::Rust => &["rs"],
            LanguageId::Go => &["go"],
            LanguageId::Python => &["py"],
            LanguageId::Php => &["php"],
            LanguageId::Manifest => &[],
        }
    }

    /// Detect a language from a file extension.
    pub fn from_extension(ext: &str) -> Option<LanguageId> {
        let ext = ext.to_ascii_lowercase();
        LanguageId::ALL
            .into_iter()
            .find(|lang| lang.extensions().contains(&ext.as_str()))
    }

    /// Detect a language from its stable lower-case identifier.
    pub fn from_name(name: &str) -> Option<LanguageId> {
        LanguageId::ALL
            .into_iter()
            .find(|language| language.as_str() == name.to_ascii_lowercase())
    }

    /// The first extension owned by this language.
    pub fn primary_extension(self) -> &'static str {
        self.extensions()[0]
    }
}
