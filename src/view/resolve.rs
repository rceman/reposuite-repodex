//! Resolve a ViewLocator to a canonical RepositoryView (§16-§18).
//!
//! `root` is the intentional escape hatch (any absolute path). `project` +
//! `view` resolve through the registry; a `view` name must be a simple relative
//! identifier contained inside a registered view root after canonicalization —
//! no traversal, no absolute path, no symlink escape (§17-§18).

use std::path::{Path, PathBuf};

use super::git;
use super::model::{err, RepositoryView, ViewError, ViewLocator, ViewResult};
use super::registry::ProjectRegistry;

/// Validate a `view` name: a simple relative identifier, no `..`, no root, no
/// separators that escape the view root (§17).
fn valid_view_name(name: &str) -> bool {
    if name.is_empty() || name.starts_with('/') || name.starts_with('~') {
        return false;
    }
    if name.contains("..") {
        return false;
    }
    // single path component or nested relative path without parent refs
    !name
        .split('/')
        .any(|c| c.is_empty() || c == ".." || c == ".")
}

/// Containment check: `child` must live inside `root` after canonicalization
/// (§18). Both are canonicalized first so symlinks can't escape.
fn contained(child: &Path, root: &Path) -> bool {
    child.starts_with(root)
}

/// Resolve a locator into a RepositoryView with git metadata. Does not build an
/// index — see `view::index`.
pub fn resolve(locator: &ViewLocator) -> ViewResult<RepositoryView> {
    match locator {
        ViewLocator::Root(root) => resolve_root(root, "root"),
        ViewLocator::Project { project, view } => resolve_project(project, view.as_deref()),
    }
}

fn make_view(
    repository_id: &str,
    view_id: &str,
    root: &Path,
    locator: &str,
) -> ViewResult<RepositoryView> {
    let canonical = std::fs::canonicalize(root).map_err(|_| {
        (
            ViewError::RootNotFound,
            format!("view root `{}` does not exist", root.display()),
        )
    })?;
    if !canonical.is_dir() {
        return err(
            ViewError::RootNotFound,
            format!("view root `{}` is not a directory", canonical.display()),
        );
    }
    let g = git::read(&canonical);
    Ok(RepositoryView {
        repository_id: repository_id.to_string(),
        view_id: view_id.to_string(),
        canonical_root: canonical,
        head: g.head,
        branch: g.branch,
        detached: g.detached,
        dirty: g.dirty,
        git_common_dir: g.common_dir,
        view_fingerprint: None,
        locator: locator.to_string(),
    })
}

fn resolve_root(root: &Path, locator: &str) -> ViewResult<RepositoryView> {
    let view_id = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "view".to_string());
    make_view("root", &view_id, root, locator)
}

fn resolve_project(alias: &str, view: Option<&str>) -> ViewResult<RepositoryView> {
    let registry = ProjectRegistry::load();
    let project = registry.get(alias).ok_or_else(|| {
        (
            ViewError::ProjectNotFound,
            format!("project `{alias}` is not registered"),
        )
    })?;
    match view {
        None => {
            // Default view = registered root checkout (§7).
            let root = project.root.as_ref().ok_or_else(|| {
                (
                    ViewError::ViewNotFound,
                    format!("project `{alias}` has no default root checkout"),
                )
            })?;
            make_view(alias, "default", root, "project")
        }
        Some(name) => {
            if !valid_view_name(name) {
                return err(
                    ViewError::ViewPathEscape,
                    format!("view name `{name}` is not a safe relative identifier"),
                );
            }
            // Search registered view roots for the named view.
            for vroot in &project.view_roots {
                let candidate = vroot.join(name);
                if candidate.is_dir() {
                    let canon_root = std::fs::canonicalize(vroot).unwrap_or_else(|_| vroot.clone());
                    let canon_cand =
                        std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
                    if !contained(&canon_cand, &canon_root) {
                        return err(
                            ViewError::ViewPathEscape,
                            format!("view `{name}` escapes its registered view root"),
                        );
                    }
                    return make_view(alias, name, &canon_cand, "project");
                }
            }
            err(
                ViewError::ViewNotFound,
                format!("view `{name}` not found under project `{alias}` view roots"),
            )
        }
    }
}

/// Build a ViewLocator from CLI flags, enforcing mutual exclusivity (§9).
pub fn locator_from_flags(
    root: Option<&str>,
    project: Option<&str>,
    view: Option<&str>,
) -> ViewResult<ViewLocator> {
    match (root, project, view) {
        (Some(_), Some(_), _) => err(
            ViewError::LocatorConflict,
            "--root and --project are mutually exclusive".to_string(),
        ),
        (Some(_), _, Some(_)) => err(
            ViewError::LocatorConflict,
            "--view requires --project, not --root".to_string(),
        ),
        (Some(r), None, None) => Ok(ViewLocator::Root(PathBuf::from(r))),
        (None, Some(p), v) => Ok(ViewLocator::Project {
            project: p.to_string(),
            view: v.map(|s| s.to_string()),
        }),
        (None, None, Some(_)) => err(
            ViewError::LocatorConflict,
            "--view requires --project".to_string(),
        ),
        (None, None, None) => err(
            ViewError::InvalidRequest,
            "a locator is required: --root <path> or --project <alias> [--view <name>]".to_string(),
        ),
    }
}
