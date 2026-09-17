//! CLI behaviour tests.
//!
//! The CLI format is experimental, but exit codes, JSON-only stdout and the
//! presence of the documented commands are part of the TASK 1 contract.

mod support;

use std::process::{Command, Output};

use support::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_reposuite-repodex");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("the binary must be runnable")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout must be utf-8")
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr must be utf-8")
}

fn code_of(output: &Output) -> i32 {
    output.status.code().expect("exit code")
}

#[test]
fn languages_reports_all_four_and_their_grammars_load() {
    let output = run(&["languages", "--json"]);
    assert_eq!(code_of(&output), 0, "{}", stderr_of(&output));
    let json: serde_json::Value = serde_json::from_str(&stdout_of(&output)).expect("json");
    assert_eq!(json["schema_version"], 1);
    let languages = json["languages"].as_array().expect("languages");
    assert_eq!(languages.len(), 4);
    let names: Vec<&str> = languages
        .iter()
        .map(|entry| entry["language"].as_str().expect("language"))
        .collect();
    assert_eq!(names, vec!["rust", "go", "python", "php"]);
    for entry in languages {
        assert_eq!(entry["grammar_loads"], serde_json::Value::Bool(true));
        assert!(entry["grammar_abi_version"].as_u64().expect("abi") >= 13);
        assert!(!entry["extensions"]
            .as_array()
            .expect("extensions")
            .is_empty());
        assert!(entry["grammar_version"]
            .as_str()
            .expect("version")
            .contains('.'));
    }
    // Text mode is human readable and mentions the runtime root.
    let output = run(&["languages"]);
    assert_eq!(code_of(&output), 0);
    let text = stdout_of(&output);
    for expected in ["Rust", "Go", "Python", "PHP", "runtime root:"] {
        assert!(text.contains(expected), "missing `{expected}` in {text}");
    }
}

#[test]
fn parse_reports_facts_and_exit_codes() {
    let fixture = support::fixture("rust/declarations.rs");
    let output = run(&["parse", fixture.to_str().expect("path")]);
    assert_eq!(code_of(&output), 0);
    let text = stdout_of(&output);
    assert!(text.contains("language:   Rust"));
    assert!(text.contains("status:     clean"));
    assert!(text.contains("declarations:"));
    assert!(text.contains("imports:"));
    assert!(text.contains("references:"));
    assert!(text.contains("call-like:"));

    let malformed = support::fixture("rust/malformed.rs");
    let output = run(&["parse", malformed.to_str().expect("path")]);
    assert_eq!(
        code_of(&output),
        1,
        "recovery must be visible in the exit code"
    );
    assert!(stdout_of(&output).contains("status:     recovered"));
}

#[test]
fn parse_json_contains_machine_readable_facts_only() {
    let fixture = support::fixture("python/test_sample.py");
    let output = run(&["parse", fixture.to_str().expect("path"), "--json"]);
    assert_eq!(code_of(&output), 0);
    let json: serde_json::Value = serde_json::from_str(&stdout_of(&output)).expect("json only");
    assert_eq!(json["command"], "parse");
    assert_eq!(json["language"], "python");
    assert_eq!(json["status"], "clean");
    assert!(
        json["analysis"]["declarations"]
            .as_array()
            .expect("declarations")
            .len()
            >= 8
    );
    assert!(json["canonical_digest"]
        .as_str()
        .expect("digest")
        .starts_with("fnv1a64:"));
    // Canonical facts are opt-in.
    assert!(json["canonical_facts"].is_null());

    let output = run(&[
        "parse",
        fixture.to_str().expect("path"),
        "--json",
        "--include-facts",
    ]);
    let json: serde_json::Value = serde_json::from_str(&stdout_of(&output)).expect("json only");
    let facts = json["canonical_facts"].as_array().expect("facts");
    assert!(facts.len() > 10);
    // The canonical export is the complete normalized rendering, so it opens
    // with the file header and carries the full schema and path.
    let header = facts[0].as_str().expect("line");
    assert!(header.starts_with("file schema="), "{header}");
    assert!(header.contains("path=test_sample.py"), "{header}");
    // Nested ranges are rendered with their coordinates, not only their bytes.
    assert!(
        facts
            .iter()
            .filter_map(|line| line.as_str())
            .any(|line| line.contains("target_range=") && line.contains("(")),
        "the canonical export must render nested ranges"
    );
}

#[test]
fn parse_rejects_unsupported_input_clearly() {
    let temp = TempDir::new("cli-unsupported");
    let path = temp.write("notes.txt", b"hello\n");
    let output = run(&["parse", path.to_str().expect("path")]);
    assert_eq!(code_of(&output), 2);
    assert!(stderr_of(&output).contains("unsupported file extension"));

    let missing = temp.path().join("nope.rs");
    let output = run(&["parse", missing.to_str().expect("path")]);
    assert_eq!(code_of(&output), 1);
    assert!(stdout_of(&output).contains("status:     failed"));
}

#[test]
fn scan_reports_counters_and_facts() {
    let temp = TempDir::new("cli-scan");
    temp.write("src/one.rs", b"pub fn one() {}\n");
    temp.write("src/two.go", b"package p\n\nfunc Two() {}\n");
    temp.write("notes.md", b"# notes\n");

    let output = run(&["scan", temp.path().to_str().expect("path")]);
    assert_eq!(code_of(&output), 0, "{}", stderr_of(&output));
    let text = stdout_of(&output);
    for expected in [
        "visited files:   3",
        "supported:     2",
        "unsupported: 1",
        "declarations",
        "elapsed:",
    ] {
        assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
    }

    let output = run(&["scan", temp.path().to_str().expect("path"), "--json"]);
    let json: serde_json::Value = serde_json::from_str(&stdout_of(&output)).expect("json only");
    assert_eq!(json["command"], "scan");
    assert_eq!(json["visited_files"], 3);
    assert_eq!(json["supported_files"], 2);
    assert_eq!(json["unsupported_files"], 1);
    assert_eq!(json["parsed_clean_files"], 2);
    assert_eq!(json["parsed_with_recovery_files"], 0);
    assert_eq!(json["trees_returned"], 2);
    assert_eq!(json["failed_files"], 0);
    assert_eq!(json["declarations"], 3);
    assert!(json["elapsed_ms"].as_f64().expect("elapsed") >= 0.0);
    assert!(json["recovery_rate_among_parsed"].as_f64().expect("rate") == 0.0);
    let files = json["files"].as_array().expect("files");
    assert_eq!(files.len(), 3);
    assert_eq!(files[0]["path"], "notes.md");
    assert_eq!(files[0]["outcome"], "unsupported");
    // JSON output carries no canonical facts unless asked for.
    assert!(json["canonical"].is_null());
}

#[test]
fn scan_json_can_export_complete_canonical_facts() {
    let temp = TempDir::new("cli-scan-facts");
    temp.write("a.rs", b"pub fn alpha() {}\n");
    temp.write("b.py", b"def beta():\n    pass\n");
    let output = run(&[
        "scan",
        temp.path().to_str().expect("path"),
        "--json",
        "--include-facts",
    ]);
    let json: serde_json::Value = serde_json::from_str(&stdout_of(&output)).expect("json only");
    let canonical = &json["canonical"];
    assert!(canonical["digest"]
        .as_str()
        .expect("digest")
        .starts_with("fnv1a64:"));
    let files = canonical["files"].as_array().expect("files");
    assert_eq!(files.len(), 2);
    let alpha = files
        .iter()
        .find(|file| file["path"] == "a.rs")
        .expect("a.rs");
    let lines = alpha["lines"].as_array().expect("lines");
    assert!(lines.iter().any(|line| line
        .as_str()
        .expect("line")
        .starts_with("decl 0 kind=function name=alpha")));
}

#[test]
fn scan_exit_code_flags_recovery_and_failures() {
    let temp = TempDir::new("cli-scan-recovery");
    temp.write("ok.rs", b"pub fn ok() {}\n");
    temp.write("broken.rs", b"fn broken( {\n}\n");
    let output = run(&["scan", temp.path().to_str().expect("path")]);
    assert_eq!(code_of(&output), 0, "recovery alone is not a failure");
    let text = stdout_of(&output);
    assert!(text.contains("with recovery: 1"));
    assert!(text.contains("files needing attention:"));
    assert!(text.contains("broken.rs"));
}

#[test]
fn scan_requires_an_existing_directory() {
    let temp = TempDir::new("cli-scan-missing");
    let missing = temp.path().join("nope");
    let output = run(&["scan", missing.to_str().expect("path")]);
    assert_eq!(code_of(&output), 2);
    assert!(stderr_of(&output).contains("not a directory"));

    let output = run(&["scan", temp.path().to_str().expect("path"), "--bogus"]);
    assert_eq!(code_of(&output), 2);
    assert!(stderr_of(&output).contains("unknown option"));
}

#[test]
fn scan_never_creates_runtime_directories() {
    let temp = TempDir::new("cli-no-runtime-state");
    let runtime_root = temp.path().join("runtime");
    temp.write("repo/a.rs", b"pub fn a() {}\n");
    let output = Command::new(BIN)
        .args([
            "scan",
            temp.path().join("repo").to_str().expect("path"),
            "--json",
        ])
        .env("REPOSUITE_REPODEX_HOME", &runtime_root)
        .output()
        .expect("run");
    assert_eq!(code_of(&output), 0, "{}", stderr_of(&output));
    assert!(
        !runtime_root.exists(),
        "TASK 1 commands must not create persistent runtime state"
    );
    // `languages` reports the overridden root without creating it.
    let output = Command::new(BIN)
        .args(["languages", "--json"])
        .env("REPOSUITE_REPODEX_HOME", &runtime_root)
        .output()
        .expect("run");
    let json: serde_json::Value = serde_json::from_str(&stdout_of(&output)).expect("json");
    assert_eq!(
        json["runtime_root"],
        serde_json::Value::String(runtime_root.display().to_string())
    );
    assert_eq!(json["runtime_root_source"], "REPOSUITE_REPODEX_HOME");
    assert!(!runtime_root.exists());
}

#[test]
fn help_and_version_are_available() {
    let output = run(&["--version"]);
    assert_eq!(code_of(&output), 0);
    assert!(stdout_of(&output).contains("reposuite-repodex"));

    let output = run(&["--help"]);
    assert_eq!(code_of(&output), 0);
    let text = stdout_of(&output);
    for command in ["languages", "parse", "scan"] {
        assert!(text.contains(command));
    }
    // No command is a usage error.
    let output = run(&[]);
    assert_eq!(code_of(&output), 2);
}

#[test]
fn unknown_commands_fail_cleanly() {
    let output = run(&["navigate"]);
    assert_eq!(code_of(&output), 2);
    assert!(stderr_of(&output).contains("unknown command"));
}

#[test]
fn json_stdout_is_never_polluted_by_logs() {
    let temp = TempDir::new("cli-json-purity");
    temp.write("a.rs", b"pub fn a() {}\n");
    for args in [
        vec!["languages", "--json"],
        vec!["scan", temp.path().to_str().expect("path"), "--json"],
        vec![
            "parse",
            support::fixture("rust/tests.rs").to_str().expect("path"),
            "--json",
        ],
    ] {
        let output = run(&args);
        let stdout = stdout_of(&output);
        serde_json::from_str::<serde_json::Value>(&stdout)
            .unwrap_or_else(|error| panic!("stdout must be JSON only for {args:?}: {error}"));
    }
}

#[test]
fn max_file_size_override_is_honoured() {
    let temp = TempDir::new("cli-max-size");
    let body = b"pub fn small() {}\n";
    let path = temp.write("small.rs", body);
    let output = run(&[
        "parse",
        path.to_str().expect("path"),
        "--max-file-size",
        "4",
    ]);
    assert_eq!(code_of(&output), 1);
    assert!(stdout_of(&output).contains("status:     unsupported"));
    let output = run(&[
        "parse",
        path.to_str().expect("path"),
        "--max-file-size=4096",
    ]);
    assert_eq!(code_of(&output), 0);
    assert!(stdout_of(&output).contains("status:     clean"));
}

/// A scan that visits everything must report itself complete in the JSON output
/// and exit 0. This half of the traversal contract is platform-independent and
/// runs on every platform, including native Windows.
#[test]
fn a_complete_scan_reports_itself_complete_in_json() {
    let clean = TempDir::new("cli-traversal-clean");
    clean.write("a.rs", b"fn a() {}\n");
    let complete = run(&["scan", clean.path().to_str().expect("path"), "--json"]);
    assert_eq!(code_of(&complete), 0, "{}", stderr_of(&complete));
    let json: serde_json::Value = serde_json::from_str(&stdout_of(&complete)).expect("json only");
    assert_eq!(json["scan_complete"], true);
    assert_eq!(json["traversal_failures"], 0);
    assert!(json["traversal_failure_details"]
        .as_array()
        .expect("details array")
        .is_empty());
}

/// Provoking a traversal failure needs a directory the process cannot read,
/// which is a POSIX permission operation, so the failure-injection half of the
/// traversal contract is Unix-only. The reporting path is platform-independent;
/// only the way this test provokes it is not.
///
/// No Windows ACL manipulation is implemented. On non-Unix platforms this test
/// is simply absent, and the completeness assertions above still run.
#[cfg(unix)]
mod unreadable_directory {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Set a directory unreadable, returning whether the platform actually
    /// denied access to this process.
    fn deny_read(path: &std::path::Path) -> bool {
        if std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).is_err() {
            return false;
        }
        std::fs::read_dir(path).is_err()
    }

    fn restore_read(path: &std::path::Path) {
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
    }

    /// A scan that could not visit part of the tree must say so, in the text
    /// output, in the JSON output and in the exit code.
    ///
    /// A scan that quietly drops a subtree makes every count derived from it
    /// untrustworthy, so "the scan is complete" has to be a checkable claim.
    #[test]
    fn a_scan_that_cannot_visit_a_directory_reports_it_and_fails() {
        let temp = TempDir::new("cli-traversal");
        temp.write("keep.rs", b"fn keep() {}\n");
        let locked = temp.path().join("locked");
        std::fs::create_dir_all(&locked).expect("create locked");
        std::fs::write(locked.join("hidden.rs"), b"fn hidden() {}\n").expect("write hidden");

        if !deny_read(&locked) {
            restore_read(&locked);
            eprintln!(
                "skipping traversal-failure assertion: this environment cannot make a \
                 directory unreadable"
            );
            return;
        }

        let text = run(&["scan", temp.path().to_str().expect("path")]);
        let json_output = run(&["scan", temp.path().to_str().expect("path"), "--json"]);
        restore_read(&locked);

        // Text output: the failure is named and the scan is called incomplete.
        let stdout = stdout_of(&text);
        assert!(
            stdout.contains("SCAN INCOMPLETE"),
            "the text output must not claim complete coverage:\n{stdout}"
        );
        assert!(stdout.contains("traversal failures: 1"), "{stdout}");
        assert!(stdout.contains("locked"), "{stdout}");
        // The accessible file was still analyzed.
        assert!(stdout.contains("clean:         1"), "{stdout}");

        // Exit code: an incomplete scan is an analysis failure, not success.
        assert_eq!(code_of(&text), 1, "{}", stderr_of(&text));
        assert_eq!(code_of(&json_output), 1, "{}", stderr_of(&json_output));

        // JSON output: machine-readable completeness, not just prose.
        let json: serde_json::Value =
            serde_json::from_str(&stdout_of(&json_output)).expect("json only");
        assert_eq!(json["scan_complete"], false);
        // The counter and the detail array are separate keys: a field named the
        // same as a flattened counter would produce a duplicate JSON key.
        assert_eq!(json["traversal_failures"], 1);
        let failures = json["traversal_failure_details"]
            .as_array()
            .expect("traversal_failure_details array");
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0]["path"], "locked");
        assert_eq!(failures[0]["kind"], "directory");
        assert!(!failures[0]["message"].as_str().expect("message").is_empty());
    }
}
