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

use super::go_topology::GoPackageTopology;
use super::model::{
    rule, FactLocator, LinkOutcome, LinkProvenance, LinkRecord, LinkTarget, MetadataDependency,
};
use super::structure::{directory_of, Structure};

/// Every Go structural relationship derivable from the snapshot.
pub fn links(
    structure: &Structure,
    analyses: &[FileAnalysis],
    topology: &GoPackageTopology,
) -> Vec<LinkRecord> {
    let mut records = Vec::new();
    records.extend(package_membership_links(structure, analyses));
    records.extend(import_links(structure, analyses, topology));
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
    topology: &GoPackageTopology,
) -> Vec<LinkRecord> {
    let mut records = Vec::new();

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
                    classify_go_import(structure, topology, &written);
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
    topology: &GoPackageTopology,
    written: &str,
) -> (
    &'static str,
    LinkOutcome,
    Vec<String>,
    Vec<MetadataDependency>,
) {
    // TASK 4A: an import is `local_module` when it prefixes into ANY
    // repository-local module (the nearest matching module path wins), not only
    // the repo-root module. `external` covers everything else.
    let matched = topology
        .modules
        .iter()
        .filter(|m| {
            written == m.module_path
                || written
                    .strip_prefix(m.module_path.as_str())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
        .max_by_key(|m| m.module_path.len());

    let Some(module) = matched else {
        // `written` matches no repository-local module path. The dependency is
        // on the whole discovered module set — any go.mod change could make it
        // local.
        let dependency = topology.metadata_dependencies();
        let reason = if topology.modules.is_empty() {
            "no repository-local go.mod, so no import path can be shown to be local".to_string()
        } else {
            format!("`{written}` is outside every repository-local module")
        };
        return (
            rule::GO_IMPORT_EXTERNAL,
            LinkOutcome::out_of_scope(reason),
            vec!["module_prefix=none".to_string()],
            dependency,
        );
    };

    let module_path = module.module_path.as_str();
    let remainder = written
        .strip_prefix(module_path)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or("")
        .trim_end_matches('/');
    // The package directory is module_root_dir + remainder.
    let directory = if module.root_dir.is_empty() {
        remainder.to_string()
    } else if remainder.is_empty() {
        module.root_dir.clone()
    } else {
        format!("{}/{}", module.root_dir, remainder)
    };

    let dependency = vec![MetadataDependency {
        relative_path: module.manifest_path.clone(),
        content_digest: module.manifest_digest.clone(),
        present: true,
        rule_id: rule::GO_IMPORT_LOCAL_MODULE.to_string(),
        field: "module".to_string(),
        value: module_path.to_string(),
    }];
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
            "`{written}` maps to directory `{display}` inside local module `{module_path}`, but no \
             indexed Go file declares a package there"
        ),
    );
    let evidence = vec![
        format!("module_prefix={module_path}"),
        format!("mapped_directory={display}"),
    ];
    (rule::GO_IMPORT_LOCAL_MODULE, outcome, evidence, dependency)
}
