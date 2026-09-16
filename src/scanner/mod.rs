//! Deterministic repository scanner.
//!
//! The scanner walks a repository once, single-threaded, prunes a fixed set of
//! directories, never follows symlinks and dispatches supported files to the
//! matching adapter. Discovery order can never influence results because the
//! scanner sorts entries and the normalized facts are a pure function of the
//! file bytes.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::model::{
    AnalysisStatus, Diagnostic, DiagnosticKind, FileAnalysis, LanguageId, SourceFile,
};
use crate::parser::{Analyzer, ParserRegistry};
use crate::paths::relative_path;

/// Directories that are always pruned.
pub const PRUNED_DIRECTORIES: [&str; 7] = [
    ".git",
    "target",
    "vendor",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
];

/// Options for one scan.
#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    /// Follow `.gitignore` files that live inside the scan root.
    pub respect_gitignore: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            respect_gitignore: true,
        }
    }
}

/// What happened to one visited path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOutcome {
    /// Parsed without recovery artifacts.
    Clean,
    /// Parsed, but Tree-sitter had to recover.
    Recovered,
    /// Supported language, but the input was unsupported (encoding or size).
    Skipped,
    /// Reading or parsing failed.
    Failed,
    /// Extension is not one of the supported languages.
    Unsupported,
}

impl FileOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            FileOutcome::Clean => "clean",
            FileOutcome::Recovered => "recovered",
            FileOutcome::Skipped => "skipped",
            FileOutcome::Failed => "failed",
            FileOutcome::Unsupported => "unsupported",
        }
    }
}

/// Per-file scan record.
#[derive(Debug, Clone)]
pub struct FileReport {
    pub relative_path: String,
    pub language: Option<LanguageId>,
    pub outcome: FileOutcome,
    pub analysis: Option<FileAnalysis>,
}

/// Aggregate scan report.
#[derive(Debug, Clone, Default)]
pub struct ScanReport {
    /// Regular files visited after pruning (supported or not).
    pub visited_files: u64,
    /// Visited files whose extension is a supported language.
    pub supported_files: u64,
    /// Visited files whose extension is not supported.
    pub unsupported_files: u64,
    /// Supported files parsed without recovery artifacts.
    pub parsed_clean_files: u64,
    /// Supported files parsed with recovery artifacts.
    pub parsed_with_recovery_files: u64,
    /// Supported files skipped because of size or encoding.
    pub size_limit_skips: u64,
    pub encoding_skips: u64,
    /// Supported files whose read failed.
    pub read_failures: u64,
    /// Supported files whose grammar could not be loaded or that produced no tree.
    pub parser_failures: u64,
    /// Files where extraction (including query execution) reported an error.
    pub extraction_failures: u64,
    pub declarations: u64,
    pub imports: u64,
    pub references: u64,
    pub call_like: u64,
    pub test_candidates: u64,
    /// Sum of the byte lengths of every file that was actually analyzed.
    pub bytes_processed: u64,
    /// Sum of the byte lengths of every visited file, including skipped ones.
    pub bytes_visited: u64,
    pub files: Vec<FileReport>,
}

impl ScanReport {
    /// Files for which Tree-sitter returned a tree.
    pub fn trees_returned(&self) -> u64 {
        self.parsed_clean_files + self.parsed_with_recovery_files
    }

    /// Denominator for recovery rates: only files that produced a tree.
    pub fn recovery_rate_among_parsed(&self) -> Option<f64> {
        let trees = self.trees_returned();
        if trees == 0 {
            None
        } else {
            Some(self.parsed_with_recovery_files as f64 / trees as f64)
        }
    }

    pub fn failed_files(&self) -> u64 {
        self.read_failures + self.parser_failures
    }

    pub fn skipped_files(&self) -> u64 {
        self.size_limit_skips + self.encoding_skips
    }
}

/// Walks a repository and analyzes every supported file.
pub struct Scanner<'a> {
    analyzer: &'a Analyzer,
    options: ScanOptions,
}

impl<'a> Scanner<'a> {
    pub fn new(analyzer: &'a Analyzer, options: ScanOptions) -> Self {
        Self { analyzer, options }
    }

    pub fn scan(&self, root: &Path) -> std::io::Result<ScanReport> {
        // A missing or non-directory root is a scan-level error, not an empty
        // result: silently reporting "0 files" would hide a typo.
        let metadata = std::fs::metadata(root).map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("cannot read scan root {}: {error}", root.display()),
            )
        })?;
        if !metadata.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("scan root {} is not a directory", root.display()),
            ));
        }
        let mut report = ScanReport::default();
        let entries = self.collect(root)?;
        for path in entries {
            let language = path
                .extension()
                .and_then(|extension| extension.to_str())
                .and_then(LanguageId::from_extension);
            report.visited_files += 1;
            match language {
                None => {
                    report.unsupported_files += 1;
                    report.files.push(FileReport {
                        relative_path: relative_path(root, &path),
                        language: None,
                        outcome: FileOutcome::Unsupported,
                        analysis: None,
                    });
                }
                Some(language) => {
                    report.supported_files += 1;
                    let analysis = self.analyzer.analyze_path(root, &path);
                    accumulate(&mut report, &analysis);
                    report.files.push(FileReport {
                        relative_path: analysis.file.relative_path.clone(),
                        language: Some(language),
                        outcome: outcome_of(&analysis),
                        analysis: Some(analysis),
                    });
                }
            }
        }
        report.files.sort_by(|left, right| {
            left.relative_path
                .as_str()
                .cmp(right.relative_path.as_str())
        });
        Ok(report)
    }

    /// Discover candidate files in a deterministic order.
    fn collect(&self, root: &Path) -> std::io::Result<Vec<PathBuf>> {
        let mut builder = WalkBuilder::new(root);
        builder
            .standard_filters(false)
            .hidden(false)
            .parents(false)
            .ignore(false)
            .git_global(false)
            .git_exclude(false)
            .follow_links(false)
            .require_git(false);
        // Only ignore policy that lives inside the scan root may affect results.
        builder.git_ignore(self.options.respect_gitignore);
        builder.filter_entry(|entry| {
            if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                let name = entry.file_name().to_string_lossy();
                return !PRUNED_DIRECTORIES.contains(&name.as_ref());
            }
            true
        });

        let mut paths = Vec::new();
        for entry in builder.build() {
            let entry = match entry {
                Ok(entry) => entry,
                // A directory that cannot be read must not abort the scan.
                Err(_) => continue,
            };
            let file_type = match entry.file_type() {
                Some(file_type) => file_type,
                None => continue,
            };
            if !file_type.is_file() {
                continue;
            }
            paths.push(entry.into_path());
        }
        paths.sort_by_key(|path| relative_path(root, path));
        Ok(paths)
    }
}

fn outcome_of(analysis: &FileAnalysis) -> FileOutcome {
    match analysis.status {
        AnalysisStatus::Clean => FileOutcome::Clean,
        AnalysisStatus::Recovered => FileOutcome::Recovered,
        AnalysisStatus::Unsupported => FileOutcome::Skipped,
        AnalysisStatus::Failed => FileOutcome::Failed,
    }
}

fn accumulate(report: &mut ScanReport, analysis: &FileAnalysis) {
    for diagnostic in &analysis.diagnostics {
        match diagnostic.kind {
            DiagnosticKind::FileTooLarge => report.size_limit_skips += 1,
            DiagnosticKind::UnsupportedEncoding => report.encoding_skips += 1,
            DiagnosticKind::IoError | DiagnosticKind::DisappearedDuringScan => {
                report.read_failures += 1
            }
            DiagnosticKind::GrammarLoadError => report.parser_failures += 1,
            DiagnosticKind::QueryError => report.extraction_failures += 1,
            _ => {}
        }
    }
    match analysis.status {
        AnalysisStatus::Clean => report.parsed_clean_files += 1,
        AnalysisStatus::Recovered => report.parsed_with_recovery_files += 1,
        _ => {}
    }
    report.declarations += analysis.declarations.len() as u64;
    report.imports += analysis.imports.len() as u64;
    report.references += analysis.references.len() as u64;
    report.call_like += analysis.calls.len() as u64;
    report.test_candidates += analysis.test_candidate_count() as u64;
    report.bytes_visited += u64::from(analysis.file.byte_len);
    if matches!(
        analysis.status,
        AnalysisStatus::Clean | AnalysisStatus::Recovered
    ) {
        report.bytes_processed += u64::from(analysis.file.byte_len);
    }
}

/// A scan failure that is not attributable to one file.
pub fn scan_error(root: &Path, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error(
        DiagnosticKind::IoError,
        format!("cannot scan {}: {}", root.display(), message.into()),
    )
}

/// Convenience wrapper used by the CLI and tests.
pub fn scan_repository(
    registry: &ParserRegistry,
    root: &Path,
    options: ScanOptions,
) -> Result<ScanReport, String> {
    let analyzer = Analyzer::new(crate::parser::AnalyzerConfig::default())
        .map_err(|error| error.to_string())?;
    let _ = registry;
    Scanner::new(&analyzer, options)
        .scan(root)
        .map_err(|error| error.to_string())
}

/// Description of one supported language, used by the `languages` command.
pub struct LanguageDescription {
    pub language: LanguageId,
    pub extensions: Vec<String>,
    pub grammar: crate::parser::GrammarInfo,
    pub grammar_loads: bool,
    pub grammar_message: String,
    pub abi_version: u32,
}

/// Enumerate supported languages and verify their grammars load.
pub fn describe_languages(registry: &ParserRegistry) -> Vec<LanguageDescription> {
    registry
        .verify_grammars()
        .into_iter()
        .map(|check| LanguageDescription {
            language: check.language,
            extensions: check
                .language
                .extensions()
                .iter()
                .map(|extension| format!(".{extension}"))
                .collect(),
            grammar: check.info,
            grammar_loads: check.ok,
            grammar_message: check.message,
            abi_version: check.abi_version,
        })
        .collect()
}

/// Build a `SourceFile` view for a path without analyzing it.
pub fn source_file_view(
    root: &Path,
    path: &Path,
    language: LanguageId,
    bytes: &[u8],
) -> SourceFile {
    SourceFile::new(relative_path(root, path), language, bytes)
}
