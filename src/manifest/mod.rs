//! Manifest/module/package artifact analysis (§14-§28).
//!
//! go.mod and Cargo.toml are *not* source code — they carry module, package,
//! workspace and dependency declarations. They are visited by the scanner but
//! have no source-language `LanguageId`, so before this task they were
//! validity-only metadata. Here each is parsed into typed, deterministic facts
//! emitted as `Declaration`s on a synthetic `FileAnalysis`, so the artifact
//! flows through the same index/graph pipeline as source: queryable by
//! vocabulary, current-view bound, content-addressed, producer-versioned.
//!
//! Narrow semantics: a `require`/`[dependencies]` entry is a FACT about the
//! manifest *declaration* — never a claim that source code uses the dependency.
//! Ownership (source file -> owning module/package) is a deterministic
//! layout-derived relation emitted by the graph builder, FACT only under an
//! unambiguous nearest-manifest boundary.

use crate::model::{DeclarationKind, ScopeKind};
use crate::parser::builder::{DeclarationDraft, FactBuilder};

/// Recognised manifest basenames -> artifact kind.
pub fn manifest_kind(path: &str) -> Option<&'static str> {
    match path.rsplit('/').next().unwrap_or(path) {
        "go.mod" => Some("go_mod"),
        "Cargo.toml" => Some("cargo_toml"),
        _ => None,
    }
}

/// Build a `FileAnalysis` whose declarations are the typed manifest facts.
/// `path` is the repo-relative manifest path; `source` the current bytes.
pub fn analyze(path: &str, source: &[u8]) -> crate::model::FileAnalysis {
    let mut b = FactBuilder::new(path, crate::model::LanguageId::Manifest, source);
    let file_range = b.range_from_offsets(0, source.len() as u32);
    b.push_scope(ScopeKind::File, None::<String>, file_range, None);
    let text = String::from_utf8_lossy(source);
    match manifest_kind(path) {
        Some("go_mod") => go_mod(&mut b, &text),
        Some("cargo_toml") => cargo(&mut b, &text),
        _ => {}
    }
    b.pop_scope();
    b.finish()
}

fn decl(b: &mut FactBuilder, kind: DeclarationKind, name: &str, line: usize, text: &str) {
    // byte offset of the start of `line` (1-based) in `text`
    let mut off = 0u32;
    for (i, l) in text.split_inclusive('\n').enumerate() {
        if i == line {
            break;
        }
        off += l.len() as u32;
    }
    let end = off + text.lines().nth(line).map(|l| l.len() as u32).unwrap_or(0);
    let range = b.range_from_offsets(off, end.max(off));
    b.push_declaration(DeclarationDraft {
        kind,
        name: name.to_string(),
        name_range: range,
        range,
        body_range: None,
        flags: Vec::new(),
    });
}

// ---- go.mod ----

fn go_mod(b: &mut FactBuilder, text: &str) {
    let mut in_require = false;
    for (ln, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if line == "(" {
            continue;
        }
        if line == ")" {
            in_require = false;
            continue;
        }
        if line.starts_with("require (") || line == "require(" {
            in_require = true;
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if let Some(rest) = line.strip_prefix("module ") {
            decl(
                b,
                DeclarationKind::Module,
                &format!("go module {}", rest.trim()),
                ln,
                text,
            );
        } else if let Some(rest) = line.strip_prefix("go ") {
            decl(
                b,
                DeclarationKind::Package,
                &format!("go version {}", rest.trim()),
                ln,
                text,
            );
        } else if let Some(rest) = line.strip_prefix("toolchain ") {
            decl(
                b,
                DeclarationKind::Package,
                &format!("go toolchain {}", rest.trim()),
                ln,
                text,
            );
        } else if line.starts_with("require ") || (in_require && parts.len() >= 2) {
            let kv = if line.starts_with("require ") {
                line.trim_start_matches("require ").trim()
            } else {
                line.trim()
            };
            let name = kv.split_whitespace().next().unwrap_or("");
            if !name.is_empty() {
                decl(
                    b,
                    DeclarationKind::Package,
                    &format!("go require {}", kv),
                    ln,
                    text,
                );
            }
        } else if let Some(rest) = line.strip_prefix("replace ") {
            decl(
                b,
                DeclarationKind::Package,
                &format!("go replace {}", rest.trim()),
                ln,
                text,
            );
        } else if let Some(rest) = line.strip_prefix("exclude ") {
            decl(
                b,
                DeclarationKind::Package,
                &format!("go exclude {}", rest.trim()),
                ln,
                text,
            );
        } else if let Some(rest) = line.strip_prefix("retract ") {
            decl(
                b,
                DeclarationKind::Package,
                &format!("go retract {}", rest.trim()),
                ln,
                text,
            );
        }
    }
}

// ---- Cargo.toml ----

fn cargo(b: &mut FactBuilder, text: &str) {
    let mut section = String::new();
    for (ln, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').to_string();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let k = k.trim();
        let v = v.trim().trim_matches('"').to_string();
        match section.as_str() {
            "package" => match k {
                "name" => decl(
                    b,
                    DeclarationKind::Package,
                    &format!("cargo package {}", v),
                    ln,
                    text,
                ),
                "version" => decl(
                    b,
                    DeclarationKind::Package,
                    &format!("cargo version {}", v),
                    ln,
                    text,
                ),
                "edition" => decl(
                    b,
                    DeclarationKind::Package,
                    &format!("cargo edition {}", v),
                    ln,
                    text,
                ),
                "rust-version" => decl(
                    b,
                    DeclarationKind::Package,
                    &format!("cargo rust-version {}", v),
                    ln,
                    text,
                ),
                _ => {}
            },
            "workspace" => match k {
                "members" => decl(
                    b,
                    DeclarationKind::Module,
                    &format!("cargo workspace members {}", v),
                    ln,
                    text,
                ),
                "exclude" => decl(
                    b,
                    DeclarationKind::Package,
                    &format!("cargo workspace exclude {}", v),
                    ln,
                    text,
                ),
                _ => {}
            },
            s if s == "dependencies" || s == "dev-dependencies" || s == "build-dependencies" => {
                let dtype = s.strip_suffix("-dependencies").unwrap_or(s);
                decl(
                    b,
                    DeclarationKind::Package,
                    &format!("cargo {} {}", dtype, k),
                    ln,
                    text,
                );
            }
            _ => {}
        }
    }
}

/// Repository-native deterministic vocabulary bridge (§18-§26). A role term
/// (crate/module/package/…) expands to the *concrete* names this repository's
/// manifests actually declare — the alias value is repository-derived, never a
/// global synonym. DERIVED query terms only; never FACT. <=4/term, one layer.
pub fn native_aliases(terms: &[String], manifest_terms: &[String]) -> Vec<String> {
    // Bounded domain role vocabulary — the *value* substituted comes from the
    // repository's own manifest terms, not a fixed synonym.
    const ROLE: &[&str] = &[
        "crate",
        "package",
        "module",
        "manifest",
        "dependency",
        "dependencies",
        "workspace",
        "edition",
        "version",
    ];
    let mut out = Vec::new();
    for t in terms {
        if ROLE.contains(&t.as_str()) {
            for mt in manifest_terms.iter().take(4) {
                if !out.contains(mt) {
                    out.push(mt.clone());
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out.truncate(12);
    out
}
