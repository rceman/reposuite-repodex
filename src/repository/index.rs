//! Exact, source-grounded repository lookup over a loaded snapshot.
//!
//! This is **not** semantic lookup. Every query matches the *written* syntax
//! exactly and returns every source-grounded occurrence. Equal names are never
//! assumed to refer to the same entity: `declarations_named("User")` may return
//! several unrelated declarations in different files, and that is correct.
//!
//! Occurrences are located by a compound locator (`relative_path` plus the
//! file-local fact id), because file-local ids are not global semantic symbol
//! ids and are not promised to be stable across source edits.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;

use crate::model::{FileAnalysis, SourceRange};
use crate::repository::artifact::{self, SnapshotError};
use crate::repository::manifest::RepositoryManifest;

/// A declaration occurrence located in the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeclarationHit {
    pub relative_path: String,
    pub declaration_id: u32,
    pub kind: String,
    pub name: String,
    pub range: SourceRange,
    pub name_range: SourceRange,
    pub scope_id: u32,
    /// Number of pieces of test evidence attached to this declaration.
    pub test_evidence: u64,
}

/// An import-item occurrence located in the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportHit {
    pub relative_path: String,
    pub import_id: u32,
    pub form: String,
    pub module: Option<String>,
    pub target: String,
    pub target_range: SourceRange,
    pub alias: Option<String>,
    pub category: String,
    pub statement_range: SourceRange,
}

/// A call-like occurrence located in the repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallHit {
    pub relative_path: String,
    pub call_id: u32,
    pub form: String,
    pub callee_written: String,
    pub callee_range: SourceRange,
    pub expression_range: SourceRange,
    pub dynamic_callee: bool,
}

/// An in-memory exact index over every file analysis in one snapshot.
#[derive(Debug, Default)]
pub struct RepositoryFactIndex {
    /// Analyses in canonical relative-path order.
    files: Vec<FileAnalysis>,
    by_path: BTreeMap<String, usize>,
    declarations: BTreeMap<String, Vec<DeclarationHit>>,
    import_items: BTreeMap<String, Vec<ImportHit>>,
    calls: BTreeMap<String, Vec<CallHit>>,
    tests: Vec<DeclarationHit>,
}

impl RepositoryFactIndex {
    /// Load every file analysis from a snapshot directory.
    pub fn load(dir: &Path) -> Result<Self, SnapshotError> {
        let manifest = artifact::read_manifest(dir)?;
        Self::load_with_manifest(dir, &manifest)
    }

    /// Load every file analysis using an already-read manifest.
    pub fn load_with_manifest(
        dir: &Path,
        manifest: &RepositoryManifest,
    ) -> Result<Self, SnapshotError> {
        let mut analyses = Vec::with_capacity(manifest.files.len());
        for file in &manifest.files {
            analyses.push(artifact::read_file_analysis(dir, file)?);
        }
        Ok(Self::from_analyses(analyses))
    }

    /// Build an index from analyses already in memory.
    pub fn from_analyses(analyses: Vec<FileAnalysis>) -> Self {
        let mut index = Self {
            files: analyses,
            ..Self::default()
        };
        index.rebuild();
        index
    }

    fn rebuild(&mut self) {
        self.by_path.clear();
        self.declarations.clear();
        self.import_items.clear();
        self.calls.clear();
        self.tests.clear();
        for (position, analysis) in self.files.iter().enumerate() {
            self.by_path
                .insert(analysis.file.relative_path.clone(), position);
            for declaration in &analysis.declarations {
                let hit = DeclarationHit {
                    relative_path: analysis.file.relative_path.clone(),
                    declaration_id: declaration.declaration_id,
                    kind: declaration.kind.as_str().to_string(),
                    name: declaration.name.clone(),
                    range: declaration.range,
                    name_range: declaration.name_range,
                    scope_id: declaration.scope_id,
                    test_evidence: declaration.test_evidence.len() as u64,
                };
                if !declaration.test_evidence.is_empty() {
                    self.tests.push(hit.clone());
                }
                self.declarations
                    .entry(declaration.name.clone())
                    .or_default()
                    .push(hit);
            }
            for import in &analysis.imports {
                for item in &import.items {
                    self.import_items
                        .entry(item.target.clone())
                        .or_default()
                        .push(ImportHit {
                            relative_path: analysis.file.relative_path.clone(),
                            import_id: import.import_id,
                            form: import.form.as_str().to_string(),
                            module: import.module.clone(),
                            target: item.target.clone(),
                            target_range: item.target_range,
                            alias: item.alias.clone(),
                            category: item.category.as_str().to_string(),
                            statement_range: import.statement_range,
                        });
                }
            }
            for call in &analysis.calls {
                self.calls
                    .entry(call.callee_written.clone())
                    .or_default()
                    .push(CallHit {
                        relative_path: analysis.file.relative_path.clone(),
                        call_id: call.call_id,
                        form: call.form.as_str().to_string(),
                        callee_written: call.callee_written.clone(),
                        callee_range: call.callee_range,
                        expression_range: call.expression_range,
                        dynamic_callee: call.dynamic_callee,
                    });
            }
        }
    }

    /// Every file analysis, in canonical relative-path order.
    pub fn files(&self) -> &[FileAnalysis] {
        &self.files
    }

    /// The analysis for one relative path.
    pub fn file(&self, relative_path: &str) -> Option<&FileAnalysis> {
        self.by_path
            .get(relative_path)
            .map(|position| &self.files[*position])
    }

    /// Every declaration written with exactly this name.
    pub fn declarations_named(&self, name: &str) -> &[DeclarationHit] {
        self.declarations
            .get(name)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Every distinct written declaration name, in deterministic order.
    pub fn declaration_names(&self) -> impl Iterator<Item = &str> {
        self.declarations.keys().map(String::as_str)
    }

    /// Every import item written with exactly this target.
    pub fn import_items_targeting(&self, target: &str) -> &[ImportHit] {
        self.import_items
            .get(target)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Every distinct written import-item target, in deterministic order.
    pub fn import_targets(&self) -> impl Iterator<Item = &str> {
        self.import_items.keys().map(String::as_str)
    }

    /// Every call-like occurrence written with exactly this callee form.
    pub fn calls_named(&self, callee_written: &str) -> &[CallHit] {
        self.calls
            .get(callee_written)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Every distinct written callee form, in deterministic order.
    pub fn callee_names(&self) -> impl Iterator<Item = &str> {
        self.calls.keys().map(String::as_str)
    }

    /// Every declaration carrying at least one piece of test evidence.
    pub fn test_declarations(&self) -> &[DeclarationHit] {
        &self.tests
    }
}
