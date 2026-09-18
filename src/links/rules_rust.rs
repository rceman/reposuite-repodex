//! Rust structural link rules.
//!
//! Two bounded rules:
//!
//! * [`rule::RUST_MOD_STANDARD_FILE`] — `mod name;` mapped onto the standard
//!   source-file forms.
//! * [`rule::RUST_USE_CRATE_PATH`] — `use crate::...` resolved against the
//!   structural module tree built from `mod` declarations.
//!
//! `self::`, `super::`, external and prelude paths are not resolved. There is no
//! name resolution, no glob expansion, no re-export following and no macro
//! expansion.

use crate::model::{FileAnalysis, ImportForm, ImportOccurrence, LanguageId};

use super::model::{rule, FactLocator, LinkOutcome, LinkProvenance, LinkRecord, LinkTarget};
use super::structure::{RustCrate, Structure};

/// Every Rust structural relationship derivable from the snapshot.
pub fn links(structure: &Structure, analyses: &[FileAnalysis]) -> Vec<LinkRecord> {
    let mut records = Vec::new();
    records.extend(module_file_links(structure));
    records.extend(use_path_links(structure, analyses));
    records
}

// ---------------------------------------------------------------------------
// rust.mod.standard_file
// ---------------------------------------------------------------------------

fn module_file_links(structure: &Structure) -> Vec<LinkRecord> {
    let mut records = Vec::new();
    for external in structure.rust_external_mods.values() {
        let candidates: Vec<LinkTarget> = external
            .present
            .iter()
            .map(|path| LinkTarget::file(path.clone()))
            .collect();
        let attempted = external.attempted.join(", ");
        let reason = format!("neither standard module file exists (looked for {attempted})");
        let outcome = LinkOutcome::from_candidates(candidates, reason);
        let source = FactLocator::declaration(&external.relative_path, external.declaration_id);
        let provenance = LinkProvenance {
            language: "rust".to_string(),
            rule_id: rule::RUST_MOD_STANDARD_FILE.to_string(),
            source: source.clone(),
            written: external.written.clone(),
            evidence: vec![
                "syntax=mod-item".to_string(),
                format!("candidate_files=[{attempted}]"),
            ],
            metadata: Vec::new(),
        };
        records.push(LinkRecord::new(
            "module_file",
            source,
            external.written.clone(),
            outcome,
            provenance,
        ));
    }
    records
}

// ---------------------------------------------------------------------------
// rust.use.crate_path
// ---------------------------------------------------------------------------

/// One `use` path to resolve.
struct UsePath {
    /// Effective written path, formed from written source parts.
    written: String,
    /// Segments after the leading `crate`.
    segments: Vec<String>,
    /// Index of the import item this path came from, when it came from one.
    item_index: Option<u32>,
    /// True when the path is a glob (`use crate::a::*`), which targets the
    /// module rather than a name inside it.
    glob: bool,
}

fn use_paths(import: &ImportOccurrence) -> Vec<UsePath> {
    let mut paths = Vec::new();
    match import.form {
        ImportForm::Single => {
            if let Some(item) = import.items.first() {
                paths.push(UsePath {
                    written: item.target.clone(),
                    segments: split_path(&item.target),
                    item_index: Some(0),
                    glob: false,
                });
            }
        }
        ImportForm::Wildcard => {
            let module = import.module.clone().unwrap_or_default();
            paths.push(UsePath {
                written: format!("{module}::*"),
                segments: split_path(&module),
                item_index: None,
                glob: true,
            });
        }
        ImportForm::Grouped => {
            let module = import.module.clone().unwrap_or_default();
            for (index, item) in import.items.iter().enumerate() {
                let mut segments = split_path(&module);
                let item_segments = split_path(&item.target);
                let glob = item.wildcard;
                segments.extend(item_segments.iter().cloned());
                paths.push(UsePath {
                    written: format!("{module}::{}", item.target),
                    segments,
                    item_index: Some(index as u32),
                    glob,
                });
            }
        }
    }
    paths
}

fn split_path(path: &str) -> Vec<String> {
    path.split("::")
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

fn use_path_links(structure: &Structure, analyses: &[FileAnalysis]) -> Vec<LinkRecord> {
    let mut records = Vec::new();
    for analysis in analyses {
        if analysis.file.language != LanguageId::Rust {
            continue;
        }
        for import in &analysis.imports {
            for path in use_paths(import) {
                let source = FactLocator {
                    relative_path: analysis.file.relative_path.clone(),
                    fact_kind: super::model::FactKind::Import,
                    fact_id: import.import_id,
                    item_index: path.item_index,
                };
                let (outcome, evidence) =
                    if path.segments.first().map(String::as_str) == Some("crate") {
                        resolve_crate_path(structure, &analysis.file.relative_path, &path)
                    } else {
                        (
                            LinkOutcome::out_of_scope(format!(
                            "path `{}` does not begin with `crate`; self/super/external paths are \
                             not resolved in TASK 3B",
                            path.written
                        )),
                            vec![format!(
                                "leading_segment={}",
                                path.segments.first().cloned().unwrap_or_default()
                            )],
                        )
                    };
                let rule_id = if path.segments.first().map(String::as_str) == Some("crate") {
                    rule::RUST_USE_CRATE_PATH
                } else {
                    rule::RUST_USE_NON_CRATE_PATH
                };
                let provenance = LinkProvenance {
                    language: "rust".to_string(),
                    rule_id: rule_id.to_string(),
                    source: source.clone(),
                    written: path.written.clone(),
                    evidence,
                    metadata: Vec::new(),
                };
                records.push(LinkRecord::new(
                    "use_path",
                    source,
                    path.written.clone(),
                    outcome,
                    provenance,
                ));
            }
        }
    }
    records
}

/// Resolve one `use crate::...` path.
fn resolve_crate_path(
    structure: &Structure,
    file: &str,
    path: &UsePath,
) -> (LinkOutcome, Vec<String>) {
    // `path.segments[0] == "crate"`.
    let after_crate = &path.segments[1..];
    if after_crate.is_empty() {
        return (
            LinkOutcome::unresolved("`use crate;` names the crate root and imports nothing"),
            vec!["segments_after_crate=0".to_string()],
        );
    }

    let owners: Vec<&RustCrate> = structure
        .rust_crates
        .iter()
        .filter(|krate| krate.files.contains(file))
        .collect();
    if owners.is_empty() {
        return (
            LinkOutcome::unresolved(format!(
                "no crate root reaches `{file}` through `mod` declarations, so `crate::` has no \
                 structural meaning here"
            )),
            vec!["crate_roots=0".to_string()],
        );
    }

    let mut candidates: Vec<LinkTarget> = Vec::new();
    let mut evidence: Vec<String> = Vec::new();
    let mut failure_reasons: Vec<String> = Vec::new();

    for owner in &owners {
        evidence.push(format!("crate_root={}", owner.root_file));
        let (prefix, final_segment) = if path.glob {
            (after_crate, None)
        } else {
            let (final_segment, prefix) = after_crate.split_last().expect("non-empty");
            (prefix, Some(final_segment))
        };
        match descend(owner, prefix) {
            Err(reason) => failure_reasons.push(format!("{}: {reason}", owner.root_file)),
            Ok(modules) => {
                evidence.push(format!("module_path=crate::{}", prefix.join("::")));
                for index in modules {
                    let module = &owner.modules[index];
                    match final_segment {
                        // A glob targets the module itself.
                        None => candidates.push(LinkTarget::file(module.file.clone())),
                        Some(name) => {
                            for declaration in &module.declarations {
                                if declaration.name == *name {
                                    candidates.push(LinkTarget::declaration(
                                        declaration.relative_path.clone(),
                                        declaration.declaration_id,
                                        format!("crate::{}", module.path[1..].join("::")),
                                        declaration.declaration_kind.clone(),
                                        declaration.name.clone(),
                                    ));
                                }
                            }
                            if let Some(children) = module.children.get(name) {
                                for child in children {
                                    candidates
                                        .push(LinkTarget::file(owner.modules[*child].file.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if candidates.is_empty() && !failure_reasons.is_empty() {
        evidence.push(format!("failures=[{}]", failure_reasons.join("; ")));
    }

    let empty_reason = if failure_reasons.is_empty() {
        format!(
            "`crate::{}` resolved to a module but named no declaration or child module in it",
            after_crate.join("::")
        )
    } else {
        format!(
            "`crate::{}` could not be resolved: {}",
            after_crate.join("::"),
            failure_reasons.join("; ")
        )
    };
    (
        LinkOutcome::from_candidates(candidates, empty_reason),
        evidence,
    )
}

/// Walk `prefix` from the crate root, returning every module it could reach.
fn descend(krate: &RustCrate, prefix: &[String]) -> Result<Vec<usize>, String> {
    let mut current = vec![0usize];
    for segment in prefix {
        let mut next = Vec::new();
        for index in &current {
            if let Some(children) = krate.modules[*index].children.get(segment) {
                next.extend(children.iter().copied());
            }
        }
        if next.is_empty() {
            return Err(format!(
                "no module `{segment}` reachable from the crate root"
            ));
        }
        next.sort_unstable();
        next.dedup();
        current = next;
    }
    Ok(current)
}
