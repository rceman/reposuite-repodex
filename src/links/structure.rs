//! Structural identities derived from a snapshot.
//!
//! Everything in this module is derivable from written syntax plus repository
//! organization. None of it is a universal runtime semantic identity:
//!
//! * a Go package group is structural over *indexed files*, not a particular
//!   build configuration;
//! * a PHP qualified name is *syntactic* — it is not proof that a class is
//!   autoloadable or that it exists at runtime;
//! * a Python module path assumes the repository root is the package root
//!   (policy P-PY-1);
//! * a Rust module relationship assumes the standard Rust 2018+ file layout and
//!   ignores `#[path]`.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{DeclarationKind, FileAnalysis, LanguageId, ScopeKind};

use super::model::{LinkTarget, StructuralEntity};

/// A class-like PHP declaration reachable by its syntactic qualified name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhpQualifiedDecl {
    pub relative_path: String,
    pub declaration_id: u32,
    pub declaration_kind: String,
    pub name: String,
    /// True for a `namespace` declaration rather than a class-like one.
    pub is_namespace: bool,
}

impl PhpQualifiedDecl {
    pub fn target(&self, qualified_name: &str) -> LinkTarget {
        LinkTarget::declaration(
            self.relative_path.clone(),
            self.declaration_id,
            qualified_name,
            self.declaration_kind.clone(),
            self.name.clone(),
        )
    }
}

/// A direct declaration of one Rust module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustModuleDecl {
    pub name: String,
    pub relative_path: String,
    pub declaration_id: u32,
    pub declaration_kind: String,
}

/// One module in a Rust crate's module tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustModule {
    /// Module path from the crate root, e.g. `["crate", "a", "b"]`.
    pub path: Vec<String>,
    /// Directory in which this module's child module files live.
    pub dir: String,
    /// The file that defines this module.
    pub file: String,
    /// Declarations directly inside this module.
    pub declarations: Vec<RustModuleDecl>,
    /// Child modules by name. More than one entry means the name is ambiguous
    /// inside this module, which is preserved rather than collapsed.
    pub children: BTreeMap<String, Vec<usize>>,
}

/// A Rust crate root and its reachable module tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustCrate {
    pub root_file: String,
    pub modules: Vec<RustModule>,
    /// Files reachable from this crate root through `mod` declarations.
    pub files: BTreeSet<String>,
}

/// An external `mod name;` declaration and the files it could name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustExternalMod {
    pub relative_path: String,
    pub declaration_id: u32,
    pub name: String,
    pub written: String,
    /// Standard source-file forms the rule considered.
    pub attempted: Vec<String>,
    /// Those forms that are present in the snapshot.
    pub present: Vec<String>,
}

/// Structural facts derived once from a snapshot and shared by every rule.
#[derive(Debug, Clone, Default)]
pub struct Structure {
    /// Indexed files, in canonical path order.
    pub files: Vec<String>,
    /// `language` by relative path.
    pub language_by_path: BTreeMap<String, LanguageId>,

    /// Go package groups: directory -> package name -> member files.
    pub go_packages: BTreeMap<String, BTreeMap<String, Vec<String>>>,

    /// Every declared PHP namespace, and every ancestor prefix of one.
    pub php_namespace_prefixes: BTreeSet<String>,
    /// PHP syntactic qualified name -> declarations with that name.
    pub php_qualified: BTreeMap<String, Vec<PhpQualifiedDecl>>,

    /// Python dotted module path -> files defining it.
    pub python_modules: BTreeMap<String, Vec<String>>,

    /// Rust crate roots.
    pub rust_crates: Vec<RustCrate>,
    /// Rust external `mod` declarations, keyed by `(path, declaration_id)`.
    pub rust_external_mods: BTreeMap<(String, u32), RustExternalMod>,

    /// Derived structural entities.
    pub entities: Vec<StructuralEntity>,
}

impl Structure {
    /// Derive every structural identity from a snapshot's file analyses.
    pub fn build(analyses: &[FileAnalysis]) -> Self {
        let mut structure = Structure::default();
        for analysis in analyses {
            let path = analysis.file.relative_path.clone();
            structure.files.push(path.clone());
            structure
                .language_by_path
                .insert(path.clone(), analysis.file.language);
            match analysis.file.language {
                LanguageId::Go => structure.collect_go_package(analysis),
                LanguageId::Php => structure.collect_php_namespaces(analysis),
                LanguageId::Python => structure.collect_python_modules(analysis),
                LanguageId::Rust => {}
            }
        }
        structure.files.sort();
        structure.collect_rust_crates(analyses);
        structure.collect_python_modules_from_files();
        structure.build_entities();
        structure
    }

    fn collect_go_package(&mut self, analysis: &FileAnalysis) {
        let Some(package) = analysis
            .declarations
            .iter()
            .find(|declaration| declaration.kind == DeclarationKind::Package)
        else {
            return;
        };
        let directory = directory_of(&analysis.file.relative_path);
        self.go_packages
            .entry(directory)
            .or_default()
            .entry(package.name.clone())
            .or_default()
            .push(analysis.file.relative_path.clone());
    }

    fn collect_php_namespaces(&mut self, analysis: &FileAnalysis) {
        for declaration in &analysis.declarations {
            let namespace = php_enclosing_namespace(analysis, declaration.scope_id);
            match declaration.kind {
                DeclarationKind::Namespace => {
                    // A `namespace A\B;` declaration names the namespace itself.
                    // For a brace-nested namespace the full path is the enclosing
                    // chain plus the written name.
                    let qualified = match &namespace {
                        Some(enclosing) => format!("{enclosing}\\{}", declaration.name),
                        None => declaration.name.clone(),
                    };
                    add_namespace_prefixes(&mut self.php_namespace_prefixes, &qualified);
                    self.php_qualified
                        .entry(qualified)
                        .or_default()
                        .push(PhpQualifiedDecl {
                            relative_path: analysis.file.relative_path.clone(),
                            declaration_id: declaration.declaration_id,
                            declaration_kind: declaration.kind.as_str().to_string(),
                            name: declaration.name.clone(),
                            is_namespace: true,
                        });
                }
                DeclarationKind::Class
                | DeclarationKind::Interface
                | DeclarationKind::Trait
                | DeclarationKind::Enum => {
                    let qualified = match &namespace {
                        Some(namespace) => format!("{namespace}\\{}", declaration.name),
                        None => declaration.name.clone(),
                    };
                    self.php_qualified
                        .entry(qualified)
                        .or_default()
                        .push(PhpQualifiedDecl {
                            relative_path: analysis.file.relative_path.clone(),
                            declaration_id: declaration.declaration_id,
                            declaration_kind: declaration.kind.as_str().to_string(),
                            name: declaration.name.clone(),
                            is_namespace: false,
                        });
                }
                _ => {}
            }
        }
    }

    fn collect_python_modules(&mut self, analysis: &FileAnalysis) {
        // Record the file so `collect_python_modules_from_files` can map dotted
        // paths onto it; the mapping itself needs the whole file list.
        let _ = analysis;
    }

    /// Map repository-relative Python files onto dotted module paths.
    ///
    /// Policy P-PY-1: the repository root is the package root. `a/b/c.py` is
    /// module `a.b.c`; `a/b/c/__init__.py` is also module `a.b.c`.
    fn collect_python_modules_from_files(&mut self) {
        let python: Vec<String> = self
            .files
            .iter()
            .filter(|path| self.language_by_path.get(*path) == Some(&LanguageId::Python))
            .cloned()
            .collect();
        for path in python {
            if let Some(module) = python_module_path(&path) {
                self.python_modules.entry(module).or_default().push(path);
            }
        }
        for files in self.python_modules.values_mut() {
            files.sort();
        }
    }

    fn collect_rust_crates(&mut self, analyses: &[FileAnalysis]) {
        let by_path: BTreeMap<&str, &FileAnalysis> = analyses
            .iter()
            .map(|analysis| (analysis.file.relative_path.as_str(), analysis))
            .collect();

        let roots: Vec<String> = self
            .files
            .iter()
            .filter(|path| {
                self.language_by_path.get(*path) == Some(&LanguageId::Rust)
                    && matches!(file_name(path), "lib.rs" | "main.rs")
            })
            .cloned()
            .collect();

        for root in roots {
            if let Some(crate_tree) = self.build_rust_crate_tree(&root, &by_path) {
                self.rust_crates.push(crate_tree);
            }
        }
        self.rust_crates
            .sort_by(|left, right| left.root_file.cmp(&right.root_file));
    }

    /// Build a [`RustCrate`] module tree rooted at `root`, using the same
    /// `mod`-declaration traversal TASK 3B applies to every crate root — reused
    /// for the conventional `lib.rs`/`main.rs` roots and for Cargo target roots
    /// discovered by the crate/target topology (TASK 3F).
    pub fn build_rust_crate_tree(
        &mut self,
        root: &str,
        by_path: &BTreeMap<&str, &FileAnalysis>,
    ) -> Option<RustCrate> {
        let analysis = by_path.get(root)?;
        // A crate root's submodules live in the directory *containing* the
        // root file, for every root — `lib.rs`/`main.rs`/`mod.rs` as well as a
        // `bin`/`test`/`example`/`bench` target root such as `tests/foo.rs`,
        // whose `mod shared;` resolves to `tests/shared/…`, not `tests/foo/…`.
        let root_dir = directory_of(root);
        let mut crate_tree = RustCrate {
            root_file: root.to_string(),
            modules: vec![RustModule {
                path: vec!["crate".to_string()],
                dir: root_dir,
                file: root.to_string(),
                declarations: Vec::new(),
                children: BTreeMap::new(),
            }],
            files: BTreeSet::from([root.to_string()]),
        };
        let mut visited = BTreeSet::from([root.to_string()]);
        self.visit_rust_module(&mut crate_tree, 0, analysis, 0, by_path, &mut visited);
        crate_tree.modules[0]
            .declarations
            .sort_by(|left, right| left.name.cmp(&right.name));
        Some(crate_tree)
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_rust_module<'a>(
        &mut self,
        crate_tree: &mut RustCrate,
        module_index: usize,
        analysis: &'a FileAnalysis,
        scope_id: u32,
        by_path: &BTreeMap<&'a str, &'a FileAnalysis>,
        visited: &mut BTreeSet<String>,
    ) {
        let module_dir = crate_tree.modules[module_index].dir.clone();
        let module_path = crate_tree.modules[module_index].path.clone();

        // Direct children of this module scope.
        let children: Vec<(String, DeclarationKind, Option<bool>)> = analysis
            .declarations
            .iter()
            .filter(|declaration| declaration.scope_id == scope_id)
            .map(|declaration| {
                (
                    declaration.name.clone(),
                    declaration.kind,
                    Some(declaration.body_range.is_some()),
                )
            })
            .collect();

        let mut external_mods: Vec<(String, u32, String)> = Vec::new();
        let mut inline_mods: Vec<(String, u32)> = Vec::new();
        let mut plain: Vec<RustModuleDecl> = Vec::new();

        for declaration in analysis
            .declarations
            .iter()
            .filter(|declaration| declaration.scope_id == scope_id)
        {
            if declaration.kind == DeclarationKind::Module {
                match declaration.body_range {
                    // `mod name;` — an external module file.
                    None => external_mods.push((
                        declaration.name.clone(),
                        declaration.declaration_id,
                        declaration.name.clone(),
                    )),
                    // `mod name { .. }` — inline, part of this same file.
                    Some(_) => {
                        inline_mods.push((declaration.name.clone(), declaration.declaration_id))
                    }
                }
                continue;
            }
            plain.push(RustModuleDecl {
                name: declaration.name.clone(),
                relative_path: analysis.file.relative_path.clone(),
                declaration_id: declaration.declaration_id,
                declaration_kind: declaration.kind.as_str().to_string(),
            });
        }
        let _ = children;

        crate_tree.modules[module_index].declarations.extend(plain);

        // External modules: resolve the two standard file forms.
        for (name, declaration_id, written) in external_mods {
            let first = join_dir(&module_dir, &format!("{name}.rs"));
            let second = join_dir(&module_dir, &format!("{name}/mod.rs"));
            let mut present = Vec::new();
            if by_path.contains_key(first.as_str()) {
                present.push(first.clone());
            }
            if by_path.contains_key(second.as_str()) {
                present.push(second.clone());
            }
            self.rust_external_mods.insert(
                (analysis.file.relative_path.clone(), declaration_id),
                RustExternalMod {
                    relative_path: analysis.file.relative_path.clone(),
                    declaration_id,
                    name: name.clone(),
                    written,
                    attempted: vec![first.clone(), second.clone()],
                    present: present.clone(),
                },
            );
            // Only a uniquely resolved module extends the tree; an ambiguous or
            // missing one is reported as such and never guessed.
            if present.len() != 1 {
                continue;
            }
            let child_file = present[0].clone();
            if !visited.insert(child_file.clone()) {
                continue;
            }
            let Some(child_analysis) = by_path.get(child_file.as_str()) else {
                continue;
            };
            let mut path = module_path.clone();
            path.push(name.clone());
            let index = crate_tree.modules.len();
            crate_tree.modules.push(RustModule {
                path,
                dir: rust_module_dir(&child_file),
                file: child_file.clone(),
                declarations: Vec::new(),
                children: BTreeMap::new(),
            });
            crate_tree.files.insert(child_file.clone());
            crate_tree.modules[module_index]
                .children
                .entry(name)
                .or_default()
                .push(index);
            self.visit_rust_module(crate_tree, index, child_analysis, 0, by_path, visited);
        }

        // Inline modules: the scope with the same name and range as the
        // declaration.
        for (name, declaration_id) in inline_mods {
            let Some(declaration) = analysis
                .declarations
                .iter()
                .find(|declaration| declaration.declaration_id == declaration_id)
            else {
                continue;
            };
            let Some(scope) = analysis.scopes.iter().find(|scope| {
                scope.kind == ScopeKind::Module
                    && scope.name.as_deref() == Some(name.as_str())
                    && scope.range == declaration.range
            }) else {
                continue;
            };
            let mut path = module_path.clone();
            path.push(name.clone());
            let index = crate_tree.modules.len();
            crate_tree.modules.push(RustModule {
                path,
                dir: join_dir(&module_dir, &name),
                file: analysis.file.relative_path.clone(),
                declarations: Vec::new(),
                children: BTreeMap::new(),
            });
            crate_tree.modules[module_index]
                .children
                .entry(name)
                .or_default()
                .push(index);
            self.visit_rust_module(
                crate_tree,
                index,
                analysis,
                scope.scope_id,
                by_path,
                visited,
            );
        }
    }

    fn build_entities(&mut self) {
        let mut entities = Vec::new();

        for (directory, packages) in &self.go_packages {
            for (package, files) in packages {
                let key = format!("{directory}:{package}");
                entities.push(StructuralEntity {
                    entity_id: StructuralEntity::derive_id("go_package", &key),
                    structural_kind: "go_package".to_string(),
                    language: "go".to_string(),
                    key,
                    files: files.clone(),
                    assumptions: vec![
                        "membership is structural over indexed files, not a Go build \
                         configuration"
                            .to_string(),
                        "build tags and file-name constraints are not evaluated".to_string(),
                    ],
                });
            }
        }

        // One entity per declared PHP namespace.
        let mut namespace_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (qualified, declarations) in &self.php_qualified {
            for declaration in declarations {
                if declaration.is_namespace {
                    namespace_files
                        .entry(qualified.clone())
                        .or_default()
                        .push(declaration.relative_path.clone());
                }
            }
        }
        for (namespace, mut files) in namespace_files {
            files.sort();
            files.dedup();
            entities.push(StructuralEntity {
                entity_id: StructuralEntity::derive_id("php_namespace", &namespace),
                structural_kind: "php_namespace".to_string(),
                language: "php".to_string(),
                key: namespace,
                files,
                assumptions: vec![
                    "the namespace is derived from written `namespace` syntax only".to_string(),
                    "no autoload or runtime availability is claimed".to_string(),
                ],
            });
        }

        // One entity per Python package (a directory holding an __init__.py).
        let mut packages: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (module, files) in &self.python_modules {
            for file in files {
                if file.ends_with("/__init__.py") || file == "__init__.py" {
                    packages
                        .entry(module.clone())
                        .or_default()
                        .push(file.clone());
                }
            }
        }
        for (package, mut files) in packages {
            files.sort();
            files.dedup();
            entities.push(StructuralEntity {
                entity_id: StructuralEntity::derive_id("python_package", &package),
                structural_kind: "python_package".to_string(),
                language: "python".to_string(),
                key: package,
                files,
                assumptions: vec![
                    "the repository root is the package root (policy P-PY-1)".to_string(),
                    "namespace packages are not detected".to_string(),
                ],
            });
        }

        // One entity per Rust crate root and one per resolved module.
        for crate_tree in &self.rust_crates {
            let key = crate_tree.root_file.clone();
            entities.push(StructuralEntity {
                entity_id: StructuralEntity::derive_id("rust_crate", &key),
                structural_kind: "rust_crate".to_string(),
                language: "rust".to_string(),
                key,
                files: crate_tree.files.iter().cloned().collect(),
                assumptions: vec![
                    "crate roots are files named lib.rs or main.rs".to_string(),
                    "only modules reachable through `mod` declarations are included".to_string(),
                ],
            });
            for module in &crate_tree.modules {
                if module.path.len() < 2 {
                    continue;
                }
                let key = format!("{}::{}", crate_tree.root_file, module.path[1..].join("::"));
                entities.push(StructuralEntity {
                    entity_id: StructuralEntity::derive_id("rust_module", &key),
                    structural_kind: "rust_module".to_string(),
                    language: "rust".to_string(),
                    key,
                    files: vec![module.file.clone()],
                    assumptions: vec![
                        "the standard Rust 2018+ file layout is assumed".to_string(),
                        "`#[path]` attributes are not evaluated".to_string(),
                    ],
                });
            }
        }

        entities.sort_by(|left, right| {
            (&left.structural_kind, &left.key).cmp(&(&right.structural_kind, &right.key))
        });
        entities.dedup_by(|left, right| left.entity_id == right.entity_id);
        self.entities = entities;
    }

    /// The Python files that define one dotted module path.
    pub fn python_files_for(&self, module: &str) -> Vec<String> {
        self.python_modules.get(module).cloned().unwrap_or_default()
    }

    /// The Go package groups in one directory.
    pub fn go_packages_in(&self, directory: &str) -> Option<&BTreeMap<String, Vec<String>>> {
        self.go_packages.get(directory)
    }

    /// The entity id of a Go package group.
    pub fn go_package_entity(&self, directory: &str, package: &str) -> Option<&StructuralEntity> {
        let key = format!("{directory}:{package}");
        self.entities
            .iter()
            .find(|entity| entity.structural_kind == "go_package" && entity.key == key)
    }

    /// The entity id of a PHP namespace.
    pub fn php_namespace_entity(&self, namespace: &str) -> Option<&StructuralEntity> {
        self.entities
            .iter()
            .find(|entity| entity.structural_kind == "php_namespace" && entity.key == namespace)
    }
}

/// Directory part of a `/`-separated relative path (empty at the root).
pub fn directory_of(path: &str) -> String {
    match path.rfind('/') {
        Some(index) => path[..index].to_string(),
        None => String::new(),
    }
}

/// File name part of a `/`-separated relative path.
pub fn file_name(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[index + 1..],
        None => path,
    }
}

/// Join a possibly-empty directory with a child name.
pub fn join_dir(directory: &str, name: &str) -> String {
    if directory.is_empty() {
        name.to_string()
    } else {
        format!("{directory}/{name}")
    }
}

/// Directory in which a Rust module file's child modules live.
///
/// `src/lib.rs` -> `src`, `src/a/mod.rs` -> `src/a`, `src/a.rs` -> `src/a`.
pub fn rust_module_dir(file: &str) -> String {
    let directory = directory_of(file);
    let name = file_name(file);
    match name {
        "mod.rs" | "lib.rs" | "main.rs" => directory,
        _ => {
            let stem = name.strip_suffix(".rs").unwrap_or(name);
            join_dir(&directory, stem)
        }
    }
}

/// Dotted Python module path for a repository-relative file, under policy
/// P-PY-1 (the repository root is the package root).
pub fn python_module_path(file: &str) -> Option<String> {
    let name = file_name(file);
    let directory = directory_of(file);
    let stem = name.strip_suffix(".py")?;
    let dotted_dir = if directory.is_empty() {
        String::new()
    } else {
        directory.replace('/', ".")
    };
    if stem == "__init__" {
        return if dotted_dir.is_empty() {
            None
        } else {
            Some(dotted_dir)
        };
    }
    Some(if dotted_dir.is_empty() {
        stem.to_string()
    } else {
        format!("{dotted_dir}.{stem}")
    })
}

/// The written namespace enclosing `scope_id`, when the file declares one.
fn php_enclosing_namespace(analysis: &FileAnalysis, scope_id: u32) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for id in analysis.scope_chain(scope_id) {
        let scope = &analysis.scopes[id as usize];
        if scope.kind == ScopeKind::Namespace {
            if let Some(name) = &scope.name {
                parts.push(name.clone());
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\\"))
    }
}

fn add_namespace_prefixes(target: &mut BTreeSet<String>, namespace: &str) {
    let segments: Vec<&str> = namespace.split('\\').collect();
    for end in 1..=segments.len() {
        target.insert(segments[..end].join("\\"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_module_directories_follow_the_standard_layout() {
        assert_eq!(rust_module_dir("src/lib.rs"), "src");
        assert_eq!(rust_module_dir("src/main.rs"), "src");
        assert_eq!(rust_module_dir("src/a.rs"), "src/a");
        assert_eq!(rust_module_dir("src/a/mod.rs"), "src/a");
        assert_eq!(rust_module_dir("src/a/b.rs"), "src/a/b");
        assert_eq!(rust_module_dir("lib.rs"), "");
    }

    #[test]
    fn python_module_paths_are_derived_from_files() {
        assert_eq!(python_module_path("a/b/c.py").as_deref(), Some("a.b.c"));
        assert_eq!(
            python_module_path("a/b/__init__.py").as_deref(),
            Some("a.b")
        );
        assert_eq!(python_module_path("c.py").as_deref(), Some("c"));
        assert_eq!(python_module_path("__init__.py"), None);
    }

    #[test]
    fn namespace_prefixes_include_every_ancestor() {
        let mut set = BTreeSet::new();
        add_namespace_prefixes(&mut set, "App\\Service\\Deep");
        assert!(set.contains("App"));
        assert!(set.contains("App\\Service"));
        assert!(set.contains("App\\Service\\Deep"));
        assert_eq!(set.len(), 3);
    }
}
