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
    /// (mtime_ns, size, change_ns) skip-rehash fingerprint — a *hint*, never the
    /// authority on content identity (§8-§9). `change_ns` is the inode
    /// status-change/creation time, which a rewrite bumps even when mtime and
    /// size are forced equal — so a same-size same-second edit can't masquerade.
    #[serde(default)]
    pub mtime_secs: u64,
    #[serde(default)]
    pub mtime_ns: u64,
    #[serde(default)]
    pub change_ns: u64,
    #[serde(default)]
    pub size: u64,
}

/// A metadata file the topology/link producers consult (e.g. `go.mod`,
/// `Cargo.toml`). Recorded so a metadata content/inventory change invalidates
/// the view index even when no source byte changed (§4-§6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataFile {
    pub relative_path: String,
    pub content_digest: String,
}

/// A view's effective manifest (sorted by relative path).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViewManifest {
    pub files: Vec<ViewFile>,
    /// Relevant metadata inputs consulted by derived producers (§6, §18).
    #[serde(default)]
    pub metadata: Vec<MetadataFile>,
}

/// Metadata filenames the link/topology producers actually consult. This is the
/// bounded discovery scope — not arbitrary filesystem scanning (§6).
pub const METADATA_FILES: &[&str] = &["go.mod", "Cargo.toml"];

impl ViewManifest {
    /// Content fingerprint — digest over the canonical (path,digest) list of
    /// source files AND the consulted metadata inputs. This is the view's
    /// authoritative content identity, independent of cwd, branch, or HEAD
    /// (§20). A metadata-only change therefore yields a distinct fingerprint.
    pub fn fingerprint(&self) -> String {
        let mut seed = String::new();
        for f in &self.files {
            seed.push_str(&format!(
                "file path={} lang={} content={}\n",
                f.relative_path, f.language, f.content_digest
            ));
        }
        for m in &self.metadata {
            seed.push_str(&format!(
                "meta path={} content={}\n",
                m.relative_path, m.content_digest
            ));
        }
        digest::content_digest(seed.as_bytes())
    }
}

/// Fast-path skip hint: (mtime_ns, change_ns, size). `change_ns` is the inode
/// status-change time (Unix ctime) or creation time elsewhere — bumped by any
/// write even when mtime+size are held equal, so a same-size same-second edit
/// cannot reuse a stale digest (§8-§9). This is only a *hint*; the content hash
/// remains the authority on identity.
pub fn stat_key(path: &Path) -> (u64, u64, u64) {
    let md = match fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return (0, 0, 0),
    };
    let mtime_ns = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let change_ns = change_time_ns(&md);
    (mtime_ns, change_ns, md.len())
}

#[cfg(unix)]
fn change_time_ns(md: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    md.ctime() as u64 * 1_000_000_000 + md.ctime_nsec() as u64
}
#[cfg(not(unix))]
fn change_time_ns(md: &fs::Metadata) -> u64 {
    // No ctime; use creation time (birth time) — a rename/recreate bumps it, a
    // same-inode rewrite is covered by mtime_ns. Best-effort on non-Unix.
    md.created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
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
    let mut metadata = Vec::new();
    for path in &paths {
        let rel = relative_path(&canonical, path);
        // Metadata input (go.mod/Cargo.toml): always hashed — they are small and
        // their digest is a validity input, not a rehash optimization (§6).
        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if METADATA_FILES.contains(&fname) {
            let read = input::read_bounded(path, u64::MAX);
            metadata.push(MetadataFile {
                relative_path: rel.clone(),
                content_digest: digest::content_digest(&read.bytes),
            });
            continue; // metadata is not an analyzed source file
        }
        let Some(language) = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(LanguageId::from_extension)
        else {
            continue;
        };
        let (mtime_ns, change_ns, size) = stat_key(path);
        // Reuse the recorded digest only when the full stat hint matches.
        let (digest_hex, src_bytes) = match previous.get(&rel) {
            Some(prev)
                if prev.mtime_ns == mtime_ns
                    && prev.change_ns == change_ns
                    && prev.size == size =>
            {
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
            mtime_secs: mtime_ns / 1_000_000_000,
            mtime_ns,
            change_ns,
            size,
        });
    }
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    metadata.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    let manifest = ViewManifest { files, metadata };
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
