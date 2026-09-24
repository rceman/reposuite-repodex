//! InvestigationMemory store — disk-backed, rebuildable, incremental.
//!
//! Layout:
//!   memory/
//!     entries/<investigation_id>.json   — MemoryEntry (signature + evidence)
//!     postings.jsonl                    — term -> [investigation_id] index
//!     checkpoint.json                   — per-investigation processed digest
//!     manifest.json                     — integrity
//!
//! Bounded lookup: a query's terms/identifiers/paths select candidate
//! investigations via postings, so lookup cost scales with candidate count —
//! never a scan of all episodes (§21). Evidence stays factual (§22-§23);
//! ignored-but-surfaced is neutral (§24).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::derived::model::InvestigationEpisode;
use crate::derived::DerivedStore;
use crate::repository::digest;

use super::model::*;
use super::signature::query_signature;

const DEFAULT_CANDIDATE_CAP: usize = 64;
const DEFAULT_LIMIT: usize = 10;

/// Similarity weights — fixed, documented (§20). Not benchmark-tuned.
/// Seed-entity overlap (§20) is a future input once entity resolution lands.
const W_IDENTIFIER: f64 = 3.0;
const W_TERM: f64 = 1.0;
const W_PATH: f64 = 2.0;

fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f64;
    let union = (a.len() + b.len()) as f64 - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

#[derive(Debug)]
pub enum MemoryError {
    Io { path: PathBuf, reason: String },
    Corrupt { reason: String },
}
impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemoryError::Io { path, reason } => write!(f, "{}: {}", path.display(), reason),
            MemoryError::Corrupt { reason } => write!(f, "memory corrupt: {reason}"),
        }
    }
}
impl std::error::Error for MemoryError {}

fn write_json<T: Serialize>(p: &Path, v: &T) -> Result<(), MemoryError> {
    if let Some(par) = p.parent() {
        fs::create_dir_all(par).map_err(|e| MemoryError::Io {
            path: par.to_path_buf(),
            reason: e.to_string(),
        })?;
    }
    let s = serde_json::to_string_pretty(v).unwrap_or_default();
    fs::write(p, s).map_err(|e| MemoryError::Io {
        path: p.to_path_buf(),
        reason: e.to_string(),
    })
}

/// The InvestigationMemory store.
#[derive(Default)]
pub struct MemoryStore {
    /// investigation_id -> MemoryEntry
    pub entries: BTreeMap<String, MemoryEntry>,
    /// term/identifier/path -> posting of investigation ids (bounded retrieval)
    pub postings: BTreeMap<String, BTreeSet<String>>,
    pub checkpoint: MemoryCheckpoint,
}

impl MemoryStore {
    pub fn open(dir: &Path) -> Result<Self, MemoryError> {
        fs::create_dir_all(dir).map_err(|e| MemoryError::Io {
            path: dir.to_path_buf(),
            reason: e.to_string(),
        })?;
        Self::load(dir)
    }

    fn entry_path(dir: &Path, inv: &str) -> PathBuf {
        dir.join("entries").join(format!("{inv}.json"))
    }

    pub fn load(dir: &Path) -> Result<Self, MemoryError> {
        let mut ms = MemoryStore::default();
        let cp = dir.join("checkpoint.json");
        if cp.exists() {
            ms.checkpoint =
                serde_json::from_str(&fs::read_to_string(&cp).map_err(|e| MemoryError::Io {
                    path: cp.clone(),
                    reason: e.to_string(),
                })?)
                .map_err(|e| MemoryError::Corrupt {
                    reason: format!("checkpoint: {e}"),
                })?;
        }
        let ed = dir.join("entries");
        if ed.exists() {
            for f in fs::read_dir(&ed).map_err(|e| MemoryError::Io {
                path: ed.clone(),
                reason: e.to_string(),
            })? {
                let f = f.map_err(|e| MemoryError::Io {
                    path: ed.clone(),
                    reason: e.to_string(),
                })?;
                if f.path().extension().and_then(|s| s.to_str()) == Some("json") {
                    let ent: MemoryEntry =
                        serde_json::from_str(&fs::read_to_string(f.path()).map_err(|e| {
                            MemoryError::Io {
                                path: f.path(),
                                reason: e.to_string(),
                            }
                        })?)
                        .map_err(|e| MemoryError::Corrupt {
                            reason: format!("entry {}: {e}", f.path().display()),
                        })?;
                    ms.entries.insert(ent.investigation_id.clone(), ent);
                }
            }
        }
        let pp = dir.join("postings.jsonl");
        if pp.exists() {
            let t = fs::read_to_string(&pp).map_err(|e| MemoryError::Io {
                path: pp.clone(),
                reason: e.to_string(),
            })?;
            for line in t.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                let v: serde_json::Value =
                    serde_json::from_str(line).map_err(|e| MemoryError::Corrupt {
                        reason: format!("posting: {e}"),
                    })?;
                let term = v["term"].as_str().unwrap_or("").to_string();
                let ids: BTreeSet<String> = v["investigations"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                ms.postings.insert(term, ids);
            }
        }
        Ok(ms)
    }

    pub fn save(&self, dir: &Path) -> Result<(), MemoryError> {
        for e in self.entries.values() {
            write_json(&Self::entry_path(dir, &e.investigation_id), e)?;
        }
        let mut lines = String::new();
        for (term, ids) in &self.postings {
            lines.push_str(
                &serde_json::json!({"term":term,"investigations":ids.iter().collect::<Vec<_>>()})
                    .to_string(),
            );
            lines.push('\n');
        }
        let pp = dir.join("postings.jsonl");
        fs::write(&pp, lines).map_err(|e| MemoryError::Io {
            path: pp,
            reason: e.to_string(),
        })?;
        write_json(&dir.join("checkpoint.json"), &self.checkpoint)?;
        Ok(())
    }

    pub fn write_manifest(&self, dir: &Path) -> Result<MemoryManifest, MemoryError> {
        let mut bytes = 0u64;
        let mut dig = String::new();
        for p in [dir.join("postings.jsonl"), dir.join("checkpoint.json")] {
            if p.exists() {
                let t = fs::read_to_string(&p).unwrap_or_default();
                dig.push_str(&t);
                bytes += t.len() as u64;
            }
        }
        let m = MemoryManifest {
            schema: "reposuite.investigation-memory.v1".into(),
            schema_version: MEMORY_SCHEMA_VERSION,
            investigations: self.entries.len() as u64,
            posting_terms: self.postings.len() as u64,
            artifact_bytes: bytes,
            content_digest: digest::sha256_text(&dig),
        };
        write_json(&dir.join("manifest.json"), &m)?;
        Ok(m)
    }

    /// Rebuild the postings index from all entries (deterministic).
    pub fn rebuild_postings(&mut self) {
        self.postings.clear();
        for (inv, e) in &self.entries {
            for t in e
                .signature
                .terms
                .iter()
                .chain(e.signature.identifiers.iter())
            {
                self.postings
                    .entry(t.clone())
                    .or_default()
                    .insert(inv.clone());
            }
            for p in &e.signature.paths {
                self.postings
                    .entry(p.clone())
                    .or_default()
                    .insert(inv.clone());
            }
        }
    }

    /// Derive a MemoryEntry from an InvestigationEpisode. `known_paths` grounds
    /// signature path tokens + freshness; `current_paths` = the live repo path
    /// set for staleness (defaults to `known_paths` when the snapshot is the
    /// remembered head).
    pub fn entry_from_episode(
        ep: &InvestigationEpisode,
        known_paths: &BTreeSet<String>,
        current_paths: &BTreeSet<String>,
    ) -> MemoryEntry {
        let query = ep.query_text.clone().unwrap_or_default();
        let signature = query_signature(&query, known_paths, None);
        let mut evidence: BTreeMap<String, MemoryEvidence> = BTreeMap::new();
        // surfaced
        for (path, rank) in &ep.surfaced_paths {
            evidence
                .entry(path.clone())
                .or_insert_with(|| MemoryEvidence {
                    path: path.clone(),
                    ..Default::default()
                })
                .surfaced = true;
            if let Some(e) = evidence.get_mut(path) {
                e.surfaced_rank = *rank;
            }
        }
        // observed / read
        for ex in &ep.source_exposures {
            let e = evidence
                .entry(ex.path.clone())
                .or_insert_with(|| MemoryEvidence {
                    path: ex.path.clone(),
                    ..Default::default()
                });
            e.observed = true;
            e.first_observed_sequence = Some(ex.first_observed_sequence);
            e.explicitly_read = ex.explicit_read_count > 0;
            e.explicit_read_count = ex.explicit_read_count;
        }
        // final-answer mentions
        for m in &ep.final_answer_path_mentions {
            let e = evidence
                .entry(m.path.clone())
                .or_insert_with(|| MemoryEvidence {
                    path: m.path.clone(),
                    ..Default::default()
                });
            e.mentioned_in_final_answer = true;
        }
        for p in &ep.evidence_path_mentions {
            let e = evidence.entry(p.clone()).or_insert_with(|| MemoryEvidence {
                path: p.clone(),
                ..Default::default()
            });
            e.structured_evidence_mention = true;
        }
        // provenance + cost + outcome per path
        for e in evidence.values_mut() {
            e.repository_id = ep.repository_id.clone();
            e.repo_head = ep.repo_heads.first().cloned();
            e.input_tokens = ep.input_tokens;
            e.output_tokens = ep.output_tokens;
            e.tool_calls = Some(ep.tools.tool_calls_total);
            e.task_outcomes = ep.task_outcomes.clone();
            e.freshness = Some(classify_freshness(
                &e.path,
                current_paths,
                ep.repo_heads.first().cloned(),
            ));
        }
        MemoryEntry {
            schema: "reposuite.memory-entry.v1".into(),
            schema_version: MEMORY_SCHEMA_VERSION,
            investigation_id: ep.investigation_id.clone(),
            session_ids: ep.session_ids.clone(),
            repository_id: ep.repository_id.clone(),
            project_scope: super::project::scope_from_episode(
                ep.project_id.as_deref(),
                ep.repository_id.as_deref(),
            ),
            repo_head: ep.repo_heads.first().cloned(),
            completed_at: ep.completed_at.clone(),
            signature,
            evidence,
            symbols: BTreeMap::new(),
            input_tokens: ep.input_tokens,
            output_tokens: ep.output_tokens,
            tool_calls: ep.tools.tool_calls_total,
            task_outcomes: ep.task_outcomes.clone(),
        }
    }

    /// Bounded query: signature -> candidate investigations via postings ->
    /// score -> ranked path evidence. Never scans all episodes (§21).
    pub fn query(
        &self,
        query: &str,
        known_paths: &BTreeSet<String>,
        limit: usize,
        candidate_cap: usize,
    ) -> Vec<MemoryMatch> {
        self.query_scoped(query, known_paths, limit, candidate_cap, None)
    }

    /// Like `query` but restricted to a canonical project scope (§4). `scope`
    /// `Some(s)` returns only entries stamped `project_scope == s`.
    pub fn query_scoped(
        &self,
        query: &str,
        known_paths: &BTreeSet<String>,
        limit: usize,
        candidate_cap: usize,
        scope: Option<&str>,
    ) -> Vec<MemoryMatch> {
        let sig = query_signature(query, known_paths, None);
        let qterms: BTreeSet<String> = sig.terms.iter().cloned().collect();
        let qids: BTreeSet<String> = sig.identifiers.iter().cloned().collect();
        let qpaths: BTreeSet<String> = sig.paths.iter().cloned().collect();
        // bounded candidate set via postings (§21)
        let mut cand: BTreeSet<String> = BTreeSet::new();
        for t in sig
            .terms
            .iter()
            .chain(sig.identifiers.iter())
            .chain(sig.paths.iter())
        {
            if let Some(ids) = self.postings.get(t) {
                cand.extend(ids.iter().cloned());
                if cand.len() >= candidate_cap {
                    break;
                }
            }
        }
        let cap = if candidate_cap == 0 {
            DEFAULT_CANDIDATE_CAP
        } else {
            candidate_cap
        };
        let lim = if limit == 0 { DEFAULT_LIMIT } else { limit };
        let mut matches: Vec<MemoryMatch> = cand
            .iter()
            .take(cap)
            .filter_map(|inv| {
                let e = self.entries.get(inv)?;
                // §4 hard isolation: when a scope filter is given, only entries
                // stamped with that exact canonical project scope match. An
                // unscoped entry (bare "root"/unknown origin) never leaks into a
                // scoped project query.
                if let Some(scope) = scope {
                    if e.project_scope.as_deref() != Some(scope) {
                        return None;
                    }
                }
                let terms: BTreeSet<String> = e.signature.terms.iter().cloned().collect();
                let ids: BTreeSet<String> = e.signature.identifiers.iter().cloned().collect();
                let paths: BTreeSet<String> = e.signature.paths.iter().cloned().collect();
                let jid = jaccard(&qids, &ids);
                let jt = jaccard(&qterms, &terms);
                let jp = jaccard(&qpaths, &paths);
                let mut components = BTreeMap::new();
                components.insert("identifier".into(), jid * W_IDENTIFIER);
                components.insert("term".into(), jt * W_TERM);
                components.insert("path".into(), jp * W_PATH);
                let similarity = jid * W_IDENTIFIER + jt * W_TERM + jp * W_PATH;
                if similarity <= 0.0 {
                    return None;
                }
                let mut paths: Vec<MemoryEvidence> = e.evidence.values().cloned().collect();
                paths.sort_by_key(|p| std::cmp::Reverse(p.strongest() as u8));
                // symbol-level evidence (§38-§40): sort strongest signal first.
                let mut symbols: Vec<SymbolMemoryEvidence> = e.symbols.values().cloned().collect();
                symbols.sort_by_key(|s| {
                    std::cmp::Reverse((
                        s.mentioned_in_final_answer,
                        s.declaration_exposed,
                        s.explicitly_read,
                        s.reference_exposed,
                    ))
                });
                Some(MemoryMatch {
                    investigation_id: inv.clone(),
                    similarity,
                    components,
                    paths,
                    symbols,
                })
            })
            .collect();
        matches.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.investigation_id.cmp(&b.investigation_id))
        });
        matches.truncate(lim);
        matches
    }

    /// Aggregated path candidates across the top matches, ranked by a
    /// deterministic evidence-strength + similarity composite (§46).
    pub fn query_paths(
        &self,
        query: &str,
        known_paths: &BTreeSet<String>,
        limit: usize,
    ) -> Vec<(String, EvidenceStrength, f64, Vec<String>)> {
        let matches = self.query(query, known_paths, 8, DEFAULT_CANDIDATE_CAP);
        let mut agg: BTreeMap<String, (EvidenceStrength, f64, Vec<String>)> = BTreeMap::new();
        for m in &matches {
            for p in &m.paths {
                let e = agg.entry(p.path.clone()).or_insert((
                    EvidenceStrength::SurfacedOnly,
                    0.0,
                    Vec::new(),
                ));
                let s = p.strongest();
                if s > e.0 {
                    e.0 = s;
                }
                e.1 = (e.1).max(m.similarity);
                e.2.push(m.investigation_id.clone());
            }
        }
        let mut v: Vec<_> = agg.into_iter().collect();
        v.sort_by(|a, b| {
            (b.1 .0 as u8, (b.1 .1 * 1000.0) as u64).cmp(&(a.1 .0 as u8, (a.1 .1 * 1000.0) as u64))
        });
        v.truncate(limit);
        v.into_iter()
            .map(|(p, (s, sc, iv))| (p, s, sc, iv))
            .collect()
    }
}

/// Classify memory freshness against the current repo path set (§30). Path
/// still present -> Fresh; absent -> Removed; unknown snapshot -> Unknown.
/// `changed` requires GitActivity/digest evidence — conservative default is
/// `Unknown` when no current snapshot exists (never claim fresh from a mere
/// name match against nothing).
fn classify_freshness(
    path: &str,
    current_paths: &BTreeSet<String>,
    _head: Option<String>,
) -> Freshness {
    if current_paths.is_empty() {
        return Freshness::Unknown;
    }
    if current_paths.contains(path) {
        Freshness::Fresh
    } else {
        Freshness::Removed
    }
}

/// Build/update memory from a derived store. `full` rebuilds; else only
/// investigations whose episode changed are (re)indexed (§36). Returns the
/// store + a small report.
pub fn build_memory(
    ds: &DerivedStore,
    out: &Path,
    full: bool,
    known_paths: &BTreeSet<String>,
    current_paths: &BTreeSet<String>,
) -> Result<(MemoryStore, serde_json::Value), MemoryError> {
    let mut ms = if full {
        MemoryStore::default()
    } else {
        MemoryStore::load(out)?
    };
    let mut rebuilt = 0u64;
    // bind memory to the derived/event-store snapshot it was built from
    let derived_digest = ds.checkpoint.event_store_digest.clone();
    let mut changed = false;
    for (inv, ep) in &ds.episodes {
        let ep_digest = digest::sha256_text(&serde_json::to_string(ep).unwrap_or_default());
        if !full && ms.checkpoint.processed.get(inv.as_str()) == Some(&ep_digest) {
            continue; // unchanged
        }
        let entry = MemoryStore::entry_from_episode(ep, known_paths, current_paths);
        ms.entries.insert(inv.clone(), entry);
        ms.checkpoint.processed.insert(inv.clone(), ep_digest);
        rebuilt += 1;
        changed = true;
    }
    if full || changed {
        ms.rebuild_postings();
    }
    ms.checkpoint.schema_version = MEMORY_SCHEMA_VERSION;
    ms.checkpoint.derived_digest = derived_digest;
    let report = serde_json::json!({
        "investigations": ms.entries.len(),
        "rebuilt": rebuilt,
        "posting_terms": ms.postings.len(),
        "full": full,
    });
    Ok((ms, report))
}

impl MemoryStore {
    /// Enrich each entry with symbol-level evidence from a SymbolExposure store
    /// (§37-§40). File-level memory is preserved; symbol facts are added
    /// alongside. `current_paths`/`repo_head` drive conservative freshness (§41).
    pub fn enrich_symbols(
        &mut self,
        sexp: &crate::symbol_exposure::SymbolExposureStore,
        current_paths: &BTreeSet<String>,
    ) {
        use crate::symbol_exposure::ExposureKind;
        for entry in self.entries.values_mut() {
            let Some(doc) = sexp.load(&entry.investigation_id) else {
                continue;
            };
            let mentioned: std::collections::BTreeSet<&String> =
                doc.final_answer_symbol_mentions.iter().collect();
            for x in &doc.exposures {
                let e = entry.symbols.entry(x.symbol_id.clone()).or_insert_with(|| {
                    SymbolMemoryEvidence {
                        symbol_id: x.symbol_id.clone(),
                        locator: x.symbol_locator.clone(),
                        facets: x.facets.clone(),
                        symbol_name: x.symbol_name.clone(),
                        path: x.path.clone(),
                        symbol_kind: x.symbol_kind.clone(),
                        ..Default::default()
                    }
                });
                match x.exposure_kind {
                    ExposureKind::DeclarationOccurrence => e.declaration_exposed = true,
                    ExposureKind::EnclosingDeclaration => e.enclosing_exposed = true,
                    ExposureKind::ReferenceOccurrence => e.reference_exposed = true,
                }
                match x.observation_provenance.as_str() {
                    "explicit_read" => e.explicitly_read = true,
                    "search_snippet" => e.via_search_snippet = true,
                    _ => {}
                }
                if mentioned.contains(&x.symbol_id) {
                    e.mentioned_in_final_answer = true;
                }
                if e.file_content_digest.is_none() {
                    e.file_content_digest = x.file_content_digest.clone();
                }
                if e.freshness.is_none() {
                    e.freshness = Some(classify_freshness(
                        &e.path,
                        current_paths,
                        entry.repo_head.clone(),
                    ));
                }
            }
        }
    }
}
