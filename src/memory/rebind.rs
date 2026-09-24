//! Target-view rebinding (Part III-V, §9-§28).
//!
//! Hard rule: a `HistoricalMemoryHit` may recommend WHERE to investigate, but
//! only the current validated RepositoryView produces current evidence. There is
//! NO direct `historical -> current` conversion; the only route is
//! `TargetViewRebinder` -> `TargetViewMemoryAnnotation`.
//!
//! A stable declaration locator replaces the file-local ordinal `decl:path#id`
//! (§9-§10): `{lang}:{path}:{kind}:{qualified_name}` where `qualified_name` is
//! the lexical scope path + name (and the impl target for methods). Facets are
//! cheap byte digests over the existing decl/body ranges (§11-§13), computed
//! from the *current* view's source bytes — never a replay of old coordinates.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::model::{Declaration, FileAnalysis};
use crate::repository::digest;

/// Extraction-policy identity — bumped if the locator/facet derivation changes.
pub const REBIND_POLICY: &str = "repodex.rebind.v1";

/// Binding state for a historical symbol against the current view (§16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingState {
    /// Locator resolves + name/header/body facets all match (same path too).
    ExactFresh,
    /// Same qualified identity + facets, but the file moved (new path/range).
    MovedButSame,
    /// Locator + signature/header match, body digest differs (§18).
    ChangedImplementation,
    /// Locator resolves but the declaration/header signature differs (§19).
    ChangedInterface,
    /// No current declaration matches the locator.
    Absent,
    /// More than one current declaration matches; not resolved to one.
    Ambiguous,
    /// Insufficient data to determine a binding (e.g. missing facets).
    Unknown,
}

/// Intrinsic entity facets — equality of selected source facets only (§12).
/// They do NOT prove semantic equivalence, behavior, callers, or completeness.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclFacets {
    /// Digest of the written declaration name.
    pub name_digest: String,
    /// Digest of the declaration header (decl range minus body bytes) — the
    /// signature/interface facet.
    pub header_digest: String,
    /// Digest of the body bytes, when the construct has a body (None otherwise).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_digest: Option<String>,
    /// The extraction-policy identity these facets were computed under.
    pub policy: String,
}

/// A stable declaration locator (§10): `{lang}:{path}:{kind}:{qualified_name}`.
/// Qualified name comes from `scope_path` + the written name; methods carry the
/// impl-target scope, so overloads/locals stay distinguishable.
pub fn decl_locator(a: &FileAnalysis, d: &Declaration) -> String {
    let scope = a.scope_path(d.scope_id);
    let qname = if scope.is_empty() {
        d.name.clone()
    } else {
        format!("{scope}::{}", d.name)
    };
    format!(
        "{}:{}:{}:{}",
        a.file.language.as_str(),
        d.relative_path,
        d.kind.as_str(),
        qname
    )
}

/// The path-free qualified key used to group a symbol across files/moves:
/// `{lang}:{kind}:{qualified_name}`.
pub fn qname_key(a: &FileAnalysis, d: &Declaration) -> String {
    let scope = a.scope_path(d.scope_id);
    let qname = if scope.is_empty() {
        d.name.clone()
    } else {
        format!("{scope}::{}", d.name)
    };
    format!("{}:{}:{}", a.file.language.as_str(), d.kind.as_str(), qname)
}

/// Compute the intrinsic facets for `decl` from the current source `bytes`.
/// `body_digest=None` when the construct has no body (§13).
pub fn decl_facets(bytes: &[u8], d: &Declaration) -> DeclFacets {
    let name_digest = digest::content_digest(d.name.as_bytes());
    let (hs, he) = (d.range.byte_start as usize, d.range.byte_end as usize);
    let he = he.min(bytes.len());
    let header_end = match &d.body_range {
        Some(b) if (b.byte_start as usize) <= he && (b.byte_start as usize) >= hs => {
            b.byte_start as usize
        }
        _ => he,
    };
    let header_digest = digest::content_digest(&bytes[hs.min(header_end)..header_end]);
    let body_digest = d.body_range.as_ref().map(|b| {
        let (s, e) = (
            (b.byte_start as usize).min(bytes.len()),
            (b.byte_end as usize).min(bytes.len()),
        );
        digest::content_digest(&bytes[s..e])
    });
    DeclFacets {
        name_digest,
        header_digest,
        body_digest,
        policy: REBIND_POLICY.to_string(),
    }
}

/// A historical symbol bound (or not) to the current validated view (§29).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetViewMemoryAnnotation {
    pub project_scope: String,
    pub locator: String,
    pub symbol_name: String,
    /// Binding state (§16) — NEVER collapsed to a bare `fresh` bool.
    pub state: BindingState,
    /// Current repo-relative path (only when bound to a current decl).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_path: Option<String>,
    /// Current canonical locator (`decl:path#id`) of the bound decl.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_key: Option<String>,
    /// Current declaration range — current view only (§30), never historical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declaration_range: Option<crate::query::projection::RangeOut>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_range: Option<crate::query::projection::RangeOut>,
    /// Historical provenance carried through untouched (§7-§8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_digest: Option<String>,
    /// "fact" only for ExactFresh/MovedButSame; everything else is "candidate".
    pub evidence_class: String,
}

/// The rebinder: resolves historical locators/facets against the current view.
/// Reads current source bytes (for facets) from `view_root` and current decl
/// ranges from the validated snapshot under `index_dir`.
pub struct TargetViewRebinder {
    files_dir: PathBuf,
    view_root: PathBuf,
    /// path -> object_key (snapshot manifest).
    object_of: BTreeMap<String, String>,
    /// qualified `lang:kind:qname` -> current decls (for move/duplicate detect).
    by_qname: BTreeMap<String, Vec<(String, Declaration)>>,
    /// path -> loaded FileAnalysis (lazy).
    analyses: BTreeMap<String, Option<FileAnalysis>>,
    /// path -> current source bytes (lazy, for facet computation).
    bytes: BTreeMap<String, Option<Vec<u8>>>,
    project_scope: String,
}

impl TargetViewRebinder {
    /// Open a rebinder over the validated view. `index_dir` = `indexes/{key}`
    /// (holds `snapshot/`); `view_root` supplies current source bytes for facets.
    pub fn open(index_dir: &Path, view_root: &Path, project_scope: &str) -> Option<Self> {
        let snap = index_dir.join("snapshot");
        let text = std::fs::read_to_string(snap.join("manifest.json")).ok()?;
        let m: serde_json::Value = serde_json::from_str(&text).ok()?;
        let mut object_of = BTreeMap::new();
        for f in m["files"].as_array().into_iter().flatten() {
            if let (Some(p), Some(k)) = (f["relative_path"].as_str(), f["object_key"].as_str()) {
                object_of.insert(p.to_string(), k.to_string());
            }
        }
        let mut rb = Self {
            files_dir: snap.join("files"),
            view_root: view_root.to_path_buf(),
            object_of,
            by_qname: BTreeMap::new(),
            analyses: BTreeMap::new(),
            bytes: BTreeMap::new(),
            project_scope: project_scope.to_string(),
        };
        rb.index_all();
        Some(rb)
    }

    fn analysis(&mut self, path: &str) -> Option<&FileAnalysis> {
        if !self.analyses.contains_key(path) {
            let a = self
                .object_of
                .get(path)
                .and_then(|k| {
                    std::fs::read_to_string(self.files_dir.join(format!("{k}.json"))).ok()
                })
                .and_then(|t| serde_json::from_str::<FileAnalysis>(&t).ok());
            self.analyses.insert(path.to_string(), a);
        }
        self.analyses.get(path)?.as_ref()
    }

    fn bytes(&mut self, path: &str) -> Option<&[u8]> {
        if !self.bytes.contains_key(path) {
            let b = std::fs::read(self.view_root.join(path)).ok();
            self.bytes.insert(path.to_string(), b);
        }
        self.bytes.get(path)?.as_deref()
    }

    fn index_all(&mut self) {
        let paths: Vec<String> = self.object_of.keys().cloned().collect();
        for p in paths {
            // Clone (qkey, decl) pairs first so the `self.analysis` borrow ends
            // before `self.by_qname` is mutated.
            let pairs: Vec<(String, Declaration)> = match self.analysis(&p) {
                Some(a) => a
                    .declarations
                    .iter()
                    .map(|d| (qname_key(a, d), d.clone()))
                    .collect(),
                None => continue,
            };
            for (qk, d) in pairs {
                self.by_qname.entry(qk).or_default().push((p.clone(), d));
            }
        }
    }

    /// Rebind a historical symbol to the current view (§15-§22).
    /// `hist_locator` = stable `{lang}:{path}:{kind}:{qname}`; `hist_facets`
    /// (optional) disambiguate changed/moved symbols. Returns an annotation —
    /// never a current-evidence claim by itself.
    pub fn rebind(
        &mut self,
        hist_locator: &str,
        hist_name: &str,
        hist_path: &str,
        hist_facets: Option<&DeclFacets>,
        origin_path: Option<String>,
        origin_digest: Option<String>,
    ) -> TargetViewMemoryAnnotation {
        let qkey = {
            // strip `{lang}:{path}:` -> `{kind}:{qname}`, re-prefix lang
            let lang = hist_locator.split(':').next().unwrap_or("");
            hist_locator
                .splitn(3, ':')
                .nth(2)
                .map(|rest| format!("{lang}:{rest}"))
                .unwrap_or_else(|| hist_locator.to_string())
        };
        let candidates = self.by_qname.get(&qkey).cloned().unwrap_or_default();
        let scope = self.project_scope.clone();
        let base = |st: BindingState,
                    path: Option<String>,
                    key: Option<String>,
                    dr,
                    br,
                    op: Option<String>,
                    od: Option<String>| {
            TargetViewMemoryAnnotation {
                project_scope: scope.clone(),
                locator: hist_locator.to_string(),
                symbol_name: hist_name.to_string(),
                state: st,
                current_path: path,
                current_key: key,
                declaration_range: dr,
                body_range: br,
                origin_path: op,
                origin_digest: od,
                evidence_class: match st {
                    BindingState::ExactFresh | BindingState::MovedButSame => "fact".into(),
                    _ => "candidate".into(),
                },
            }
        };
        if candidates.is_empty() {
            return base(
                BindingState::Absent,
                None,
                None,
                None,
                None,
                origin_path.clone(),
                origin_digest.clone(),
            );
        }
        // §21-§22: multiple distinct-path candidates with nothing to disambiguate
        // -> AMBIGUOUS (never guess identity across rename/duplicate).
        let distinct: std::collections::BTreeSet<_> =
            candidates.iter().map(|(p, _)| p.clone()).collect();
        if candidates.len() > 1 && distinct.len() > 1 && hist_facets.is_none() {
            return base(
                BindingState::Ambiguous,
                None,
                None,
                None,
                None,
                origin_path.clone(),
                origin_digest.clone(),
            );
        }
        // Prefer same-path candidate; then compare facets to pick the true one.
        let mut cands = candidates;
        cands.sort_by_key(|(p, _)| (*p != hist_path) as u8);
        // With facets, find the candidate whose header/body match best.
        let (path, decl) = match hist_facets {
            Some(f) if !f.name_digest.is_empty() => {
                let mut best = None;
                for (p, d) in &cands {
                    if let Some(b) = self.bytes(p) {
                        let cf = decl_facets(b, d);
                        let score = (cf.header_digest == f.header_digest) as u8 * 2
                            + (cf.body_digest == f.body_digest) as u8;
                        if score > 0 {
                            best = Some((p.clone(), d.clone(), cf, score));
                            if score == 3 {
                                break;
                            }
                        }
                    }
                }
                match best {
                    Some((p, d, _, _)) => (p, d),
                    None => {
                        // facets present but none match -> interface/impl changed
                        let (p, d) = cands[0].clone();
                        let cur = self.bytes(&p).map(|b| decl_facets(b, &d));
                        let st = match cur {
                            Some(c) if c.header_digest == f.header_digest => {
                                BindingState::ChangedImplementation
                            }
                            _ => BindingState::ChangedInterface,
                        };
                        return base(st, Some(p), None, None, None, origin_path, origin_digest);
                    }
                }
            }
            _ => cands[0].clone(),
        };
        // Compute current facets for the chosen decl.
        let cf = self.bytes(&path).map(|b| decl_facets(b, &decl));
        let state = match (hist_facets, cf) {
            (None, _) => {
                if path == hist_path {
                    BindingState::ExactFresh
                } else {
                    BindingState::MovedButSame
                }
            }
            (Some(f), Some(cur)) => {
                if f.header_digest == cur.header_digest && f.body_digest == cur.body_digest {
                    if path == hist_path {
                        BindingState::ExactFresh
                    } else {
                        BindingState::MovedButSame
                    }
                } else if f.header_digest == cur.header_digest {
                    BindingState::ChangedImplementation // body differs
                } else {
                    BindingState::ChangedInterface // signature/header differs
                }
            }
            (Some(_), None) => BindingState::Unknown, // current bytes unavailable
        };
        let dr = crate::query::projection::RangeOut {
            path: path.clone(),
            start_line: decl.range.row_start + 1,
            end_line: decl.range.row_end + 1,
            byte_start: decl.range.byte_start,
            byte_end: decl.range.byte_end,
        };
        let br = decl
            .body_range
            .as_ref()
            .map(|b| crate::query::projection::RangeOut {
                path: path.clone(),
                start_line: b.row_start + 1,
                end_line: b.row_end + 1,
                byte_start: b.byte_start,
                byte_end: b.byte_end,
            });
        let key = format!("decl:{}#{}", path, decl.declaration_id);
        base(
            state,
            Some(path),
            Some(key),
            Some(dr),
            br,
            origin_path,
            origin_digest,
        )
    }
}
