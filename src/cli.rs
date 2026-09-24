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

use crate::candidates::{self, CandidateIndex};
use crate::canonical;
use crate::links::{self, LinkIndex, LinkOutcome};
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
        "links" => command_links(&rest),
        "candidates" => command_candidates(&rest),
        "graph" => command_graph(&rest),
        "query" => command_query(&rest),
        "project" => command_project(&rest),
        "serve" | "start" | "stop" | "restart" | "status" => {
            command_service(command.as_str(), &rest)
        }
        "config" => command_config(&rest),
        "temporal" => command_temporal(&rest),
        "agent-events" | "agent_events" => command_agent_events(&rest),
        "investigations" => command_investigations(&rest),
        "agent-activity" | "agent_activity" => command_agent_activity(&rest),
        "memory" => command_memory(&rest),
        "symbol-exposure" | "symbol_exposure" => command_symbol_exposure(&rest),
        "system-one" | "system_one" => command_system_one(&rest),
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
    reposuite-repodex links build <snapshot-dir> --repository <repo> --output <links-dir> [--json]
    reposuite-repodex links verify <links-dir> --snapshot <snapshot-dir> [--repository <repo>] [--json]
    reposuite-repodex links stats <links-dir> [--json]
    reposuite-repodex links show <links-dir> <path> [--json]
    reposuite-repodex links exact|ambiguous|unresolved|out-of-scope <links-dir> [--json]
    reposuite-repodex candidates build <snapshot-dir> --links <links-dir> --output <candidates-dir> [--json]
    reposuite-repodex candidates verify <candidates-dir> --snapshot <snapshot-dir> --links <links-dir> [--json]
    reposuite-repodex candidates stats <candidates-dir> [--json]
    reposuite-repodex candidates show <candidates-dir> <path> [--json]
    reposuite-repodex candidates none|single|multiple|out-of-scope <candidates-dir> [--json]
    reposuite-repodex graph build <snapshot-dir> --links <links-dir> --candidates <candidates-dir> --output <graph-dir> [--json]
    reposuite-repodex graph verify <graph-dir> --snapshot <snapshot-dir> --links <links-dir> --candidates <candidates-dir> [--json]
    reposuite-repodex graph stats <graph-dir> [--json]
    reposuite-repodex graph node <graph-dir> <id-or-key> [--json]
    reposuite-repodex graph outgoing|incoming|neighborhood <graph-dir> <id-or-key> [--json]
    reposuite-repodex graph find <graph-dir> <term> [--domain name|path|entity|id|any] [--json]
    reposuite-repodex graph callers|callees <graph-dir> <decl-or-file> [--json]
    reposuite-repodex graph paths <graph-dir> <from> <to> [--depth N] [--max-paths N] [--json]
    reposuite-repodex query --graph <graph-dir> <text> [--intent find|callers|callees|related]
        [--target <node>] [--to <node>] [--depth N] [--max-results N] [--tokens N]
        [--exhaustive] [--explain] [--json | --human]
    reposuite-repodex query --root <view-path> --query <text> [--human] [--intent ...]
        [--depth N] [--max-results N] [--tokens N] [--state-dir <dir>]
    reposuite-repodex query --project <alias> [--view <name>] --query <text> [...]
    reposuite-repodex query --json            (reads a request object from stdin; see below)
    reposuite-repodex project register <alias> --root <default-checkout> --views-root <dir>...
    reposuite-repodex project show|remove <alias> | list [--json]
    reposuite-repodex project resolve <alias> [--view <name>]
    reposuite-repodex serve [--state-dir <dir>]            run service in foreground
    reposuite-repodex start|stop|restart|status [--json] [--state-dir <dir>]
    reposuite-repodex query ... --service | --direct       route via service or offline

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
    /// `links build`/`links verify` snapshot directory dependency.
    snapshot: Option<String>,
    /// `links build`/`links verify` repository checkout root, used only to read
    /// the metadata a required link rule needs.
    repository: Option<String>,
    /// `candidates build`/`candidates verify` TASK 3B link artifact directory.
    links: Option<String>,
    /// `graph build`/`graph verify` candidate artifact directory.
    candidates: Option<String>,
    /// `index update` opt-in to rebuilding from an incompatible snapshot.
    allow_incompatible: bool,
    /// `index find` selectors.
    declaration: Option<String>,
    /// `graph find`/`query` lookup domain or intent.
    domain: Option<String>,
    /// `graph paths`/`query` traversal depth bound.
    depth: Option<usize>,
    /// `graph paths`/`query` result-count bound.
    max_results: Option<usize>,
    /// `query`/`graph` artifact directory for the query engine.
    graph: Option<String>,
    /// `query --explain` debug output.
    explain: bool,
    /// `query` output token budget.
    tokens: Option<usize>,
    /// `query --exhaustive` mode.
    exhaustive: bool,
    /// `query --human` human-readable output.
    human: bool,
    /// `query` callers/callees/paths target node.
    target: Option<String>,
    /// `query`/`paths` second endpoint.
    to: Option<String>,
    /// `query --memory off|file|symbol` — additive memory guidance policy.
    memory: Option<String>,
    /// `query --context static|adaptive` — deterministic context compiler policy.
    context: Option<String>,
    /// `query --recipes off|auto|force` — guarded evidence recipe policy.
    recipes: Option<String>,
    import: Option<String>,
    call: Option<String>,
    /// `agent-events ingest --format` input format.
    format: Option<String>,
    /// `agent-events ingest` repository checkout root for path normalization;
    /// also the `--root` explicit RepositoryView locator.
    repo_root: Option<String>,
    /// `query --query <text>` query text (alternative to positional).
    query_text: Option<String>,
    /// `query`/`project` registered project alias locator.
    project: Option<String>,
    /// `query --view <name>` / `project resolve --view` named view inside a project.
    view: Option<String>,
    /// `project register --views-root <dir>` (repeatable) view-root directories.
    views_roots: Vec<String>,
    /// `query`/`project` RepoDex state dir override (tests/isolation).
    state_dir: Option<String>,
    /// `agent-events derive` source event-store directory.
    store: Option<String>,
    /// `memory build` SymbolExposure store for symbol-level evidence (§38).
    symbol_exposure: Option<String>,
    /// `derived` full rebuild flag.
    full: bool,
    tests_only: bool,
    /// `config --unset <key>` removes a config key.
    unset: bool,
    /// `query --service` route via the running RepoDex service (§20).
    service: bool,
    /// `query --direct` force process-local (no service) (§20).
    direct: bool,
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
            "--service" => options.service = true,
            "--direct" => options.direct = true,
            "--include-facts" | "--facts" => options.include_facts = true,
            "--no-gitignore" => options.respect_gitignore = false,
            "--allow-incompatible" => options.allow_incompatible = true,
            "--test" | "--tests" => options.tests_only = true,
            "--unset" => options.unset = true,
            "--output" => options.output = Some(value_for(args, &mut index, name, inline_value)?),
            "--previous" => {
                options.previous = Some(value_for(args, &mut index, name, inline_value)?)
            }
            "--snapshot" => {
                options.snapshot = Some(value_for(args, &mut index, name, inline_value)?)
            }
            "--repository" => {
                options.repository = Some(value_for(args, &mut index, name, inline_value)?)
            }
            "--links" => options.links = Some(value_for(args, &mut index, name, inline_value)?),
            "--candidates" => {
                options.candidates = Some(value_for(args, &mut index, name, inline_value)?)
            }
            "--domain" | "--intent" => {
                options.domain = Some(value_for(args, &mut index, name, inline_value)?)
            }
            "--depth" | "--max-depth" => {
                let v = value_for(args, &mut index, name, inline_value)?;
                options.depth = Some(v.parse().map_err(|_| format!("invalid --depth `{v}`"))?);
            }
            "--max-results" | "--max-paths" => {
                let v = value_for(args, &mut index, name, inline_value)?;
                options.max_results = Some(
                    v.parse()
                        .map_err(|_| format!("invalid --max-results `{v}`"))?,
                );
            }
            "--graph" => options.graph = Some(value_for(args, &mut index, name, inline_value)?),
            "--explain" => options.explain = true,
            "--tokens" => {
                let v = value_for(args, &mut index, name, inline_value)?;
                options.tokens = Some(v.parse().map_err(|_| format!("invalid --tokens `{v}`"))?);
            }
            "--exhaustive" => options.exhaustive = true,
            "--human" => options.human = true,
            "--target" => options.target = Some(value_for(args, &mut index, name, inline_value)?),
            "--to" => options.to = Some(value_for(args, &mut index, name, inline_value)?),
            "--memory" => options.memory = Some(value_for(args, &mut index, name, inline_value)?),
            "--context" => options.context = Some(value_for(args, &mut index, name, inline_value)?),
            "--recipes" => options.recipes = Some(value_for(args, &mut index, name, inline_value)?),
            "--declaration" => {
                options.declaration = Some(value_for(args, &mut index, name, inline_value)?)
            }
            "--import" => options.import = Some(value_for(args, &mut index, name, inline_value)?),
            "--call" => options.call = Some(value_for(args, &mut index, name, inline_value)?),
            "--format" => options.format = Some(value_for(args, &mut index, name, inline_value)?),
            "--repo-root" | "--root" => {
                options.repo_root = Some(value_for(args, &mut index, name, inline_value)?)
            }
            "--query" => {
                options.query_text = Some(value_for(args, &mut index, name, inline_value)?)
            }
            "--project" => options.project = Some(value_for(args, &mut index, name, inline_value)?),
            "--view" => options.view = Some(value_for(args, &mut index, name, inline_value)?),
            "--views-root" | "--view-root" | "--views" => {
                options
                    .views_roots
                    .push(value_for(args, &mut index, name, inline_value)?)
            }
            "--state-dir" | "--repodex-state" => {
                options.state_dir = Some(value_for(args, &mut index, name, inline_value)?)
            }
            "--store" => options.store = Some(value_for(args, &mut index, name, inline_value)?),
            "--symbol-exposure" | "--sexp" => {
                options.symbol_exposure = Some(value_for(args, &mut index, name, inline_value)?)
            }
            "--full" => options.full = true,
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

// ---------------------------------------------------------------------------
// `links` subcommands: derived cross-file structural relationships.
// ---------------------------------------------------------------------------

fn command_links(args: &[String]) -> Result<u8, String> {
    let Some(subcommand) = args.first() else {
        return Err(
            "links requires a subcommand: build, verify, stats, show, exact, ambiguous, \
             unresolved or out-of-scope"
                .to_string(),
        );
    };
    let rest = &args[1..];
    match subcommand.as_str() {
        "build" => command_links_build(rest),
        "verify" => command_links_verify(rest),
        "stats" => command_links_stats(rest),
        "show" => command_links_show(rest),
        "exact" => command_links_outcome(rest, "exact"),
        "ambiguous" => command_links_outcome(rest, "ambiguous"),
        "unresolved" => command_links_outcome(rest, "unresolved"),
        "out-of-scope" | "out_of_scope" => command_links_outcome(rest, "out_of_scope"),
        other => Err(format!(
            "unknown links subcommand `{other}`; expected build, verify, stats, show, exact, \
             ambiguous, unresolved or out-of-scope"
        )),
    }
}

#[derive(Serialize)]
struct LinkStatsOutput {
    schema_version: u32,
    command: &'static str,
    link_manifest_version: u32,
    link_schema_version: u32,
    link_rule_abi_version: u32,
    snapshot_digest: String,
    snapshot_schema_version: u32,
    snapshot_analyzer_fingerprint: String,
    link_fingerprint: String,
    link_fingerprint_text: String,
    link_digest: String,
    links: u64,
    structural_entities: u64,
    outcomes: links::OutcomeCounts,
    kinds: std::collections::BTreeMap<String, u64>,
    metadata: Vec<links::MetadataDependency>,
    rules: Vec<links::RuleDocumentation>,
    artifact_bytes: u64,
}

fn command_links_build(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let snapshot = require_positional(&options, "links build requires a snapshot directory")?;
    let output = options
        .output
        .clone()
        .ok_or_else(|| "links build requires --output <links-dir>".to_string())?;
    let repository = options.repository.clone();
    if let Some(repository) = &repository {
        if !Path::new(repository).is_dir() {
            return Err(format!("`{repository}` is not a directory"));
        }
    }
    let outcome = links::build_links(
        Path::new(snapshot),
        repository.as_deref().map(Path::new),
        Path::new(&output),
    )
    .map_err(|error| error.to_string())?;

    let stats = &outcome.stats;
    let manifest = &outcome.manifest;
    if options.json {
        print_json(&serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "command": "links",
            "output": output,
            "snapshot_digest": manifest.snapshot_digest,
            "link_digest": manifest.link_digest,
            "link_fingerprint": manifest.link_fingerprint,
            "links": stats.links_total,
            "structural_entities": stats.structural_entities,
            "outcomes": stats.outcomes,
            "kinds": stats.kinds,
            "metadata_dependencies": stats.metadata_dependencies,
            "snapshot_bytes": stats.snapshot_bytes,
            "link_bytes": stats.link_bytes,
            "size_ratio": stats.size_ratio(),
            "duration_ms": stats.duration_ms,
            "phases": {
                "snapshot_load_ms": stats.phases.snapshot_load_ms,
                "metadata_ms": stats.phases.metadata_ms,
                "structure_ms": stats.phases.structure_ms,
                "derive_ms": stats.phases.derive_ms,
                "serialize_ms": stats.phases.serialize_ms,
                "verify_ms": stats.phases.verify_ms,
            },
        }))?;
    } else {
        println!("output:        {output}");
        println!("snapshot:      {}", manifest.snapshot_digest);
        println!("link digest:   {}", manifest.link_digest);
        println!("fingerprint:   {}", manifest.link_fingerprint);
        println!(
            "files:         {} (snapshot {} bytes, links {} bytes, ratio {:.4})",
            stats.snapshot_files,
            stats.snapshot_bytes,
            stats.link_bytes,
            stats.size_ratio()
        );
        println!(
            "links:         {} (exact {}, ambiguous {}, unresolved {}, out-of-scope {})",
            stats.links_total,
            stats.outcomes.exact,
            stats.outcomes.ambiguous,
            stats.outcomes.unresolved,
            stats.outcomes.out_of_scope
        );
        println!("entities:      {}", stats.structural_entities);
        println!("metadata deps: {}", stats.metadata_dependencies);
        for (kind, count) in &stats.kinds {
            println!("  kind {kind}: {count}");
        }
        println!(
            "elapsed:       {:.1} ms (load {:.1}, metadata {:.1}, structure {:.1}, derive {:.1}, \
             serialize {:.1}, verify {:.1})",
            stats.duration_ms,
            stats.phases.snapshot_load_ms,
            stats.phases.metadata_ms,
            stats.phases.structure_ms,
            stats.phases.derive_ms,
            stats.phases.serialize_ms,
            stats.phases.verify_ms,
        );
    }
    Ok(EXIT_OK)
}

fn command_links_verify(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(&options, "links verify requires a link artifact directory")?;
    let snapshot = options
        .snapshot
        .clone()
        .ok_or_else(|| "links verify requires --snapshot <snapshot-dir>".to_string())?;
    let repository = options.repository.clone();
    let report = links::verify(
        Path::new(dir),
        Path::new(&snapshot),
        repository.as_deref().map(Path::new),
    )
    .map_err(|error| {
        format!(
            "{error}\nnote: verification checks the artifact's internal consistency with the \
             recorded snapshot and metadata; it does not prove any relationship is semantically \
             correct at runtime"
        )
    })?;
    if options.json {
        print_json(&serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "command": "links",
            "links_dir": dir,
            "snapshot_dir": snapshot,
            "valid": true,
            "link_manifest_version": report.link_manifest_version,
            "link_schema_version": report.link_schema_version,
            "link_rule_abi_version": report.link_rule_abi_version,
            "snapshot_digest": report.snapshot_digest,
            "link_digest": report.link_digest,
            "links": report.links,
            "structural_entities": report.structural_entities,
            "outcomes": report.outcomes,
            "artifact_bytes": report.artifact_bytes,
            "metadata_checked": report.metadata_checked,
            "metadata_dependencies": report.metadata_dependencies,
        }))?;
    } else {
        println!("snapshot:       {}", report.snapshot_digest);
        println!("link digest:    {}", report.link_digest);
        println!(
            "manifest:       link version {}, schema {}, rule abi {}",
            report.link_manifest_version, report.link_schema_version, report.link_rule_abi_version
        );
        println!(
            "links:          {} (exact {}, ambiguous {}, unresolved {}, out-of-scope {})",
            report.links,
            report.outcomes.exact,
            report.outcomes.ambiguous,
            report.outcomes.unresolved,
            report.outcomes.out_of_scope
        );
        println!("entities:       {}", report.structural_entities);
        println!("artifact bytes: {}", report.artifact_bytes);
        println!(
            "metadata:       {} dependencies, checked {}",
            report.metadata_dependencies, report.metadata_checked
        );
        if !report.metadata_checked {
            println!(
                "note: metadata dependencies were not re-checked; pass --repository <repo> to \
                 verify them against the working tree."
            );
        }
        println!("valid:          true");
        println!(
            "note: verification checks internal consistency only; it does not prove any\n\
             note: relationship is semantically correct at runtime."
        );
    }
    Ok(EXIT_OK)
}

fn command_links_stats(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(&options, "links stats requires a link artifact directory")?;
    let index = LinkIndex::load(Path::new(dir)).map_err(|error| error.to_string())?;
    let manifest = index.manifest();
    let output = LinkStatsOutput {
        schema_version: SCHEMA_VERSION,
        command: "links",
        link_manifest_version: manifest.link_manifest_version,
        link_schema_version: manifest.link_schema_version,
        link_rule_abi_version: manifest.link_rule_abi_version,
        snapshot_digest: manifest.snapshot_digest.clone(),
        snapshot_schema_version: manifest.snapshot_schema_version,
        snapshot_analyzer_fingerprint: manifest.snapshot_analyzer_fingerprint.clone(),
        link_fingerprint: manifest.link_fingerprint.clone(),
        link_fingerprint_text: manifest.link_fingerprint_text.clone(),
        link_digest: manifest.link_digest.clone(),
        links: index.links().len() as u64,
        structural_entities: index.entities().len() as u64,
        outcomes: manifest.outcomes.clone(),
        kinds: manifest.kinds.clone(),
        metadata: manifest.metadata.clone(),
        rules: manifest.rules.clone(),
        artifact_bytes: links::artifact_size(Path::new(dir)),
    };
    if options.json {
        print_json(&output)?;
    } else {
        println!("snapshot:      {}", output.snapshot_digest);
        println!("link digest:   {}", output.link_digest);
        println!("fingerprint:   {}", output.link_fingerprint);
        println!("links:         {}", output.links);
        println!("entities:      {}", output.structural_entities);
        println!(
            "outcomes:      exact {}, ambiguous {}, unresolved {}, out-of-scope {}",
            output.outcomes.exact,
            output.outcomes.ambiguous,
            output.outcomes.unresolved,
            output.outcomes.out_of_scope
        );
        for (kind, count) in &output.kinds {
            println!("  kind {kind}: {count}");
        }
        println!("metadata deps: {}", output.metadata.len());
        for dependency in &output.metadata {
            println!(
                "  {} present={} field={} value={}",
                dependency.relative_path, dependency.present, dependency.field, dependency.value
            );
        }
        println!("rules:         {}", output.rules.len());
        for (rule_id, count) in index.counts_by_rule() {
            println!("  {rule_id}: {count}");
        }
        println!("artifact bytes: {}", output.artifact_bytes);
    }
    Ok(EXIT_OK)
}

#[derive(Serialize)]
struct LinkRow {
    link_id: String,
    kind: String,
    rule_id: String,
    language: String,
    source: String,
    written: String,
    outcome: String,
    targets: Vec<String>,
    evidence: Vec<String>,
    metadata: Vec<links::MetadataDependency>,
}

fn link_row(link: &links::LinkRecord) -> LinkRow {
    LinkRow {
        link_id: link.link_id.clone(),
        kind: link.kind.clone(),
        rule_id: link.rule_id.clone(),
        language: link.language.clone(),
        source: link.source.key(),
        written: link.written.clone(),
        outcome: link.outcome.as_str().to_string(),
        targets: link
            .outcome
            .all_targets()
            .iter()
            .map(|target| target.render())
            .collect(),
        evidence: link.provenance.evidence.clone(),
        metadata: link.provenance.metadata.clone(),
    }
}

fn print_link_rows(rows: &[LinkRow]) {
    for row in rows {
        println!(
            "{} {} {} {} {} -> {}",
            row.language,
            row.outcome,
            row.rule_id,
            row.source,
            row.written,
            row.targets.join(", ")
        );
    }
}

fn command_links_show(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(&options, "links show requires a link artifact directory")?;
    let path = options
        .positional
        .get(1)
        .cloned()
        .ok_or_else(|| "links show requires a source path".to_string())?;
    let index = LinkIndex::load(Path::new(dir)).map_err(|error| error.to_string())?;
    let mut records: Vec<&links::LinkRecord> = index.from_source(&path);
    records.extend(index.targeting_file(&path));
    records.sort_by_key(|link| link.source.key());
    records.dedup_by(|left, right| left.link_id == right.link_id);
    let rows: Vec<LinkRow> = records.iter().map(|link| link_row(link)).collect();
    if options.json {
        print_json(&serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "command": "links",
            "path": path,
            "links": rows,
        }))?;
    } else {
        println!("path:  {path}");
        println!("links: {}", rows.len());
        print_link_rows(&rows);
    }
    Ok(EXIT_OK)
}

fn command_links_outcome(args: &[String], outcome: &str) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(&options, "links requires a link artifact directory")?;
    let index = LinkIndex::load(Path::new(dir)).map_err(|error| error.to_string())?;
    let records = match outcome {
        "exact" => index.exact(),
        "ambiguous" => index.ambiguous(),
        "unresolved" => index.unresolved(),
        _ => index.out_of_scope(),
    };
    let rows: Vec<LinkRow> = records.iter().map(|link| link_row(link)).collect();
    if options.json {
        print_json(&serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "command": "links",
            "outcome": outcome,
            "count": rows.len(),
            "links": rows,
        }))?;
    } else {
        println!("outcome: {outcome}");
        println!("count:   {}", rows.len());
        print_link_rows(&rows);
    }
    Ok(EXIT_OK)
}

/// Re-exported so the outcome helper can be used from tests without pulling in
/// the whole module path.
pub fn outcome_name(outcome: &LinkOutcome) -> &'static str {
    outcome.as_str()
}

// ---------------------------------------------------------------------------
// `candidates` subcommands: derived Rust call-candidate records.
// ---------------------------------------------------------------------------

fn command_candidates(args: &[String]) -> Result<u8, String> {
    let Some(subcommand) = args.first() else {
        return Err(
            "candidates requires a subcommand: build, verify, stats, show, none, single, \
             multiple or out-of-scope"
                .to_string(),
        );
    };
    let rest = &args[1..];
    match subcommand.as_str() {
        "build" => command_candidates_build(rest),
        "verify" => command_candidates_verify(rest),
        "stats" => command_candidates_stats(rest),
        "show" => command_candidates_show(rest),
        "none" | "no-candidate" | "no_candidate" => {
            command_candidates_outcome(rest, "no_candidate")
        }
        "single" | "single-candidate" | "single_candidate" => {
            command_candidates_outcome(rest, "single_candidate")
        }
        "multiple" | "multiple-candidates" | "multiple_candidates" => {
            command_candidates_outcome(rest, "multiple_candidates")
        }
        "out-of-scope" | "out_of_scope" => command_candidates_outcome(rest, "out_of_scope"),
        other => Err(format!(
            "unknown candidates subcommand `{other}`; expected build, verify, stats, show, \
             none, single, multiple or out-of-scope"
        )),
    }
}

fn command_candidates_build(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let snapshot = require_positional(&options, "candidates build requires a snapshot directory")?;
    let links_dir = options
        .links
        .clone()
        .ok_or_else(|| "candidates build requires --links <links-dir>".to_string())?;
    if !Path::new(&links_dir).is_dir() {
        return Err(format!("`{links_dir}` is not a link artifact directory"));
    }
    let output = options
        .output
        .clone()
        .ok_or_else(|| "candidates build requires --output <candidates-dir>".to_string())?;
    let outcome = candidates::build_candidates(
        Path::new(snapshot),
        Path::new(&links_dir),
        Path::new(&output),
    )
    .map_err(|error| error.to_string())?;

    let stats = &outcome.stats;
    let manifest = &outcome.manifest;
    if options.json {
        print_json(&serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "command": "candidates",
            "output": output,
            "snapshot_digest": manifest.snapshot_digest,
            "link_digest": manifest.link_digest,
            "candidate_digest": manifest.candidate_digest,
            "candidate_fingerprint": manifest.candidate_fingerprint,
            "records": stats.records_total,
            "cardinalities": stats.cardinalities,
            "snapshot_bytes": stats.snapshot_bytes,
            "link_bytes": stats.link_bytes,
            "candidate_bytes": stats.candidate_bytes,
            "size_ratio": stats.size_ratio(),
            "link_ratio": stats.link_ratio(),
            "duration_ms": stats.duration_ms,
            "phases": {
                "snapshot_load_ms": stats.phases.snapshot_load_ms,
                "derive_ms": stats.phases.derive_ms,
                "serialize_ms": stats.phases.serialize_ms,
                "verify_ms": stats.phases.verify_ms,
            },
        }))?;
    } else {
        println!("output:        {output}");
        println!("snapshot:      {}", manifest.snapshot_digest);
        println!("link:          {}", manifest.link_digest);
        println!("candidate:     {}", manifest.candidate_digest);
        println!("fingerprint:   {}", manifest.candidate_fingerprint);
        println!(
            "files:         {} (snapshot {} bytes, links {} bytes, candidates {} bytes, \
             snapshot-ratio {:.4}, link-ratio {:.4})",
            stats.snapshot_files,
            stats.snapshot_bytes,
            stats.link_bytes,
            stats.candidate_bytes,
            stats.size_ratio(),
            stats.link_ratio()
        );
        println!(
            "records:       {} (single {}, multiple {}, none {}, out-of-scope {})",
            stats.records_total,
            stats.cardinalities.single_candidate,
            stats.cardinalities.multiple_candidates,
            stats.cardinalities.no_candidate,
            stats.cardinalities.out_of_scope
        );
        println!(
            "elapsed:       {:.1} ms (load {:.1}, derive {:.1}, serialize {:.1}, verify {:.1})",
            stats.duration_ms,
            stats.phases.snapshot_load_ms,
            stats.phases.derive_ms,
            stats.phases.serialize_ms,
            stats.phases.verify_ms,
        );
    }
    Ok(EXIT_OK)
}

fn command_candidates_verify(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(
        &options,
        "candidates verify requires a candidate artifact directory",
    )?;
    let snapshot = options
        .snapshot
        .clone()
        .ok_or_else(|| "candidates verify requires --snapshot <snapshot-dir>".to_string())?;
    let links_dir = options
        .links
        .clone()
        .ok_or_else(|| "candidates verify requires --links <links-dir>".to_string())?;
    let report = candidates::verify(Path::new(dir), Path::new(&snapshot), Path::new(&links_dir))
        .map_err(|error| {
            format!(
                "{error}\nnote: verification checks the artifact's internal consistency with the \
             recorded snapshot and link artifact; it does not prove any call resolves to a \
             candidate at runtime"
            )
        })?;
    if options.json {
        print_json(&serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "command": "candidates",
            "candidates_dir": dir,
            "snapshot_dir": snapshot,
            "links_dir": links_dir,
            "valid": true,
            "candidate_manifest_version": report.candidate_manifest_version,
            "candidate_schema_version": report.candidate_schema_version,
            "candidate_rule_abi_version": report.candidate_rule_abi_version,
            "snapshot_digest": report.snapshot_digest,
            "link_digest": report.link_digest,
            "candidate_digest": report.candidate_digest,
            "records": report.records,
            "cardinalities": report.cardinalities,
            "artifact_bytes": report.artifact_bytes,
        }))?;
    } else {
        println!("valid:            true");
        println!("candidates:       {dir}");
        println!("snapshot:         {snapshot}");
        println!("links:            {links_dir}");
        println!("snapshot digest:  {}", report.snapshot_digest);
        println!("link digest:      {}", report.link_digest);
        println!("candidate digest: {}", report.candidate_digest);
        println!(
            "records:          {} (single {}, multiple {}, none {}, out-of-scope {})",
            report.records,
            report.cardinalities.single_candidate,
            report.cardinalities.multiple_candidates,
            report.cardinalities.no_candidate,
            report.cardinalities.out_of_scope
        );
        println!("artifact bytes:   {}", report.artifact_bytes);
        println!(
            "note:             internal consistency only; a candidate is not a resolved call target"
        );
    }
    Ok(EXIT_OK)
}

fn command_candidates_stats(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(
        &options,
        "candidates stats requires a candidate artifact directory",
    )?;
    let index = CandidateIndex::load(Path::new(dir)).map_err(|error| error.to_string())?;
    let manifest = &index.manifest;
    let counts = index.cardinality_counts();
    let rules = index.counts_by_rule();
    if options.json {
        print_json(&serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "command": "candidates",
            "candidate_manifest_version": manifest.candidate_manifest_version,
            "candidate_schema_version": manifest.candidate_schema_version,
            "candidate_rule_abi_version": manifest.candidate_rule_abi_version,
            "snapshot_digest": manifest.snapshot_digest,
            "link_digest": manifest.link_digest,
            "candidate_fingerprint": manifest.candidate_fingerprint,
            "candidate_fingerprint_text": manifest.candidate_fingerprint_text,
            "candidate_digest": manifest.candidate_digest,
            "records": manifest.records,
            "cardinalities": manifest.cardinalities,
            "by_rule": rules,
            "rules": manifest.rules,
        }))?;
    } else {
        println!(
            "candidate manifest version: {}",
            manifest.candidate_manifest_version
        );
        println!(
            "candidate schema version:   {}",
            manifest.candidate_schema_version
        );
        println!(
            "candidate rule abi version: {}",
            manifest.candidate_rule_abi_version
        );
        println!("snapshot digest:            {}", manifest.snapshot_digest);
        println!("link digest:                {}", manifest.link_digest);
        println!(
            "candidate fingerprint:      {}",
            manifest.candidate_fingerprint
        );
        println!("candidate digest:           {}", manifest.candidate_digest);
        println!("records:                    {}", manifest.records);
        println!("  single:   {}", counts.single_candidate);
        println!("  multiple: {}", counts.multiple_candidates);
        println!("  none:     {}", counts.no_candidate);
        println!("  out-of-scope: {}", counts.out_of_scope);
        for (rule_id, count) in &rules {
            println!("  rule {rule_id}: {count}");
        }
        for rule in &manifest.rules {
            println!("rule {}", rule.rule_id);
            println!("  in scope:   {}", rule.in_scope_calls);
            println!("  candidates: {}", rule.candidate_declarations);
            println!("  selection:  {}", rule.selection_rule);
            println!("  one means:  {}", rule.single_candidate_meaning);
            println!("  none means: {}", rule.no_candidate_meaning);
        }
    }
    Ok(EXIT_OK)
}

#[derive(Serialize)]
struct CandidateRow {
    record_id: String,
    rule_id: String,
    language: String,
    source: String,
    written: String,
    outcome: String,
    candidates: Vec<String>,
    scope_path: String,
    evidence: Vec<String>,
}

fn candidate_row(record: &candidates::CallCandidateRecord) -> CandidateRow {
    CandidateRow {
        record_id: record.record_id.clone(),
        rule_id: record.rule_id.clone(),
        language: record.language.clone(),
        source: record.source.key(),
        written: record.written.clone(),
        outcome: record.outcome.as_str().to_string(),
        candidates: record
            .outcome
            .candidates()
            .iter()
            .map(|candidate| candidate.render())
            .collect(),
        scope_path: record.provenance.scope_path.clone(),
        evidence: record.provenance.evidence.clone(),
    }
}

fn print_candidate_rows(rows: &[CandidateRow]) {
    for row in rows {
        println!(
            "{} {} {} {} {} -> {}",
            row.language,
            row.outcome,
            row.rule_id,
            row.source,
            row.written,
            row.candidates.join(", ")
        );
    }
}

fn command_candidates_show(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(
        &options,
        "candidates show requires a candidate artifact directory",
    )?;
    let path = options
        .positional
        .get(1)
        .cloned()
        .ok_or_else(|| "candidates show requires a relative path".to_string())?;
    let index = CandidateIndex::load(Path::new(dir)).map_err(|error| error.to_string())?;
    let mut records = index.from_source(&path);
    records.extend(index.targeting_file(&path));
    records.sort_by_key(|record| (record.source.relative_path.clone(), record.source.fact_id));
    records.dedup_by_key(|record| record.record_id.clone());
    let rows: Vec<CandidateRow> = records.iter().map(|record| candidate_row(record)).collect();
    if options.json {
        print_json(&serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "command": "candidates",
            "path": path,
            "count": rows.len(),
            "records": rows,
        }))?;
    } else {
        println!("path:  {path}");
        println!("count: {}", rows.len());
        print_candidate_rows(&rows);
    }
    Ok(EXIT_OK)
}

fn command_candidates_outcome(args: &[String], outcome: &str) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(
        &options,
        "candidates requires a candidate artifact directory",
    )?;
    let index = CandidateIndex::load(Path::new(dir)).map_err(|error| error.to_string())?;
    let records = match outcome {
        "no_candidate" => index.none(),
        "single_candidate" => index.single(),
        "multiple_candidates" => index.multiple(),
        _ => index.out_of_scope(),
    };
    let rows: Vec<CandidateRow> = records.iter().map(|record| candidate_row(record)).collect();
    if options.json {
        print_json(&serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "command": "candidates",
            "outcome": outcome,
            "count": rows.len(),
            "records": rows,
        }))?;
    } else {
        println!("outcome: {outcome}");
        println!("count:   {}", rows.len());
        print_candidate_rows(&rows);
    }
    Ok(EXIT_OK)
}

// `graph` subcommands: the TASK 5A investigation graph.
fn command_graph(args: &[String]) -> Result<u8, String> {
    let Some(subcommand) = args.first() else {
        return Err(
            "graph requires a subcommand: build, verify, stats, node, outgoing, incoming or \
             neighborhood"
                .to_string(),
        );
    };
    let rest = &args[1..];
    match subcommand.as_str() {
        "build" => command_graph_build(rest),
        "verify" => command_graph_verify(rest),
        "stats" => command_graph_stats(rest),
        "node" => command_graph_lookup(rest, "node"),
        "outgoing" => command_graph_lookup(rest, "outgoing"),
        "incoming" => command_graph_lookup(rest, "incoming"),
        "neighborhood" => command_graph_lookup(rest, "neighborhood"),
        "find" => command_graph_find(rest),
        "callers" => command_graph_callers(rest),
        "callees" => command_graph_callees(rest),
        "paths" => command_graph_paths(rest),
        other => Err(format!("unknown graph subcommand `{other}`")),
    }
}

fn command_graph_build(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let snapshot = require_positional(&options, "graph build requires a snapshot directory")?;
    let links = options
        .links
        .clone()
        .ok_or_else(|| "graph build requires --links <links-dir>".to_string())?;
    let candidates = options
        .candidates
        .clone()
        .ok_or_else(|| "graph build requires --candidates <candidates-dir>".to_string())?;
    let output = options
        .output
        .clone()
        .ok_or_else(|| "graph build requires --output <graph-dir>".to_string())?;
    let outcome = crate::graph::build_graph(
        Path::new(snapshot),
        Path::new(&links),
        Path::new(&candidates),
        Path::new(&output),
    )
    .map_err(|e| e.to_string())?;
    let stats = &outcome.stats;
    if options.json {
        print_json(&serde_json::json!({
            "schema_version": SCHEMA_VERSION,
            "command": "graph build",
            "output": output,
            "graph_digest": outcome.manifest.graph_digest,
            "nodes": stats.nodes,
            "edges": stats.edges,
            "candidate_edges": stats.candidate_edges,
            "graph_bytes": stats.graph_bytes,
            "duration_ms": stats.duration_ms,
        }))?;
    } else {
        println!("output:        {output}");
        println!("graph:         {}", outcome.manifest.graph_digest);
        println!("nodes:         {}", stats.nodes);
        println!("edges:         {}", stats.edges);
        println!("candidate edges: {}", stats.candidate_edges);
        println!("bytes:         {}", stats.graph_bytes);
        println!("elapsed:       {:.1} ms", stats.duration_ms);
    }
    Ok(EXIT_OK)
}

fn command_graph_verify(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let graph = require_positional(&options, "graph verify requires a graph directory")?;
    let snapshot = options
        .snapshot
        .clone()
        .ok_or_else(|| "graph verify requires --snapshot <snapshot-dir>".to_string())?;
    let links = options
        .links
        .clone()
        .ok_or_else(|| "graph verify requires --links <links-dir>".to_string())?;
    let candidates = options
        .candidates
        .clone()
        .ok_or_else(|| "graph verify requires --candidates <candidates-dir>".to_string())?;
    match crate::graph::verify(
        Path::new(graph),
        Path::new(&snapshot),
        Path::new(&links),
        Path::new(&candidates),
    ) {
        Ok(()) => {
            if options.json {
                print_json(&serde_json::json!({"command":"graph verify","status":"ok"}))?;
            } else {
                println!("graph: verified");
            }
            Ok(EXIT_OK)
        }
        Err(e) => Err(e.to_string()),
    }
}

fn command_graph_stats(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(&options, "graph stats requires a graph directory")?;
    let index = crate::graph::GraphIndex::load(Path::new(dir)).map_err(|e| e.to_string())?;
    let stats = index.stats();
    if options.json {
        print_json(&serde_json::json!({
            "command": "graph stats",
            "nodes": stats.nodes,
            "edges": stats.edges,
            "node_kinds": stats.node_kinds,
            "edge_kinds": stats.edge_kinds,
            "candidate_edges": stats.candidate_edges,
            "candidate_sets": stats.candidate_sets,
            "call_dispositions": stats.call_dispositions,
            "languages": stats.languages,
        }))?;
    } else {
        println!("nodes:            {}", stats.nodes);
        println!("edges:            {}", stats.edges);
        println!("candidate edges:  {}", stats.candidate_edges);
        println!("candidate sets:   {}", stats.candidate_sets);
        for (k, v) in &stats.node_kinds {
            println!("  node {:<13} {}", k, v);
        }
        for (k, v) in &stats.edge_kinds {
            println!("  edge {:<28} {}", k, v);
        }
        for (d, v) in &stats.call_dispositions {
            println!("  call {:<18} {}", d, v);
        }
        println!("languages:        {}", stats.languages.join(", "));
    }
    Ok(EXIT_OK)
}

fn command_graph_lookup(args: &[String], mode: &str) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(
        &options,
        &format!("graph {mode} requires a graph directory"),
    )?;
    let id = options
        .positional
        .get(1)
        .cloned()
        .ok_or_else(|| format!("graph {mode} requires a node id or key"))?;
    let index = crate::graph::GraphIndex::load(Path::new(dir)).map_err(|e| e.to_string())?;
    match mode {
        "node" => {
            let Some(node) = index.node(&id) else {
                return Err(format!("no graph node `{id}`"));
            };
            if options.json {
                print_json(&serde_json::json!({"command":"graph node","node":node}))?;
            } else {
                println!("id:         {}", node.node_id);
                println!("key:        {}", node.key);
                println!("kind:       {}", node.kind.as_str());
                println!("language:   {}", node.language);
                println!("path:       {}", node.path);
                println!("label:      {}", node.label);
                if let Some(d) = &node.disposition {
                    println!("disposition:{d}");
                }
            }
        }
        "outgoing" | "incoming" => {
            let edges = if mode == "outgoing" {
                index.outgoing(&id)
            } else {
                index.incoming(&id)
            };
            let rows: Vec<_> = edges
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "edge_id": e.edge_id, "kind": e.kind,
                        "evidence": e.evidence_class.as_str(),
                        "source": e.source, "target": e.target,
                        "candidate_set": e.candidate_set_id, "rule": e.rule_id,
                    })
                })
                .collect();
            if options.json {
                print_json(
                    &serde_json::json!({"command":format!("graph {mode}"),"node":id,"count":rows.len(),"edges":rows}),
                )?;
            } else {
                println!("{mode} edges for {id}: {}", rows.len());
                for e in &edges {
                    println!(
                        "  {} {} {} -> {}",
                        e.evidence_class.as_str(),
                        e.kind,
                        e.source,
                        e.target
                    );
                }
            }
        }
        "neighborhood" => {
            let neighbors = index.neighborhood(&id, &crate::graph::NeighborFilter::both());
            if options.json {
                let rows: Vec<_> = neighbors
                    .iter()
                    .map(|n| {
                        serde_json::json!({"direction":n.direction,"edge":n.edge,"node":n.node})
                    })
                    .collect();
                print_json(
                    &serde_json::json!({"command":"graph neighborhood","node":id,"count":rows.len(),"neighbors":rows}),
                )?;
            } else {
                println!("neighborhood of {id}: {}", neighbors.len());
                for n in &neighbors {
                    println!(
                        "  {} {} {} {} ({})",
                        n.direction,
                        n.edge.evidence_class.as_str(),
                        n.edge.kind,
                        n.node.key,
                        n.node.kind.as_str()
                    );
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(EXIT_OK)
}

// Phase A deterministic investigation primitives.
fn graph_index(options: &Options) -> Result<crate::graph::GraphIndex, String> {
    let dir = require_positional(options, "graph command requires a graph directory")?;
    crate::graph::GraphIndex::load(Path::new(dir)).map_err(|e| e.to_string())
}

fn graph_arg(options: &Options, n: usize, what: &str) -> Result<String, String> {
    options
        .positional
        .get(n)
        .cloned()
        .ok_or_else(|| format!("missing {what}"))
}

fn command_graph_find(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let index = graph_index(&options)?;
    let term = graph_arg(&options, 1, "find term")?;
    let domain = match options.domain.as_deref() {
        None | Some("any") => crate::graph::LookupDomain::Any,
        Some("id") => crate::graph::LookupDomain::Id,
        Some("name") => crate::graph::LookupDomain::Name,
        Some("path") => crate::graph::LookupDomain::Path,
        Some("entity") | Some("package") => crate::graph::LookupDomain::Entity,
        Some(d) => return Err(format!("unknown find domain `{d}`")),
    };
    let matches = index.find(&term, domain);
    if options.json {
        print_json(&serde_json::json!({
            "command":"graph find","term":term,"count":matches.len(),
            "matches":matches.iter().map(|n|serde_json::json!({"id":n.node_id,"key":n.key,"kind":n.kind.as_str(),"label":n.label,"path":n.path})).collect::<Vec<_>>(),
        }))?;
    } else {
        println!("find `{term}`: {} match(es)", matches.len());
        for n in &matches {
            println!("  {:<12} {:<40} {}", n.kind.as_str(), n.label, n.key);
        }
    }
    Ok(EXIT_OK)
}

fn command_graph_callers(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let index = graph_index(&options)?;
    let decl = graph_arg(&options, 1, "declaration id/key/name")?;
    // Resolve a name to declaration node(s); ambiguous names report ambiguity.
    let mut targets = index.find(&decl, crate::graph::LookupDomain::Name);
    if targets.is_empty() {
        targets = index.find(&decl, crate::graph::LookupDomain::Id);
    }
    if targets.is_empty() {
        return Err(format!("no declaration `{decl}`"));
    }
    let mut rows = Vec::new();
    for t in &targets {
        for e in index.callers(&t.node_id) {
            let src = index.node(&e.source);
            rows.push(serde_json::json!({
                "declaration": t.label,
                "caller_call_site": src.map(|n| n.key.clone()),
                "caller_label": src.map(|n| n.label.clone()),
                "candidate_set": e.candidate_set_id,
                "rule": e.rule_id, "evidence": e.evidence_class.as_str(),
            }));
        }
    }
    if options.json {
        print_json(
            &serde_json::json!({"command":"graph callers","target":decl,"count":rows.len(),"candidate_callers":rows}),
        )?;
    } else {
        println!("candidate callers of `{decl}`: {}", rows.len());
        for (i, _) in rows.iter().enumerate() {
            let r = &rows[i];
            println!(
                "  {} -> {} ({})",
                r["caller_call_site"].as_str().unwrap_or("?"),
                decl,
                r["evidence"].as_str().unwrap_or("candidate")
            );
        }
    }
    Ok(EXIT_OK)
}

fn command_graph_callees(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let index = graph_index(&options)?;
    let id = graph_arg(&options, 1, "declaration/file id or key")?;
    let mut targets = index.find(&id, crate::graph::LookupDomain::Name);
    if targets.is_empty() {
        targets = index.find(&id, crate::graph::LookupDomain::Id);
    }
    if targets.is_empty() {
        targets = index.find(&id, crate::graph::LookupDomain::Path);
    }
    if targets.is_empty() {
        return Err(format!("no node `{id}`"));
    }
    let mut rows = Vec::new();
    for t in &targets {
        for (contains, cand, decl) in index.callees(&t.node_id) {
            let call = index.node(&contains.target);
            rows.push(serde_json::json!({
                "via_call_site": call.map(|n| n.key.clone()),
                "call": call.map(|n| n.label.clone()),
                "candidate_target": decl.label,
                "candidate_path": decl.path,
                "candidate_set": cand.candidate_set_id,
                "evidence": cand.evidence_class.as_str(),
            }));
        }
    }
    if options.json {
        print_json(
            &serde_json::json!({"command":"graph callees","node":id,"count":rows.len(),"candidate_callees":rows}),
        )?;
    } else {
        println!("candidate callees of `{id}`: {}", rows.len());
        for r in &rows {
            println!(
                "  {} -[call {}]-> {} ({})",
                r["via_call_site"].as_str().unwrap_or("?"),
                r["call"].as_str().unwrap_or("?"),
                r["candidate_target"].as_str().unwrap_or("?"),
                r["evidence"].as_str().unwrap_or("candidate")
            );
        }
    }
    Ok(EXIT_OK)
}

fn command_graph_paths(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let index = graph_index(&options)?;
    let from = graph_arg(&options, 1, "path source")?;
    let to = graph_arg(&options, 2, "path target")?;
    let mut opts = crate::graph::PathOptions::default();
    if let Some(d) = options.depth {
        opts.max_depth = d;
    }
    if let Some(m) = options.max_results {
        opts.max_paths = m;
    }
    // Resolve endpoints by id/key or unique name.
    let resolve = |s: &str| -> Option<String> {
        if let Some(n) = index.node(s) {
            return Some(n.node_id.clone());
        }
        let m = index.find(s, crate::graph::LookupDomain::Name);
        if m.len() == 1 {
            return Some(m[0].node_id.clone());
        }
        None
    };
    let Some(from_id) = resolve(&from) else {
        return Err(format!("cannot uniquely resolve `{from}`"));
    };
    let Some(to_id) = resolve(&to) else {
        return Err(format!("cannot uniquely resolve `{to}`"));
    };
    let paths = index.paths(&from_id, &to_id, &opts);
    if options.json {
        let rows: Vec<_> = paths
            .iter()
            .map(|p| {
                serde_json::json!({
                    "evidence": p.evidence_label(),
                    "candidate_sets": p.candidate_sets,
                    "nodes": p.nodes.iter().map(|n| n.key.clone()).collect::<Vec<_>>(),
                    "edges": p.edges.iter().map(|e| e.kind.clone()).collect::<Vec<_>>(),
                })
            })
            .collect();
        print_json(
            &serde_json::json!({"command":"graph paths","from":from,"to":to,"count":paths.len(),"paths":rows}),
        )?;
    } else {
        println!("paths `{from}` -> `{to}`: {}", paths.len());
        for p in &paths {
            let route: Vec<_> = p.nodes.iter().map(|n| n.label.clone()).collect();
            println!("  [{}] {}", p.evidence_label(), route.join(" -> "));
        }
    }
    Ok(EXIT_OK)
}

// `query` — the deterministic textual query engine (Phase B).
fn command_query(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    // RepositoryView locator mode: --root | --project [--view], or --json stdin.
    // --graph remains the direct prebuilt-index path (existing behavior).
    if options.graph.is_none() {
        return command_query_view(&options);
    }
    let graph_dir = options
        .graph
        .clone()
        .ok_or_else(|| "query requires --graph <graph-dir>".to_string())?;
    let text = options
        .query_text
        .clone()
        .or_else(|| options.positional.first().cloned())
        .ok_or_else(|| "query requires a query string".to_string())?;
    let index = crate::graph::GraphIndex::load(Path::new(&graph_dir)).map_err(|e| e.to_string())?;
    let engine = crate::query::QueryEngine::new(&index);
    let mode = if options.exhaustive {
        crate::query::QueryMode::Exhaustive
    } else {
        crate::query::QueryMode::Ranked
    };
    let intent = match options.domain.as_deref() {
        None => None,
        Some(d) => match crate::query::QueryIntent::parse(d) {
            Some(i) => Some(i),
            None => return Err(format!("unknown query intent `{d}`")),
        },
    };
    let plan = crate::query::QueryPlan::parse(
        &text,
        mode,
        intent,
        options.target.clone(),
        options.to.clone(),
        options.max_results.unwrap_or(50),
        options.tokens,
    );
    let mut plan = plan;
    plan.max_depth = options.depth.unwrap_or(4);
    plan.validate()?; // §19: reject malformed plans before running
                      // Optional System One layer: config absent/disabled => pure deterministic
                      // run. Enabled => bounded query/rerank advice with deterministic fallback.
    let explicit_intent = options.domain.is_some();
    let so_cfg = crate::system_one::config::load(&crate::system_one::config::config_path())
        .ok()
        .and_then(|c| c.system_one);
    let result = match so_cfg {
        Some(c) if c.enabled => {
            let so = crate::system_one::SystemOne::new(Some(c));
            let (r, _prov) = so.run(&engine, &plan, explicit_intent);
            r
        }
        _ => engine.run(&plan),
    };
    if options.explain {
        return print_query_explain(&result, options.json);
    }
    // ONE canonical projection for all transports (§6). `--graph` has no
    // co-located snapshot, so current ranges are omitted (not invented, §13).
    let proj = crate::query::projection::build(&result, None);
    if options.json {
        let mut body = proj.to_json();
        body["command"] = serde_json::json!("query");
        body["mode"] = serde_json::json!(if options.exhaustive {
            "exhaustive"
        } else {
            "ranked"
        });
        print_json(&body)?;
    } else if !options.human {
        // Default output is RDX — the compact agent-facing protocol (Phase C).
        print!("{}", proj.to_rdx());
    } else {
        print!("{}", proj.to_human());
    }
    Ok(EXIT_OK)
}

// ---- RepositoryView query (Part III-XVI) ----

/// Serialize a deterministic QueryResult into the canonical machine `result`
/// object — built from the ONE canonical EvidenceProjection (§6). `index_dir`
/// (the view's `indexes/{key}` dir) enables current-range materialization; the
/// projection is a backward-compatible superset (legacy fields retained, §38).
fn query_result_json(
    outcome: &crate::view::service::ViewQueryOutcome,
    index_dir: Option<&Path>,
) -> serde_json::Value {
    let mut p = crate::query::projection::build(&outcome.result, index_dir);
    attach_memory(&mut p, Some(&outcome.memory));
    if let Some(rc) = &outcome.recipe {
        p.recipe = Some(
            serde_json::json!({"recipe_id":rc.recipe_id,"family":rc.family,"produced":rc.produced.len(),"fell_back":rc.fell_back}),
        );
    }
    // Adaptive context compiler: prune optional evidence to the minimal
    // faithful packet for the query shape (§37). `static` is a no-op.
    if outcome.context_policy == crate::context::ContextPolicy::Adaptive {
        crate::context::compile_into(
            &mut p,
            outcome.shape,
            outcome.memory.mode_parse(),
            &crate::context::ContextBudget::default(),
        );
    }
    p.to_json()
}

/// Attach the memory composition to the projection when the mode requested it
/// and the composition produced something (never an empty/stub block).
fn attach_memory(
    p: &mut crate::query::projection::EvidenceProjection,
    memory: Option<&crate::memory::compose::MemoryComposition>,
) {
    use crate::memory::compose::MemoryMode;
    if let Some(m) = memory {
        if m.mode != MemoryMode::Off.as_str() && !m.degraded && m.matched_investigations > 0 {
            p.memory = serde_json::to_value(m).ok();
        }
    }
}

/// `query` in RepositoryView locator mode (§4-§9):
///   flags:  --root <path> | --project <alias> [--view <name>] + --query <text>
///   machine: --json reads one request object from stdin -> JSON to stdout.
fn command_query_view(options: &Options) -> Result<u8, String> {
    use crate::view as v;
    let state = options.state_dir.as_deref().map(PathBuf::from);
    // Machine stdin mode (§30-§31): --json + a request object on stdin.
    if options.json {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)
            .map_err(|e| format!("read stdin: {e}"))?;
        let req: v::QueryRequest = serde_json::from_str(buf.trim()).map_err(|e| {
            machine_error(
                &v::ViewError::InvalidRequest,
                &format!("malformed request JSON: {e}"),
            );
            format!("INVALID_REQUEST: malformed request JSON: {e}")
        })?;
        // On any failure emit the structured error to stdout + stderr and exit
        // non-zero (§44-§45).
        let locator = match v::service::locator_from_request(&req) {
            Ok(l) => l,
            Err((k, m)) => {
                machine_error(&k, &m);
                return Ok(k.exit_code());
            }
        };
        let text = match req.query.clone() {
            Some(t) => t,
            None => {
                machine_error(&v::ViewError::InvalidRequest, "request missing `query`");
                return Ok(v::ViewError::InvalidRequest.exit_code());
            }
        };
        let intent = req
            .intent
            .as_deref()
            .and_then(crate::query::QueryIntent::parse);
        let params = v::service::ViewQueryParams {
            query_text: &text,
            mode: crate::query::QueryMode::Ranked,
            intent,
            target: req.target.clone(),
            to: req.to.clone(),
            max_results: req.max_results.unwrap_or(50),
            token_budget: None,
            depth: req.depth.unwrap_or(4),
            memory_mode: crate::memory::compose::MemoryMode::parse(
                req.memory_mode.as_deref().unwrap_or("off"),
            ),
            context_policy: crate::context::ContextPolicy::parse(
                req.context_policy.as_deref().unwrap_or("static"),
            ),
            recipes: crate::recipe::RecipePolicy::parse(req.recipes.as_deref().unwrap_or("off")),
            state_override: state.as_deref(),
        };
        let outcome = match v::run_view_query(&locator, &params) {
            Ok(o) => o,
            Err((k, m)) => {
                machine_error(&k, &m);
                return Ok(k.exit_code());
            }
        };
        // machine-readable JSON only on stdout (§31).
        let body = serde_json::json!({
            "schema": v::QUERY_RESPONSE_SCHEMA,
            "repository_view": view_meta_json(&outcome.view),
            "query": text,
            "result": query_result_json(&outcome, Some(&outcome.ensure.index_dir())),
            "index": {"fingerprint":outcome.ensure.fingerprint,
                      "reused":outcome.ensure.index_reused,
                      "analyses_reused":outcome.ensure.analyses_reused,
                      "analyses_parsed":outcome.ensure.analyses_parsed},
            "ensure_ms":outcome.ensure.ensure_ms,
        });
        print_json(&body)?;
        return Ok(EXIT_OK);
    }
    // Flag mode (§34): locator + query text -> RDX (default) or human.
    let locator = v::locator_from_flags(
        options.repo_root.as_deref(),
        options.project.as_deref(),
        options.view.as_deref(),
    )
    .map_err(|(k, m)| machine_err_string(&k, &m))?;
    let text = options
        .query_text
        .clone()
        .or_else(|| options.positional.first().cloned())
        .ok_or_else(|| "query requires a query string (--query <text>)".to_string())?;
    // Service routing (§20): --service forces it; --direct forces offline;
    // default = use the running service when one is live, else direct.
    let use_service = if options.direct {
        false
    } else if options.service {
        true
    } else {
        service_running(&state)
    };
    if use_service {
        return query_via_service(&state, &locator, &text, options);
    }
    let intent = match options.domain.as_deref() {
        None => None,
        Some(d) => match crate::query::QueryIntent::parse(d) {
            Some(i) => Some(i),
            None => return Err(format!("unknown query intent `{d}`")),
        },
    };
    let mode = if options.exhaustive {
        crate::query::QueryMode::Exhaustive
    } else {
        crate::query::QueryMode::Ranked
    };
    let params = v::service::ViewQueryParams {
        query_text: &text,
        mode,
        intent,
        target: options.target.clone(),
        to: options.to.clone(),
        max_results: options.max_results.unwrap_or(50),
        token_budget: options.tokens,
        depth: options.depth.unwrap_or(4),
        memory_mode: crate::memory::compose::MemoryMode::parse(
            options.memory.as_deref().unwrap_or("off"),
        ),
        context_policy: crate::context::ContextPolicy::parse(
            options.context.as_deref().unwrap_or("static"),
        ),
        recipes: crate::recipe::RecipePolicy::parse(options.recipes.as_deref().unwrap_or("off")),
        state_override: state.as_deref(),
    };
    let outcome =
        v::run_view_query(&locator, &params).map_err(|(k, m)| machine_err_string(&k, &m))?;
    let mut proj =
        crate::query::projection::build(&outcome.result, Some(&outcome.ensure.index_dir()));
    attach_memory(&mut proj, Some(&outcome.memory));
    if outcome.context_policy == crate::context::ContextPolicy::Adaptive {
        crate::context::compile_into(
            &mut proj,
            outcome.shape,
            outcome.memory.mode_parse(),
            &crate::context::ContextBudget::default(),
        );
    }
    if options.human {
        println!("view: {}", outcome.view.canonical_root.display());
        println!("fingerprint: {}", outcome.ensure.fingerprint);
        print!("{}", proj.to_human());
    } else {
        // Default agent-facing output: RDX2 (faithful: rank/path/range/via, §39).
        print!("{}", proj.to_rdx());
    }
    Ok(EXIT_OK)
}

/// Is a RepoDex service live for this state dir? (§20 auto-routing)
fn service_running(state_override: &Option<PathBuf>) -> bool {
    let state = state_override
        .clone()
        .unwrap_or_else(crate::view::registry::state_dir);
    crate::service::runtime::live(&state).is_some()
}

/// Route a flag-mode query through the running service's /v1/query (§20).
fn query_via_service(
    state_override: &Option<PathBuf>,
    locator: &crate::view::ViewLocator,
    text: &str,
    options: &Options,
) -> Result<u8, String> {
    use crate::service::{http, runtime, token};
    let state = state_override
        .clone()
        .unwrap_or_else(crate::view::registry::state_dir);
    let desc = runtime::live(&state).ok_or_else(|| {
        "no running RepoDex service (--service requires `repodex start`)".to_string()
    })?;
    let token = token::load_or_generate(&state)?;
    // Build the request matching the request.v1 locator semantics.
    let req = match locator {
        crate::view::ViewLocator::Root(p) => serde_json::json!({
            "schema": crate::view::QUERY_REQUEST_SCHEMA,
            "root": p, "query": text,
            "max_results": options.max_results,
        }),
        crate::view::ViewLocator::Project { project, view } => serde_json::json!({
            "schema": crate::view::QUERY_REQUEST_SCHEMA,
            "project": project, "view": view, "query": text,
            "max_results": options.max_results,
        }),
    };
    let (st, body) = http::post_json(
        &desc.host,
        desc.port,
        "/v1/query",
        Some(&token),
        &req,
        30000,
    )
    .map_err(|e| format!("service query failed: {e}"))?;
    if st != 200 {
        let code = body["error"]["code"].as_str().unwrap_or("QUERY_FAILED");
        let msg = body["error"]["message"].as_str().unwrap_or("query failed");
        return Err(format!("{code}: {msg}"));
    }
    // Render the service's canonical projection faithfully (§36): the service
    // already returned the full EvidenceProjection JSON, so rank/path/locator/
    // evidence/via/candidate_set/ranges are all preserved — no lossy re-render.
    if options.json {
        print_json(&body)?;
    } else {
        print!("{}", render_projection_text(&body["result"], options.human));
    }
    Ok(EXIT_OK)
}

/// Render a service-returned EvidenceProjection JSON to RDX-ish compact or human
/// text without dropping Agent-relevant fields (§36). Mirrors `to_rdx`/`to_human`.
fn render_projection_text(r: &serde_json::Value, human: bool) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let seeds = r["seeds"].as_array().cloned().unwrap_or_default();
    let related = r["related"].as_array().cloned().unwrap_or_default();
    if human {
        let _ = writeln!(out, "query: {}", r["query"].as_str().unwrap_or(""));
        let _ = writeln!(
            out,
            "seeds: {} of {} matching ({})",
            r["emitted_seed_count"]
                .as_u64()
                .unwrap_or(seeds.len() as u64),
            r["eligible_seed_count"].as_u64().unwrap_or(0),
            if r["seed_selection_complete"].as_bool().unwrap_or(true) {
                "all matching seeds emitted"
            } else {
                "truncated"
            }
        );
        for s in &seeds {
            let _ = writeln!(
                out,
                "  #{:<3} {:<12} {:<30} {}",
                s["rank"].as_u64().unwrap_or(0),
                s["kind"].as_str().unwrap_or(""),
                s["label"].as_str().unwrap_or(""),
                s["path"].as_str().unwrap_or("")
            );
        }
        for rel in &related {
            let _ = writeln!(
                out,
                "  {} -[{}:{}]-> {}",
                rel["from_label"].as_str().unwrap_or(""),
                rel["kind"].as_str().unwrap_or(""),
                rel["evidence"].as_str().unwrap_or(""),
                rel["to_label"].as_str().unwrap_or("")
            );
        }
        return out;
    }
    // Compact faithful: rank + kind + path + locator + ranges per seed.
    let _ = writeln!(out, "#RDX1 v2 (service)");
    for s in &seeds {
        let mut line = format!(
            "S{} {} {} {}",
            s["rank"].as_u64().unwrap_or(0),
            s["kind"].as_str().unwrap_or(""),
            s["path"].as_str().unwrap_or(""),
            s["label"].as_str().unwrap_or("")
        );
        if let Some(dr) = s["declaration_range"].as_object() {
            let _ = write!(
                line,
                " decl={}:{}-{}",
                dr["start_line"], dr["byte_start"], dr["byte_end"]
            );
        }
        out.push_str(&line);
        out.push('\n');
    }
    for rel in &related {
        let mut line = format!(
            "R {} {} -> {} {}",
            rel["kind"].as_str().unwrap_or(""),
            rel["from_label"].as_str().unwrap_or(""),
            rel["to_label"].as_str().unwrap_or(""),
            rel["evidence"].as_str().unwrap_or("")
        );
        if let Some(cs) = rel["candidate_set"].as_str() {
            let _ = write!(line, " cs={cs}");
        }
        out.push_str(&line);
        out.push('\n');
    }
    if let Some(c) = r["connector"].as_object() {
        if c["found"].as_bool() == Some(false) {
            out.push_str("P none bounded_no_route depth<=3\n");
        } else if let Some(routes) = c["routes"].as_array() {
            for (i, rt) in routes.iter().enumerate() {
                let mut p = format!("P {}", i + 1);
                if let Some(steps) = rt["steps"].as_array() {
                    for st in steps {
                        let _ = write!(
                            p,
                            " | {} {} {}",
                            st["from_label"].as_str().unwrap_or(""),
                            st["relation"].as_str().unwrap_or(""),
                            st["to_label"].as_str().unwrap_or("")
                        );
                    }
                }
                out.push_str(&p);
                out.push('\n');
            }
        }
    }
    out
}

/// RepositoryView metadata for the machine response (§33).
fn view_meta_json(view: &crate::view::RepositoryView) -> serde_json::Value {
    serde_json::json!({
        "repository_id":view.repository_id,"view_id":view.view_id,
        "root":view.canonical_root,"head":view.head,"branch":view.branch,
        "detached":view.detached,"dirty":view.dirty,
        "git_common_dir":view.git_common_dir,
        "view_fingerprint":view.view_fingerprint,"locator":view.locator,
    })
}

/// Print a machine error object to stdout + the message to stderr (§44-§45).
fn machine_error(kind: &crate::view::ViewError, msg: &str) {
    let _ = print_json(&serde_json::json!({
        "schema": crate::view::QUERY_RESPONSE_SCHEMA,
        "error": {"code": kind.as_str(), "message": msg},
    }));
    eprintln!("{}: {msg}", kind.as_str());
}
fn machine_err_string(kind: &crate::view::ViewError, msg: &str) -> String {
    eprintln!("{}: {msg}", kind.as_str());
    format!("{}: {msg}", kind.as_str())
}

// ---- `project` registry commands (Part V, XVII) ----
fn command_project(args: &[String]) -> Result<u8, String> {
    use crate::view as v;
    let sub = args
        .first()
        .ok_or_else(|| {
            "project requires a subcommand (register|show|list|remove|resolve)".to_string()
        })?
        .as_str();
    let options = parse_options(&args[1..])?;
    let mut reg = v::ProjectRegistry::load();
    match sub {
        "register" | "add" => {
            let alias = options
                .positional
                .first()
                .cloned()
                .ok_or_else(|| "project register requires an alias".to_string())?;
            let p = reg
                .register(&alias, options.repo_root.as_deref(), &options.views_roots)
                .map_err(|(k, m)| format!("{}: {m}", k.as_str()))?;
            println!(
                "registered {} (root={:?}, view_roots={})",
                p.alias,
                p.root,
                p.view_roots.len()
            );
        }
        "show" => {
            let alias = options
                .positional
                .first()
                .cloned()
                .ok_or_else(|| "project show requires an alias".to_string())?;
            let p = reg
                .get(&alias)
                .ok_or_else(|| format!("project `{alias}` not found"))?;
            if options.json {
                print_json(&serde_json::json!(p))?;
            } else {
                println!("alias: {}", p.alias);
                println!(
                    "root:  {}",
                    p.root
                        .as_ref()
                        .map(|x| x.display().to_string())
                        .unwrap_or_else(|| "(none)".into())
                );
                for vr in &p.view_roots {
                    println!("view_root: {}", vr.display());
                }
            }
        }
        "list" => {
            if options.json {
                print_json(&serde_json::json!(reg.list()))?;
            } else {
                for p in reg.list() {
                    println!(
                        "{:<12} root={} view_roots={}",
                        p.alias,
                        p.root
                            .as_ref()
                            .map(|x| x.display().to_string())
                            .unwrap_or_else(|| "-".into()),
                        p.view_roots.len()
                    );
                }
            }
        }
        "remove" | "rm" | "unregister" => {
            let alias = options
                .positional
                .first()
                .cloned()
                .ok_or_else(|| "project remove requires an alias".to_string())?;
            reg.remove(&alias)
                .map_err(|(k, m)| format!("{}: {m}", k.as_str()))?;
            println!("removed {alias}");
        }
        "resolve" => {
            // Show canonical RepositoryView metadata for project [--view] or --root.
            let alias = options
                .positional
                .first()
                .cloned()
                .or_else(|| options.project.clone())
                .ok_or_else(|| "project resolve requires an alias or --project".to_string())?;
            let locator = v::ViewLocator::Project {
                project: alias,
                view: options.view.clone(),
            };
            let view = v::resolve(&locator).map_err(|(k, m)| format!("{}: {m}", k.as_str()))?;
            print_json(&view_meta_json(&view))?;
        }
        other => return Err(format!("unknown project subcommand `{other}`")),
    }
    Ok(EXIT_OK)
}

// ---- service lifecycle (Part I, VI) ----
fn command_service(cmd: &str, args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let state = options
        .state_dir
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(crate::view::registry::state_dir);
    use crate::service::lifecycle as lc;
    match cmd {
        "serve" => lc::serve(&state),
        "start" => lc::start(&state),
        "stop" => lc::stop(&state),
        "restart" => lc::restart(&state),
        "status" => lc::status(&state, options.json),
        _ => Err(format!("unknown service command `{cmd}`")),
    }
}

fn print_query_explain(result: &crate::query::QueryResult, json: bool) -> Result<u8, String> {
    let p = &result.plan;
    if json {
        print_json(&serde_json::json!({
            "command":"query --explain","raw":p.raw,"terms":p.terms,"mode":format!("{:?}",p.mode).to_lowercase(),
            "intent":p.intent.as_str(),"max_results":p.max_results,"token_budget":p.token_budget,
            "total":result.total,"shown":result.shown,"complete":result.complete,
            "seed_factors":result.seeds.iter().map(|s|serde_json::json!({"key":s.node.key,"score":s.score,"factors":s.factors})).collect::<Vec<_>>(),
        }))?;
    } else {
        println!("query explain");
        println!("  raw:           {}", p.raw);
        println!("  terms:         {}", p.terms.join(" "));
        println!(
            "  mode:          {}",
            if p.mode == crate::query::QueryMode::Exhaustive {
                "exhaustive"
            } else {
                "ranked"
            }
        );
        println!("  intent:        {}", p.intent.as_str());
        println!("  max_results:   {}", p.max_results);
        println!(
            "  token_budget:  {}",
            p.token_budget.map(|t| t.to_string()).unwrap_or("-".into())
        );
        println!("  total seeds:   {}", result.total);
        println!("  shown:         {}", result.shown);
        println!("  complete:      {}", result.complete);
        println!("  seed ranking:");
        for s in &result.seeds {
            println!(
                "    {:<10} score={} factors={}",
                s.node.label,
                s.score,
                s.factors.join(",")
            );
        }
    }
    Ok(EXIT_OK)
}

// ---------------------------------------------------------------------------
// config — key-oriented get/set on ~/reposuite/repodex/config.toml
// ---------------------------------------------------------------------------

fn command_config(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let key = options.positional.first().cloned().ok_or_else(|| {
        "config requires a key, e.g. `config system-one.enabled true`".to_string()
    })?;
    let value = options.positional.get(1).cloned();
    let path = crate::system_one::config::config_path();
    let mut doc = crate::system_one::config::load_value(&path)?;
    if options.unset {
        crate::system_one::config::remove_key(&mut doc, &key)?;
        crate::system_one::config::save_value(&path, &doc)?;
        println!("{key} <removed>");
        return Ok(EXIT_OK);
    }
    match value {
        Some(v) => {
            crate::system_one::config::set_key(&mut doc, &key, &v)?;
            crate::system_one::config::save_value(&path, &doc)?;
            println!("{key} = {v}");
        }
        None => match crate::system_one::config::get_key(&doc, &key) {
            Some(v) => println!("{key} = {v}"),
            None => println!("{key} = <unset>"),
        },
    }
    Ok(EXIT_OK)
}

// ---------------------------------------------------------------------------
// system-one — status (no network) + probe (one request)
// ---------------------------------------------------------------------------

fn command_system_one(args: &[String]) -> Result<u8, String> {
    let sub = args
        .first()
        .map(|s| s.as_str())
        .ok_or_else(|| "system-one requires a subcommand (status|probe)".to_string())?;
    match sub {
        "status" => command_system_one_status(),
        "probe" => command_system_one_probe(&args[1..]),
        _ => Err(format!("unknown system-one subcommand `{sub}`")),
    }
}

fn command_system_one_status() -> Result<u8, String> {
    let path = crate::system_one::config::config_path();
    let cfg = crate::system_one::config::load(&path)?;
    let Some(so) = cfg.system_one else {
        println!("system-one: disabled (no [system_one] in {path:?})");
        return Ok(EXIT_OK);
    };
    println!(
        "system-one: {}",
        if so.enabled { "enabled" } else { "disabled" }
    );
    println!(
        "  roles: query={} rerank={}",
        so.roles.query.as_deref().unwrap_or("-"),
        so.roles.rerank.as_deref().unwrap_or("-")
    );
    for (name, m) in &so.models {
        println!(
            "  model {name}: {} {} timeout={}ms auth={}",
            m.protocol,
            m.url,
            m.timeout_ms,
            match &m.auth {
                crate::system_one::config::Auth::None => "none",
                crate::system_one::config::Auth::Bearer { .. } => "bearer",
                crate::system_one::config::Auth::Header { header, .. } => header.as_str(),
            }
        );
    }
    match so.validate() {
        Ok(()) => println!("  config: valid"),
        Err(e) => println!("  config: INVALID — {e}"),
    }
    Ok(EXIT_OK)
}

fn command_system_one_probe(args: &[String]) -> Result<u8, String> {
    let name = args
        .first()
        .cloned()
        .ok_or_else(|| "system-one probe requires a model name".to_string())?;
    let path = crate::system_one::config::config_path();
    let cfg = crate::system_one::config::load(&path)?;
    let so = cfg
        .system_one
        .ok_or_else(|| "no [system_one] configured".to_string())?;
    let mc = so
        .models
        .get(&name)
        .ok_or_else(|| format!("unknown model `{name}`"))?;
    let model =
        crate::system_one::HttpSystemOneModel::new(name.clone(), mc).map_err(|e| e.to_string())?;
    // Minimal deterministic protocol question — validates transport+schema,
    // not answer quality.
    let mut req = crate::system_one::SystemOneRequest::new(
        mc.model.clone(),
        serde_json::json!({"probe": true}),
    );
    req.questions.insert(
        "probe".into(),
        crate::system_one::Question::noul("Is the service reachable?"),
    );
    match crate::system_one::SystemOneModel::decide(&model, &req) {
        Ok(resp) => {
            println!("probe {name}: ok ({} answer(s))", resp.answers.len());
            println!("note: protocol probe success does not prove query quality");
            Ok(EXIT_OK)
        }
        Err(e) => Err(format!("probe {name}: {e}")),
    }
}

// ---------------------------------------------------------------------------
// temporal — Git-derived temporal/activity index (TEMPORAL evidence family).
// Query/inspection reads only the persisted artifact; no Git runs at lookup.
// ---------------------------------------------------------------------------

fn command_temporal(args: &[String]) -> Result<u8, String> {
    let sub = args.first().map(|s| s.as_str()).ok_or_else(|| {
        "temporal requires a subcommand (build|update|verify|stats|file)".to_string()
    })?;
    match sub {
        "build" => command_temporal_build(&args[1..], false),
        "update" => command_temporal_build(&args[1..], true),
        "verify" => command_temporal_verify(&args[1..]),
        "stats" => command_temporal_stats(&args[1..]),
        "file" => command_temporal_file(&args[1..]),
        _ => Err(format!("unknown temporal subcommand `{sub}`")),
    }
}

fn command_temporal_build(args: &[String], incremental: bool) -> Result<u8, String> {
    let options = parse_options(args)?;
    let repo = require_positional(
        &options,
        "temporal build/update requires a repository directory",
    )?;
    let output = options
        .output
        .clone()
        .ok_or_else(|| "temporal build/update requires --output <dir>".to_string())?;
    let outcome = if incremental {
        let prev = options
            .previous
            .clone()
            .ok_or_else(|| "temporal update requires --previous <dir>".to_string())?;
        crate::temporal::update_temporal(Path::new(repo), Path::new(&prev), Path::new(&output))
    } else {
        crate::temporal::build_temporal(Path::new(repo), Path::new(&output))
    }
    .map_err(|e| e.to_string())?;
    let m = &outcome.manifest;
    if options.json {
        print_json(&serde_json::json!({
            "command": if incremental {"temporal update"} else {"temporal build"},
            "mode": outcome.stats.mode,
            "output": output,
            "indexed_head": m.indexed_head,
            "files_tracked": m.files_tracked,
            "commits_processed": m.commits_processed,
            "change_events": m.change_events,
            "artifact_bytes": m.artifact_bytes,
            "wall_ms": outcome.stats.wall_ms,
            "peak_rss_kb": outcome.stats.peak_rss_kb,
            "content_digest": m.content_digest,
        }))?;
    } else {
        println!("mode:            {}", outcome.stats.mode);
        println!("head:            {}", m.indexed_head);
        println!("files:           {}", m.files_tracked);
        println!("commits:         {}", m.commits_processed);
        println!("change events:   {}", m.change_events);
        println!("artifact bytes:  {}", m.artifact_bytes);
        println!("elapsed:         {:.1} ms", outcome.stats.wall_ms);
        println!("peak RSS:        {} kB", outcome.stats.peak_rss_kb);
    }
    Ok(EXIT_OK)
}

fn command_temporal_verify(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(&options, "temporal verify requires an artifact directory")?;
    let m = crate::temporal::verify(Path::new(dir)).map_err(|e| e.to_string())?;
    // Optional: confirm the artifact still binds to this repository's HEAD.
    let mut head_note = String::new();
    if let Some(repo) = options.repository.clone() {
        let head =
            crate::temporal::collect::head_commit(Path::new(&repo)).map_err(|e| e.to_string())?;
        if head == m.indexed_head {
            head_note = "head matches".to_string();
        } else if crate::temporal::collect::is_ancestor(Path::new(&repo), &m.indexed_head, &head)
            .map_err(|e| e.to_string())?
        {
            head_note = "index is behind HEAD (incremental update available)".to_string();
        } else {
            return Err("temporal index HEAD diverged from repository".to_string());
        }
    }
    if options.json {
        print_json(&serde_json::json!({
            "command":"temporal verify","ok":true,"dir":dir,
            "schema_version":m.schema_version,"policy_version":m.policy_version,
            "indexed_head":m.indexed_head,"files_tracked":m.files_tracked,
            "content_digest":m.content_digest,"head":head_note,
        }))?;
    } else {
        println!("temporal index: valid");
        println!("  schema/policy: {}/{}", m.schema_version, m.policy_version);
        println!("  head:          {}", m.indexed_head);
        println!("  files:         {}", m.files_tracked);
        if !head_note.is_empty() {
            println!("  repository:    {head_note}");
        }
    }
    Ok(EXIT_OK)
}

fn command_temporal_stats(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(&options, "temporal stats requires an artifact directory")?;
    let idx = crate::temporal::TemporalIndex::load(Path::new(dir)).map_err(|e| e.to_string())?;
    let m = &idx.manifest;
    let mut total_changes = 0u64;
    let mut max_count = 0u32;
    let mut hot = String::new();
    for f in idx.files() {
        total_changes += u64::from(f.change_commit_count);
        if f.change_commit_count > max_count {
            max_count = f.change_commit_count;
            hot = f.path.clone();
        }
    }
    if options.json {
        print_json(&serde_json::json!({
            "command":"temporal stats","indexed_head":m.indexed_head,
            "files_tracked":m.files_tracked,"total_change_events":total_changes,
            "mean_changes_per_file": if !idx.is_empty() {total_changes as f64/idx.len() as f64} else {0.0},
            "max_file_changes":max_count,"hottest_file":hot,
            "commits_processed":m.commits_processed,"artifact_bytes":m.artifact_bytes,
        }))?;
    } else {
        println!("head:            {}", m.indexed_head);
        println!("files:           {}", m.files_tracked);
        println!("change events:   {total_changes}");
        println!(
            "mean/file:       {:.2}",
            if !idx.is_empty() {
                total_changes as f64 / idx.len() as f64
            } else {
                0.0
            }
        );
        println!("hottest:         {hot} ({max_count} changes)");
        println!("commits:         {}", m.commits_processed);
        println!("artifact bytes:  {}", m.artifact_bytes);
    }
    Ok(EXIT_OK)
}

fn command_temporal_file(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = require_positional(&options, "temporal file requires an artifact directory")?;
    let path = options
        .positional
        .get(1)
        .cloned()
        .ok_or_else(|| "temporal file requires a repository-relative path".to_string())?;
    let idx = crate::temporal::TemporalIndex::load(Path::new(dir)).map_err(|e| e.to_string())?;
    let Some(d) = idx.describe(&path) else {
        return Err(format!("no temporal record for `{path}`"));
    };
    if options.json {
        print_json(&d)?;
    } else {
        println!("{d}");
    }
    Ok(EXIT_OK)
}

// ---------------------------------------------------------------------------
// agent-events — canonical AgentEvent v1 telemetry ingest + store.
// Harness-neutral contract. RepoDex ingests ONLY canonical
// `reposuite.agent-event.v1` JSONL; native harness formats are converted by an
// external adapter (`scripts/adapters/` or future RepoSuite Relay), never here.
// ---------------------------------------------------------------------------

fn command_agent_events(args: &[String]) -> Result<u8, String> {
    let sub = args.first().map(|s| s.as_str()).ok_or_else(|| {
        "agent-events requires a subcommand (ingest|verify|stats|session|export)".to_string()
    })?;
    match sub {
        "ingest" => command_agent_events_ingest(&args[1..]),
        "verify" => command_agent_events_verify(&args[1..]),
        "stats" => command_agent_events_stats(&args[1..]),
        "session" => command_agent_events_session(&args[1..]),
        "export" => command_agent_events_export(&args[1..]),
        "derive" => command_agent_events_derive(&args[1..]),
        "derive-verify" | "derive_verify" => command_agent_events_derive_verify(&args[1..]),
        _ => Err(format!("unknown agent-events subcommand `{sub}`")),
    }
}

fn agent_events_store_dir(options: &Options) -> Result<String, String> {
    options
        .output
        .clone()
        .or(options.snapshot.clone())
        .ok_or_else(|| "requires --output <store-dir>".to_string())
}

fn command_agent_events_ingest(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let format = options
        .format
        .clone()
        .unwrap_or_else(|| "reposuite-v1".to_string());
    let file: Option<String> = options.positional.first().cloned();
    let dir = agent_events_store_dir(&options)?;
    let mut store =
        crate::agent_event::AgentEventStore::open(Path::new(&dir)).map_err(|e| e.to_string())?;
    // read source text (stdin `-` or file)
    let text = match file.as_deref() {
        Some("-") | None => {
            use std::io::Read;
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .map_err(|e| e.to_string())?;
            s
        }
        Some(f) => std::fs::read_to_string(f).map_err(|e| format!("read {f}: {e}"))?,
    };
    // RepoDex ingests canonical `reposuite.agent-event.v1` JSONL only. Native
    // harness formats are converted externally by
    // scripts/adapters/* or future RepoSuite Relay — never parsed here (§3).
    let events: Vec<crate::agent_event::AgentEvent> = match format.as_str() {
        "reposuite-v1" | "v1" => {
            let mut evs = Vec::new();
            for (n, line) in text.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                evs.push(
                    serde_json::from_str::<crate::agent_event::AgentEvent>(line)
                        .map_err(|e| format!("event line {}: {e}", n + 1))?,
                );
            }
            evs
        }
        other => {
            return Err(format!(
                "unknown ingest format `{other}`; RepoDex accepts canonical \
                 reposuite.agent-event.v1 JSONL only — convert native harness \
                 formats with scripts/adapters/ first"
            ))
        }
    };
    let mut sink = crate::agent_event::StoreSink { store: &mut store };
    let report = crate::agent_event::AgentEventSink::ingest_batch(&mut sink, &events)
        .map_err(|e| e.to_string())?;
    store.update_manifest().map_err(|e| e.to_string())?;
    if options.json {
        print_json(&serde_json::json!({
            "command":"agent-events ingest","format":format,
            "accepted":report.accepted,"duplicates":report.duplicates,
            "rejected":report.rejected,"errors":report.errors,
        }))?;
    } else {
        println!(
            "ingest: accepted={} duplicates={} rejected={}",
            report.accepted, report.duplicates, report.rejected
        );
        for e in &report.errors {
            println!("  error: {e}");
        }
    }
    Ok(EXIT_OK)
}

fn command_agent_events_verify(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = agent_events_store_dir(&options)?;
    let store =
        crate::agent_event::AgentEventStore::open(Path::new(&dir)).map_err(|e| e.to_string())?;
    let sids = store.sessions().map_err(|e| e.to_string())?;
    let mut events = 0u64;
    let mut seq_anom = 0u64;
    let mut dup_ids = 0u64;
    for sid in &sids {
        let evs = store.session_events(sid).map_err(|e| e.to_string())?;
        let mut last: i64 = -1;
        let mut seen = std::collections::HashSet::new();
        for e in &evs {
            crate::agent_event::validate_event(e).map_err(|e| e.to_string())?;
            if !seen.insert(e.event_id.clone()) {
                dup_ids += 1;
            }
            if e.sequence as i64 <= last {
                seq_anom += 1;
            }
            last = e.sequence as i64;
            events += 1;
        }
    }
    if options.json {
        print_json(&serde_json::json!({
            "command":"agent-events verify","ok":seq_anom==0&&dup_ids==0,
            "sessions":sids.len(),"events":events,
            "sequence_anomalies":seq_anom,"duplicate_ids":dup_ids,
        }))?;
    } else {
        println!(
            "sessions={} events={} seq_anomalies={} dup_ids={}",
            sids.len(),
            events,
            seq_anom,
            dup_ids
        );
    }
    Ok(EXIT_OK)
}

fn command_agent_events_stats(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = agent_events_store_dir(&options)?;
    let store =
        crate::agent_event::AgentEventStore::open(Path::new(&dir)).map_err(|e| e.to_string())?;
    let sids = store.sessions().map_err(|e| e.to_string())?;
    let mut by_type: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    let mut events = 0u64;
    let mut in_tok = 0u64;
    let mut out_tok = 0u64;
    let mut src_files = std::collections::HashSet::new();
    for sid in &sids {
        for e in store.session_events(sid).map_err(|e| e.to_string())? {
            *by_type.entry(e.event_type.clone()).or_insert(0) += 1;
            events += 1;
            if e.event_type == "model_call_completed" {
                if let Some(v) = e.data.get("input_tokens").and_then(|v| v.as_u64()) {
                    in_tok += v;
                }
                if let Some(v) = e.data.get("output_tokens").and_then(|v| v.as_u64()) {
                    out_tok += v;
                }
            }
            if e.event_type == "source_observed" {
                if let Some(p) = e.data.get("path").and_then(|v| v.as_str()) {
                    src_files.insert(p.to_string());
                }
            }
        }
    }
    if options.json {
        print_json(&serde_json::json!({
            "command":"agent-events stats","sessions":sids.len(),"events":events,
            "by_type":by_type,"input_tokens":in_tok,"output_tokens":out_tok,
            "source_files_observed":src_files.len(),
        }))?;
    } else {
        println!(
            "sessions={} events={} input_tokens={} output_tokens={} src_files={}",
            sids.len(),
            events,
            in_tok,
            out_tok,
            src_files.len()
        );
    }
    Ok(EXIT_OK)
}

fn command_agent_events_session(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = agent_events_store_dir(&options)?;
    let sid = options
        .positional
        .first()
        .cloned()
        .ok_or_else(|| "session <id> requires a session id".to_string())?;
    let store =
        crate::agent_event::AgentEventStore::open(Path::new(&dir)).map_err(|e| e.to_string())?;
    let evs = store.session_events(&sid).map_err(|e| e.to_string())?;
    if evs.is_empty() {
        return Err(format!("no session `{sid}`"));
    }
    if options.json {
        print_json(&serde_json::json!({"session_id":sid,"events":evs}))?;
    } else {
        for e in &evs {
            println!("#{} {} {}", e.sequence, e.event_type, e.timestamp);
        }
    }
    Ok(EXIT_OK)
}

fn command_agent_events_export(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let dir = agent_events_store_dir(&options)?;
    let store =
        crate::agent_event::AgentEventStore::open(Path::new(&dir)).map_err(|e| e.to_string())?;
    let sids = store.sessions().map_err(|e| e.to_string())?;
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut w = stdout.lock();
    for sid in &sids {
        for e in store.session_events(sid).map_err(|e| e.to_string())? {
            writeln!(
                w,
                "{}",
                serde_json::to_string(&e).map_err(|e| e.to_string())?
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(EXIT_OK)
}

// ---------------------------------------------------------------------------
// Derived layer — InvestigationEpisode + SourceExposure + AgentActivity.
// Rebuildable from the raw Agent Event Store; observational only (no ranking).
// ---------------------------------------------------------------------------

/// Canonical repo path set for final-answer mention extraction (§18) via
/// `git ls-files` at build time (not a lookup-time dependency).
fn known_repo_paths(repo_root: Option<&str>) -> std::collections::BTreeSet<String> {
    let mut set = std::collections::BTreeSet::new();
    if let Some(root) = repo_root {
        if let Ok(out) = std::process::Command::new("git")
            .args(["-C", root, "ls-files"])
            .output()
        {
            for l in String::from_utf8_lossy(&out.stdout).lines() {
                if !l.trim().is_empty() {
                    set.insert(l.trim().to_string());
                }
            }
        }
    }
    set
}

fn derived_dir(options: &Options) -> Result<String, String> {
    options
        .output
        .clone()
        .or(options.snapshot.clone())
        .ok_or_else(|| "requires --output <derived-dir>".to_string())
}

fn command_agent_events_derive(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let store_dir = options
        .store
        .clone()
        .ok_or_else(|| "derive requires --store <event-store-dir>".to_string())?;
    let out = derived_dir(&options)?;
    let store = crate::agent_event::AgentEventStore::open(Path::new(&store_dir))
        .map_err(|e| e.to_string())?;
    let known = known_repo_paths(options.repo_root.as_deref());
    let (ds, rep) = crate::derived::derive(
        &store,
        Path::new(&out),
        options.full,
        &known,
        options.repo_root.as_deref(),
    )
    .map_err(|e| e.to_string())?;
    ds.save(Path::new(&out)).map_err(|e| e.to_string())?;
    let m = ds
        .write_manifest(Path::new(&out))
        .map_err(|e| e.to_string())?;
    if options.json {
        print_json(&serde_json::json!({
            "command":"agent-events derive","full":options.full,
            "sessions_processed":rep.sessions_processed,"sessions_changed":rep.sessions_changed,
            "new_events_folded":rep.new_events_folded,
            "investigations_rebuilt":rep.investigations_rebuilt,
            "episodes":rep.episodes,"activity_paths":rep.activity_paths,
            "artifact_bytes":m.artifact_bytes,
        }))?;
    } else {
        println!(
            "derive: sessions={} changed={} new_events={} investigations_rebuilt={} episodes={} paths={}",
            rep.sessions_processed, rep.sessions_changed, rep.new_events_folded,
            rep.investigations_rebuilt, rep.episodes, rep.activity_paths
        );
    }
    Ok(EXIT_OK)
}

fn command_agent_events_derive_verify(args: &[String]) -> Result<u8, String> {
    let options = parse_options(args)?;
    let out = derived_dir(&options)?;
    let ds = crate::derived::DerivedStore::load(Path::new(&out)).map_err(|e| e.to_string())?;
    let bad = crate::derived::verify(&ds);
    if options.json {
        print_json(&serde_json::json!({
            "command":"derive-verify","ok":bad.is_empty(),"anomalies":bad,
            "episodes":ds.episodes.len(),"sessions":ds.parts.len(),
            "activity_paths":ds.activity.paths.len(),
        }))?;
    } else {
        println!("verify: ok={} anomalies={}", bad.is_empty(), bad.len());
        for b in &bad {
            println!("  {b}");
        }
    }
    Ok(if bad.is_empty() {
        EXIT_OK
    } else {
        EXIT_ANALYSIS_FAILURES
    })
}

fn command_investigations(args: &[String]) -> Result<u8, String> {
    let sub = args
        .first()
        .map(|s| s.as_str())
        .ok_or_else(|| "investigations requires a subcommand (show|list|stats)".to_string())?;
    let options = parse_options(&args[1..])?;
    let out = derived_dir(&options)?;
    let ds = crate::derived::DerivedStore::load(Path::new(&out)).map_err(|e| e.to_string())?;
    match sub {
        "show" => {
            let id = options
                .positional
                .first()
                .cloned()
                .ok_or_else(|| "investigations show <id>".to_string())?;
            let ep = ds
                .episodes
                .get(&id)
                .ok_or_else(|| format!("no investigation `{id}`"))?;
            print_json(&serde_json::to_value(ep).map_err(|e| e.to_string())?)?;
        }
        "list" => {
            let ids: Vec<&String> = ds.episodes.keys().collect();
            if options.json {
                print_json(&serde_json::json!({"investigations":ids}))?;
            } else {
                for i in ids {
                    println!("{i}");
                }
            }
        }
        "stats" => {
            let mut in_tok = 0u64;
            let mut out_tok = 0u64;
            let mut tools = 0u64;
            let mut src_events = 0u64;
            for ep in ds.episodes.values() {
                in_tok += ep.input_tokens.unwrap_or(0);
                out_tok += ep.output_tokens.unwrap_or(0);
                tools += ep.tools.tool_calls_total;
                src_events += ep.source_observation_events;
            }
            if options.json {
                print_json(&serde_json::json!({
                    "investigations":ds.episodes.len(),"sessions":ds.parts.len(),
                    "activity_paths":ds.activity.paths.len(),
                    "input_tokens":in_tok,"output_tokens":out_tok,
                    "tool_calls":tools,"source_observation_events":src_events,
                }))?;
            } else {
                println!(
                    "investigations={} sessions={} paths={} in_tok={} out_tok={} tools={}",
                    ds.episodes.len(),
                    ds.parts.len(),
                    ds.activity.paths.len(),
                    in_tok,
                    out_tok,
                    tools
                );
            }
        }
        _ => return Err(format!("unknown investigations subcommand `{sub}`")),
    }
    Ok(EXIT_OK)
}

fn command_agent_activity(args: &[String]) -> Result<u8, String> {
    let sub = args
        .first()
        .map(|s| s.as_str())
        .ok_or_else(|| "agent-activity requires a subcommand (file|stats)".to_string())?;
    let options = parse_options(&args[1..])?;
    let out = derived_dir(&options)?;
    let ds = crate::derived::DerivedStore::load(Path::new(&out)).map_err(|e| e.to_string())?;
    match sub {
        "file" => {
            let path = options
                .positional
                .first()
                .cloned()
                .ok_or_else(|| "agent-activity file <path>".to_string())?;
            // find by path across repositories (observational, no ranking)
            let found: Vec<&crate::derived::PathActivity> = ds
                .activity
                .paths
                .values()
                .filter(|a| a.path == path)
                .collect();
            if found.is_empty() {
                return Err(format!("no activity for `{path}`"));
            }
            if options.json {
                print_json(&serde_json::json!({"path":path,"records":found}))?;
            } else {
                for a in found {
                    println!(
                        "{}: obs={} reads={} sessions={} investigations={} mentions={}",
                        a.path,
                        a.observation_event_count_total,
                        a.explicit_read_count_total,
                        a.sessions_observed_count,
                        a.investigations_observed_count,
                        a.final_answer_mention_count
                    );
                }
            }
        }
        "stats" => {
            if options.json {
                print_json(&serde_json::json!({"activity_paths":ds.activity.paths.len()}))?;
            } else {
                println!("activity_paths={}", ds.activity.paths.len());
            }
        }
        _ => return Err(format!("unknown agent-activity subcommand `{sub}`")),
    }
    Ok(EXIT_OK)
}

// ---------------------------------------------------------------------------
// memory — Investigation Memory: rebuildable navigation priors derived from
// InvestigationEpisodes. Observational; no ranking/usefulness score (§16-§38).
// ---------------------------------------------------------------------------

fn memory_dir(options: &Options) -> Result<String, String> {
    options
        .output
        .clone()
        .ok_or_else(|| "memory requires --output <memory-dir>".to_string())
}

fn command_memory(args: &[String]) -> Result<u8, String> {
    let sub = args.first().map(|s| s.as_str()).ok_or_else(|| {
        "memory requires a subcommand (build|verify|stats|query|show)".to_string()
    })?;
    let options = parse_options(&args[1..])?;
    let mdir = memory_dir(&options)?;
    match sub {
        "build" | "update" => {
            // derived store is --store; memory output is --output
            let sdir = options
                .store
                .clone()
                .ok_or_else(|| "memory build requires --store <derived-dir>".to_string())?;
            let ds =
                crate::derived::DerivedStore::load(Path::new(&sdir)).map_err(|e| e.to_string())?;
            let known = known_repo_paths(options.repo_root.as_deref());
            let (mut ms, rep) = crate::memory::build_memory(
                &ds,
                Path::new(&mdir),
                options.full,
                &known,
                &known, // remembered snapshot == current (static benchmark repo)
            )
            .map_err(|e| e.to_string())?;
            // Optional symbol-level enrichment from a SymbolExposure store (§38).
            if let Some(sxdir) = options.symbol_exposure.clone() {
                let sexp = crate::symbol_exposure::SymbolExposureStore::open(Path::new(&sxdir));
                ms.enrich_symbols(&sexp, &known);
            }
            ms.save(Path::new(&mdir)).map_err(|e| e.to_string())?;
            let m = ms
                .write_manifest(Path::new(&mdir))
                .map_err(|e| e.to_string())?;
            if options.json {
                print_json(
                    &serde_json::json!({"command":"memory build","full":options.full,
                    "investigations":rep["investigations"],"rebuilt":rep["rebuilt"],
                    "posting_terms":rep["posting_terms"],"artifact_bytes":m.artifact_bytes}),
                )?;
            } else {
                println!(
                    "memory: {} investigations ({} rebuilt), {} posting terms",
                    rep["investigations"], rep["rebuilt"], rep["posting_terms"]
                );
            }
        }
        "verify" => {
            let ms =
                crate::memory::MemoryStore::load(Path::new(&mdir)).map_err(|e| e.to_string())?;
            let mpath = Path::new(&mdir).join("manifest.json");
            let mut anomalies = Vec::new();
            if !mpath.exists() {
                anomalies.push("missing manifest.json");
            } else {
                let m: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(&mpath).unwrap_or_default())
                        .map_err(|e| e.to_string())?;
                if m["schema_version"].as_u64().unwrap_or(0)
                    != crate::memory::MEMORY_SCHEMA_VERSION as u64
                {
                    anomalies.push("schema_version mismatch");
                }
            }
            if options.json {
                print_json(
                    &serde_json::json!({"command":"memory verify","ok":anomalies.is_empty(),
                    "investigations":ms.entries.len(),"anomalies":anomalies}),
                )?;
            } else {
                println!(
                    "memory verify: {} investigations, {} anomalies",
                    ms.entries.len(),
                    anomalies.len()
                );
            }
            if !anomalies.is_empty() {
                return Ok(EXIT_ANALYSIS_FAILURES);
            }
        }
        "stats" => {
            let ms =
                crate::memory::MemoryStore::load(Path::new(&mdir)).map_err(|e| e.to_string())?;
            if options.json {
                print_json(&serde_json::json!({"command":"memory stats",
                    "investigations":ms.entries.len(),"posting_terms":ms.postings.len()}))?;
            } else {
                println!(
                    "memory: {} investigations, {} posting terms",
                    ms.entries.len(),
                    ms.postings.len()
                );
            }
        }
        "query" => {
            let q = options.positional.join(" ");
            if q.is_empty() {
                return Err("memory query requires a query (positional text)".to_string());
            }
            let known = known_repo_paths(options.repo_root.as_deref());
            let ms =
                crate::memory::MemoryStore::load(Path::new(&mdir)).map_err(|e| e.to_string())?;
            let limit = options.max_results.unwrap_or(10);
            let matches = ms.query(&q, &known, limit, 64);
            if options.json {
                let arr: Vec<serde_json::Value> = matches
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "investigation_id": m.investigation_id,
                            "similarity": m.similarity,
                            "components": m.components,
                            "paths": m.paths.iter().take(limit).map(|p| serde_json::json!({
                                "path": p.path, "evidence": format!("{:?}", p.strongest()),
                                "freshness": p.freshness.map(|f| format!("{f:?}")),
                                "surfaced": p.surfaced, "observed": p.observed,
                                "explicitly_read": p.explicitly_read,
                                "mentioned": p.mentioned_in_final_answer,
                            })).collect::<Vec<_>>(),
                            "symbols": m.symbols.iter().take(limit).map(|s| serde_json::json!({
                                "symbol_id": s.symbol_id, "symbol": s.symbol_name,
                                "kind": s.symbol_kind, "path": s.path,
                                "declaration_exposed": s.declaration_exposed,
                                "enclosing_exposed": s.enclosing_exposed,
                                "reference_exposed": s.reference_exposed,
                                "explicitly_read": s.explicitly_read,
                                "via_search_snippet": s.via_search_snippet,
                                "final_answer_mentioned": s.mentioned_in_final_answer,
                                "freshness": s.freshness.map(|f| format!("{f:?}")),
                            })).collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                print_json(&serde_json::json!({"command":"memory query","query":q,"matches":arr}))?;
            } else {
                for m in &matches {
                    println!("{:.3} {}", m.similarity, m.investigation_id);
                }
            }
        }
        "show" => {
            let inv = options
                .positional
                .first()
                .cloned()
                .ok_or_else(|| "memory show requires <investigation_id>".to_string())?;
            let ms =
                crate::memory::MemoryStore::load(Path::new(&mdir)).map_err(|e| e.to_string())?;
            match ms.entries.get(&inv) {
                Some(e) => print_json(e)?,
                None => return Err(format!("no memory entry for `{inv}`")),
            }
        }
        _ => return Err(format!("unknown memory subcommand `{sub}`")),
    }
    Ok(EXIT_OK)
}

fn command_symbol_exposure(args: &[String]) -> Result<u8, String> {
    use crate::symbol_exposure as sx;
    let sub = args.first().map(|s| s.as_str()).ok_or_else(|| {
        "symbol-exposure requires a subcommand (derive|investigation|source|activity|stats)"
            .to_string()
    })?;
    let options = parse_options(&args[1..])?;
    let out_dir = options
        .output
        .clone()
        .ok_or_else(|| "symbol-exposure requires --output <symbol-exposure-dir>".to_string())?;
    match sub {
        "derive" => {
            let store_dir = options.store.clone().ok_or_else(|| {
                "symbol-exposure derive requires --store <agent-event-dir>".to_string()
            })?;
            let snap_dir = options.snapshot.clone().ok_or_else(|| {
                "symbol-exposure derive requires --snapshot <index-dir>".to_string()
            })?;
            let estore = crate::agent_event::AgentEventStore::open(Path::new(&store_dir))
                .map_err(|e| e.to_string())?;
            let manifest = crate::repository::artifact::read_manifest(Path::new(&snap_dir))
                .map_err(|e| e.to_string())?;
            let sources = sx::SourceStore::default();
            let provider = sx::SymbolMapProvider::new(Path::new(&snap_dir), &manifest, &sources);
            let mut store = sx::SymbolExposureStore::open(Path::new(&out_dir));
            // Fold all sessions' events (background enrichment — synchronous for CLI).
            for sid in estore.sessions().map_err(|e| e.to_string())? {
                let events = estore.session_events(&sid).map_err(|e| e.to_string())?;
                sx::store::derive_symbol_exposures(&events, &provider, &mut store);
            }
            store.save().map_err(|e| e.to_string())?;
            // symbol activity
            let mut act = sx::SymbolActivityStore::open(&Path::new(&out_dir).join("activity"));
            std::fs::create_dir_all(act.contrib_path().parent().unwrap())
                .map_err(|e| e.to_string())?;
            let mut contrib =
                std::fs::File::create(act.contrib_path()).map_err(|e| e.to_string())?;
            for doc in store.docs.values() {
                for x in &doc.exposures {
                    act.fold(x, &doc.investigation_id, &mut contrib);
                }
            }
            act.save().map_err(|e| e.to_string())?;
            let d = &store.diag;
            let pd = provider.diag.borrow();
            let rep = serde_json::json!({
                "command": "symbol-exposure derive",
                "investigations": store.docs.len(),
                "source_observed_events": d.source_observed_events,
                "exposures": d.exposures,
                "unique_symbols": d.unique_symbols,
                "resolved_indexed_hit": d.resolved_indexed_hit,
                "resolved_cached_version": d.resolved_cached_version,
                "resolved_new_parse": d.resolved_new_parse,
                "resolved_assumed_indexed": d.resolved_assumed_indexed,
                "unresolved": d.unresolved,
                "unresolved_missing_source_version": d.unresolved_missing_source_version,
                "unresolved_unsupported_language": d.unresolved_unsupported_language,
                "unresolved_no_symbol_overlap": d.unresolved_no_symbol_overlap,
            });
            drop(pd);
            if options.json {
                print_json(&rep)?;
            } else {
                println!(
                    "symbol-exposure: {} investigations, {} exposures, {} unique symbols",
                    store.docs.len(),
                    d.exposures,
                    d.unique_symbols
                );
            }
        }
        "investigation" | "inv" => {
            let inv = options
                .positional
                .first()
                .cloned()
                .ok_or_else(|| "symbol-exposure investigation requires <id>".to_string())?;
            let store = sx::SymbolExposureStore::open(Path::new(&out_dir));
            match store.load(&inv) {
                Some(doc) => {
                    if options.json {
                        print_json(&serde_json::to_value(&doc).map_err(|e| e.to_string())?)?;
                    } else {
                        for x in &doc.exposures {
                            println!(
                                "{} {} {} [{}] {}",
                                x.exposure_kind.as_str(),
                                x.symbol_id,
                                x.symbol_name,
                                x.observation_provenance.as_str(),
                                x.overlap.as_str()
                            );
                        }
                    }
                }
                None => return Err(format!("no symbol-exposure doc for `{inv}`")),
            }
        }
        "source" => {
            // exposures for one source_observed event (session:seq)
            let key = options
                .positional
                .first()
                .cloned()
                .ok_or_else(|| "symbol-exposure source requires <session:seq>".to_string())?;
            let store = sx::SymbolExposureStore::open(Path::new(&out_dir));
            let mut hits = Vec::new();
            for id in store.investigation_ids() {
                if let Some(doc) = store.load(&id) {
                    for x in &doc.exposures {
                        if format!("{}:{}", x.session_id, x.source_event_sequence) == key {
                            hits.push(serde_json::to_value(x).map_err(|e| e.to_string())?);
                        }
                    }
                }
            }
            print_json(&serde_json::json!({"source_event": key, "exposures": hits}))?;
        }
        "activity" => {
            // symbol activity by symbol id (or top-N summary)
            let adir = Path::new(&out_dir).join("activity").join("symbols.jsonl");
            let target = options.positional.first().cloned();
            let mut rows = Vec::new();
            if let Ok(s) = std::fs::read_to_string(&adir) {
                for line in s.lines() {
                    if let Ok(a) = serde_json::from_str::<sx::SymbolActivity>(line) {
                        if target
                            .as_deref()
                            .map(|t| a.symbol_id.contains(t) || a.symbol_name == t)
                            .unwrap_or(true)
                        {
                            rows.push(a);
                        }
                    }
                }
            }
            rows.sort_by(|a, b| {
                b.observation_event_count_total
                    .cmp(&a.observation_event_count_total)
            });
            let lim = options.max_results.unwrap_or(20);
            if options.json {
                let v: Vec<_> = rows.iter().take(lim).collect();
                print_json(&serde_json::json!({"command":"symbol-exposure activity","symbols":v}))?;
            } else {
                for a in rows.iter().take(lim) {
                    println!(
                        "{:>5} {} {} ({} reads)",
                        a.observation_event_count_total,
                        a.symbol_id,
                        a.symbol_name,
                        a.explicit_read_exposure_count
                    );
                }
            }
        }
        "stats" => {
            let store = sx::SymbolExposureStore::open(Path::new(&out_dir));
            let ids = store.investigation_ids();
            let mut tot = 0u64;
            let mut kinds = std::collections::BTreeMap::new();
            for id in &ids {
                if let Some(d) = store.load(id) {
                    tot += d.exposures.len() as u64;
                    for x in &d.exposures {
                        *kinds
                            .entry(x.exposure_kind.as_str().to_string())
                            .or_insert(0u64) += 1;
                    }
                }
            }
            print_json(&serde_json::json!({"command":"symbol-exposure stats",
                "investigations": ids.len(), "exposures": tot, "by_kind": kinds}))?;
        }
        _ => return Err(format!("unknown symbol-exposure subcommand `{sub}`")),
    }
    Ok(EXIT_OK)
}
