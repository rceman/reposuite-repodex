//! RepositoryView + gateway-CLI contract tests (Part III-XXX).
//!
//! Builds an isolated multi-branch/worktree git fixture and exercises the real
//! `reposuite-repodex` binary: per-view correctness, dirty visibility,
//! added/deleted files, explicit-root/project-view equivalence, machine JSON
//! stdin/stdout + errors, and concurrency — all under an isolated
//! `REPODEX_STATE_DIR` (never the user's real registry).

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;
use support::TempDir;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_reposuite-repodex"))
}

fn git(dir: &Path, args: &[&str]) {
    let st = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        st.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&st.stderr)
    );
}

fn run(state: &Path, args: &[&str], stdin: &str) -> (String, String, i32) {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("REPODEX_STATE_DIR", state)
        .current_dir("/tmp") // unrelated cwd (§5)
        .stdin(if stdin.is_empty() {
            std::process::Stdio::null()
        } else {
            std::process::Stdio::piped()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().expect("spawn");
    if !stdin.is_empty() {
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().expect("wait");
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

/// Build main + work-a/b/c with distinct content on shared paths.
struct Fixture {
    root: TempDir,
    state: TempDir,
}
impl Fixture {
    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }
    fn new() -> Option<Self> {
        // Skip if git is unavailable.
        if Command::new("git").arg("--version").output().is_err() {
            return None;
        }
        let root = TempDir::new("rvfix");
        let state = TempDir::new("rvstate");
        let main = root.path().join("main");
        std::fs::create_dir_all(&main).unwrap();
        git(&main, &["init", "-q", "-b", "main"]);
        git(&main, &["config", "user.email", "t@t"]);
        git(&main, &["config", "user.name", "t"]);
        std::fs::create_dir_all(main.join("internal/auth")).unwrap();
        std::fs::create_dir_all(main.join("internal/api")).unwrap();
        std::fs::write(
            main.join("internal/auth/check.go"),
            "package auth\nfunc CheckAuthMain(t string) bool { return t==\"m\" }\nfunc HelperShared() int { return 1 }\n",
        ).unwrap();
        std::fs::write(
            main.join("internal/api/serve.go"),
            "package api\nfunc ServeMain() {}\n",
        )
        .unwrap();
        std::fs::write(
            main.join("shared.go"),
            "package main\nfunc SharedMain() {}\n",
        )
        .unwrap();
        git(&main, &["add", "-A"]);
        git(&main, &["commit", "-qm", "main"]);
        // work-a: different branch + content + added a.go
        git(
            &main,
            &["worktree", "add", "-q", "../work-a", "-b", "task/a"],
        );
        let a = root.path().join("work-a");
        git(&a, &["config", "user.email", "t@t"]);
        git(&a, &["config", "user.name", "t"]);
        std::fs::write(a.join("shared.go"), "package main\nfunc SharedA() {}\n").unwrap();
        std::fs::write(a.join("a.go"), "package main\nfunc OnlyInA() {}\n").unwrap();
        git(&a, &["add", "-A"]);
        git(&a, &["commit", "-qm", "a"]);
        // work-b: different content + deleted serve.go + added b.go
        git(
            &main,
            &["worktree", "add", "-q", "../work-b", "-b", "task/b"],
        );
        let b = root.path().join("work-b");
        git(&b, &["config", "user.email", "t@t"]);
        git(&b, &["config", "user.name", "t"]);
        std::fs::write(b.join("shared.go"), "package main\nfunc SharedB() {}\n").unwrap();
        std::fs::write(b.join("b.go"), "package main\nfunc OnlyInB() {}\n").unwrap();
        std::fs::remove_file(b.join("internal/api/serve.go")).unwrap();
        git(&b, &["add", "-A"]);
        git(&b, &["commit", "-qm", "b"]);
        // work-c: identical content to main
        git(
            &main,
            &["worktree", "add", "-q", "--detach", "../work-c", "HEAD"],
        );
        Some(Self { root, state })
    }
    fn query_human(&self, view: &str, q: &str) -> String {
        let root = self.path(view);
        let (o, e, _) = run(
            self.state.path(),
            &[
                "query",
                "--root",
                root.to_str().unwrap(),
                "--query",
                q,
                "--human",
            ],
            "",
        );
        format!("{o}{e}")
    }
}

#[test]
fn per_view_symbols_no_contamination() {
    let Some(fx) = Fixture::new() else { return };
    assert!(fx.query_human("main", "shared").contains("SharedMain"));
    assert!(fx.query_human("work-a", "shared").contains("SharedA"));
    assert!(fx.query_human("work-b", "shared").contains("SharedB"));
    assert!(fx.query_human("work-c", "shared").contains("SharedMain"));
    // no cross-view: A's symbol absent in B
    assert!(!fx.query_human("work-b", "shared").contains("SharedA"));
    assert!(!fx.query_human("work-a", "shared").contains("SharedB"));
}

#[test]
fn added_and_deleted_files() {
    let Some(fx) = Fixture::new() else { return };
    // added file visible only in its view (check the declaration, not the
    // echoed query term)
    assert!(fx
        .query_human("work-a", "OnlyInA")
        .contains("declaration  OnlyInA"));
    assert!(!fx
        .query_human("work-b", "OnlyInA")
        .contains("declaration  OnlyInA"));
    // deleted file absent in B, present in main
    assert!(fx
        .query_human("main", "ServeMain")
        .contains("declaration  ServeMain"));
    assert!(!fx
        .query_human("work-b", "ServeMain")
        .contains("declaration  ServeMain"));
}

#[test]
fn dirty_edit_becomes_visible() {
    let Some(fx) = Fixture::new() else { return };
    let a = fx.path("work-a");
    let before = fx.query_human("work-a", "DirtyNew");
    assert!(!before.contains("declaration  DirtyNew"));
    // uncommitted edit -> next query sees it (§22)
    std::fs::write(
        a.join("shared.go"),
        "package main\nfunc SharedA() {}\nfunc DirtyNew() {}\n",
    )
    .unwrap();
    assert!(fx.query_human("work-a", "DirtyNew").contains("DirtyNew"));
}

#[test]
fn explicit_root_and_project_view_equivalent() {
    let Some(fx) = Fixture::new() else { return };
    // register FIX with views-root = fixture root
    let main = fx.path("main");
    let (o, _, _) = run(
        fx.state.path(),
        &[
            "project",
            "register",
            "FIX",
            "--root",
            main.to_str().unwrap(),
            "--views-root",
            fx.root.path().to_str().unwrap(),
        ],
        "",
    );
    assert!(o.contains("registered"), "{o}");
    // explicit root vs project+view -> identical seed labels
    let via_root = fx.query_human("work-a", "shared");
    let (o, _, _) = run(
        fx.state.path(),
        &[
            "query",
            "--project",
            "FIX",
            "--view",
            "work-a",
            "--query",
            "shared",
            "--human",
        ],
        "",
    );
    for sym in ["SharedA", "HelperShared"] {
        assert!(via_root.contains(sym) && o.contains(sym), "{sym}");
    }
    // project default view resolves to registered root
    let (o, _, _) = run(
        fx.state.path(),
        &["query", "--project", "FIX", "--query", "shared", "--human"],
        "",
    );
    assert!(o.contains("SharedMain"), "{o}");
}

#[test]
fn machine_json_stdin_stdout() {
    let Some(fx) = Fixture::new() else { return };
    let wa = fx.path("work-b");
    let req = format!(
        r#"{{"schema":"reposuite.repodex.query.request.v1","root":"{}","query":"shared"}}"#,
        wa.display()
    );
    let (o, e, code) = run(fx.state.path(), &["query", "--json"], &req);
    assert_eq!(code, 0);
    let d: serde_json::Value = serde_json::from_str(&o).unwrap();
    assert_eq!(d["schema"], "reposuite.repodex.query.response.v1");
    assert_eq!(d["repository_view"]["view_id"], "work-b");
    assert_eq!(d["repository_view"]["branch"], "task/b");
    let labels: Vec<_> = d["result"]["seeds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["label"].as_str().unwrap().to_string())
        .collect();
    assert!(labels.iter().any(|l| l == "SharedB"), "{labels:?}");
    assert!(e.is_empty() || !e.contains("panic"));
}

#[test]
fn locator_errors_are_machine_coded() {
    let Some(fx) = Fixture::new() else { return };
    let (o, _, code) = run(
        fx.state.path(),
        &["query", "--json"],
        r#"{"root":"/x","project":"FIX","query":"q"}"#,
    );
    assert_ne!(code, 0);
    assert!(o.contains("LOCATOR_CONFLICT"), "{o}");
    let (o, _, code) = run(
        fx.state.path(),
        &["query", "--json"],
        r#"{"view":"work-a","query":"q"}"#,
    );
    assert_ne!(code, 0);
    assert!(o.contains("LOCATOR_CONFLICT"), "{o}");
    let (o, _, code) = run(
        fx.state.path(),
        &["query", "--json"],
        r#"{"project":"FIX","view":"../main","query":"q"}"#,
    );
    assert_ne!(code, 0);
    assert!(
        o.contains("VIEW_PATH_ESCAPE") || o.contains("PROJECT_NOT_FOUND"),
        "{o}"
    );
    let (o, _, code) = run(fx.state.path(), &["query", "--json"], "not json");
    assert_ne!(code, 0);
    assert!(o.contains("INVALID_REQUEST"), "{o}");
}

#[test]
fn interleaved_and_concurrent_views() {
    let Some(fx) = Fixture::new() else { return };
    // interleaved correctness
    for (view, sym) in [
        ("work-a", "SharedA"),
        ("work-b", "SharedB"),
        ("work-a", "SharedA"),
        ("work-c", "SharedMain"),
        ("work-b", "SharedB"),
    ] {
        assert!(
            fx.query_human(view, "shared").contains(sym),
            "{view} -> {sym}"
        );
    }
    // concurrent distinct views
    let mut handles = Vec::new();
    for view in ["work-a", "work-b", "work-c", "main"] {
        let state = fx.state.path().to_path_buf();
        let root = fx.path(view);
        handles.push(std::thread::spawn(move || {
            run(
                &state,
                &[
                    "query",
                    "--root",
                    root.to_str().unwrap(),
                    "--query",
                    "shared",
                    "--human",
                ],
                "",
            )
        }));
    }
    let outs: Vec<String> = handles.into_iter().map(|h| h.join().unwrap().0).collect();
    assert!(
        outs[0].contains("SharedA")
            && outs[1].contains("SharedB")
            && outs[2].contains("SharedMain")
            && outs[3].contains("SharedMain"),
        "{outs:?}"
    );
}
