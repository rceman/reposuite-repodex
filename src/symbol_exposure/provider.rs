//! Content-addressed symbol-map provider (§9-§14).
//!
//! Resolves `(path, file_content_digest)` -> the `FileAnalysis` symbol map for
//! exactly that source version. Fast path reuses the indexed analysis when the
//! observed digest equals the indexed `content_digest` (no reparse). Unseen
//! versions are looked up in a digest-keyed cache; when the raw source bytes
//! for that version are available they are parsed once and cached — never
//! parsed per read (§11).

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::model::FileAnalysis;
use crate::repository::{artifact, digest, IndexedFile, RepositoryManifest};

/// Content-addressed source-byte store (§12-§14). Harness-neutral. Keyed by the
/// authoritative `sha256:` content digest. Populated only when RepoDex would
/// otherwise lose a source version; indexed versions are never duplicated.
#[derive(Debug, Default)]
pub struct SourceStore {
    /// digest -> raw source bytes for a non-indexed version.
    bytes: HashMap<String, Rc<Vec<u8>>>,
    /// Number of deduplicated store hits (same digest referenced again).
    pub dedup_hits: u64,
    /// Bytes retained (sum over unique stored digests).
    pub stored_bytes: u64,
    /// Bytes that would have been stored if every put copied unconditionally.
    pub put_attempt_bytes: u64,
}

impl SourceStore {
    /// Offer source bytes for a version. Deduplicates on digest (§13).
    pub fn put(&mut self, bytes: Vec<u8>) -> String {
        let d = digest::content_digest(&bytes);
        self.put_attempt_bytes += bytes.len() as u64;
        if let std::collections::hash_map::Entry::Vacant(e) = self.bytes.entry(d.clone()) {
            self.stored_bytes += bytes.len() as u64;
            e.insert(Rc::new(bytes));
        } else {
            self.dedup_hits += 1;
        }
        d
    }

    /// Look up source bytes for a digest.
    pub fn get(&self, digest: &str) -> Option<&Rc<Vec<u8>>> {
        self.bytes.get(digest)
    }

    pub fn objects(&self) -> usize {
        self.bytes.len()
    }
}

/// Outcome of resolving a symbol map for one (path, digest) (§80).
#[derive(Debug)]
pub struct MapResolution {
    pub analysis: Option<Rc<FileAnalysis>>,
    pub source: super::model::ResolutionSource,
    /// Resolution state when no usable map was produced.
    pub unresolved: Option<super::model::ResolutionState>,
    /// The content digest the map was actually built for.
    pub digest: Option<String>,
}

impl MapResolution {
    fn ok(
        analysis: Option<Rc<FileAnalysis>>,
        source: super::model::ResolutionSource,
        d: String,
    ) -> Self {
        Self {
            analysis,
            source,
            unresolved: None,
            digest: Some(d),
        }
    }
    fn fail(state: super::model::ResolutionState, d: Option<String>) -> Self {
        Self {
            analysis: None,
            source: super::model::ResolutionSource::Unresolved,
            unresolved: Some(state),
            digest: d,
        }
    }
}

/// Resolves `(path, content_digest)` -> the symbol map for that exact version.
pub struct SymbolMapProvider<'a> {
    snap_dir: &'a Path,
    /// path -> IndexedFile (manifest record, carries content_digest + object_key).
    files: HashMap<String, &'a IndexedFile>,
    /// Indexed analyses cached per path (loaded lazily by object_key).
    indexed: RefCell<HashMap<String, Rc<FileAnalysis>>>,
    /// Content-addressed symbol-map cache for non-indexed versions.
    /// key = "{path}#{digest}".
    versions: RefCell<HashMap<String, Rc<FileAnalysis>>>,
    /// Source-byte store for unseen versions.
    sources: &'a SourceStore,
    /// Optional live checkout root — supplies bytes for the indexed version
    /// (whose bytes equal the live file) when capturing decl facets (§11).
    source_root: RefCell<Option<PathBuf>>,
    /// Diagnostics counters (§80).
    pub diag: RefCell<ProviderDiag>,
}

#[derive(Debug, Default)]
pub struct ProviderDiag {
    pub indexed_hit: u64,
    pub cached_version: u64,
    pub new_parse: u64,
    pub assumed_indexed: u64,
    pub missing_source_version: u64,
    pub unsupported_language: u64,
    pub unknown_path: u64,
}

impl<'a> SymbolMapProvider<'a> {
    pub fn new(
        snap_dir: &'a Path,
        manifest: &'a RepositoryManifest,
        sources: &'a SourceStore,
    ) -> Self {
        let files = manifest
            .files
            .iter()
            .map(|f| (f.relative_path.clone(), f))
            .collect();
        Self {
            snap_dir,
            files,
            indexed: RefCell::new(HashMap::new()),
            versions: RefCell::new(HashMap::new()),
            sources,
            source_root: RefCell::new(None),
            diag: RefCell::new(ProviderDiag::default()),
        }
    }

    /// Set the live checkout root so indexed-version bytes (== the live file)
    /// can be read for decl-facet capture (§11).
    pub fn with_source_root(self, root: &Path) -> Self {
        *self.source_root.borrow_mut() = Some(root.to_path_buf());
        self
    }

    /// Source bytes for `digest`, when addressable: the content-addressed store
    /// (non-indexed versions) or the live file when `digest` matches the indexed
    /// version of `path`. `None` when the version's bytes are unknown (§13).
    pub fn bytes_for(&self, path: &str, digest: Option<&str>) -> Option<Rc<Vec<u8>>> {
        if let Some(d) = digest {
            if let Some(b) = self.sources.get(d) {
                return Some(b.clone());
            }
            if let Some(f) = self.files.get(path) {
                if f.content_digest == d {
                    if let Some(root) = self.source_root.borrow().as_ref() {
                        if let Ok(b) = std::fs::read(root.join(path)) {
                            return Some(Rc::new(b));
                        }
                    }
                }
            }
        }
        None
    }

    /// Load the indexed FileAnalysis for a path (cached per path).
    fn indexed_analysis(&self, path: &str) -> Option<Rc<FileAnalysis>> {
        if let Some(a) = self.indexed.borrow().get(path) {
            return Some(a.clone());
        }
        let file = self.files.get(path)?;
        let a = artifact::read_file_analysis(self.snap_dir, file).ok()?;
        let rc = Rc::new(a);
        self.indexed
            .borrow_mut()
            .insert(path.to_string(), rc.clone());
        Some(rc)
    }

    /// Does the manifest know this path?
    pub fn knows_path(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// Resolve the symbol map for `(path, digest)`.
    ///
    /// `digest` = the observed `file_content_digest` (`sha256:`). `None` means
    /// the observation carried no version identity — resolved against the
    /// indexed version and flagged `AssumedIndexed` (read-only corpus).
    pub fn resolve(&self, path: &str, digest: Option<&str>) -> MapResolution {
        use super::model::ResolutionSource::*;
        use super::model::ResolutionState as RS;
        let file = match self.files.get(path) {
            Some(f) => *f,
            None => {
                self.diag.borrow_mut().unknown_path += 1;
                return MapResolution::fail(
                    RS::UnresolvedNoSymbolOverlap,
                    digest.map(|d| d.to_string()),
                );
            }
        };
        match digest {
            // No version identity -> bound to indexed version (§45/§46 read-only).
            None => {
                self.diag.borrow_mut().assumed_indexed += 1;
                MapResolution::ok(
                    self.indexed_analysis(path),
                    AssumedIndexed,
                    file.content_digest.clone(),
                )
            }
            // Fast path: observed version == indexed version -> reuse (§9).
            Some(d) if d == file.content_digest => {
                self.diag.borrow_mut().indexed_hit += 1;
                MapResolution::ok(self.indexed_analysis(path), IndexedHit, d.to_string())
            }
            // Unseen/alternate version -> content-addressed lookup (§10).
            Some(d) => {
                let key = format!("{path}#{d}");
                if let Some(a) = self.versions.borrow().get(&key) {
                    self.diag.borrow_mut().cached_version += 1;
                    return MapResolution::ok(Some(a.clone()), CachedVersion, d.to_string());
                }
                // Source bytes for that exact version available -> parse once.
                if let Some(bytes) = self.sources.get(d) {
                    let lang = std::path::Path::new(path)
                        .extension()
                        .and_then(|e| e.to_str())
                        .and_then(crate::model::LanguageId::from_extension);
                    match lang {
                        Some(lang) => {
                            if let Ok(analyzer) = crate::parser::Analyzer::new(
                                crate::parser::AnalyzerConfig::default(),
                            ) {
                                let a = analyzer.analyze_bytes(path, lang, bytes);
                                self.diag.borrow_mut().new_parse += 1;
                                let rc = Rc::new(a);
                                self.versions.borrow_mut().insert(key, rc.clone());
                                return MapResolution::ok(Some(rc), NewParse, d.to_string());
                            }
                            self.diag.borrow_mut().missing_source_version += 1;
                            return MapResolution::fail(
                                RS::UnresolvedMissingSourceVersion,
                                Some(d.to_string()),
                            );
                        }
                        None => {
                            self.diag.borrow_mut().unsupported_language += 1;
                            return MapResolution::fail(
                                RS::UnresolvedUnsupportedLanguage,
                                Some(d.to_string()),
                            );
                        }
                    }
                }
                // Digest differs and no source bytes -> never guess (§30).
                self.diag.borrow_mut().missing_source_version += 1;
                MapResolution::fail(RS::UnresolvedMissingSourceVersion, Some(d.to_string()))
            }
        }
    }
}

/// Locate the snapshot/manifest dir used to derive symbol maps.
pub fn manifest_path(snap_dir: &Path) -> PathBuf {
    snap_dir.join("manifest.json")
}
