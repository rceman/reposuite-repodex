//! Parser registry and per-file analysis orchestration.
//!
//! ```text
//! source bytes
//!     -> tree-sitter Parser (pinned grammar)
//!     -> language adapter (language-specific extraction)
//!     -> normalized unresolved syntax facts
//! ```
//!
//! Nothing in this module resolves semantic identity.

pub mod builder;
pub mod go;
pub mod php;
pub mod python;
pub mod recovery;
pub mod rust;

pub use builder::{DeclarationDraft, FactBuilder};
pub use recovery::{RecoveryError, RecoveryScanner};

use std::cell::RefCell;
use std::fmt;
use std::path::Path;
use tree_sitter::{Language, Parser, Tree};

use crate::input;
use crate::model::{
    range_from_offsets, AnalysisStatus, Diagnostic, DiagnosticKind, FileAnalysis, LanguageId,
    ScopeKind, SourceRange,
};

/// Static description of the grammar an adapter is built on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrammarInfo {
    pub crate_name: &'static str,
    pub crate_version: &'static str,
    pub upstream_repository: &'static str,
    pub license: &'static str,
    /// Upstream query files shipped by the grammar crate that were inspected.
    pub upstream_queries: &'static [&'static str],
    /// RepoDex query files used by this adapter.
    pub repo_queries: &'static [&'static str],
    /// Root node kind produced by this grammar.
    pub root_node_kind: &'static str,
    /// Tree-sitter runtime range this grammar is used with.
    pub tree_sitter_compatibility: &'static str,
}

/// Failure to build or use a language adapter.
#[derive(Debug)]
pub struct AdapterError {
    pub language: Option<LanguageId>,
    pub message: String,
}

impl fmt::Display for AdapterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.language {
            Some(language) => write!(formatter, "{}: {}", language.as_str(), self.message),
            None => write!(formatter, "{}", self.message),
        }
    }
}

impl std::error::Error for AdapterError {}

/// A language-specific extractor.
pub trait LanguageAdapter: Send + Sync {
    fn language(&self) -> LanguageId;

    fn grammar(&self) -> GrammarInfo;

    fn ts_language(&self) -> Language;

    fn recovery(&self) -> &RecoveryScanner;

    /// Push every normalized fact for `tree` into `builder`.
    ///
    /// Implementations must not resolve semantic identity: no resolved symbols,
    /// no resolved references, no call edges.
    fn extract(&self, builder: &mut FactBuilder<'_>, tree: &Tree);
}

/// The set of adapters available in this build, in TASK 1 priority order.
pub struct ParserRegistry {
    adapters: Vec<Box<dyn LanguageAdapter>>,
}

impl ParserRegistry {
    pub fn new() -> Result<Self, AdapterError> {
        let adapters: Vec<Box<dyn LanguageAdapter>> = vec![
            Box::new(rust::RustAdapter::new()?),
            Box::new(go::GoAdapter::new()?),
            Box::new(python::PythonAdapter::new()?),
            Box::new(php::PhpAdapter::new()?),
        ];
        Ok(Self { adapters })
    }

    pub fn languages(&self) -> Vec<LanguageId> {
        self.adapters
            .iter()
            .map(|adapter| adapter.language())
            .collect()
    }

    pub fn adapters(&self) -> &[Box<dyn LanguageAdapter>] {
        &self.adapters
    }

    pub fn adapter(&self, language: LanguageId) -> &dyn LanguageAdapter {
        self.adapters
            .iter()
            .find(|adapter| adapter.language() == language)
            .map(|adapter| adapter.as_ref())
            .expect("every supported language has an adapter")
    }

    pub fn adapter_for_path(&self, path: &Path) -> Option<&dyn LanguageAdapter> {
        let extension = path.extension()?.to_str()?;
        let language = LanguageId::from_extension(extension)?;
        Some(self.adapter(language))
    }

    /// Load every grammar into a fresh Tree-sitter parser and parse a trivial
    /// snippet. Used by the `languages` command and by tests so that grammar
    /// loadability is verified rather than assumed.
    pub fn verify_grammars(&self) -> Vec<GrammarCheck> {
        self.adapters
            .iter()
            .map(|adapter| {
                let info = adapter.grammar();
                let language = adapter.ts_language();
                let abi_version = language.abi_version() as u32;
                let mut parser = Parser::new();
                match parser.set_language(&language) {
                    Ok(()) => match parser.parse("", None) {
                        Some(tree) => {
                            let root_kind = tree.root_node().kind();
                            if root_kind == info.root_node_kind {
                                GrammarCheck {
                                    language: adapter.language(),
                                    info,
                                    abi_version,
                                    ok: true,
                                    message: format!("root node `{root_kind}`"),
                                }
                            } else {
                                GrammarCheck {
                                    language: adapter.language(),
                                    info,
                                    abi_version,
                                    ok: false,
                                    message: format!(
                                        "expected root node `{}`, got `{root_kind}`",
                                        info.root_node_kind
                                    ),
                                }
                            }
                        }
                        None => GrammarCheck {
                            language: adapter.language(),
                            info,
                            abi_version,
                            ok: false,
                            message: "parser returned no tree for empty input".to_string(),
                        },
                    },
                    Err(error) => GrammarCheck {
                        language: adapter.language(),
                        info,
                        abi_version,
                        ok: false,
                        message: format!("set_language failed: {error}"),
                    },
                }
            })
            .collect()
    }
}

/// Result of loading one grammar into the runtime.
#[derive(Debug, Clone)]
pub struct GrammarCheck {
    pub language: LanguageId,
    pub info: GrammarInfo,
    pub abi_version: u32,
    pub ok: bool,
    pub message: String,
}

/// Analysis configuration.
#[derive(Debug, Clone, Copy)]
pub struct AnalyzerConfig {
    pub max_file_size: u64,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        Self {
            max_file_size: input::DEFAULT_MAX_FILE_SIZE,
        }
    }
}

/// Ties the registry, a reusable parser and the input rules together.
pub struct Analyzer {
    registry: ParserRegistry,
    parser: RefCell<Parser>,
    config: AnalyzerConfig,
}

impl Analyzer {
    pub fn new(config: AnalyzerConfig) -> Result<Self, AdapterError> {
        Ok(Self {
            registry: ParserRegistry::new()?,
            parser: RefCell::new(Parser::new()),
            config,
        })
    }

    pub fn registry(&self) -> &ParserRegistry {
        &self.registry
    }

    pub fn config(&self) -> AnalyzerConfig {
        self.config
    }

    /// Analyze an in-memory buffer.
    pub fn analyze_bytes(
        &self,
        relative_path: &str,
        language: LanguageId,
        source: &[u8],
    ) -> FileAnalysis {
        if let Err(error) = input::validate_bytes(source.to_vec(), self.config.max_file_size) {
            return FileAnalysis::unsupported(
                relative_path,
                language,
                source,
                error.status(),
                error.diagnostic(),
            );
        }
        let adapter = self.registry.adapter(language);
        let mut parser = self.parser.borrow_mut();
        analyze_with_adapter(adapter, &mut parser, relative_path, source)
    }

    /// Read and analyze a file from disk.
    ///
    /// `root` is only used to compute the relative path recorded in the
    /// normalized facts; the absolute path never reaches the model.
    pub fn analyze_path(&self, root: &Path, path: &Path) -> FileAnalysis {
        let language = match path.extension().and_then(|extension| extension.to_str()) {
            Some(extension) => match LanguageId::from_extension(extension) {
                Some(language) => language,
                None => {
                    let relative_path = crate::paths::relative_path(root, path);
                    return FileAnalysis::unsupported(
                        relative_path,
                        LanguageId::Rust,
                        b"",
                        AnalysisStatus::Unsupported,
                        Diagnostic::error(
                            DiagnosticKind::UnsupportedLanguage,
                            format!("unsupported file extension `{extension}`"),
                        ),
                    );
                }
            },
            None => {
                let relative_path = crate::paths::relative_path(root, path);
                return FileAnalysis::unsupported(
                    relative_path,
                    LanguageId::Rust,
                    b"",
                    AnalysisStatus::Unsupported,
                    Diagnostic::error(
                        DiagnosticKind::UnsupportedLanguage,
                        "file has no extension".to_string(),
                    ),
                );
            }
        };
        let relative_path = crate::paths::relative_path(root, path);
        match input::read_source(path, self.config.max_file_size) {
            Ok(input) => self.analyze_bytes(&relative_path, language, &input.bytes),
            Err(error) => FileAnalysis::unsupported(
                relative_path,
                language,
                b"",
                error.status(),
                error.diagnostic(),
            ),
        }
    }
}

/// Run one adapter over one source buffer.
pub fn analyze_with_adapter(
    adapter: &dyn LanguageAdapter,
    parser: &mut Parser,
    relative_path: &str,
    source: &[u8],
) -> FileAnalysis {
    let language = adapter.language();
    let ts_language = adapter.ts_language();
    if let Err(error) = parser.set_language(&ts_language) {
        return FileAnalysis::unsupported(
            relative_path,
            language,
            source,
            AnalysisStatus::Failed,
            Diagnostic::error(
                DiagnosticKind::GrammarLoadError,
                format!(
                    "tree-sitter rejected grammar {} {}: {error}",
                    adapter.grammar().crate_name,
                    adapter.grammar().crate_version
                ),
            ),
        );
    }
    let tree = match parser.parse(source, None) {
        Some(tree) => tree,
        None => {
            return FileAnalysis::unsupported(
                relative_path,
                language,
                source,
                AnalysisStatus::Failed,
                Diagnostic::error(
                    DiagnosticKind::GrammarLoadError,
                    "tree-sitter returned no tree for this input".to_string(),
                ),
            );
        }
    };

    extract_from_tree(adapter, relative_path, source, &tree)
}

/// Extract normalized facts from an already-parsed tree.
///
/// The incremental harness uses this so that the incrementally parsed tree and
/// the independently full-parsed tree go through exactly the same extraction
/// path.
pub fn extract_from_tree(
    adapter: &dyn LanguageAdapter,
    relative_path: &str,
    source: &[u8],
    tree: &Tree,
) -> FileAnalysis {
    let language = adapter.language();
    let mut builder = FactBuilder::new(relative_path, language, source);
    let file_range = file_scope_range(source);
    builder.push_scope(ScopeKind::File, None::<String>, file_range, None);
    adapter.extract(&mut builder, tree);
    builder.pop_scope();

    apply_recovery(adapter.recovery(), language, tree, &mut builder);
    builder.finish()
}

/// Run a recovery scan and fold its outcome into the builder.
///
/// This is the single place where a recovery result becomes status and
/// diagnostics, so the match-limit path cannot be handled differently by
/// different callers. [`extract_from_tree`] passes the adapter's own scanner;
/// the match-limit regression test passes a scanner with a deliberately low
/// limit through the same function.
pub fn apply_recovery(
    scanner: &RecoveryScanner,
    language: LanguageId,
    tree: &Tree,
    builder: &mut FactBuilder<'_>,
) {
    if let Err(error) = scanner.scan(tree, builder) {
        let kind = match error {
            RecoveryError::MatchLimitExceeded { .. } => {
                builder.mark_incomplete();
                DiagnosticKind::QueryMatchLimitExceeded
            }
            RecoveryError::Query(_) => DiagnosticKind::QueryError,
        };
        builder.push_diagnostic(Diagnostic::error(
            kind,
            format!("recovery query failed for {}: {error}", language.as_str()),
        ));
    }
}

fn file_scope_range(source: &[u8]) -> SourceRange {
    range_from_offsets(source, 0, source.len() as u32)
}
