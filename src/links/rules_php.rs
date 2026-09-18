//! PHP structural link rules.
//!
//! PHP gives a useful syntax-grounded namespace layer:
//!
//! * [`rule::PHP_NAMESPACE_DECLARATION`] — declarations grouped under their
//!   written namespace, producing a **syntactic** qualified name.
//! * [`rule::PHP_USE_QUALIFIED_NAME`] — `use A\B\C;` compared textually against
//!   those syntactic qualified names.
//! * [`rule::PHP_USE_EXTERNAL`] — a `use` whose namespace is not declared in the
//!   repository.
//! * [`rule::PHP_USE_NON_CLASS_IMPORT`] — `use function` / `use const`, kept
//!   distinct and never linked to class declarations.
//!
//! Composer autoloading is **not** required and PSR-4 is **not** assumed. A
//! syntactic qualified name is not proof that a class is autoloadable or that it
//! exists at runtime.

use crate::model::{DeclarationKind, FileAnalysis, ImportCategory, LanguageId, ScopeKind};

use super::model::{
    rule, FactKind, FactLocator, LinkOutcome, LinkProvenance, LinkRecord, LinkTarget,
};
use super::structure::Structure;

/// Every PHP structural relationship derivable from the snapshot.
pub fn links(structure: &Structure, analyses: &[FileAnalysis]) -> Vec<LinkRecord> {
    let mut records = Vec::new();
    records.extend(namespace_links(structure, analyses));
    records.extend(use_links(structure, analyses));
    records
}

// ---------------------------------------------------------------------------
// php.namespace.declaration
// ---------------------------------------------------------------------------

fn namespace_links(structure: &Structure, analyses: &[FileAnalysis]) -> Vec<LinkRecord> {
    let mut records = Vec::new();
    for analysis in analyses {
        if analysis.file.language != LanguageId::Php {
            continue;
        }
        for declaration in &analysis.declarations {
            match declaration.kind {
                DeclarationKind::Namespace => {
                    let Some(entity) = structure.php_namespace_entity(&declaration.name) else {
                        continue;
                    };
                    records.push(namespace_record(
                        analysis,
                        declaration,
                        entity,
                        &declaration.name,
                        &declaration.name,
                    ));
                }
                DeclarationKind::Class
                | DeclarationKind::Interface
                | DeclarationKind::Trait
                | DeclarationKind::Enum => {
                    // A declaration in the global namespace has no namespace
                    // entity to belong to, and TASK 3B does not invent one.
                    let Some(namespace) = enclosing_namespace(analysis, declaration.scope_id)
                    else {
                        continue;
                    };
                    let Some(entity) = structure.php_namespace_entity(&namespace) else {
                        continue;
                    };
                    let qualified = format!("{namespace}\\{}", declaration.name);
                    records.push(namespace_record(
                        analysis,
                        declaration,
                        entity,
                        &qualified,
                        &namespace,
                    ));
                }
                _ => {}
            }
        }
    }
    records
}

fn namespace_record(
    analysis: &FileAnalysis,
    declaration: &crate::model::Declaration,
    entity: &super::model::StructuralEntity,
    qualified: &str,
    namespace: &str,
) -> LinkRecord {
    let source = FactLocator::declaration(&analysis.file.relative_path, declaration.declaration_id);
    let outcome = LinkOutcome::exact(LinkTarget::Structure {
        entity_id: entity.entity_id.clone(),
        structural_kind: entity.structural_kind.clone(),
        key: entity.key.clone(),
    });
    let provenance = LinkProvenance {
        language: "php".to_string(),
        rule_id: rule::PHP_NAMESPACE_DECLARATION.to_string(),
        source: source.clone(),
        written: qualified.to_string(),
        evidence: vec![
            format!("namespace={namespace}"),
            format!("declaration_kind={}", declaration.kind.as_str()),
            format!("qualified_name={qualified}"),
        ],
        metadata: Vec::new(),
    };
    LinkRecord::new(
        "namespace_membership",
        source,
        qualified.to_string(),
        outcome,
        provenance,
    )
}

// ---------------------------------------------------------------------------
// php.use.qualified_name / php.use.external / php.use.non_class_import
// ---------------------------------------------------------------------------

fn use_links(structure: &Structure, analyses: &[FileAnalysis]) -> Vec<LinkRecord> {
    let mut records = Vec::new();
    for analysis in analyses {
        if analysis.file.language != LanguageId::Php {
            continue;
        }
        for import in &analysis.imports {
            for (index, item) in import.items.iter().enumerate() {
                let source = FactLocator {
                    relative_path: analysis.file.relative_path.clone(),
                    fact_kind: FactKind::Import,
                    fact_id: import.import_id,
                    item_index: Some(index as u32),
                };
                // A grouped `use A\B\{C, D};` carries the shared prefix in
                // `module` and each entry relative to it, so the written
                // qualified name is composed from the two written parts.
                let written = match &import.module {
                    Some(module) => format!("{module}\\{}", item.target),
                    None => item.target.clone(),
                };
                let mut evidence = vec![
                    format!("category={}", item.category.as_str()),
                    format!("written_target={written}"),
                ];
                if let Some(module) = &import.module {
                    evidence.push(format!("grouped_prefix={module}"));
                }
                if let Some(alias) = &item.alias {
                    evidence.push(format!("alias={alias}"));
                }

                let (rule_id, outcome) = match item.category {
                    // Function and constant imports are never linked to class
                    // declarations.
                    ImportCategory::Function | ImportCategory::Constant => (
                        rule::PHP_USE_NON_CLASS_IMPORT,
                        LinkOutcome::out_of_scope(format!(
                            "`{}` is a {} import; TASK 3B does not link function or constant \
                             imports, and never links them to class declarations",
                            written,
                            item.category.as_str()
                        )),
                    ),
                    _ => match structure.php_qualified.get(&written) {
                        Some(declarations) if !declarations.is_empty() => {
                            let mut candidates: Vec<LinkTarget> = declarations
                                .iter()
                                .map(|declaration| declaration.target(&written))
                                .collect();
                            super::model::sort_candidates(&mut candidates);
                            evidence.push(format!(
                                "syntactic_qualified_declarations={}",
                                candidates.len()
                            ));
                            if candidates.len() == 1 {
                                (
                                    rule::PHP_USE_QUALIFIED_NAME,
                                    LinkOutcome::exact(candidates.remove(0)),
                                )
                            } else {
                                (
                                    rule::PHP_USE_QUALIFIED_NAME,
                                    LinkOutcome::Ambiguous { candidates },
                                )
                            }
                        }
                        _ => {
                            let namespace = namespace_of(&written);
                            if is_declared_namespace(structure, &namespace) {
                                (
                                    rule::PHP_USE_QUALIFIED_NAME,
                                    LinkOutcome::unresolved(format!(
                                        "namespace `{namespace}` is declared in the snapshot but no \
                                         declaration has the syntactic qualified name `{written}`"
                                    )),
                                )
                            } else {
                                (
                                    rule::PHP_USE_EXTERNAL,
                                    LinkOutcome::out_of_scope(format!(
                                        "namespace `{namespace}` is not declared anywhere in the \
                                         snapshot, so `{written}` is treated as external"
                                    )),
                                )
                            }
                        }
                    },
                };

                let provenance = LinkProvenance {
                    language: "php".to_string(),
                    rule_id: rule_id.to_string(),
                    source: source.clone(),
                    written: written.clone(),
                    evidence,
                    metadata: Vec::new(),
                };
                records.push(LinkRecord::new(
                    "use_import",
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

/// The namespace part of a qualified name, i.e. everything before the last
/// separator. Empty for a name with no namespace.
fn namespace_of(qualified: &str) -> String {
    match qualified.rfind('\\') {
        Some(index) => qualified[..index].to_string(),
        None => String::new(),
    }
}

/// True when `namespace`, or any of its ancestors, is declared in the snapshot.
fn is_declared_namespace(structure: &Structure, namespace: &str) -> bool {
    if namespace.is_empty() {
        return false;
    }
    let segments: Vec<&str> = namespace.split('\\').collect();
    for end in (1..=segments.len()).rev() {
        if structure
            .php_namespace_prefixes
            .contains(&segments[..end].join("\\"))
        {
            return true;
        }
    }
    false
}

/// The written namespace enclosing `scope_id`.
fn enclosing_namespace(analysis: &FileAnalysis, scope_id: u32) -> Option<String> {
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
