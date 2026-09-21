//! TASK 4A: Go package & module topology.
//!
//! The structural answer to "which Go package is this file in, and which Go
//! module owns that package?". Built from persisted `package` clauses plus
//! repository-local `go.mod` files — never `go list`, `go env`, a compiler, or
//! a module-cache lookup.
//!
//! ```text
//! RepositoryFactSnapshot
//!         +
//! go.mod bytes (content-digest-addressed)
//!         ↓
//! GoPackageTopology
//!         ├── GoModule   one per discovered go.mod
//!         ├── GoPackage  one per (directory, package_name) clause group
//!         └── membership file -> package
//! ```
//!
//! Package identity is `module-context + directory + package_name`, never the
//! package name alone (two `package util` directories are distinct). Nested
//! `go.mod` files are independent module boundaries; a file belongs to the
//! nearest enclosing module root and may be `NoKnownModule`. `package main`
//! and `package <x>_test` are distinct kinds — an external-test package never
//! gains same-package visibility into its underlying package.

use std::collections::BTreeMap;
use std::path::Path;

use crate::model::FileAnalysis;
use crate::repository::digest;
use crate::scanner::{walk_files, ScanOptions};

use super::metadata::parse_module_directive;
use super::model::{MetadataDependency, StructuralEntity};
use super::structure::{directory_of, Structure};

/// The structural kind string for a Go module entity.
pub const GO_MODULE_KIND: &str = "go_module";

/// A repository-local `go.mod` reference, content-addressed.
#[derive(Debug, Clone)]
pub struct GoManifestRef {
    /// Repository-relative `go.mod` path.
    pub relative_path: String,
    /// Content digest (present/absent/malformed distinction).
    pub content_digest: String,
    /// The `module` directive value, when readable.
    pub module_path: Option<String>,
    /// Why the `module` directive could not be read, when it could not.
    pub malformed_reason: Option<String>,
}

/// A discovered Go module — one per supported `go.mod`.
#[derive(Debug, Clone)]
pub struct GoModule {
    /// Stable identity: the repository-relative module root directory (`""` for
    /// the repo root). Checkout-independent.
    pub module_id: String,
    /// Repository-relative `go.mod` path.
    pub manifest_path: String,
    /// Repository-relative directory containing the `go.mod`.
    pub root_dir: String,
    /// The `module` directive path (e.g. `example.com/acme/project`).
    pub module_path: String,
    /// Content digest of the `go.mod`.
    pub manifest_digest: String,
    /// Discovery rule.
    pub rule_id: String,
}

/// The structural kind of a Go package (§6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GoPackageKind {
    /// A normal `package foo`.
    Ordinary,
    /// `package main` — a command package.
    CommandMain,
    /// `package foo_test` — the external-test package for `foo`.
    ExternalTest,
}

impl GoPackageKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            GoPackageKind::Ordinary => "ordinary",
            GoPackageKind::CommandMain => "command_main",
            GoPackageKind::ExternalTest => "external_test",
        }
    }
}

/// A Go package — a `(directory, package_name)` group of indexed files.
#[derive(Debug, Clone)]
pub struct GoPackage {
    /// Stable identity: `{module_root_dir or "."}/{directory}:{name}` —
    /// module context + directory + package name, never the name alone.
    pub package_id: String,
    /// Repository-relative directory holding the files.
    pub directory: String,
    /// The written `package` name.
    pub name: String,
    /// Structural package kind.
    pub kind: GoPackageKind,
    /// Owning module, when a `go.mod` encloses the directory.
    pub module_id: Option<String>,
    /// Package directory relative to the module root (`""` at the root).
    pub module_rel_dir: Option<String>,
    /// Derived import path (module path + module-relative dir); `None` for
    /// external-test packages and packages with no known module.
    pub import_path: Option<String>,
    /// Member files (`file -> role` is derivable: `_test.go` = test).
    pub files: Vec<String>,
    /// Discovery rule.
    pub rule_id: String,
}

/// The derived Go package/module topology.
#[derive(Debug, Default)]
pub struct GoPackageTopology {
    pub modules: Vec<GoModule>,
    pub packages: Vec<GoPackage>,
    /// file -> package_id (a file has exactly one `package` clause).
    pub memberships: BTreeMap<String, String>,
    /// Diagnostics for malformed manifests / ambiguous directories.
    pub diagnostics: Vec<String>,
    /// Every `go.mod` consulted — for content-digest metadata dependencies.
    pub manifests: Vec<GoManifestRef>,
}

impl GoPackageTopology {
    /// Build the topology. `repository` supplies the `go.mod` files; the
    /// `structure` supplies the persisted `package` clause groupings.
    pub fn build(
        structure: &Structure,
        analyses: &[FileAnalysis],
        repository: Option<&Path>,
    ) -> Self {
        let mut topology = GoPackageTopology::default();
        let Some(root) = repository else {
            return topology;
        };

        // 1. Discover every repository-local `go.mod`.
        let manifest_paths = discover_go_mods(root, &mut topology.diagnostics);
        for relative in manifest_paths {
            let bytes = match std::fs::read(root.join(&relative)) {
                Ok(b) => b,
                Err(e) => {
                    topology
                        .diagnostics
                        .push(format!("`{relative}` could not be read: {e}"));
                    continue;
                }
            };
            let digest = digest::content_digest(&bytes);
            let module_path = std::str::from_utf8(&bytes)
                .ok()
                .and_then(parse_module_directive);
            let malformed_reason = module_path
                .is_none()
                .then(|| "no readable `module` directive".to_string());
            topology.manifests.push(GoManifestRef {
                relative_path: relative.clone(),
                content_digest: digest.clone(),
                module_path: module_path.clone(),
                malformed_reason: malformed_reason.clone(),
            });
            if let Some(module_path) = module_path {
                let root_dir = directory_of(&relative);
                topology.modules.push(GoModule {
                    module_id: module_id_of(&root_dir),
                    manifest_path: relative,
                    root_dir,
                    module_path,
                    manifest_digest: digest,
                    rule_id: "go.module.go_mod".to_string(),
                });
            } else {
                topology
                    .diagnostics
                    .push(format!("`{relative}` has no readable `module` directive"));
            }
        }
        topology
            .modules
            .sort_by(|a, b| a.module_id.cmp(&b.module_id));

        // 2. One package per (directory, package_name) clause group. A file's
        //    package clause determines its package; `_test.go` is only a file
        //    role, and `package foo_test` is a distinct external-test package.
        for (directory, names) in &structure.go_packages {
            for (name, files) in names {
                let kind = if name == "main" {
                    GoPackageKind::CommandMain
                } else if name.ends_with("_test") {
                    GoPackageKind::ExternalTest
                } else {
                    GoPackageKind::Ordinary
                };
                // Nearest enclosing `go.mod` directory — valid *or* malformed —
                // is the module boundary. A malformed `go.mod` still excludes
                // its subtree from the parent module (the files become
                // `NoKnownModule`, not parent members).
                let owner = {
                    let boundary_dir = topology
                        .manifests
                        .iter()
                        .map(|m| directory_of(&m.relative_path))
                        .filter(|d| dir_within(directory, d))
                        .max_by_key(|d| d.len());
                    boundary_dir.and_then(|bd| topology.modules.iter().find(|m| m.root_dir == bd))
                };
                let (module_id, module_rel_dir, import_path) = match owner {
                    Some(m) => {
                        let rel = strip_dir_prefix(directory, &m.root_dir);
                        let import_path = match kind {
                            // An external-test package gets no fabricated
                            // canonical import path (§17).
                            GoPackageKind::ExternalTest => None,
                            _ => Some(if rel.is_empty() {
                                m.module_path.clone()
                            } else {
                                format!("{}/{}", m.module_path, rel)
                            }),
                        };
                        (Some(m.module_id.clone()), Some(rel), import_path)
                    }
                    None => (None, None, None),
                };
                if matches!(kind, GoPackageKind::Ordinary) && names.len() > 1 {
                    // A directory declaring multiple non-test package names is
                    // structurally ambiguous — preserved, not repaired (§9).
                    topology.diagnostics.push(format!(
                        "directory `{directory}` declares multiple package names"
                    ));
                }
                let mut member_files = files.clone();
                member_files.sort();
                topology.packages.push(GoPackage {
                    package_id: package_id_of(module_id.as_deref(), directory, name, kind),
                    directory: directory.clone(),
                    name: name.clone(),
                    kind,
                    module_id,
                    module_rel_dir,
                    import_path,
                    files: member_files,
                    rule_id: match kind {
                        GoPackageKind::Ordinary => "go.package.directory_clause",
                        GoPackageKind::CommandMain => "go.package.command_main",
                        GoPackageKind::ExternalTest => "go.package.external_test",
                    }
                    .to_string(),
                });
            }
        }
        topology
            .packages
            .sort_by(|a, b| a.package_id.cmp(&b.package_id));

        // 3. file -> package membership.
        for package in &topology.packages {
            for file in &package.files {
                topology
                    .memberships
                    .insert(file.clone(), package.package_id.clone());
            }
        }
        let _ = analyses;
        topology
    }

    /// One `go_module` entity per discovered module. (Package membership is
    /// carried by the existing `go_package` entities, enriched by
    /// [`Self::enrich_package_entities`].)
    pub fn entities(&self) -> Vec<StructuralEntity> {
        let mut out = Vec::new();
        for m in &self.modules {
            out.push(StructuralEntity {
                entity_id: StructuralEntity::derive_id(GO_MODULE_KIND, &m.module_id),
                structural_kind: GO_MODULE_KIND.to_string(),
                language: "go".to_string(),
                key: m.module_id.clone(),
                // The `go.mod` itself is a metadata file, not an analyzed
                // source file, so it cannot be a `file` member; module
                // membership flows through package membership.
                files: Vec::new(),
                assumptions: vec![
                    format!("module_path={}", m.module_path),
                    format!("manifest={}", m.manifest_path),
                    format!("manifest_digest={}", m.manifest_digest),
                    format!("root_dir={}", m.root_dir),
                    format!("rule={}", m.rule_id),
                    "module boundary is nearest enclosing go.mod, not repo root".to_string(),
                ],
            });
        }
        out
    }

    /// Enrich the existing `go_package` structural entities (keyed
    /// `dir:name`, used by `go.package.same_directory` /
    /// `go.import.local_module`) with the TASK 4A topology provenance —
    /// package kind, owning module, and derived import path. Entity ids and
    /// membership are unchanged, so existing link targets stay valid.
    pub fn enrich_package_entities(&self, entities: &mut [StructuralEntity]) {
        let by_key: BTreeMap<(&str, &str), &GoPackage> = self
            .packages
            .iter()
            .map(|p| ((p.directory.as_str(), p.name.as_str()), p))
            .collect();
        for entity in entities.iter_mut() {
            if entity.structural_kind != "go_package" {
                continue;
            }
            // `go_package` key is `directory:name` (`:name` at the root).
            let Some(colon) = entity.key.rfind(':') else {
                continue;
            };
            let (directory, name) = (&entity.key[..colon], &entity.key[colon + 1..]);
            let Some(pkg) = by_key.get(&(directory, name)) else {
                continue;
            };
            entity.assumptions.extend([
                format!("package_kind={}", pkg.kind.as_str()),
                format!(
                    "module={}",
                    pkg.module_id.clone().unwrap_or_else(|| "none".into())
                ),
                format!(
                    "module_rel_dir={}",
                    pkg.module_rel_dir.clone().unwrap_or_default()
                ),
                format!(
                    "import_path={}",
                    pkg.import_path.clone().unwrap_or_else(|| "none".into())
                ),
                format!("topology_rule={}", pkg.rule_id),
            ]);
        }
    }

    /// Content-digest dependencies for every consulted `go.mod`, so a manifest
    /// change invalidates the derived artifact.
    pub fn metadata_dependencies(&self) -> Vec<MetadataDependency> {
        self.manifests
            .iter()
            .map(|m| MetadataDependency {
                relative_path: m.relative_path.clone(),
                content_digest: m.content_digest.clone(),
                present: true,
                rule_id: "go.module.go_mod".to_string(),
                field: "module".to_string(),
                value: m.module_path.clone().unwrap_or_else(|| {
                    format!(
                        "malformed: {}",
                        m.malformed_reason.clone().unwrap_or_default()
                    )
                }),
            })
            .collect()
    }
}

/// Discover every `go.mod` under `root`, repo-relative and sorted.
fn discover_go_mods(root: &Path, diagnostics: &mut Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let options = ScanOptions {
        respect_gitignore: true,
    };
    let (paths, _failures) = match walk_files(root, options) {
        Ok(result) => result,
        Err(error) => {
            diagnostics.push(format!("repository walk failed: {error}"));
            return out;
        }
    };
    for abs in paths {
        let Ok(rel) = abs.strip_prefix(root) else {
            continue;
        };
        if rel.file_name().and_then(|n: &std::ffi::OsStr| n.to_str()) != Some("go.mod") {
            continue;
        }
        if let Ok(r) = std::fs::canonicalize(&abs) {
            if !r.starts_with(std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())) {
                diagnostics.push(format!("`{}` escapes the repository root", rel.display()));
                continue;
            }
        }
        out.push(rel.to_string_lossy().replace('\\', "/"));
    }
    out.sort();
    out
}

/// A module's stable id: its root directory (`""` -> `<root>` for the repo
/// root) — deterministic and checkout-independent.
fn module_id_of(root_dir: &str) -> String {
    if root_dir.is_empty() {
        "<root>".to_string()
    } else {
        root_dir.to_string()
    }
}

/// A package's stable id: module context + directory + name (+ kind so an
/// external-test package never collides with its ordinary package).
fn package_id_of(
    module_id: Option<&str>,
    directory: &str,
    name: &str,
    kind: GoPackageKind,
) -> String {
    let module = module_id.unwrap_or("nomodule");
    let dir = if directory.is_empty() { "." } else { directory };
    match kind {
        GoPackageKind::ExternalTest => format!("{module}@{dir}:{name}"),
        _ => format!("{module}@{dir}:{name}"),
    }
}

/// Whether `child` directory is `parent` or a descendant of it.
fn dir_within(child: &str, parent: &str) -> bool {
    if parent.is_empty() {
        return true; // repo-root module encloses everything not under a nested module
    }
    child == parent || child.starts_with(&format!("{parent}/"))
}

/// `child` relative to enclosing `parent` dir (`""` when equal).
fn strip_dir_prefix(child: &str, parent: &str) -> String {
    if parent.is_empty() {
        return child.to_string();
    }
    child
        .strip_prefix(&format!("{parent}/"))
        .unwrap_or(child)
        .to_string()
}
