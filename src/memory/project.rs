//! Project-wide memory scope (Part I, §3-§6).
//!
//! Memory is scoped to a *logical project*, not a branch. Worktrees of the same
//! project share a scope via the git common-dir; registered projects share via
//! the alias. An explicit `--root` NEVER uses the bare constant `"root"` — it
//! derives a stable scope from the git common-dir (or the canonical root when no
//! git metadata exists), so unrelated projects cannot collide. Branch name is
//! provenance only, never part of identity.

use crate::repository::digest;
use crate::view::RepositoryView;

/// Derive the canonical memory scope for a view.
///
/// * registered project -> `project:{alias}` (shared across its views/worktrees)
/// * any git checkout -> `repo:{sha256(git_common_dir)}` (worktrees share it)
/// * non-git `--root`   -> `root:{sha256(canonical_root)}` (isolated per dir)
///
/// The result is never the bare `"root"` constant and never contains a branch
/// name, so two unrelated projects cannot accidentally share a scope.
pub fn project_scope(view: &RepositoryView) -> String {
    // Prefer the git common-dir identity: it is the truest logical-repo key,
    // identical across all linked worktrees and branches, and distinct for
    // unrelated repositories regardless of how the view was located.
    if let Some(cd) = &view.git_common_dir {
        return format!(
            "repo:{}",
            digest::content_digest(cd.to_string_lossy().as_bytes())
        );
    }
    if view.repository_id != "root" {
        return format!("project:{}", view.repository_id);
    }
    format!(
        "root:{}",
        digest::content_digest(view.canonical_root.to_string_lossy().as_bytes())
    )
}

/// The scope string a memory contribution was recorded under. Set on
/// `MemoryEntry.project_scope` / `SymbolMemoryEvidence` so a query can match it.
pub fn scope_from_episode(project_id: Option<&str>, repository_id: Option<&str>) -> Option<String> {
    // A contribution only carries a resolvable scope when it named a registered
    // project or a git-derived repository id — a bare "root" is ambiguous and is
    // left unscoped (isolated: it will not match a scoped query).
    let canon = |s: &str| -> Option<String> {
        if s.starts_with("repo:") || s.starts_with("project:") || s.starts_with("root:") {
            Some(s.to_string()) // already a canonical scope
        } else if s == "root" || s.is_empty() {
            None // bare "root" is ambiguous -> unscoped (isolated)
        } else {
            Some(format!("project:{s}")) // plain alias -> project scope
        }
    };
    match (project_id, repository_id) {
        (Some(p), _) => canon(p),
        (None, Some(r)) => canon(r),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn view(repo_id: &str, common: Option<&str>, root: &str) -> RepositoryView {
        RepositoryView {
            repository_id: repo_id.to_string(),
            view_id: "v".into(),
            canonical_root: PathBuf::from(root),
            head: None,
            branch: Some("main".into()),
            detached: false,
            dirty: false,
            git_common_dir: common.map(PathBuf::from),
            view_fingerprint: None,
            locator: "x".into(),
        }
    }

    #[test]
    fn worktrees_share_scope_via_common_dir() {
        let a = view("root", Some("/p/.git"), "/p");
        let b = view("root", Some("/p/.git"), "/p/worktree-b");
        assert_eq!(project_scope(&a), project_scope(&b));
    }

    #[test]
    fn unrelated_roots_never_share() {
        let a = view("root", None, "/tmp/proj-a");
        let b = view("root", None, "/tmp/proj-b");
        assert_ne!(project_scope(&a), project_scope(&b));
        assert_ne!(project_scope(&a), "root");
    }

    #[test]
    fn registered_project_prefers_common_dir_then_alias() {
        // With a git common dir the shared repo identity wins (worktrees unify).
        let v = view("gtw", Some("/x/.git"), "/x");
        assert!(project_scope(&v).starts_with("repo:"));
        // Without git metadata, the registered alias is the scope.
        let v2 = view("gtw", None, "/x");
        assert_eq!(project_scope(&v2), "project:gtw");
    }
}
