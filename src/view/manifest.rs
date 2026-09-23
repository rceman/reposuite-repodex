//! Compute a RepositoryView's effective source manifest: path → content digest
//! (+ language + bytes). This is the content fingerprint used both for
//! content-addressed FileAnalysis reuse and for keying the view's derived index
//! (§26-§28). A per-root cache skips rehashing files whose mtime+size are
//! unchanged, so dirty detection stays cheap without a daemon (§23).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::input;
use crate::model::LanguageId;
use crate::paths::relative_path;
use crate::repository::digest;
use crate::scanner::{walk_files_excluding, ScanOptions};

/// One manifest entry for a source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewFile {
    pub relative_path: String,
    pub language: String,
    pub source_bytes: u64,
    pub content_digest: String,
    /// mtime+size fingerprint used to skip rehashing (not content identity).
    #[serde(default)]
    pub mtime_secs: u64,
    #[serde(default)]
    pub size: u64,
}

/// A view's effective manifest (sorted by relative path).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViewManifest {
    pub files: Vec<ViewFile>,
}

impl ViewManifest {
    /// Content fingerprint — digest over the canonical (path,digest) list.
    /// This is the view's authoritative content identity, independent of cwd,
    /// branch, or HEAD (§20).
    pub fn fingerprint(&self) -> String {
        let mut seed = String::new();
        for f in &self.files {
            seed.push_str(&format!(
                "file path={} lang={} content={}\n",
                f.relative_path, f.language, f.content_digest
            ));
        }
        digest::content_digest(seed.as_bytes())
    }
}

fn mtime_size(path: &Path) -> (u64, u64) {
    let md = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return (0, 0),
    };
    let mtime = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (mtime, md.len())
}

/// Compute the effective manifest for `root`. `cache_path`, when present,
/// stores the previous manifest so unchanged files (by mtime+size) reuse their
/// recorded digest without rehashing.
pub fn compute(root: &Path, cache_path: Option<&Path>) -> std::io::Result<ViewManifest> {
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let previous: BTreeMap<String, ViewFile> = cache_path
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<ViewManifest>(&s).ok())
        .map(|m| {
            m.files
                .into_iter()
                .map(|f| (f.relative_path.clone(), f))
                .collect()
        })
        .unwrap_or_default();

    let (paths, _failures) = walk_files_excluding(
        &canonical,
        ScanOptions {
            respect_gitignore: true,
        },
        &[],
    )?;

    let mut files = Vec::new();
    for path in &paths {
        let Some(language) = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(LanguageId::from_extension)
        else {
            continue;
        };
        let rel = relative_path(&canonical, path);
        let (mtime, size) = mtime_size(path);
        // Reuse recorded digest when mtime+size match (no rehash).
        let (digest_hex, src_bytes) = match previous.get(&rel) {
            Some(prev) if prev.mtime_secs == mtime && prev.size == size => {
                (prev.content_digest.clone(), prev.source_bytes)
            }
            _ => {
                let read = input::read_bounded(path, u64::MAX);
                (digest::content_digest(&read.bytes), read.bytes.len() as u64)
            }
        };
        files.push(ViewFile {
            relative_path: rel,
            language: language.as_str().to_string(),
            source_bytes: src_bytes,
            content_digest: digest_hex,
            mtime_secs: mtime,
            size,
        });
    }
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    let manifest = ViewManifest { files };
    if let Some(p) = cache_path {
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(body) = serde_json::to_string(&manifest) {
            let tmp = p.with_extension("tmp");
            if fs::write(&tmp, body).is_ok() {
                let _ = fs::rename(&tmp, p);
            }
        }
    }
    Ok(manifest)
}

/// Normalize a repo-root relative path for output paths (stable across OSes).
pub fn view_cache_file(state: &Path, root: &Path) -> PathBuf {
    let key = digest::content_digest(root.to_string_lossy().as_bytes());
    state
        .join("views")
        .join(format!("{}.manifest.json", &key[7..23]))
}
