//! Python structural link rules.
//!
//! Python resolution is environment-dependent, so these rules are deliberately
//! conservative:
//!
//! * [`rule::PYTHON_RELATIVE_IMPORT_PACKAGE_PATH`] — explicit relative imports,
//!   resolved through the repository's package structure under policy P-PY-1
//!   (the repository root is the package root).
//! * [`rule::PYTHON_ABSOLUTE_IMPORT_LOCAL_CANDIDATE`] — an absolute import that
//!   matches at least one repository module. **Never `Exact`**: absolute
//!   resolution depends on `sys.path`, editable installs and namespace packages,
//!   so a matching repository path is a candidate, not the resolved module.
//! * [`rule::PYTHON_ABSOLUTE_IMPORT_EXTERNAL`] — an absolute import with no
//!   repository module matching.
//!
//! Dynamic imports (`importlib.import_module`, `__import__`) are not resolved;
//! they remain ordinary call-like syntax in the snapshot.

use std::collections::BTreeMap;

use crate::model::{FileAnalysis, ImportForm, ImportOccurrence, LanguageId};

use super::model::{
    rule, FactKind, FactLocator, LinkOutcome, LinkProvenance, LinkRecord, LinkTarget,
};
use super::structure::{directory_of, Structure};

/// Every Python structural relationship derivable from the snapshot.
pub fn links(structure: &Structure, analyses: &[FileAnalysis]) -> Vec<LinkRecord> {
    let by_path: BTreeMap<&str, &FileAnalysis> = analyses
        .iter()
        .map(|analysis| (analysis.file.relative_path.as_str(), analysis))
        .collect();
    let mut records = Vec::new();
    for analysis in analyses {
        if analysis.file.language != LanguageId::Python {
            continue;
        }
        for import in &analysis.imports {
            match import.relative_levels {
                Some(levels) => records.extend(relative_links(
                    structure, &by_path, analysis, import, levels,
                )),
                None => records.extend(absolute_links(structure, &by_path, analysis, import)),
            }
        }
    }
    records
}

/// Declarations named `name` inside the files that define `package`.
fn package_attribute_candidates(
    structure: &Structure,
    by_path: &BTreeMap<&str, &FileAnalysis>,
    package: &str,
    name: &str,
) -> Vec<LinkTarget> {
    let mut candidates = Vec::new();
    for file in structure.python_files_for(package) {
        let Some(analysis) = by_path.get(file.as_str()) else {
            continue;
        };
        for declaration in &analysis.declarations {
            if declaration.name == name {
                candidates.push(LinkTarget::declaration(
                    declaration.relative_path.clone(),
                    declaration.declaration_id,
                    format!("{package}.{name}"),
                    declaration.kind.as_str().to_string(),
                    declaration.name.clone(),
                ));
            }
        }
    }
    candidates
}

// ---------------------------------------------------------------------------
// python.relative_import.package_path
// ---------------------------------------------------------------------------

fn relative_links(
    structure: &Structure,
    by_path: &BTreeMap<&str, &FileAnalysis>,
    analysis: &FileAnalysis,
    import: &ImportOccurrence,
    levels: u32,
) -> Vec<LinkRecord> {
    let path = &analysis.file.relative_path;
    let directory = directory_of(path);
    let base = base_segments(&directory, levels);
    let module = import.module.clone();

    match (base, module) {
        // The relative level escapes the repository root, or the file is not in
        // a package at all.
        (None, module) => {
            let reason = if directory.is_empty() {
                format!(
                    "`{path}` is not inside a package, so a level-{levels} relative import has no \
                     structural meaning (policy P-PY-1)"
                )
            } else {
                format!(
                    "a level-{levels} relative import from `{path}` escapes above the repository \
                     root (policy P-PY-1)"
                )
            };
            vec![relative_record(
                analysis,
                import,
                None,
                relative_written(levels, module.as_deref()),
                LinkOutcome::unresolved(reason),
                vec![format!("levels={levels}")],
            )]
        }
        // `from .mod import a, b` — the statement names one module.
        (Some(base), Some(module)) => {
            let target = join_dotted(&base, &module);
            let candidates: Vec<LinkTarget> = structure
                .python_files_for(&target)
                .into_iter()
                .map(LinkTarget::file)
                .collect();
            let slash = target.replace('.', "/");
            let reason = format!(
                "no repository module defines `{target}` (looked for {slash}.py and \
                 {slash}/__init__.py)"
            );
            vec![relative_record(
                analysis,
                import,
                None,
                relative_written(levels, Some(&module)),
                LinkOutcome::from_candidates(candidates, reason),
                vec![
                    format!("levels={levels}"),
                    format!("base_package={}", base.join(".")),
                    // The written module segment, so an auditor can tell this
                    // apart from `from . import name`, which renders the same
                    // way in `written` but means something different.
                    format!("written_module={module}"),
                ],
            )]
        }
        // `from . import name` — one link per item, because the item decides
        // whether a submodule or a package attribute is meant.
        (Some(base), None) => {
            let package = base.join(".");
            let mut records = Vec::new();
            for (index, item) in import.items.iter().enumerate() {
                let target = join_dotted(&base, &item.target);
                let mut candidates: Vec<LinkTarget> = structure
                    .python_files_for(&target)
                    .into_iter()
                    .map(LinkTarget::file)
                    .collect();
                candidates.extend(package_attribute_candidates(
                    structure,
                    by_path,
                    &package,
                    &item.target,
                ));
                let reason = format!(
                    "`{}` is defined by no repository module and by no package attribute of \
                     `{package}`",
                    item.target
                );
                records.push(relative_record(
                    analysis,
                    import,
                    Some(index as u32),
                    relative_written(levels, Some(&item.target)),
                    LinkOutcome::from_candidates(candidates, reason),
                    vec![
                        format!("levels={levels}"),
                        format!("base_package={package}"),
                        // `from . import name` names no module segment; the item
                        // may denote a submodule *or* a package attribute.
                        "written_module=<none>".to_string(),
                    ],
                ));
            }
            records
        }
    }
}

/// The package segments a relative import is anchored at.
///
/// `levels = 1` anchors at the importing file's own package; each extra level
/// removes one trailing segment. Returning `None` means the import escapes the
/// repository root, or the file is not inside a package.
fn base_segments(directory: &str, levels: u32) -> Option<Vec<String>> {
    if directory.is_empty() {
        return None;
    }
    let segments: Vec<String> = directory
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect();
    let drop = (levels - 1) as usize;
    if drop >= segments.len() {
        return None;
    }
    Some(segments[..segments.len() - drop].to_vec())
}

fn relative_written(levels: u32, module: Option<&str>) -> String {
    let dots = ".".repeat(levels as usize);
    match module {
        Some(module) => format!("{dots}{module}"),
        None => dots,
    }
}

fn join_dotted(base: &[String], tail: &str) -> String {
    if base.is_empty() {
        tail.to_string()
    } else {
        format!("{}.{}", base.join("."), tail)
    }
}

fn relative_record(
    analysis: &FileAnalysis,
    import: &ImportOccurrence,
    item_index: Option<u32>,
    written: String,
    outcome: LinkOutcome,
    evidence: Vec<String>,
) -> LinkRecord {
    build_record(
        analysis,
        import,
        item_index,
        written,
        outcome,
        rule::PYTHON_RELATIVE_IMPORT_PACKAGE_PATH,
        evidence,
    )
}

// ---------------------------------------------------------------------------
// python.absolute_import.local_candidate / python.absolute_import.external
// ---------------------------------------------------------------------------

fn absolute_links(
    structure: &Structure,
    by_path: &BTreeMap<&str, &FileAnalysis>,
    analysis: &FileAnalysis,
    import: &ImportOccurrence,
) -> Vec<LinkRecord> {
    let mut records = Vec::new();
    let module = import.module.clone();

    if import.form == ImportForm::Wildcard {
        let Some(module) = module else {
            return records;
        };
        let candidates: Vec<LinkTarget> = structure
            .python_files_for(&module)
            .into_iter()
            .map(LinkTarget::file)
            .collect();
        records.push(absolute_record(
            analysis,
            import,
            None,
            format!("{module}.*"),
            candidates,
            &module,
        ));
        return records;
    }

    for (index, item) in import.items.iter().enumerate() {
        let (written, candidates) = match &module {
            // `import a.b.c` — the item target is the whole dotted path.
            None => {
                let path = item.target.clone();
                let candidates: Vec<LinkTarget> = structure
                    .python_files_for(&path)
                    .into_iter()
                    .map(LinkTarget::file)
                    .collect();
                (path, candidates)
            }
            // `from pkg import name` — a submodule of `pkg`, or an attribute of
            // the package itself.
            Some(module) => {
                let target = format!("{module}.{}", item.target);
                let mut candidates: Vec<LinkTarget> = structure
                    .python_files_for(&target)
                    .into_iter()
                    .map(LinkTarget::file)
                    .collect();
                candidates.extend(package_attribute_candidates(
                    structure,
                    by_path,
                    module,
                    &item.target,
                ));
                (target, candidates)
            }
        };
        records.push(absolute_record(
            analysis,
            import,
            Some(index as u32),
            written.clone(),
            candidates,
            &written,
        ));
    }
    records
}

fn absolute_record(
    analysis: &FileAnalysis,
    import: &ImportOccurrence,
    item_index: Option<u32>,
    written: String,
    candidates: Vec<LinkTarget>,
    display: &str,
) -> LinkRecord {
    // Absolute imports are never `Exact`: a matching repository path is a
    // candidate, not the resolved module.
    let (rule_id, outcome) = if candidates.is_empty() {
        (
            rule::PYTHON_ABSOLUTE_IMPORT_EXTERNAL,
            LinkOutcome::out_of_scope(format!(
                "no repository module matches `{display}`; absolute resolution depends on \
                 sys.path, so this is treated as external"
            )),
        )
    } else {
        let mut candidates = candidates;
        super::model::sort_candidates(&mut candidates);
        (
            rule::PYTHON_ABSOLUTE_IMPORT_LOCAL_CANDIDATE,
            LinkOutcome::Ambiguous { candidates },
        )
    };
    build_record(
        analysis,
        import,
        item_index,
        written.clone(),
        outcome,
        rule_id,
        vec![
            format!("written_path={written}"),
            "absolute_resolution_depends_on_sys_path=true".to_string(),
        ],
    )
}

// ---------------------------------------------------------------------------
// Shared construction
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn build_record(
    analysis: &FileAnalysis,
    import: &ImportOccurrence,
    item_index: Option<u32>,
    written: String,
    outcome: LinkOutcome,
    rule_id: &str,
    evidence: Vec<String>,
) -> LinkRecord {
    let source = FactLocator {
        relative_path: analysis.file.relative_path.clone(),
        fact_kind: FactKind::Import,
        fact_id: import.import_id,
        item_index,
    };
    let provenance = LinkProvenance {
        language: "python".to_string(),
        rule_id: rule_id.to_string(),
        source: source.clone(),
        written: written.clone(),
        evidence,
        metadata: Vec::new(),
    };
    LinkRecord::new("module_import", source, written, outcome, provenance)
}
