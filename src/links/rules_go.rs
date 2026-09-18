//! Go structural link rules.
//!
//! * [`rule::GO_PACKAGE_SAME_DIRECTORY`] — package membership as a structural
//!   identity.
//! * [`rule::GO_IMPORT_LOCAL_MODULE`] — an import path inside the repository's
//!   own module, mapped onto a directory.
//! * [`rule::GO_IMPORT_EXTERNAL`] — an import path outside it.
//!
//! Build tags are never evaluated, so package membership is structural over the
//! indexed files, not a particular Go build configuration. A `_test` package is
//! a distinct group because it is a distinct declared package name.

use crate::model::{DeclarationKind, FileAnalysis, LanguageId};

use super::metadata::GoModuleState;
use super::model::{
    rule, FactLocator, LinkOutcome, LinkProvenance, LinkRecord, LinkTarget, MetadataDependency,
};
use super::structure::{directory_of, Structure};

/// Every Go structural relationship derivable from the snapshot.
pub fn links(
    structure: &Structure,
    analyses: &[FileAnalysis],
    go_module: &GoModuleState,
) -> Vec<LinkRecord> {
    let mut records = Vec::new();
    records.extend(package_membership_links(structure, analyses));
    records.extend(import_links(structure, analyses, go_module));
    records
}

// ---------------------------------------------------------------------------
// go.package.same_directory
// ---------------------------------------------------------------------------

fn package_membership_links(structure: &Structure, analyses: &[FileAnalysis]) -> Vec<LinkRecord> {
    let mut records = Vec::new();
    for analysis in analyses {
        if analysis.file.language != LanguageId::Go {
            continue;
        }
        let Some(package) = analysis
            .declarations
            .iter()
            .find(|declaration| declaration.kind == DeclarationKind::Package)
        else {
            continue;
        };
        let directory = directory_of(&analysis.file.relative_path);
        let Some(entity) = structure.go_package_entity(&directory, &package.name) else {
            continue;
        };
        let source = FactLocator::declaration(&analysis.file.relative_path, package.declaration_id);
        let outcome = LinkOutcome::exact(LinkTarget::Structure {
            entity_id: entity.entity_id.clone(),
            structural_kind: entity.structural_kind.clone(),
            key: entity.key.clone(),
        });
        let provenance = LinkProvenance {
            language: "go".to_string(),
            rule_id: rule::GO_PACKAGE_SAME_DIRECTORY.to_string(),
            source: source.clone(),
            written: package.name.clone(),
            evidence: vec![
                format!(
                    "directory={}",
                    if directory.is_empty() {
                        "."
                    } else {
                        &directory
                    }
                ),
                format!("members={}", entity.files.len()),
            ],
            metadata: Vec::new(),
        };
        records.push(LinkRecord::new(
            "package_membership",
            source,
            package.name.clone(),
            outcome,
            provenance,
        ));
    }
    records
}

// ---------------------------------------------------------------------------
// go.import.local_module / go.import.external
// ---------------------------------------------------------------------------

fn import_links(
    structure: &Structure,
    analyses: &[FileAnalysis],
    go_module: &GoModuleState,
) -> Vec<LinkRecord> {
    let mut records = Vec::new();
    let module_path = go_module.module_path().map(str::to_string);

    for analysis in analyses {
        if analysis.file.language != LanguageId::Go {
            continue;
        }
        for import in &analysis.imports {
            for (index, item) in import.items.iter().enumerate() {
                let source = FactLocator {
                    relative_path: analysis.file.relative_path.clone(),
                    fact_kind: super::model::FactKind::Import,
                    fact_id: import.import_id,
                    item_index: Some(index as u32),
                };
                let written = item.target.clone();
                let (rule_id, outcome, evidence, metadata) =
                    classify_go_import(structure, &module_path, go_module, &written);
                let provenance = LinkProvenance {
                    language: "go".to_string(),
                    rule_id: rule_id.to_string(),
                    source: source.clone(),
                    written: written.clone(),
                    evidence,
                    metadata,
                };
                records.push(LinkRecord::new(
                    "import_path",
                    source,
                    written,
                    outcome,
                    provenance,
                ));
            }
        }
    }
    records
}

fn classify_go_import(
    structure: &Structure,
    module_path: &Option<String>,
    go_module: &GoModuleState,
    written: &str,
) -> (
    &'static str,
    LinkOutcome,
    Vec<String>,
    Vec<MetadataDependency>,
) {
    let dependency = vec![go_module.dependency(rule::GO_IMPORT_LOCAL_MODULE)];
    let Some(module_path) = module_path else {
        let reason = match go_module {
            GoModuleState::Absent => {
                "no repository-root go.mod, so no import path can be shown to be local".to_string()
            }
            GoModuleState::Malformed { reason, .. } => format!(
                "the repository-root go.mod could not be read ({reason}), so no import path can \
                 be shown to be local"
            ),
            GoModuleState::Present(_) => unreachable!("module_path is Some for Present"),
        };
        return (
            rule::GO_IMPORT_EXTERNAL,
            LinkOutcome::out_of_scope(reason),
            vec!["module_prefix=unknown".to_string()],
            dependency.clone(),
        );
    };

    // `example.com/project` and `example.com/project/sub` are inside the module;
    // `example.com/projectile` is not.
    let remainder = if written == module_path {
        Some(String::new())
    } else {
        written
            .strip_prefix(module_path.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
            .map(str::to_string)
    };

    let Some(remainder) = remainder else {
        return (
            rule::GO_IMPORT_EXTERNAL,
            LinkOutcome::out_of_scope(format!(
                "`{written}` is outside the local module `{module_path}`"
            )),
            vec![format!("module_prefix={module_path}")],
            dependency.clone(),
        );
    };

    let directory = remainder.trim_end_matches('/').to_string();
    let mut candidates = Vec::new();
    if let Some(packages) = structure.go_packages_in(&directory) {
        for package in packages.keys() {
            if let Some(entity) = structure.go_package_entity(&directory, package) {
                candidates.push(LinkTarget::Structure {
                    entity_id: entity.entity_id.clone(),
                    structural_kind: entity.structural_kind.clone(),
                    key: entity.key.clone(),
                });
            }
        }
    }
    let display = if directory.is_empty() {
        "."
    } else {
        &directory
    };
    let outcome = LinkOutcome::from_candidates(
        candidates,
        format!(
            "`{written}` maps to directory `{display}` inside the local module, but no indexed Go \
             file declares a package there"
        ),
    );
    let evidence = vec![
        format!("module_prefix={module_path}"),
        format!("mapped_directory={display}"),
    ];
    (rule::GO_IMPORT_LOCAL_MODULE, outcome, evidence, dependency)
}
