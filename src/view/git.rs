//! Git metadata for a RepositoryView (§19). Uses the `git` CLI when present —
//! it is a standard repository tool, not harness-specific. All fields degrade
//! gracefully (None/false) when git is unavailable or the view is not a Git
//! checkout. Branch is metadata only, never source identity (§20).

use std::path::{Path, PathBuf};
use std::process::Command;

/// Git facts for one checkout root.
#[derive(Debug, Clone, Default)]
pub struct GitMeta {
    pub head: Option<String>,
    pub branch: Option<String>,
    pub detached: bool,
    pub dirty: bool,
    pub common_dir: Option<PathBuf>,
    /// True when the root is inside a Git work tree at all.
    pub is_repository: bool,
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Read git metadata for `root`. Cheap; never fails hard.
pub fn read(root: &Path) -> GitMeta {
    let is_repo = git(root, &["rev-parse", "--is-inside-work-tree"])
        .map(|s| s == "true")
        .unwrap_or(false);
    if !is_repo {
        return GitMeta::default();
    }
    let head = git(root, &["rev-parse", "HEAD"]);
    let branch = git(root, &["symbolic-ref", "--short", "-q", "HEAD"]);
    let detached = head.is_some() && branch.is_none();
    let dirty = git(root, &["status", "--porcelain", "--untracked-files=no"])
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let common_dir = git(root, &["rev-parse", "--git-common-dir"]).map(|p| {
        let pb = PathBuf::from(&p);
        if pb.is_absolute() {
            pb
        } else {
            root.join(pb)
        }
    });
    GitMeta {
        head,
        branch,
        detached,
        dirty,
        common_dir,
        is_repository: true,
    }
}
