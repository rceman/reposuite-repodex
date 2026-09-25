use repodex::manifest::{analyze, manifest_kind};

#[test]
fn go_mod_module_and_require_indexed() {
    let a = analyze(
        "go.mod",
        b"module example.com/app\n\ngo 1.21\n\nrequire (\n\tgithub.com/a/b v1.0.0\n)\n",
    );
    let names: Vec<String> = a.declarations.iter().map(|d| d.name.clone()).collect();
    assert!(
        names.iter().any(|n| n == "go module example.com/app"),
        "{names:?}"
    );
    assert!(names.iter().any(|n| n == "go version 1.21"), "{names:?}");
    assert!(
        names
            .iter()
            .any(|n| n == "go require github.com/a/b v1.0.0"),
        "{names:?}"
    );
}

#[test]
fn cargo_package_workspace_deps_indexed() {
    let a = analyze(
        "Cargo.toml",
        b"[package]\nname = \"foo\"\nversion = \"1.0\"\nedition = \"2021\"\nrust-version = \"1.70\"\n\n[workspace]\nmembers = [\"a\"]\n\n[dependencies]\nserde = \"1\"\n\n[dev-dependencies]\nanyhow = \"1\"\n\n[build-dependencies]\ncc = \"1\"\n",
    );
    let names: Vec<String> = a.declarations.iter().map(|d| d.name.clone()).collect();
    for want in [
        "cargo package foo",
        "cargo edition 2021",
        "cargo rust-version 1.70",
        "cargo dependencies serde",
        "cargo dev anyhow",
        "cargo build cc",
    ] {
        assert!(names.iter().any(|n| n == want), "missing {want}: {names:?}");
    }
}

#[test]
fn manifest_kind_detected() {
    assert_eq!(manifest_kind("go.mod"), Some("go_mod"));
    assert_eq!(manifest_kind("a/b/Cargo.toml"), Some("cargo_toml"));
    assert_eq!(manifest_kind("x.rs"), None);
    // negative controls: not manifests
    assert_eq!(manifest_kind("Cargo.toml.example"), None);
    assert_eq!(manifest_kind("old.go.mod.txt"), None);
}

#[test]
fn malformed_manifest_no_panic() {
    let a = analyze("Cargo.toml", b"[package\nname = broken [[[\n= = =\n");
    // degraded, not a crash; may yield partial/none facts
    assert!(a.declarations.len() <= 4);
}
