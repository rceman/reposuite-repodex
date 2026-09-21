//! `go.call.package_local_function_candidate`: bounded package-local candidate
//! search for Go plain-name calls.
//!
//! For every Go call-like occurrence whose form is `plain_name`, the rule may
//! name a source-written `function` declaration from the call's **own canonical
//! Go package** (the TASK 4A `go_package` entity identity: module + directory +
//! package clause + kind), and only after every closer blocker is ruled out:
//!
//! ```text
//! plain_name call
//!   -> a same-name LocalBindingOccurrence covers the call?  -> NoCandidate
//!   -> a dot import in the file?                            -> NoCandidate
//!   -> an import owns the file-block name?                  -> NoCandidate
//!   -> same-package non-function decl shares the name?      -> NoCandidate
//!   -> collect same-package `function` decls named callee
//!        0 -> NoCandidate  /  1 -> SingleCandidate  /  2+ -> MultipleCandidates
//! ```
//!
//! Everything else is `OutOfScope`. A candidate is evidence a declaration could
//! be relevant, never proof a call resolves to it.

use std::collections::HashMap;

use crate::links::model::{FactKind, FactLocator, StructuralEntity};
use crate::model::{
    CallLikeForm, DeclarationKind, FileAnalysis, ImportCategory, ImportItem, LanguageId,
    LocalBindingOccurrence,
};

use super::model::{
    candidate_rule, CallCandidateRecord, CandidateOutcome, CandidateProvenance, CandidateTarget,
};

/// Every Go call-candidate record derivable from the snapshot.
///
/// Emits one record per Go call-like occurrence: a candidate record for each
/// in-scope `plain_name` call and an `OutOfScope` record for every other call
/// shape, so the artifact is a complete disposition.
pub fn candidates(
    analyses: &[FileAnalysis],
    entities: &[StructuralEntity],
) -> Vec<CallCandidateRecord> {
    let mut records = Vec::new();
    let analyses_by_path: HashMap<&str, &FileAnalysis> = analyses
        .iter()
        .map(|a| (a.file.relative_path.as_str(), a))
        .collect();

    // TASK 4A `go_package` entities are the canonical package authority:
    // file -> package entity, and import_path -> package name for resolving a
    // repository-local import's file-block name without path-segment guessing.
    let mut file_to_package: HashMap<&str, &StructuralEntity> = HashMap::new();
    let mut import_path_to_name: HashMap<String, String> = HashMap::new();
    for entity in entities {
        if entity.structural_kind != "go_package" {
            continue;
        }
        for file in &entity.files {
            file_to_package.insert(file.as_str(), entity);
        }
        if let Some(import_path) = entity
            .assumptions
            .iter()
            .find_map(|a| a.strip_prefix("import_path="))
        {
            // The package name is the key's `<dir>:<name>` name segment.
            if let Some(name) = entity.key.rsplit(':').next() {
                import_path_to_name
                    .entry(import_path.to_string())
                    .or_insert_with(|| name.to_string());
            }
        }
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

        // File-block import evidence: dot imports poison the whole file block;
        // a determinable local name (alias, or a resolved repo-local package
        // name) blocks a same-name call.
        let mut dot_import = false;
        let mut import_names: Vec<String> = Vec::new();
        for import in &analysis.imports {
            for item in &import.items {
                match import_local_name(item, &import_path_to_name) {
                    LocalImport::Dot => dot_import = true,
                    LocalImport::Named(name) => import_names.push(name),
                    LocalImport::None => {}
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

            let (outcome, provenance) = if call.form != CallLikeForm::PlainName {
                (
                    CandidateOutcome::out_of_scope(format!("form={}", call.form.as_str())),
                    provenance(Vec::new()),
                )
            } else {
                let name = call.callee_written.as_str();
                let position = call.callee_range.byte_start;
                // 1. A covering same-name local binding is the nearest owner.
                if let Some(blocker) = covering_binding(&bindings_by_name, name, position) {
                    (
                        CandidateOutcome::no_candidate("shadowed_by_local_binding"),
                        provenance(vec![
                            format!("blocker_kind={}", blocker.kind.as_str()),
                            format!(
                                "blocker_range={}..{}",
                                blocker.name_range.byte_start, blocker.name_range.byte_end
                            ),
                        ]),
                    )
                // 2. A dot import makes the file block un-enumerable.
                } else if dot_import {
                    (
                        CandidateOutcome::no_candidate("dot_import_namespace_uncertain"),
                        provenance(vec!["dot_import".to_string()]),
                    )
                // 3. An import owns the file-block name.
                } else if import_names.iter().any(|n| n == name) {
                    (
                        CandidateOutcome::no_candidate("blocked_by_import_binding"),
                        provenance(vec![format!("import_name={name}")]),
                    )
                // 4. Same-package source declarations.
                } else {
                    package_function_candidates(package, name, &analyses_by_path, &provenance)
                }
            };
            records.push(CallCandidateRecord::new(
                "go",
                candidate_rule::GO_CALL_PACKAGE_LOCAL_FUNCTION_CANDIDATE,
                source,
                call.callee_written.clone(),
                outcome,
                provenance,
            ));
        }
    }
    records
}

/// The nearest same-name binding covering `position`, by innermost visibility
/// start (§13). `covers` on persisted `visibility_ranges` is authoritative.
fn covering_binding<'a>(
    bindings_by_name: &'a HashMap<&str, Vec<&'a LocalBindingOccurrence>>,
    name: &str,
    position: u32,
) -> Option<&'a LocalBindingOccurrence> {
    bindings_by_name
        .get(name)?
        .iter()
        .filter(|b| b.covers(position))
        // Nearest introduction = latest visibility start.
        .max_by_key(|b| {
            b.visibility_ranges
                .iter()
                .map(|r| r.byte_start)
                .max()
                .unwrap_or(0)
        })
        .copied()
}

/// The file-block local name an import introduces.
enum LocalImport {
    /// `import . "pkg"` — introduces external names into the file block.
    Dot,
    /// A determinable local name (an explicit alias, or a repo-local import
    /// resolved to its package name through TASK 4A topology).
    Named(String),
    /// Blank import, or an external import whose package name is not safely
    /// derivable from persisted facts.
    None,
}

fn import_local_name(
    item: &ImportItem,
    import_path_to_name: &HashMap<String, String>,
) -> LocalImport {
    match item.category {
        ImportCategory::Dot => LocalImport::Dot,
        ImportCategory::Blank => LocalImport::None,
        _ => {
            if let Some(alias) = &item.alias {
                // An explicit alias is the file-block name (`import h "..."`).
                LocalImport::Named(alias.clone())
            } else {
                // A normal import's local name is the imported package's clause
                // name. Resolve it through repository-local topology; never
                // guess it from the final path segment.
                match import_path_to_name.get(&item.target) {
                    Some(name) => LocalImport::Named(name.clone()),
                    None => LocalImport::None,
                }
            }
        }
    }
}

/// Collect same-package `function` declarations named `name`, after ruling out
/// a same-name non-function package declaration (§25/§26).
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
    if !non_function.is_empty() {
        // §26: a same-name non-function declaration makes the package-block
        // namespace ambiguous — prefer no candidate over a false function.
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
