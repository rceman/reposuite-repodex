//! RepoSuite RepoDex - deterministic, source-grounded repository analysis.
//!
//! TASK 1 scope:
//!
//! ```text
//! repository files
//!     -> language detection
//!     -> tree-sitter parsing
//!     -> language-specific extraction
//!     -> normalized unresolved syntax facts
//! ```
//!
//! RepoDex does **not** resolve semantic identity. There are no resolved
//! symbols, no resolved references, no call edges, no repository map and no
//! navigator in this crate.
//!
//! The architectural boundary is:
//!
//! ```text
//! Tree-sitter syntax
//!       -> language adapter
//!       -> normalized unresolved syntax facts
//!       -> FUTURE semantic resolver
//!       -> FUTURE repository map
//!       -> FUTURE navigator
//! ```

pub mod agent_event;
pub mod candidates;
pub mod canonical;
pub mod cli;
pub mod context;
pub mod derived;
pub mod graph;
pub mod incremental;
pub mod input;
pub mod learning;
pub mod links;
pub mod manifest;
pub mod memory;
pub mod model;
pub mod parser;
pub mod paths;
pub mod query;
pub mod rdx1;
pub mod recipe;
pub mod repository;
pub mod scanner;
pub mod service;
pub mod symbol_exposure;
pub mod system_one;
pub mod temporal;
pub mod utility;
pub mod view;
pub mod witness;

pub use model::{
    AnalysisStatus, BindingKind, CallLikeForm, CallLikeOccurrence, Declaration, DeclarationFlag,
    DeclarationKind, Diagnostic, DiagnosticKind, DiagnosticSeverity, FileAnalysis, ImportCategory,
    ImportForm, ImportItem, ImportOccurrence, LanguageId, LocalBindingOccurrence, ReferenceKind,
    ReferenceOccurrence, Scope, ScopeKind, SourceFile, SourceRange, TestEvidence, TestEvidenceKind,
};
pub use parser::{
    AdapterError, Analyzer, AnalyzerConfig, GrammarInfo, LanguageAdapter, ParserRegistry,
};
