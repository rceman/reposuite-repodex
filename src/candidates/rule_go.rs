//! Bounded Go call-candidate rules.
//!
//! `go.call.package_local_function_candidate` (TASK 4C): a `plain_name` call
//! may name a source-written `function` in the call's own `go_package`.
//!
//! `go.call.imported_package_function_candidate` (TASK 4D): a direct
//! `member_selector` call `pkg.Func()` may name a source-written `function` in
//! a **repository-local** imported package, when `pkg` is this file's import
//! binding, is not shadowed locally, and `Func` is an eligible exported
//! package-level function.
//!
//! ```text
//! plain_name call    -> local-binding / import / namespace blockers -> same-package functions
//! member_selector    -> direct selector? root is a file import? not shadowed?
//!                       repo-local target? terminal exported? -> imported package functions
//! other call form    -> OutOfScope
//! ```
//!
//! Everything else is `OutOfScope`. A candidate is evidence a declaration could
//! be relevant, never proof a call resolves to it.

use std::collections::HashMap;

use crate::links::model::{FactKind, FactLocator, LinkRecord, StructuralEntity};
use crate::model::{
    CallLikeForm, DeclarationKind, FileAnalysis, ImportCategory, LanguageId, LocalBindingOccurrence,
};

use super::model::{
    candidate_rule, CallCandidateRecord, CandidateOutcome, CandidateProvenance, CandidateTarget,
};

/// Every Go call-candidate record derivable from the snapshot.
///
/// Emits one record per Go call-like occurrence under the rule that owns its
/// form (`plain_name` -> package-local, `member_selector` -> imported-package),
/// so the artifact is a complete disposition of every Go call.
pub fn candidates(
    analyses: &[FileAnalysis],
    links: &[LinkRecord],
    entities: &[StructuralEntity],
) -> Vec<CallCandidateRecord> {
    let mut records = Vec::new();
    let analyses_by_path: HashMap<&str, &FileAnalysis> = analyses
        .iter()
        .map(|a| (a.file.relative_path.as_str(), a))
        .collect();

    // TASK 4A `go_package` entities are the canonical package authority:
    // file -> package entity, and entity_id -> entity for resolving the
    // persisted `go.import.local_module` link target.
    let mut file_to_package: HashMap<&str, &StructuralEntity> = HashMap::new();
    let mut entity_by_id: HashMap<&str, &StructuralEntity> = HashMap::new();
    for entity in entities {
        if entity.structural_kind != "go_package" {
            continue;
        }
        for file in &entity.files {
            file_to_package.insert(file.as_str(), entity);
        }
        entity_by_id.insert(entity.entity_id.as_str(), entity);
    }

    // The TASK 4A `import_path` links resolve each import *item* to repo-local
    // `go_package` entities (`go.import.local_module` -> `exact`/`ambiguous`
    // structure targets) or an external path (`go.import.external` ->
    // `out_of_scope`). Keyed by (relative_path, import_id, item_index); a value
    // of `None` marks a linked-but-external import. §14: an ambiguous link may
    // expose several package targets, all of which are evaluated.
    let mut import_link: HashMap<(String, u32, Option<u32>), Vec<&StructuralEntity>> =
        HashMap::new();
    for link in links {
        if link.kind != "import_path" {
            continue;
        }
        let key = (
            link.source.relative_path.clone(),
            link.source.fact_id,
            link.source.item_index,
        );
        let packages: Vec<&StructuralEntity> = link
            .outcome
            .all_targets()
            .iter()
            .filter_map(|t| match t {
                crate::links::model::LinkTarget::Structure {
                    entity_id,
                    structural_kind,
                    ..
                } if structural_kind == "go_package" => {
                    entity_by_id.get(entity_id.as_str()).copied()
                }
                _ => None,
            })
            .collect();
        import_link.insert(key, packages);
    }

    for analysis in analyses {
        if analysis.file.language != LanguageId::Go {
            continue;
        }
        let path = analysis.file.relative_path.as_str();
        let package = file_to_package.get(path).copied();
        let package_key = package.map(|e| e.key.as_str()).unwrap_or("<none>");

        let mut bindings_by_name: HashMap<&str, Vec<&LocalBindingOccurrence>> = HashMap::new();
        for binding in &analysis.bindings {
            bindings_by_name
                .entry(binding.name.as_str())
                .or_default()
                .push(binding);
        }

        // File-block import evidence. `local_names` = determinable file-block
        // names for the package-local rule's import blocker; `selector_roots`
        // maps a selector-root identifier to the import binding it names.
        let mut dot_import = false;
        let mut import_names: Vec<String> = Vec::new();
        let mut selector_roots: HashMap<String, SelectorImport> = HashMap::new();
        for import in &analysis.imports {
            for (item_index, item) in import.items.iter().enumerate() {
                match item.category {
                    ImportCategory::Dot => dot_import = true,
                    ImportCategory::Blank => {}
                    _ => {
                        let packages: Vec<&StructuralEntity> = import_link
                            .get(&(path.to_string(), import.import_id, Some(item_index as u32)))
                            .cloned()
                            .unwrap_or_default();
                        let local = if let Some(alias) = &item.alias {
                            Some(alias.clone())
                        } else if let Some(pkg) = packages.first() {
                            // Repo-local: the true package clause name from the
                            // entity key, never the path's last segment.
                            pkg.key.rsplit(':').next().map(str::to_string)
                        } else {
                            // External: the package name is unknowable; use the
                            // last path segment only to *detect* that a selector
                            // root names this import (it never yields a target).
                            item.target.rsplit('/').next().map(str::to_string)
                        };
                        if let Some(name) = local {
                            import_names.push(name.clone());
                            let external = packages.is_empty();
                            selector_roots.insert(name, SelectorImport { packages, external });
                        }
                    }
                }
            }
        }

        for call in &analysis.calls {
            let source = FactLocator {
                relative_path: path.to_string(),
                fact_kind: FactKind::Call,
                fact_id: call.call_id,
                item_index: None,
            };
            let scope_path = analysis.scope_path(call.scope_id);
            let provenance = |evidence: Vec<String>| CandidateProvenance {
                scope_path: scope_path.clone(),
                search_levels: None,
                enclosing_module: Some(package_key.to_string()),
                evidence,
            };

            let (outcome, provenance, rule_id) = match call.form {
                CallLikeForm::PlainName => {
                    let (o, p) = plain_name(
                        call,
                        package,
                        &bindings_by_name,
                        dot_import,
                        &import_names,
                        &analyses_by_path,
                        &provenance,
                    );
                    (
                        o,
                        p,
                        candidate_rule::GO_CALL_PACKAGE_LOCAL_FUNCTION_CANDIDATE,
                    )
                }
                CallLikeForm::MemberSelector => {
                    let (o, p) = imported_selector(
                        call,
                        &selector_roots,
                        &bindings_by_name,
                        &analyses_by_path,
                        &provenance,
                    );
                    (
                        o,
                        p,
                        candidate_rule::GO_CALL_IMPORTED_PACKAGE_FUNCTION_CANDIDATE,
                    )
                }
                _ => (
                    CandidateOutcome::out_of_scope(format!("form={}", call.form.as_str())),
                    provenance(Vec::new()),
                    candidate_rule::GO_CALL_PACKAGE_LOCAL_FUNCTION_CANDIDATE,
                ),
            };
            records.push(CallCandidateRecord::new(
                "go",
                rule_id,
                source,
                call.callee_written.clone(),
                outcome,
                provenance,
            ));
        }
    }
    records
}

/// TASK 4C: `plain_name` -> same-package `function`, after blockers.
fn plain_name(
    call: &crate::model::CallLikeOccurrence,
    package: Option<&StructuralEntity>,
    bindings_by_name: &HashMap<&str, Vec<&LocalBindingOccurrence>>,
    dot_import: bool,
    import_names: &[String],
    analyses_by_path: &HashMap<&str, &FileAnalysis>,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance) {
    let name = call.callee_written.as_str();
    let position = call.callee_range.byte_start;
    if let Some(blocker) = covering_binding(bindings_by_name, name, position) {
        return (
            CandidateOutcome::no_candidate("shadowed_by_local_binding"),
            provenance(vec![
                format!("blocker_kind={}", blocker.kind.as_str()),
                format!(
                    "blocker_range={}..{}",
                    blocker.name_range.byte_start, blocker.name_range.byte_end
                ),
            ]),
        );
    }
    if dot_import {
        return (
            CandidateOutcome::no_candidate("dot_import_namespace_uncertain"),
            provenance(vec!["dot_import".to_string()]),
        );
    }
    if import_names.iter().any(|n| n == name) {
        return (
            CandidateOutcome::no_candidate("blocked_by_import_binding"),
            provenance(vec![format!("import_name={name}")]),
        );
    }
    package_function_candidates(package, name, analyses_by_path, provenance)
}

/// The file-block package(s) an import item binds, for the selector rule.
struct SelectorImport<'a> {
    /// The repo-local `go_package` entities this import targets via the
    /// persisted `go.import.local_module` link. Empty for external imports.
    packages: Vec<&'a StructuralEntity>,
    /// True when the import path is outside every repo-local module.
    external: bool,
}

/// TASK 4D: `member_selector` -> repo-local imported-package `function`.
///
/// Only a *direct* `pkg.Func()` selector qualifies. The root must be one of
/// this file's import bindings and must not be shadowed locally; the terminal
/// must be an exported identifier naming a package-level `function`.
fn imported_selector(
    call: &crate::model::CallLikeOccurrence,
    selector_roots: &HashMap<String, SelectorImport>,
    bindings_by_name: &HashMap<&str, Vec<&LocalBindingOccurrence>>,
    analyses_by_path: &HashMap<&str, &FileAnalysis>,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance) {
    let Some((root, terminal)) = direct_selector(&call.callee_written) else {
        // Not a single `ident.ident` shape — a method/field/multi-hop selector.
        return (
            CandidateOutcome::out_of_scope("not_a_direct_package_selector"),
            provenance(Vec::new()),
        );
    };
    let Some(import) = selector_roots.get(root) else {
        // `obj.Method()` / `pkg` is not an import here -> receiver/value selector.
        return (
            CandidateOutcome::out_of_scope("selector_root_not_an_import"),
            provenance(Vec::new()),
        );
    };
    // The root is this file's import binding, but a covering same-name local
    // binding owns it at the call position -> it is a method call on the local
    // value, not a package call (§18-§22).
    let root_position = call.callee_range.byte_start;
    if let Some(blocker) = covering_binding(bindings_by_name, root, root_position) {
        return (
            CandidateOutcome::no_candidate("import_root_shadowed_by_local_binding"),
            provenance(vec![
                format!("blocker_kind={}", blocker.kind.as_str()),
                format!("root={root}"),
            ]),
        );
    }
    if import.external {
        return (
            CandidateOutcome::out_of_scope("external_import"),
            provenance(vec![format!("root={root}")]),
        );
    }
    // §16/§25: an unexported terminal is not accessible across the package
    // boundary. This is the lexical export rule, not type checking.
    if !is_exported(terminal) {
        return (
            CandidateOutcome::no_candidate("imported_function_not_exported"),
            provenance(vec![format!("terminal={terminal}")]),
        );
    }
    // §14: evaluate every structurally permitted package target.
    let mut functions = Vec::new();
    let mut non_function = Vec::new();
    for package in &import.packages {
        collect_package_declarations(
            package,
            terminal,
            analyses_by_path,
            &mut functions,
            &mut non_function,
        );
    }
    if !non_function.is_empty() {
        return (
            CandidateOutcome::no_candidate("imported_package_namespace_ambiguous"),
            provenance(vec![
                format!("root={root}"),
                format!("ambiguous_with={}", non_function.join(",")),
            ]),
        );
    }
    let outcome = CandidateOutcome::from_candidates(functions, "no_imported_function");
    (outcome, provenance(vec![format!("root={root}")]))
}

/// Split a normalized `member_selector` `callee_written` into a direct
/// `(root_identifier, terminal)` pair.
///
/// Invariant: a `member_selector` callee is `<operand>.<field>`. The field is a
/// `field_identifier` and never contains `.`, so splitting on the last `.`
/// separates operand from terminal. The call is a *direct package selector*
/// only when the operand text is itself a single Go identifier — `a.b` or
/// `x[0]` as the operand is a multi-hop selector and is rejected.
fn direct_selector(callee_written: &str) -> Option<(&str, &str)> {
    let (root, terminal) = callee_written.rsplit_once('.')?;
    if is_identifier(root) && is_identifier(terminal) {
        Some((root, terminal))
    } else {
        None
    }
}

/// A bare Go identifier (letter/underscore start, then letters/digits/`_`).
fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(c) if c == '_' || c.is_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c == '_' || c.is_alphanumeric())
}

/// Go's export rule: the first character is a Unicode uppercase letter.
fn is_exported(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// Collect a package's `function` and non-function declarations named `name`
/// into `functions`/`non_function` (shared by the package-local and
/// imported-package rules).
fn collect_package_declarations(
    package: &StructuralEntity,
    name: &str,
    analyses_by_path: &HashMap<&str, &FileAnalysis>,
    functions: &mut Vec<CandidateTarget>,
    non_function: &mut Vec<String>,
) {
    for file in &package.files {
        let Some(member) = analyses_by_path.get(file.as_str()) else {
            continue;
        };
        for decl in &member.declarations {
            if decl.name != name {
                continue;
            }
            if decl.kind == DeclarationKind::Function {
                functions.push(CandidateTarget::new(
                    member.file.relative_path.clone(),
                    decl.declaration_id,
                    "function",
                    decl.name.clone(),
                    member.scope_path(decl.scope_id),
                ));
            } else {
                non_function.push(format!(
                    "{}#{}:{}",
                    member.file.relative_path,
                    decl.declaration_id,
                    decl.kind.as_str()
                ));
            }
        }
    }
}

/// The nearest same-name binding covering `position`, by innermost visibility
/// start. `covers` on persisted `visibility_ranges` is authoritative.
fn covering_binding<'a>(
    bindings_by_name: &'a HashMap<&str, Vec<&'a LocalBindingOccurrence>>,
    name: &str,
    position: u32,
) -> Option<&'a LocalBindingOccurrence> {
    bindings_by_name
        .get(name)?
        .iter()
        .filter(|b| b.covers(position))
        .max_by_key(|b| {
            b.visibility_ranges
                .iter()
                .map(|r| r.byte_start)
                .max()
                .unwrap_or(0)
        })
        .copied()
}

/// Collect same-package `function` declarations named `name`, after ruling out
/// a same-name non-function package declaration.
fn package_function_candidates(
    package: Option<&StructuralEntity>,
    name: &str,
    analyses_by_path: &HashMap<&str, &FileAnalysis>,
    provenance: &dyn Fn(Vec<String>) -> CandidateProvenance,
) -> (CandidateOutcome, CandidateProvenance) {
    let Some(package) = package else {
        return (
            CandidateOutcome::no_candidate("no_package_context"),
            provenance(Vec::new()),
        );
    };
    let mut functions = Vec::new();
    let mut non_function = Vec::new();
    collect_package_declarations(
        package,
        name,
        analyses_by_path,
        &mut functions,
        &mut non_function,
    );
    if !non_function.is_empty() {
        let mut evidence = vec![format!("ambiguous_with={}", non_function.join(","))];
        evidence.push(format!("package={}", package.key));
        return (
            CandidateOutcome::no_candidate("package_namespace_ambiguous"),
            provenance(evidence),
        );
    }
    let outcome = CandidateOutcome::from_candidates(functions, "no_package_function");
    (
        outcome,
        provenance(vec![format!("package={}", package.key)]),
    )
}
