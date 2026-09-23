//! Durable project registry (§11-§15): `~/reposuite/repodex/projects.json`.
//!
//! A project maps an alias to an optional default checkout `root` plus one or
//! more `view_roots` that contain named RepositoryViews (task worktrees, etc).
//! Writes are atomic (temp file + rename) so concurrent readers never see a
//! partially-written registry. Tests set `REPODEX_STATE_DIR` to an isolated
//! directory and never touch the user's real registry.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::model::{e, err, ViewError, ViewResult};

const REGISTRY_FILE: &str = "projects.json";
const REGISTRY_VERSION: u32 = 1;

/// One registered project (§12).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// Logical project alias (`GTW`).
    pub alias: String,
    /// Optional default/root checkout used when no `--view` is given (§7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<PathBuf>,
    /// Directories that contain named RepositoryViews.
    #[serde(default)]
    pub view_roots: Vec<PathBuf>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RegistryFile {
    version: u32,
    #[serde(default)]
    projects: Vec<Project>,
}

/// Resolve the state directory. `REPODEX_STATE_DIR` overrides the default
/// `~/reposuite/repodex` so tests stay isolated (§11).
pub fn state_dir() -> PathBuf {
    if let Ok(d) = std::env::var("REPODEX_STATE_DIR") {
        return PathBuf::from(d);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("reposuite").join("repodex")
}

pub fn registry_path() -> PathBuf {
    state_dir().join(REGISTRY_FILE)
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Expand a leading `~` and canonicalize before storage (§13).
pub fn normalize_path(p: &str) -> PathBuf {
    let expanded = if let Some(rest) = p.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(rest)
    } else {
        PathBuf::from(p)
    };
    canonical(&expanded)
}

pub struct ProjectRegistry {
    file: RegistryFile,
}

impl ProjectRegistry {
    pub fn load() -> Self {
        let path = registry_path();
        let file = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<RegistryFile>(&s).ok())
            .unwrap_or_else(|| RegistryFile {
                version: REGISTRY_VERSION,
                projects: Vec::new(),
            });
        Self { file }
    }

    fn save(&self) -> ViewResult<()> {
        let path = registry_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .map_err(|x| e(ViewError::Io, format!("{}: {x}", dir.display())))?;
        }
        let body = serde_json::to_string_pretty(&self.file)
            .map_err(|x| e(ViewError::Registry, format!("serialize registry: {x}")))?;
        // Atomic: write temp in same dir then rename (§15).
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, body).map_err(|x| e(ViewError::Io, format!("{}: {x}", tmp.display())))?;
        fs::rename(&tmp, &path)
            .map_err(|x| e(ViewError::Io, format!("{}: {x}", path.display())))?;
        Ok(())
    }

    pub fn get(&self, alias: &str) -> Option<&Project> {
        self.file.projects.iter().find(|p| p.alias == alias)
    }

    pub fn list(&self) -> &[Project] {
        &self.file.projects
    }

    pub fn register(
        &mut self,
        alias: &str,
        root: Option<&str>,
        view_roots: &[String],
    ) -> ViewResult<Project> {
        let root = root.map(|r| {
            let p = normalize_path(r);
            if !p.is_dir() {
                return err(
                    ViewError::RootNotFound,
                    format!("root `{}` is not a directory", r),
                );
            }
            Ok(p)
        });
        let root = match root {
            Some(Ok(p)) => Some(p),
            Some(Err(e)) => return Err(e),
            None => None,
        };
        let mut vroots = Vec::new();
        for vr in view_roots {
            let p = normalize_path(vr);
            if !p.is_dir() {
                return err(
                    ViewError::RootNotFound,
                    format!("view root `{vr}` is not a directory"),
                );
            }
            vroots.push(p);
        }
        let project = Project {
            alias: alias.to_string(),
            root,
            view_roots: vroots,
        };
        self.file.projects.retain(|p| p.alias != alias);
        self.file.projects.push(project.clone());
        self.file.projects.sort_by(|a, b| a.alias.cmp(&b.alias));
        self.file.version = REGISTRY_VERSION;
        self.save()?;
        Ok(project)
    }

    pub fn remove(&mut self, alias: &str) -> ViewResult<()> {
        let before = self.file.projects.len();
        self.file.projects.retain(|p| p.alias != alias);
        if self.file.projects.len() == before {
            return err(
                ViewError::ProjectNotFound,
                format!("project `{alias}` is not registered"),
            );
        }
        self.save()
    }
}
