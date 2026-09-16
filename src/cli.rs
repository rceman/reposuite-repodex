//! Experimental command line interface.
//!
//! ```text
//! reposuite-repodex languages [--json]
//! reposuite-repodex parse <file> [--json] [--include-facts] [--max-file-size <bytes>]
//! reposuite-repodex scan <repository> [--json] [--include-facts] [--max-file-size <bytes>] [--no-gitignore]
//! ```
//!
//! The CLI format is EXPERIMENTAL and is not a compatibility promise.
//!
//! Exit codes:
//!
//! ```text
//! 0 = completed without analysis or recovery errors
//! 1 = completed, but at least one file had a recovery or analysis failure
//! 2 = invocation or setup failure
//! ```
//!
//! JSON stdout carries machine-readable data only. Human-readable logs go to
//! stderr.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use serde::Serialize;

use crate::canonical;
use crate::model::{
    AnalysisStatus, Diagnostic, DiagnosticKind, FileAnalysis, LanguageId, SCHEMA_VERSION,
};
use crate::parser::{Analyzer, AnalyzerConfig};
use crate::paths::{self, RepoDexPaths};
use crate::scanner::{describe_languages, ScanOptions, ScanReport, Scanner};

pub const EXIT_OK: u8 = 0;
pub const EXIT_ANALYSIS_FAILURES: u8 = 1;
pub const EXIT_INVOCATION_FAILURE: u8 = 2;

/// Entry point used by `main`.
pub fn run(args: Vec<String>) -> ExitCode {
    match dispatch(&args) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(EXIT_INVOCATION_FAILURE)
        }
    }
}

fn dispatch(args: &[String]) -> Result<u8, String> {
    let mut arguments = args.iter().skip(1);
    let Some(command) = arguments.next() else {
        print_usage();
        return Err("missing command".to_string());
    };
    let rest: Vec<String> = arguments.cloned().collect();
    match command.as_str() {
        "languages" => command_languages(&rest),
        "parse" => command_parse(&rest),
        "scan" => command_scan(&rest),
        "help" | "--help" | "-h" => {
            print_usage();
            Ok(EXIT_OK)
        }
        "--version" | "-V" => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            Ok(EXIT_OK)
        }
        other => Err(format!("unknown command `{other}`")),
    }
}

fn print_usage() {
    println!(
        "\
reposuite-repodex {} (TASK 1 foundation spike, experimental CLI)

USAGE:
    reposuite-repodex languages [--json]
    reposuite-repodex parse <file> [--json] [--include-facts] [--max-file-size <bytes>]
    reposuite-repodex scan <repository> [--json] [--include-facts] [--max-file-size <bytes>] [--no-gitignore]

EXIT CODES:
    0  completed without analysis or recovery errors
    1  completed, but at least one file had a recovery or analysis failure
    2  invocation or setup failure
",
        env!("CARGO_PKG_VERSION")
    );
}

#[derive(Debug, Default)]
struct Options {
    json: bool,
    include_facts: bool,
    max_file_size: Option<u64>,
    respect_gitignore: bool,
    positional: Vec<String>,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        respect_gitignore: true,
        ..Options::default()
    };
    let mut index = 0usize;
    while index < args.len() {
        let argument = &args[index];
        let (name, inline_value) = match argument.split_once('=') {
            Some((name, value)) => (name, Some(value.to_string())),
            None => (argument.as_str(), None),
        };
        match name {
            "--json" => options.json = true,
            "--include-facts" | "--facts" => options.include_facts = true,
            "--no-gitignore" => options.respect_gitignore = false,
            "--max-file-size" => {
                let value = match inline_value {
                    Some(value) => value,
                    None => {
                        index += 1;
                        args.get(index)
                            .cloned()
                            .ok_or_else(|| "--max-file-size requires a value".to_string())?
                    }
                };
                options.max_file_size = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("invalid --max-file-size value `{value}`"))?,
                );
            }
            other if other.starts_with("--") => {
                return Err(format!("unknown option `{other}`"));
            }
            _ => options.positional.push(argument.clone()),
        }
        index += 1;
    }
    Ok(options)
}

fn analyzer_for(options: &Options) -> Result<Analyzer, String> {
    let mut config = AnalyzerConfig::default();
    if let Some(max_file_size) = options.max_file_size {
        config.max_file_size = max_file_size;
    }
    Analyzer::new(config).map_err(|error| error.to_string())
}

#[derive(Serialize)]
struct LanguagesOutput {
    schema_version: u32,
    command: &'static str,
    runtime_root: String,
    runtime_root_source: &'static str,
    languages: Vec<LanguageEntry>,
}

#[derive(Serialize)]
struct LanguageEntry {
    language: &'static str,
    display_name: &'static str,
    extensions: Vec<String>,
    grammar_crate: &'static str,
    grammar_version: &'static str,
    grammar_upstream: &'static str,
    grammar_license: &'static str,
    grammar_abi_version: u32,
    grammar_loads: bool,
    grammar_message: String,
    tree_sitter_runtime: &'static str,
    upstream_queries: Vec<&'static str>,
    repodex_queries: Vec<&'static str>,
}

fn command_languages(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let analyzer = analyzer_for(&options)?;
    let descriptions = describe_languages(analyzer.registry());
    let (root, source) = runtime_root_display();
    let output = LanguagesOutput {
        schema_version: SCHEMA_VERSION,
        command: "languages",
        runtime_root: root,
        runtime_root_source: source,
        languages: descriptions
            .iter()
            .map(|description| LanguageEntry {
                language: description.language.as_str(),
                display_name: description.language.display_name(),
                extensions: description.extensions.clone(),
                grammar_crate: description.grammar.crate_name,
                grammar_version: description.grammar.crate_version,
                grammar_upstream: description.grammar.upstream_repository,
                grammar_license: description.grammar.license,
                grammar_abi_version: description.abi_version,
                grammar_loads: description.grammar_loads,
                grammar_message: description.grammar_message.clone(),
                tree_sitter_runtime: description.grammar.tree_sitter_compatibility,
                upstream_queries: description.grammar.upstream_queries.to_vec(),
                repodex_queries: description.grammar.repo_queries.to_vec(),
            })
            .collect(),
    };
    if options.json {
        print_json(&output)?;
    } else {
        for entry in &output.languages {
            println!(
                "{:<7} {:<9} {}  grammar {} {} (ABI {})  loads={} [{}]",
                entry.display_name,
                entry.extensions.join(" "),
                entry.grammar_upstream,
                entry.grammar_crate,
                entry.grammar_version,
                entry.grammar_abi_version,
                entry.grammar_loads,
                entry.grammar_message
            );
        }
        println!();
        println!(
            "runtime root: {} (from {})",
            output.runtime_root, output.runtime_root_source
        );
        println!("TASK 1 resolves runtime paths but never creates them.");
    }
    let all_load = output.languages.iter().all(|entry| entry.grammar_loads);
    Ok(if all_load {
        EXIT_OK
    } else {
        EXIT_ANALYSIS_FAILURES
    })
}

fn runtime_root_display() -> (String, &'static str) {
    match RepoDexPaths::from_env() {
        Some(paths) => (
            paths.root().display().to_string(),
            if std::env::var_os(paths::HOME_ENV).is_some() {
                paths::HOME_ENV
            } else {
                "platform home directory"
            },
        ),
        None => ("<unresolved>".to_string(), "unresolved"),
    }
}

#[derive(Serialize)]
struct ParseOutput {
    schema_version: u32,
    command: &'static str,
    language: Option<&'static str>,
    status: &'static str,
    canonical_digest: String,
    analysis: FileAnalysis,
    #[serde(skip_serializing_if = "Option::is_none")]
    canonical_facts: Option<Vec<String>>,
}

fn command_parse(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let Some(path) = options.positional.first() else {
        return Err("parse requires a file path".to_string());
    };
    let path = PathBuf::from(path);
    let analyzer = analyzer_for(&options)?;
    let language = language_for_path(&path)?;
    let root = path.parent().unwrap_or_else(|| Path::new("."));
    let analysis = analyzer.analyze_path(root, &path);
    let status = analysis.status;

    if options.json {
        let output = ParseOutput {
            schema_version: SCHEMA_VERSION,
            command: "parse",
            language: Some(language.as_str()),
            status: status.as_str(),
            canonical_digest: canonical::analysis_digest(&analysis),
            canonical_facts: if options.include_facts {
                Some(analysis.canonical_lines())
            } else {
                None
            },
            analysis,
        };
        print_json(&output)?;
    } else {
        print_parse_text(&analysis);
    }
    Ok(exit_code_for_status(status))
}

fn print_parse_text(analysis: &FileAnalysis) {
    println!("file:       {}", analysis.file.relative_path);
    println!("language:   {}", analysis.file.language.display_name());
    println!("status:     {}", analysis.status.as_str());
    println!(
        "bytes:      {} ({} lines, final newline: {})",
        analysis.file.byte_len, analysis.file.line_count, analysis.file.has_final_newline
    );
    println!("snapshot:   {}", analysis.file.snapshot_id);
    println!("digest:     {}", canonical::analysis_digest(analysis));
    println!(
        "facts:      {} scopes, {} declarations, {} imports, {} references, {} call-like, {} test candidates",
        analysis.scopes.len(),
        analysis.declarations.len(),
        analysis.imports.len(),
        analysis.references.len(),
        analysis.calls.len(),
        analysis.test_candidate_count()
    );
    if !analysis.diagnostics.is_empty() {
        println!("diagnostics:");
        for diagnostic in &analysis.diagnostics {
            println!("  {}", diagnostic.render());
        }
    }
    println!("declarations:");
    for declaration in &analysis.declarations {
        let flags = if declaration.flags.is_empty() {
            String::new()
        } else {
            format!(
                " [{}]",
                declaration
                    .flags
                    .iter()
                    .map(|flag| flag.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        println!(
            "  {:<10} {:<24} scope={} {}..{}{}",
            declaration.kind.as_str(),
            declaration.name,
            analysis.scope_path(declaration.scope_id),
            declaration.range.byte_start,
            declaration.range.byte_end,
            flags
        );
    }
    println!("imports:");
    for import in &analysis.imports {
        println!(
            "  module={} items=[{}] {}..{}",
            import.module.as_deref().unwrap_or("-"),
            import
                .items
                .iter()
                .map(|item| item.target.clone())
                .collect::<Vec<_>>()
                .join(", "),
            import.statement_range.byte_start,
            import.statement_range.byte_end
        );
    }
    println!("references:");
    for reference in &analysis.references {
        println!("  {:<22} {}", reference.kind.as_str(), reference.written);
    }
    println!("call-like:");
    for call in &analysis.calls {
        println!("  {:<22} {}", call.form.as_str(), call.callee_written);
    }
}

#[derive(Serialize)]
struct ScanOutput {
    schema_version: u32,
    command: &'static str,
    root: String,
    #[serde(flatten)]
    report: ScanCounters,
    files: Vec<FileEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    canonical: Option<CanonicalExport>,
}

#[derive(Serialize)]
struct CanonicalExport {
    digest: String,
    files: Vec<CanonicalFile>,
}

#[derive(Serialize)]
struct CanonicalFile {
    path: String,
    lines: Vec<String>,
}

#[derive(Serialize)]
struct ScanCounters {
    visited_files: u64,
    supported_files: u64,
    unsupported_files: u64,
    parsed_clean_files: u64,
    parsed_with_recovery_files: u64,
    trees_returned: u64,
    skipped_files: u64,
    size_limit_skips: u64,
    encoding_skips: u64,
    read_failures: u64,
    parser_failures: u64,
    extraction_failures: u64,
    failed_files: u64,
    declarations: u64,
    imports: u64,
    references: u64,
    call_like: u64,
    test_candidates: u64,
    bytes_processed: u64,
    bytes_visited: u64,
    recovery_rate_among_parsed: Option<f64>,
    elapsed_ms: f64,
}

#[derive(Serialize)]
struct FileEntry {
    path: String,
    language: Option<&'static str>,
    outcome: &'static str,
    status: Option<&'static str>,
    declarations: u64,
    imports: u64,
    references: u64,
    call_like: u64,
    test_candidates: u64,
    bytes: u64,
    diagnostics: Vec<String>,
}

fn command_scan(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let Some(root) = options.positional.first() else {
        return Err("scan requires a repository path".to_string());
    };
    let root = PathBuf::from(root);
    if !root.is_dir() {
        return Err(format!("`{}` is not a directory", root.display()));
    }
    let analyzer = analyzer_for(&options)?;
    let scanner = Scanner::new(
        &analyzer,
        ScanOptions {
            respect_gitignore: options.respect_gitignore,
        },
    );
    let started = Instant::now();
    let report = scanner
        .scan(&root)
        .map_err(|error| format!("scan failed: {error}"))?;
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;

    let counters = ScanCounters {
        visited_files: report.visited_files,
        supported_files: report.supported_files,
        unsupported_files: report.unsupported_files,
        parsed_clean_files: report.parsed_clean_files,
        parsed_with_recovery_files: report.parsed_with_recovery_files,
        trees_returned: report.trees_returned(),
        skipped_files: report.skipped_files(),
        size_limit_skips: report.size_limit_skips,
        encoding_skips: report.encoding_skips,
        read_failures: report.read_failures,
        parser_failures: report.parser_failures,
        extraction_failures: report.extraction_failures,
        failed_files: report.failed_files(),
        declarations: report.declarations,
        imports: report.imports,
        references: report.references,
        call_like: report.call_like,
        test_candidates: report.test_candidates,
        bytes_processed: report.bytes_processed,
        bytes_visited: report.bytes_visited,
        recovery_rate_among_parsed: report.recovery_rate_among_parsed(),
        elapsed_ms,
    };

    let files = report
        .files
        .iter()
        .map(|file| {
            let analysis = file.analysis.as_ref();
            FileEntry {
                path: file.relative_path.clone(),
                language: file.language.map(|language| language.as_str()),
                outcome: file.outcome.as_str(),
                status: analysis.map(|analysis| analysis.status.as_str()),
                declarations: analysis
                    .map(|analysis| analysis.declarations.len() as u64)
                    .unwrap_or(0),
                imports: analysis
                    .map(|analysis| analysis.imports.len() as u64)
                    .unwrap_or(0),
                references: analysis
                    .map(|analysis| analysis.references.len() as u64)
                    .unwrap_or(0),
                call_like: analysis
                    .map(|analysis| analysis.calls.len() as u64)
                    .unwrap_or(0),
                test_candidates: analysis
                    .map(|analysis| analysis.test_candidate_count() as u64)
                    .unwrap_or(0),
                bytes: analysis
                    .map(|analysis| u64::from(analysis.file.byte_len))
                    .unwrap_or(0),
                diagnostics: analysis
                    .map(|analysis| {
                        analysis
                            .diagnostics
                            .iter()
                            .map(|diagnostic| diagnostic.render())
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect::<Vec<_>>();

    let canonical_export = if options.include_facts {
        Some(CanonicalExport {
            digest: canonical::scan_digest(&report),
            files: report
                .files
                .iter()
                .filter_map(|file| {
                    file.analysis.as_ref().map(|analysis| CanonicalFile {
                        path: file.relative_path.clone(),
                        lines: analysis.canonical_lines(),
                    })
                })
                .collect(),
        })
    } else {
        None
    };

    let output = ScanOutput {
        schema_version: SCHEMA_VERSION,
        command: "scan",
        root: root.display().to_string(),
        report: counters,
        files,
        canonical: canonical_export,
    };

    if options.json {
        print_json(&output)?;
    } else {
        print_scan_text(&output);
    }
    Ok(
        if output.report.failed_files > 0 || output.report.extraction_failures > 0 {
            EXIT_ANALYSIS_FAILURES
        } else {
            EXIT_OK
        },
    )
}

fn print_scan_text(output: &ScanOutput) {
    let report = &output.report;
    println!("root:            {}", output.root);
    println!("visited files:   {}", report.visited_files);
    println!(
        "  supported:     {}   unsupported: {}",
        report.supported_files, report.unsupported_files
    );
    println!(
        "  clean:         {}   with recovery: {}   trees returned: {}",
        report.parsed_clean_files, report.parsed_with_recovery_files, report.trees_returned
    );
    println!(
        "  skipped:       {} (size {}, encoding {})",
        report.skipped_files, report.size_limit_skips, report.encoding_skips
    );
    println!(
        "  read failures: {}   parser failures: {}   extraction failures: {}",
        report.read_failures, report.parser_failures, report.extraction_failures
    );
    match report.recovery_rate_among_parsed {
        Some(rate) => println!("  recovery rate among parsed: {:.2}%", rate * 100.0),
        None => println!("  recovery rate among parsed: N/A (no trees returned)"),
    }
    println!(
        "facts:           {} declarations, {} imports, {} references, {} call-like, {} test candidates",
        report.declarations, report.imports, report.references, report.call_like, report.test_candidates
    );
    println!(
        "bytes:           {} processed of {} visited",
        report.bytes_processed, report.bytes_visited
    );
    println!("elapsed:         {:.2} ms", report.elapsed_ms);
    let problems = output
        .files
        .iter()
        .filter(|file| matches!(file.outcome, "recovered" | "failed" | "skipped"))
        .collect::<Vec<_>>();
    if !problems.is_empty() {
        println!("files needing attention:");
        for file in problems {
            println!("  {:<10} {}", file.outcome, file.path);
            for diagnostic in &file.diagnostics {
                println!("      {diagnostic}");
            }
        }
    }
}

fn language_for_path(path: &Path) -> Result<LanguageId, String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or_else(|| format!("`{}` has no file extension", path.display()))?;
    LanguageId::from_extension(extension)
        .ok_or_else(|| format!("unsupported file extension `.{extension}`"))
}

fn exit_code_for_status(status: AnalysisStatus) -> u8 {
    match status {
        AnalysisStatus::Clean => EXIT_OK,
        AnalysisStatus::Recovered => EXIT_ANALYSIS_FAILURES,
        AnalysisStatus::Unsupported | AnalysisStatus::Failed => EXIT_ANALYSIS_FAILURES,
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, value).map_err(|error| error.to_string())?;
    stdout.write_all(b"\n").map_err(|error| error.to_string())?;
    Ok(())
}

/// Diagnostics helper for the CLI: a scan that could not start.
#[allow(dead_code)]
fn scan_start_diagnostic(root: &Path, message: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticKind::IoError,
        format!("cannot scan {}: {message}", root.display()),
    )
}

/// Report type re-exported for library consumers that mirror the CLI.
pub type ScanReportAlias = ScanReport;
