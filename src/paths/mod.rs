//! Central resolver for RepoDex runtime state.
//!
//! Every future piece of RepoDex runtime state lives under one root:
//!
//! ```text
//! <root>/
//! ├── config/
//! ├── cache/
//! ├── indexes/
//! ├── projects/
//! ├── benchmarks/
//! └── tmp/
//! ```
//!
//! The default root is `<home>/reposuite/repodex`, where `<home>` is resolved
//! through the platform's home-directory environment variables. The literal
//! string `~` is never concatenated.
//!
//! The root can be overridden with `REPOSUITE_REPODEX_HOME`.
//!
//! TASK 1 only *resolves* paths. The `languages`, `parse` and `scan` commands
//! never create directories, so no persistent runtime state appears until a
//! later task needs it.

use std::path::{Component, Path, PathBuf};

/// Environment variable that overrides the runtime root.
pub const HOME_ENV: &str = "REPOSUITE_REPODEX_HOME";

/// Directory names under the runtime root, in reporting order.
pub const SUBDIRECTORIES: [&str; 6] = [
    "config",
    "cache",
    "indexes",
    "projects",
    "benchmarks",
    "tmp",
];

/// Resolved RepoDex runtime paths. This type never touches the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoDexPaths {
    root: PathBuf,
}

impl RepoDexPaths {
    /// Resolve the runtime root from the environment.
    ///
    /// Returns `None` when no override is set and no home directory can be
    /// determined.
    pub fn from_env() -> Option<Self> {
        if let Some(override_root) = std::env::var_os(HOME_ENV) {
            if !override_root.is_empty() {
                return Some(Self::with_root(PathBuf::from(override_root)));
            }
        }
        let home = home_dir()?;
        Some(Self::with_root(home.join("reposuite").join("repodex")))
    }

    /// Build a resolver for an explicit root. Used by tests and by the
    /// environment override.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_dir(&self) -> PathBuf {
        self.subdirectory("config")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.subdirectory("cache")
    }

    pub fn indexes_dir(&self) -> PathBuf {
        self.subdirectory("indexes")
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.subdirectory("projects")
    }

    /// Root for generated benchmark evidence. TASK 1 only resolves it.
    pub fn benchmarks_dir(&self) -> PathBuf {
        self.subdirectory("benchmarks")
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.subdirectory("tmp")
    }

    pub fn subdirectory(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// All runtime directories as `(name, path)` pairs in a stable order.
    pub fn all_subdirectories(&self) -> Vec<(&'static str, PathBuf)> {
        SUBDIRECTORIES
            .iter()
            .map(|name| (*name, self.subdirectory(name)))
            .collect()
    }
}

/// Platform-appropriate home directory resolution.
///
/// This is deliberately environment-based rather than crate-based: RepoDex
/// needs one directory and does not want a dependency for it.
pub fn home_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        if let Some(profile) = non_empty_env("USERPROFILE") {
            return Some(PathBuf::from(profile));
        }
        let drive = non_empty_env("HOMEDRIVE");
        let path = non_empty_env("HOMEPATH");
        if let (Some(drive), Some(path)) = (drive, path) {
            return Some(PathBuf::from(format!("{drive}{path}")));
        }
    } else if let Some(home) = non_empty_env("HOME") {
        return Some(PathBuf::from(home));
    }
    // Last resort that works on both platforms.
    non_empty_env("USERPROFILE")
        .or_else(|| non_empty_env("HOME"))
        .map(PathBuf::from)
}

fn non_empty_env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => Some(value),
        _ => None,
    }
}

/// Compute the `/`-separated path of `path` relative to `root`.
///
/// Absolute checkout paths must never reach normalized facts, so this is the
/// only way a path enters the model.
pub fn relative_path(root: &Path, path: &Path) -> String {
    let relative = match path.strip_prefix(root) {
        Ok(relative) => relative.to_path_buf(),
        Err(_) => path
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| path.to_path_buf()),
    };
    normalize_separators(&relative)
}

/// Render a path with `/` separators regardless of platform.
pub fn normalize_separators(path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => parts.push("..".to_string()),
            Component::CurDir => {}
            Component::RootDir => parts.push(String::new()),
            Component::Prefix(prefix) => {
                parts.push(prefix.as_os_str().to_string_lossy().into_owned())
            }
        }
    }
    if parts.first().map(|part| part.is_empty()).unwrap_or(false) {
        format!("/{}", parts[1..].join("/"))
    } else {
        parts.join("/")
    }
}
