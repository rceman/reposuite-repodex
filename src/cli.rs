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
use crate::model::{AnalysisStatus, FileAnalysis, LanguageId, SCHEMA_VERSION};
use crate::parser::{Analyzer, AnalyzerConfig};
use crate::paths::{self, RepoDexPaths};
use crate::repository::{
    self, build_snapshot, update_snapshot, BuildOptions, BuildStats, CoverageSummary, FactTotals,
    RepositoryFactIndex, RepositoryManifest, SnapshotError,
};
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
        "index" => command_index(&rest),
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
    reposuite-repodex index build <repository> --output <snapshot-dir> [--json] [--max-file-size <bytes>] [--no-gitignore]
    reposuite-repodex index update <repository> --previous <snapshot-dir> --output <snapshot-dir> [--json] [--allow-incompatible]
    reposuite-repodex index verify <snapshot-dir> [--json]
    reposuite-repodex index stats <snapshot-dir> [--json]
    reposuite-repodex index find <snapshot-dir> [--declaration <name>] [--import <target>] [--call <callee>] [--test] [--json]

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
    /// `index build`/`index update` destination directory.
    output: Option<String>,
    /// `index update` previous snapshot directory.
    previous: Option<String>,
    /// `index update` opt-in to rebuilding from an incompatible snapshot.
    allow_incompatible: bool,
    /// `index find` selectors.
    declaration: Option<String>,
    import: Option<String>,
    call: Option<String>,
    tests_only: bool,
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
            "--allow-incompatible" => options.allow_incompatible = true,
            "--test" | "--tests" => options.tests_only = true,
            "--output" => options.output = Some(value_for(args, &mut index, name, inline_value)?),
            "--previous" => {
                options.previous = Some(value_for(args, &mut index, name, inline_value)?)
            }
            "--declaration" => {
                options.declaration = Some(value_for(args, &mut index, name, inline_value)?)
            }
            "--import" => options.import = Some(value_for(args, &mut index, name, inline_value)?),
            "--call" => options.call = Some(value_for(args, &mut index, name, inline_value)?),
            "--max-file-size" => {
                let value = value_for(args, &mut index, name, inline_value)?;
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

/// Read the value of an option, either inline (`--name=value`) or as the next
/// argument.
fn value_for(
    args: &[String],
    index: &mut usize,
    name: &str,
    inline: Option<String>,
) -> Result<String, String> {
    match inline {
        Some(value) => Ok(value),
        None => {
            *index += 1;
            args.get(*index)
                .cloned()
                .ok_or_else(|| format!("{name} requires a value"))
        }
    }
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
    /// The individual failures behind `traversal_failures`. Named differently
    /// from the counter on purpose: the counters are flattened into this object,
    /// so a field with the same name would produce a duplicate JSON key.
    traversal_failure_details: Vec<TraversalFailureEntry>,
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
    /// Paths the walk could not enter or stat. Non-zero means the scan did not
    /// cover the whole subtree under the root.
    traversal_failures: u64,
    scan_complete: bool,
    recovery_rate_among_parsed: Option<f64>,
    elapsed_ms: f64,
}

/// One traversal failure, in machine-readable form.
#[derive(Serialize)]
struct TraversalFailureEntry {
    path: String,
    kind: &'static str,
    message: String,
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
        traversal_failures: report.traversal_failures(),
        scan_complete: report.is_complete(),
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

    let traversal_failure_details = report
        .traversal_failures
        .iter()
        .map(|failure| TraversalFailureEntry {
            path: failure.relative_path.clone(),
            kind: failure.kind.as_str(),
            message: failure.message.clone(),
        })
        .collect::<Vec<_>>();

    let output = ScanOutput {
        schema_version: SCHEMA_VERSION,
        command: "scan",
        root: root.display().to_string(),
        report: counters,
        traversal_failure_details,
        files,
        canonical: canonical_export,
    };

    if options.json {
        print_json(&output)?;
    } else {
        print_scan_text(&output);
    }
    Ok(
        if output.report.failed_files > 0
            || output.report.extraction_failures > 0
            || output.report.traversal_failures > 0
        {
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
    if output.traversal_failure_details.is_empty() {
        println!("  traversal failures: 0 (scan covered the whole tree)");
    } else {
        println!(
            "  traversal failures: {} (SCAN INCOMPLETE: part of the tree was not visited)",
            output.traversal_failure_details.len()
        );
        for failure in &output.traversal_failure_details {
            println!("    {} {}: {}", failure.kind, failure.path, failure.message);
        }
    }
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
        AnalysisStatus::Recovered
        | AnalysisStatus::Incomplete
        | AnalysisStatus::Unsupported
        | AnalysisStatus::Failed => EXIT_ANALYSIS_FAILURES,
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, value).map_err(|error| error.to_string())?;
    stdout.write_all(b"\n").map_err(|error| error.to_string())?;
    Ok(())
}

/// Report type re-exported for library consumers that mirror the CLI.
pub type ScanReportAlias = ScanReport;

// ---------------------------------------------------------------------------
// `index` subcommands: deterministic repository snapshots.
// ---------------------------------------------------------------------------

fn command_index(args: &[String]) -> Result<u8, String> {
    let Some(subcommand) = args.first() else {
        return Err(
            "index requires a subcommand: build, update, verify, stats or find".to_string(),
        );
    };
    let rest = &args[1..];
    match subcommand.as_str() {
        "build" => command_index_build(rest),
        "update" => command_index_update(rest),
        "verify" => command_index_verify(rest),
        "stats" => command_index_stats(rest),
        "find" => command_index_find(rest),
        other => Err(format!(
            "unknown index subcommand `{other}`; expected build, update, verify, stats or find"
        )),
    }
}

/// Work performed by a build or update, in machine-readable form.
#[derive(Serialize)]
struct StatsOutput {
    mode: &'static str,
    files_total: u64,
    reused_files: u64,
    reparsed_files: u64,
    reextracted_files: u64,
    added: u64,
    changed: u64,
    unchanged: u64,
    deleted: u64,
    bytes_hashed: u64,
    bytes_reparsed: u64,
    duration_ms: f64,
}

impl From<&BuildStats> for StatsOutput {
    fn from(stats: &BuildStats) -> Self {
        Self {
            mode: stats.mode,
            files_total: stats.files_total,
            reused_files: stats.reused_files,
            reparsed_files: stats.reparsed_files,
            reextracted_files: stats.reextracted_files,
            added: stats.added,
            changed: stats.changed,
            unchanged: stats.unchanged,
            deleted: stats.deleted,
            bytes_hashed: stats.bytes_hashed,
            bytes_reparsed: stats.bytes_reparsed,
            duration_ms: stats.duration_ms,
        }
    }
}

#[derive(Serialize)]
struct IndexBuildOutput {
    schema_version: u32,
    command: &'static str,
    output: String,
    snapshot_digest: String,
    analyzer_fingerprint: String,
    /// True when every indexed file has a complete analysis and the walk
    /// covered the whole subtree. Never inferred from counters alone.
    fully_analyzed: bool,
    /// True when the snapshot is additionally free of recovery artifacts.
    fully_clean: bool,
    coverage: CoverageSummary,
    totals: FactTotals,
    stats: StatsOutput,
}

fn command_index_build(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let root = require_positional(&options, "index build requires a repository path")?;
    let output = options
        .output
        .clone()
        .ok_or_else(|| "index build requires --output <snapshot-dir>".to_string())?;
    let root = PathBuf::from(root);
    if !root.is_dir() {
        return Err(format!("`{}` is not a directory", root.display()));
    }
    let analyzer = analyzer_for(&options)?;
    let outcome = build_snapshot(
        &analyzer,
        &root,
        Path::new(&output),
        BuildOptions {
            respect_gitignore: options.respect_gitignore,
            allow_incompatible: false,
        },
    )
    .map_err(|error| error.to_string())?;
    emit_index_build(&outcome, &output, options.json)?;
    Ok(EXIT_OK)
}

fn command_index_update(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let root = require_positional(&options, "index update requires a repository path")?;
    let output = options
        .output
        .clone()
        .ok_or_else(|| "index update requires --output <snapshot-dir>".to_string())?;
    let previous = options
        .previous
        .clone()
        .ok_or_else(|| "index update requires --previous <snapshot-dir>".to_string())?;
    let root = PathBuf::from(root);
    if !root.is_dir() {
        return Err(format!("`{}` is not a directory", root.display()));
    }
    let analyzer = analyzer_for(&options)?;
    let outcome = update_snapshot(
        &analyzer,
        &root,
        Path::new(&previous),
        Path::new(&output),
        BuildOptions {
            respect_gitignore: options.respect_gitignore,
            allow_incompatible: options.allow_incompatible,
        },
    )
    .map_err(|error| index_update_error(error, &root, &output))?;
    emit_index_build(&outcome, &output, options.json)?;
    Ok(EXIT_OK)
}

/// Turn a previous-snapshot failure into an explicit diagnostic that tells the
/// caller how to proceed. A corrupt or incompatible previous snapshot is never
/// silently replaced with a fresh build.
fn index_update_error(error: SnapshotError, root: &Path, output: &str) -> String {
    let hint = match &error {
        SnapshotError::FingerprintMismatch { .. } | SnapshotError::ConfigMismatch { .. } => {
            format!(
            "\nhint: the previous snapshot was built by a different analyzer or configuration.\n\
             hint: run `reposuite-repodex index build {} --output {}` for a fresh snapshot, or\n\
             hint: pass --allow-incompatible to rebuild from it without reusing any analysis.",
            root.display(),
            output
        )
        }
        SnapshotError::PreviousSnapshotUnusable { .. } => format!(
            "\nhint: the previous snapshot is not usable as-is.\n\
             hint: run `reposuite-repodex index build {} --output {}` for a fresh snapshot.",
            root.display(),
            output
        ),
        _ => String::new(),
    };
    format!("{error}{hint}")
}

fn emit_index_build(
    outcome: &repository::BuildOutcome,
    output: &str,
    json: bool,
) -> Result<(), String> {
    let manifest = &outcome.manifest;
    let stats = StatsOutput::from(&outcome.stats);
    if json {
        print_json(&IndexBuildOutput {
            schema_version: SCHEMA_VERSION,
            command: "index",
            output: output.to_string(),
            snapshot_digest: manifest.snapshot_digest.clone(),
            analyzer_fingerprint: manifest.analyzer_fingerprint.clone(),
            fully_analyzed: manifest.coverage.is_fully_analyzed(),
            fully_clean: manifest.coverage.is_fully_clean(),
            coverage: manifest.coverage.clone(),
            totals: manifest.totals.clone(),
            stats,
        })
    } else {
        println!("output:        {output}");
        println!("mode:          {}", stats.mode);
        println!("snapshot:      {}", manifest.snapshot_digest);
        println!("fingerprint:   {}", manifest.analyzer_fingerprint);
        println!(
            "files:         {} (reused {}, reparsed {}, added {}, changed {}, deleted {})",
            stats.files_total,
            stats.reused_files,
            stats.reparsed_files,
            stats.added,
            stats.changed,
            stats.deleted
        );
        println!(
            "coverage:      clean {} recovered {} incomplete {} failed {} skipped {}",
            manifest.coverage.clean,
            manifest.coverage.recovered,
            manifest.coverage.incomplete,
            manifest.coverage.failed,
            manifest.coverage.skipped
        );
        println!(
            "facts:         {} declarations, {} imports, {} references, {} call-like, {} test candidates",
            manifest.totals.declarations,
            manifest.totals.imports,
            manifest.totals.references,
            manifest.totals.call_like,
            manifest.totals.test_candidates
        );
        println!(
            "bytes:         hashed {}, reparsed {}",
            stats.bytes_hashed, stats.bytes_reparsed
        );
        println!("fully analyzed: {}", manifest.coverage.is_fully_analyzed());
        println!("fully clean:    {}", manifest.coverage.is_fully_clean());
        println!("elapsed:       {:.1} ms", stats.duration_ms);
        Ok(())
    }
}

#[derive(Serialize)]
struct IndexVerifyOutput {
    schema_version: u32,
    command: &'static str,
    snapshot_dir: String,
    valid: bool,
    manifest_version: u32,
    snapshot_digest: String,
    files: u64,
    checked_artifacts: u64,
    artifact_bytes: u64,
}

fn command_index_verify(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(&options, "index verify requires a snapshot directory")?;
    let report = repository::verify(Path::new(dir)).map_err(|error| {
        format!("{error}\nnote: verification checks the artifact's internal consistency only; it does not re-read the source")
    })?;
    let output = IndexVerifyOutput {
        schema_version: SCHEMA_VERSION,
        command: "index",
        snapshot_dir: dir.to_string(),
        valid: true,
        manifest_version: report.manifest_version,
        snapshot_digest: report.snapshot_digest,
        files: report.files,
        checked_artifacts: report.checked_artifacts,
        artifact_bytes: report.artifact_bytes,
    };
    if options.json {
        print_json(&output)?;
    } else {
        println!("snapshot:       {}", output.snapshot_digest);
        println!("manifest:       version {}", output.manifest_version);
        println!("files:          {}", output.files);
        println!("artifacts:      {} checked", output.checked_artifacts);
        println!("artifact bytes: {}", output.artifact_bytes);
        println!("valid:          true");
        println!(
            "note: verification checks internal consistency only; it does not prove the\n\
             note: original source files still match this snapshot."
        );
    }
    Ok(EXIT_OK)
}

#[derive(Serialize)]
struct IndexStatsOutput {
    schema_version: u32,
    command: &'static str,
    snapshot_dir: String,
    snapshot_digest: String,
    analyzer_fingerprint: String,
    analyzer_fingerprint_text: String,
    config: repository::SnapshotConfig,
    coverage: CoverageSummary,
    totals: FactTotals,
    artifact_bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    languages: Option<Vec<LanguageStats>>,
}

#[derive(Serialize)]
struct LanguageStats {
    language: String,
    files: u64,
    source_bytes: u64,
    declarations: u64,
    imports: u64,
    references: u64,
    call_like: u64,
    test_candidates: u64,
}

fn command_index_stats(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(&options, "index stats requires a snapshot directory")?;
    let manifest = repository::load_manifest(Path::new(dir)).map_err(|error| error.to_string())?;
    let artifact_bytes = repository::artifact::artifact_size(Path::new(dir));
    let output = IndexStatsOutput {
        schema_version: SCHEMA_VERSION,
        command: "index",
        snapshot_dir: dir.to_string(),
        snapshot_digest: manifest.snapshot_digest.clone(),
        analyzer_fingerprint: manifest.analyzer_fingerprint.clone(),
        analyzer_fingerprint_text: manifest.analyzer_fingerprint_text.clone(),
        config: manifest.config.clone(),
        coverage: manifest.coverage.clone(),
        totals: manifest.totals.clone(),
        artifact_bytes,
        languages: Some(language_stats(&manifest)),
    };
    if options.json {
        print_json(&output)?;
    } else {
        print_index_stats_text(&output);
    }
    Ok(EXIT_OK)
}

fn language_stats(manifest: &RepositoryManifest) -> Vec<LanguageStats> {
    let mut by_language: std::collections::BTreeMap<&str, LanguageStats> =
        std::collections::BTreeMap::new();
    for file in &manifest.files {
        let entry = by_language
            .entry(file.language.as_str())
            .or_insert_with(|| LanguageStats {
                language: file.language.clone(),
                files: 0,
                source_bytes: 0,
                declarations: 0,
                imports: 0,
                references: 0,
                call_like: 0,
                test_candidates: 0,
            });
        entry.files += 1;
        entry.source_bytes += file.source_bytes;
        entry.declarations += file.declarations;
        entry.imports += file.imports;
        entry.references += file.references;
        entry.call_like += file.call_like;
        entry.test_candidates += file.test_candidates;
    }
    by_language.into_values().collect()
}

fn print_index_stats_text(output: &IndexStatsOutput) {
    println!("snapshot:       {}", output.snapshot_digest);
    println!("fingerprint:    {}", output.analyzer_fingerprint);
    println!("config:         {}", output.config.canonical_text());
    println!(
        "coverage:       clean {} recovered {} incomplete {} failed {} skipped {} (scan complete: {})",
        output.coverage.clean,
        output.coverage.recovered,
        output.coverage.incomplete,
        output.coverage.failed,
        output.coverage.skipped,
        output.coverage.scan_complete
    );
    println!(
        "totals:         {} files, {} source bytes",
        output.coverage.supported_files, output.totals.source_bytes
    );
    println!(
        "facts:          {} declarations, {} imports, {} references, {} call-like, {} test candidates",
        output.totals.declarations,
        output.totals.imports,
        output.totals.references,
        output.totals.call_like,
        output.totals.test_candidates
    );
    println!("artifact bytes: {}", output.artifact_bytes);
    if let Some(languages) = &output.languages {
        println!("by language:");
        for language in languages {
            println!(
                "  {:<7} files {:<6} bytes {:<10} decl {:<7} call {:<7} tests {}",
                language.language,
                language.files,
                language.source_bytes,
                language.declarations,
                language.call_like,
                language.test_candidates
            );
        }
    }
}

#[derive(Serialize)]
struct IndexFindOutput {
    schema_version: u32,
    command: &'static str,
    snapshot_dir: String,
    declarations: Vec<repository::DeclarationHit>,
    imports: Vec<repository::ImportHit>,
    calls: Vec<repository::CallHit>,
    tests: Vec<repository::DeclarationHit>,
}

fn command_index_find(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(&options, "index find requires a snapshot directory")?;
    let index = RepositoryFactIndex::load(Path::new(dir)).map_err(|error| error.to_string())?;

    let declarations = options
        .declaration
        .as_deref()
        .map(|name| index.declarations_named(name).to_vec())
        .unwrap_or_default();
    let imports = options
        .import
        .as_deref()
        .map(|target| index.import_items_targeting(target).to_vec())
        .unwrap_or_default();
    let calls = options
        .call
        .as_deref()
        .map(|callee| index.calls_named(callee).to_vec())
        .unwrap_or_default();
    let tests = if options.tests_only {
        index.test_declarations().to_vec()
    } else {
        Vec::new()
    };

    let output = IndexFindOutput {
        schema_version: SCHEMA_VERSION,
        command: "index",
        snapshot_dir: dir.to_string(),
        declarations,
        imports,
        calls,
        tests,
    };
    if options.json {
        print_json(&output)?;
    } else {
        print_find_text(&output);
    }
    Ok(EXIT_OK)
}

fn print_find_text(output: &IndexFindOutput) {
    println!("declarations: {}", output.declarations.len());
    for hit in &output.declarations {
        println!(
            "  {} {} {} {}",
            hit.relative_path,
            hit.kind,
            hit.name,
            hit.name_range.render()
        );
    }
    println!("imports: {}", output.imports.len());
    for hit in &output.imports {
        println!(
            "  {} {} target={} {}",
            hit.relative_path,
            hit.form,
            hit.target,
            hit.target_range.render()
        );
    }
    println!("calls: {}", output.calls.len());
    for hit in &output.calls {
        println!(
            "  {} {} {} {}",
            hit.relative_path,
            hit.form,
            hit.callee_written,
            hit.callee_range.render()
        );
    }
    println!("tests: {}", output.tests.len());
    for hit in &output.tests {
        println!(
            "  {} {} {} {}",
            hit.relative_path,
            hit.kind,
            hit.name,
            hit.name_range.render()
        );
    }
}

fn require_positional<'a>(options: &'a Options, message: &str) -> Result<&'a str, String> {
    options
        .positional
        .first()
        .map(String::as_str)
        .ok_or_else(|| message.to_string())
}
