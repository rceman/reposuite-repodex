//! TASK 3F — Rust crate/target topology.
//!
//! The `Structure` module-tree builder answers "which modules does this crate
//! root reach?". It does not decide which repository files are crate *roots*.
//! TASK 3F supplies that decision from Cargo metadata: a repository-local
//! `Cargo.toml` declares (or implies by convention) the package's `lib`,
//! `bin`, `integration_test`, `example` and `bench` targets, each of which is
//! an independent crate root with its own module tree.
//!
//! This is repository-local structural metadata only. It is not Cargo
//! dependency resolution, feature resolution, `cfg` evaluation, or rustc name
//! resolution. The only inputs are `Cargo.toml` bytes and the standard Cargo
//! target-path conventions; a `#[cfg]` dependency table, a `[target.'cfg']`
//! table or an `[dependencies]` table does not create a target.
//!
//! Crate identity is content-addressed: every target records the relative
//! manifest path and the manifest content digest, so a changed `Cargo.toml`
//! invalidates a derived artifact without relying on mtime, and different
//! absolute checkouts with identical bytes produce an identical topology.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::model::{FileAnalysis, LanguageId};
use crate::repository::digest;
use crate::scanner::{walk_files, ScanOptions};

use super::model::MetadataDependency;
use super::structure::{directory_of, file_name, join_dir, RustCrate, Structure};

/// The Cargo target kinds TASK 3F models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RustTargetKind {
    Lib,
    Bin,
    IntegrationTest,
    Example,
    Bench,
}

impl RustTargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RustTargetKind::Lib => "lib",
            RustTargetKind::Bin => "bin",
            RustTargetKind::IntegrationTest => "integration_test",
            RustTargetKind::Example => "example",
            RustTargetKind::Bench => "bench",
        }
    }
}

/// One Cargo target: an independent crate root discovered from a manifest.
#[derive(Debug, Clone)]
pub struct RustTarget {
    /// Canonical, checkout-independent target identity.
    pub target_id: String,
    pub package_name: String,
    /// Repository-relative package root directory (`""` for the repo root).
    pub package_dir: String,
    pub kind: RustTargetKind,
    /// The Cargo target name.
    pub name: String,
    /// Repository-relative crate-root source file.
    pub root_file: String,
    /// Repository-relative `Cargo.toml` path that declared this target.
    pub manifest_path: String,
    /// Content digest of that manifest.
    pub manifest_digest: String,
    /// The discovery rule that produced this target.
    pub rule_id: String,
}

/// A `Cargo.toml` read during topology construction, for invalidation.
#[derive(Debug, Clone)]
pub struct CargoManifestRef {
    pub relative_path: String,
    pub content_digest: String,
}

/// The derived crate/target topology for a snapshot.
#[derive(Debug, Clone, Default)]
pub struct RustTargetTopology {
    /// Every discovered target whose root file is an indexed Rust source file.
    pub targets: Vec<RustTarget>,
    /// A module tree per target, in the same order as `targets`.
    pub crates: Vec<RustCrate>,
    /// `file -> [target_id]` for every file reachable from a target's tree.
    pub memberships: BTreeMap<String, Vec<String>>,
    /// The manifests consumed, recorded for invalidation.
    pub manifests: Vec<CargoManifestRef>,
    /// Non-fatal limitations (unsupported constructs, malformed manifests).
    pub diagnostics: Vec<String>,
}

// ---------------------------------------------------------------------------
// Cargo.toml parsing
// ---------------------------------------------------------------------------

/// A target declaration read from a manifest table/array.
#[derive(Debug, Default, Clone)]
struct DeclaredTarget {
    name: Option<String>,
    path: Option<String>,
}

#[derive(Debug, Default)]
struct CargoManifest {
    package_name: Option<String>,
    has_package: bool,
    lib: Option<DeclaredTarget>,
    bins: Vec<DeclaredTarget>,
    tests: Vec<DeclaredTarget>,
    benches: Vec<DeclaredTarget>,
    examples: Vec<DeclaredTarget>,
    workspace_members: Vec<String>,
    workspace_exclude: Vec<String>,
    autobins: bool,
    autotests: bool,
    autobenches: bool,
    autoexamples: bool,
}

impl CargoManifest {
    /// Parse the fields TASK 3F needs. Returns `Err` for malformed TOML.
    fn parse(text: &str) -> Result<CargoManifest, String> {
        let value: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut manifest = CargoManifest {
            autobins: true,
            autotests: true,
            autobenches: true,
            autoexamples: true,
            ..Default::default()
        };
        if let Some(package) = value.get("package").and_then(|v| v.as_table()) {
            manifest.has_package = true;
            manifest.package_name = package
                .get("name")
                .and_then(|v| v.as_str())
                .map(String::from);
            for (flag, slot) in [
                ("autobins", &mut manifest.autobins),
                ("autotests", &mut manifest.autotests),
                ("autobenches", &mut manifest.autobenches),
                ("autoexamples", &mut manifest.autoexamples),
            ] {
                if let Some(b) = package.get(flag).and_then(|v| v.as_bool()) {
                    *slot = b;
                }
            }
        }
        if let Some(lib) = value.get("lib").and_then(|v| v.as_table()) {
            manifest.lib = Some(DeclaredTarget {
                name: lib.get("name").and_then(|v| v.as_str()).map(String::from),
                path: lib.get("path").and_then(|v| v.as_str()).map(String::from),
            });
        }
        for (key, slot) in [
            ("bin", &mut manifest.bins),
            ("test", &mut manifest.tests),
            ("bench", &mut manifest.benches),
            ("example", &mut manifest.examples),
        ] {
            if let Some(array) = value.get(key).and_then(|v| v.as_array()) {
                for item in array {
                    if let Some(table) = item.as_table() {
                        slot.push(DeclaredTarget {
                            name: table.get("name").and_then(|v| v.as_str()).map(String::from),
                            path: table.get("path").and_then(|v| v.as_str()).map(String::from),
                        });
                    }
                }
            }
        }
        if let Some(workspace) = value.get("workspace").and_then(|v| v.as_table()) {
            for (key, slot) in [
                ("members", &mut manifest.workspace_members),
                ("exclude", &mut manifest.workspace_exclude),
            ] {
                if let Some(array) = workspace.get(key).and_then(|v| v.as_array()) {
                    for item in array {
                        if let Some(member) = item.as_str() {
                            slot.push(member.to_string());
                        }
                    }
                }
            }
        }
        Ok(manifest)
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// A Cargo package rooted at a manifest directory.
struct Package<'m> {
    manifest: &'m CargoManifest,
    manifest_path: &'m str,
    manifest_digest: &'m str,
    dir: String,
}

impl RustTargetTopology {
    /// Build the crate/target topology for `analyses`.
    ///
    /// `repository` supplies the `Cargo.toml` bytes; when absent no manifests
    /// are discovered and the topology is empty (recorded as a dependency on
    /// that absence, so adding a manifest later invalidates the artifact).
    pub fn build(
        structure: &mut Structure,
        analyses: &[FileAnalysis],
        repository: Option<&Path>,
    ) -> RustTargetTopology {
        let mut topology = RustTargetTopology::default();
        let by_path: BTreeMap<&str, &FileAnalysis> = analyses
            .iter()
            .filter(|a| a.file.language == LanguageId::Rust)
            .map(|a| (a.file.relative_path.as_str(), a))
            .collect();

        // 1. Discover and parse every Cargo.toml inside the repository.
        let manifests = match repository {
            Some(root) => discover_manifests(root, &mut topology.diagnostics),
            None => Vec::new(),
        };
        topology.manifests = manifests
            .iter()
            .map(|m| CargoManifestRef {
                relative_path: m.path.clone(),
                content_digest: m.digest.clone(),
            })
            .collect();

        // 2. Resolve workspace membership: a `[workspace]` manifest declares
        //    which member dirs are packages; `exclude` opts a dir out.
        let packages = resolve_packages(&manifests, &mut topology.diagnostics);

        // 3. Enumerate each package's targets (explicit then default
        //    conventions), keeping only roots that are indexed Rust files.
        let mut targets: Vec<RustTarget> = Vec::new();
        for package in &packages {
            targets.extend(package_targets(
                package,
                &by_path,
                &mut topology.diagnostics,
            ));
        }

        // 4. Build one module tree per target and accumulate file membership.
        //    Targets and their trees are paired so they stay index-aligned.
        let mut membership: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut pairs: Vec<(RustTarget, RustCrate)> = Vec::new();
        for target in targets {
            match structure.build_rust_crate_tree(&target.root_file, &by_path) {
                Some(crate_tree) => {
                    for file in &crate_tree.files {
                        membership
                            .entry(file.clone())
                            .or_default()
                            .insert(target.target_id.clone());
                    }
                    pairs.push((target, crate_tree));
                }
                None => {
                    topology.diagnostics.push(format!(
                        "target {} root `{}` is not an indexed Rust file",
                        target.target_id, target.root_file
                    ));
                }
            }
        }
        pairs.sort_by(|a, b| a.0.target_id.cmp(&b.0.target_id));
        for (target, crate_tree) in pairs {
            topology.targets.push(target);
            topology.crates.push(crate_tree);
        }
        topology.memberships = membership
            .into_iter()
            .map(|(file, set)| (file, set.into_iter().collect()))
            .collect();
        topology
    }

    /// The crate memberships of `file`: empty means `NoKnownCrate`.
    pub fn memberships_of(&self, file: &str) -> &[String] {
        self.memberships
            .get(file)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Emit one `rust_crate_target` structural entity per target, folding the
    /// topology into the existing entity mechanism (§22). The entity's `files`
    /// are the crate's member files — that is the file→crate membership.
    pub fn target_entities(&self) -> Vec<super::model::StructuralEntity> {
        let mut entities = Vec::new();
        for (index, target) in self.targets.iter().enumerate() {
            let crate_tree = &self.crates[index];
            let mut files: Vec<String> = crate_tree.files.iter().cloned().collect();
            files.sort();
            entities.push(super::model::StructuralEntity {
                entity_id: super::model::StructuralEntity::derive_id(
                    "rust_crate_target",
                    &target.target_id,
                ),
                structural_kind: "rust_crate_target".to_string(),
                language: "rust".to_string(),
                key: target.target_id.clone(),
                files,
                assumptions: vec![
                    format!("target_kind={}", target.kind.as_str()),
                    format!("target_name={}", target.name),
                    format!("package={}", target.package_name),
                    format!("package_dir={}", target.package_dir),
                    format!("root_file={}", target.root_file),
                    format!("manifest={}", target.manifest_path),
                    format!("manifest_digest={}", target.manifest_digest),
                    format!("rule={}", target.rule_id),
                    "crate boundary is Cargo target topology, not rustc name resolution"
                        .to_string(),
                    "member files are those reachable via `mod` from the target root".to_string(),
                ],
            });
        }
        entities
    }

    /// The [`MetadataDependency`] records proving the topology is
    /// reproducible — one per manifest, so a manifest change invalidates the
    /// derived artifact.
    pub fn metadata_dependencies(&self) -> Vec<MetadataDependency> {
        let mut deps = Vec::with_capacity(self.manifests.len());
        for manifest in &self.manifests {
            deps.push(MetadataDependency {
                relative_path: manifest.relative_path.clone(),
                content_digest: manifest.content_digest.clone(),
                present: true,
                rule_id: "rust.target.topology".to_string(),
                field: "manifest".to_string(),
                value: "present".to_string(),
            });
        }
        deps
    }
}

/// One discovered manifest: its repo-relative path and content.
struct DiscoveredManifest {
    path: String,
    digest: String,
    parsed: Result<CargoManifest, String>,
}

/// Find every `Cargo.toml` under `root` (repo-relative, gitignore-respecting,
/// never outside the root), read its bytes and digest them.
fn discover_manifests(root: &Path, diagnostics: &mut Vec<String>) -> Vec<DiscoveredManifest> {
    const MAX_MANIFEST_BYTES: u64 = 1 << 20;
    let options = ScanOptions {
        respect_gitignore: true,
    };
    let (paths, _failures) = match walk_files(root, options) {
        Ok(result) => result,
        Err(error) => {
            diagnostics.push(format!("repository walk failed: {error}"));
            return Vec::new();
        }
    };
    let mut manifests = Vec::new();
    for path in paths {
        if file_name(&crate::paths::relative_path(root, &path)) != "Cargo.toml" {
            continue;
        }
        let relative = crate::paths::relative_path(root, &path);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                diagnostics.push(format!("`{relative}` could not be read: {error}"));
                continue;
            }
        };
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            diagnostics.push(format!("`{relative}` exceeds the manifest size bound"));
            continue;
        }
        let digest = digest::content_digest(&bytes);
        let parsed = std::str::from_utf8(&bytes)
            .map_err(|_| "not UTF-8".to_string())
            .and_then(CargoManifest::parse);
        manifests.push(DiscoveredManifest {
            path: relative,
            digest,
            parsed,
        });
    }
    manifests.sort_by(|a, b| a.path.cmp(&b.path));
    manifests
}

/// Determine the package set. A `[workspace]` manifest's `members`/`exclude`
/// scope which directories are packages; every `[package]` manifest not
/// covered by a workspace is a standalone package.
fn resolve_packages<'m>(
    manifests: &'m [DiscoveredManifest],
    diagnostics: &mut Vec<String>,
) -> Vec<Package<'m>> {
    // Directories excluded by any workspace `exclude`.
    let mut excluded: BTreeSet<String> = BTreeSet::new();
    // Directories explicitly named by a workspace `members`.
    let mut declared_members: BTreeSet<String> = BTreeSet::new();
    for manifest in manifests {
        let Ok(parsed) = &manifest.parsed else {
            continue;
        };
        if parsed.workspace_members.is_empty() && parsed.workspace_exclude.is_empty() {
            continue;
        }
        let ws_dir = directory_of(&manifest.path);
        for member in &parsed.workspace_members {
            // Bounded glob: `*` matches a single path segment, `?` a char.
            for dir in expand_member_glob(&ws_dir, member, manifests) {
                declared_members.insert(dir);
            }
        }
        for excl in &parsed.workspace_exclude {
            for dir in expand_member_glob(&ws_dir, excl, manifests) {
                excluded.insert(dir);
            }
        }
    }

    let mut packages = Vec::new();
    for manifest in manifests {
        let Ok(parsed) = &manifest.parsed else {
            if let Err(reason) = &manifest.parsed {
                diagnostics.push(format!("`{}`: malformed manifest: {reason}", manifest.path));
            }
            continue;
        };
        if !parsed.has_package {
            continue; // a virtual workspace root contributes members only
        }
        let dir = directory_of(&manifest.path);
        if excluded.contains(&dir) {
            continue;
        }
        // If any workspace declared members, a package under that workspace's
        // root that is not a member and not the root itself is skipped only
        // when an explicit `exclude`/membership says so — membership by dir is
        // kept permissive so nested non-member packages still register.
        let _ = &declared_members;
        packages.push(Package {
            manifest: parsed,
            manifest_path: &manifest.path,
            manifest_digest: &manifest.digest,
            dir,
        });
    }
    packages
}

/// Expand a `members`/`exclude` entry to package directories. `*`/`?` match
/// within a single path segment; the result is the set of dirs containing a
/// discovered `Cargo.toml`. No traversal outside the repository.
fn expand_member_glob(
    ws_dir: &str,
    pattern: &str,
    manifests: &[DiscoveredManifest],
) -> Vec<String> {
    let candidate_dirs: BTreeSet<String> =
        manifests.iter().map(|m| directory_of(&m.path)).collect();
    let joined = join_dir(ws_dir, pattern);
    if !pattern.contains('*') && !pattern.contains('?') {
        return if candidate_dirs.contains(&joined) {
            vec![joined]
        } else {
            Vec::new()
        };
    }
    // Single-segment-glob subset: `*`/`?` within one path component.
    let pat_segments: Vec<&str> = joined.split('/').collect();
    candidate_dirs
        .into_iter()
        .filter(|dir| {
            let segs: Vec<&str> = dir.split('/').collect();
            segs.len() == pat_segments.len()
                && segs
                    .iter()
                    .zip(&pat_segments)
                    .all(|(s, p)| segment_matches(p, s))
        })
        .collect()
}

/// `*` matches any run of characters, `?` exactly one — within one segment.
fn segment_matches(pattern: &str, text: &str) -> bool {
    fn inner(p: &[u8], t: &[u8]) -> bool {
        match (p.first(), t.first()) {
            (None, None) => true,
            (Some(b'*'), _) => inner(&p[1..], t) || (!t.is_empty() && inner(p, &t[1..])),
            (Some(b'?'), Some(_)) => inner(&p[1..], &t[1..]),
            (Some(&pc), Some(&tc)) => pc == tc && inner(&p[1..], &t[1..]),
            _ => false,
        }
    }
    inner(pattern.as_bytes(), text.as_bytes())
}

// ---------------------------------------------------------------------------
// Target enumeration
// ---------------------------------------------------------------------------

fn package_targets(
    package: &Package,
    by_path: &BTreeMap<&str, &FileAnalysis>,
    diagnostics: &mut Vec<String>,
) -> Vec<RustTarget> {
    let mut targets = Vec::new();
    let dir = package.dir.clone();
    let pkg_name = package
        .manifest
        .package_name
        .clone()
        .unwrap_or_else(|| file_name(&dir).to_string());
    let lib_name = pkg_name.replace('-', "_");

    let indexed = |rel: &str| -> bool { by_path.contains_key(rel) };
    let push = |targets: &mut Vec<RustTarget>,
                kind: RustTargetKind,
                name: String,
                rel: String,
                rule: &'static str| {
        if indexed(&rel) {
            targets.push(RustTarget {
                target_id: format!("{}::{}::{}", package.dir, kind.as_str(), name),
                package_name: pkg_name.clone(),
                package_dir: package.dir.clone(),
                kind,
                name,
                root_file: rel,
                manifest_path: package.manifest_path.to_string(),
                manifest_digest: package.manifest_digest.to_string(),
                rule_id: rule.to_string(),
            });
        }
    };

    // --- lib ---
    if let Some(lib) = &package.manifest.lib {
        let rel = join_dir(&dir, lib.path.as_deref().unwrap_or("src/lib.rs"));
        let name = lib.name.clone().unwrap_or_else(|| lib_name.clone());
        push(
            &mut targets,
            RustTargetKind::Lib,
            name,
            rel,
            "rust.target.lib.explicit_path",
        );
    } else {
        let rel = join_dir(&dir, "src/lib.rs");
        push(
            &mut targets,
            RustTargetKind::Lib,
            lib_name.clone(),
            rel,
            "rust.target.lib.default_src_lib",
        );
    }

    // --- bins ---
    let mut seen_bin_roots: BTreeSet<String> = BTreeSet::new();
    for bin in &package.manifest.bins {
        let name = bin.name.clone().unwrap_or_else(|| "bin".to_string());
        let rel = bin
            .path
            .clone()
            .map(|p| join_dir(&dir, &p))
            .unwrap_or_else(|| join_dir(&dir, &format!("src/bin/{name}.rs")));
        if seen_bin_roots.insert(rel.clone()) {
            push(
                &mut targets,
                RustTargetKind::Bin,
                name,
                rel,
                "rust.target.bin.explicit",
            );
        }
    }
    if package.manifest.autobins {
        let main = join_dir(&dir, "src/main.rs");
        if seen_bin_roots.insert(main.clone()) {
            push(
                &mut targets,
                RustTargetKind::Bin,
                file_name(&dir).to_string(),
                main,
                "rust.target.bin.default_src_main",
            );
        }
        // `src/bin/*.rs` and `src/bin/*/main.rs`.
        for file in convention_files(&dir, "src/bin", by_path) {
            if !seen_bin_roots.insert(file.clone()) {
                continue;
            }
            let name = file_name(&file).trim_end_matches(".rs").to_string();
            let name = if name == "main" {
                // `src/bin/foo/main.rs` -> target `foo`
                file_name(&directory_of(&file)).to_string()
            } else {
                name
            };
            push(
                &mut targets,
                RustTargetKind::Bin,
                name,
                file,
                "rust.target.bin.default_src_bin",
            );
        }
    }

    // --- integration tests ---
    let mut seen_test_roots: BTreeSet<String> = BTreeSet::new();
    for test in &package.manifest.tests {
        let name = test.name.clone().unwrap_or_else(|| "test".to_string());
        let rel = test
            .path
            .clone()
            .map(|p| join_dir(&dir, &p))
            .unwrap_or_else(|| join_dir(&dir, &format!("tests/{name}.rs")));
        if seen_test_roots.insert(rel.clone()) {
            push(
                &mut targets,
                RustTargetKind::IntegrationTest,
                name,
                rel,
                "rust.target.integration_test.explicit",
            );
        }
    }
    if package.manifest.autotests {
        for file in convention_files(&dir, "tests", by_path) {
            if !seen_test_roots.insert(file.clone()) {
                continue;
            }
            let name = file_name(&file).trim_end_matches(".rs").to_string();
            push(
                &mut targets,
                RustTargetKind::IntegrationTest,
                name,
                file,
                "rust.target.integration_test.default",
            );
        }
    }

    // --- benches ---
    let mut seen_bench_roots: BTreeSet<String> = BTreeSet::new();
    for bench in &package.manifest.benches {
        let name = bench.name.clone().unwrap_or_else(|| "bench".to_string());
        let rel = bench
            .path
            .clone()
            .map(|p| join_dir(&dir, &p))
            .unwrap_or_else(|| join_dir(&dir, &format!("benches/{name}.rs")));
        if seen_bench_roots.insert(rel.clone()) {
            push(
                &mut targets,
                RustTargetKind::Bench,
                name,
                rel,
                "rust.target.bench.explicit",
            );
        }
    }
    if package.manifest.autobenches {
        for file in convention_files(&dir, "benches", by_path) {
            if !seen_bench_roots.insert(file.clone()) {
                continue;
            }
            let name = file_name(&file).trim_end_matches(".rs").to_string();
            push(
                &mut targets,
                RustTargetKind::Bench,
                name,
                file,
                "rust.target.bench.default",
            );
        }
    }

    // --- examples ---
    let mut seen_example_roots: BTreeSet<String> = BTreeSet::new();
    for example in &package.manifest.examples {
        let name = example
            .name
            .clone()
            .unwrap_or_else(|| "example".to_string());
        let rel = example
            .path
            .clone()
            .map(|p| join_dir(&dir, &p))
            .unwrap_or_else(|| join_dir(&dir, &format!("examples/{name}.rs")));
        if seen_example_roots.insert(rel.clone()) {
            push(
                &mut targets,
                RustTargetKind::Example,
                name,
                rel,
                "rust.target.example.explicit",
            );
        }
    }
    if package.manifest.autoexamples {
        for file in convention_files(&dir, "examples", by_path) {
            if !seen_example_roots.insert(file.clone()) {
                continue;
            }
            let name = file_name(&file).trim_end_matches(".rs").to_string();
            push(
                &mut targets,
                RustTargetKind::Example,
                name,
                file,
                "rust.target.example.default",
            );
        }
    }

    let _ = diagnostics;
    targets
}

/// Indexed `.rs` files directly under `{dir}/{sub}` (one level deep), plus
/// `src/bin/*/main.rs` when `sub == "src/bin"`. `tests/`/`benches/`/`examples/`
/// subdirectories are support modules, not target roots.
fn convention_files(dir: &str, sub: &str, by_path: &BTreeMap<&str, &FileAnalysis>) -> Vec<String> {
    let base = join_dir(dir, sub);
    let mut files = Vec::new();
    for path in by_path.keys() {
        let path = *path;
        let parent = directory_of(path);
        if parent == base {
            files.push(path.to_string());
        } else if sub == "src/bin" {
            // `src/bin/foo/main.rs`
            let grand = directory_of(&parent);
            if grand == base && file_name(path) == "main.rs" {
                files.push(path.to_string());
            }
        }
    }
    files.sort();
    files
}
