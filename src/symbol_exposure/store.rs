//! SymbolExposure store + derive driver (§29, §75).
//!
//! Consumes canonical `source_observed` AgentEvents, resolves each against the
//! symbol map for its exact content version, and persists `SymbolExposure`
//! records keyed by investigation plus resolution diagnostics. Incremental:
//! processes only new/changed observations (§29).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent_event::model::{AgentEvent, ObservationKind};

use super::model::{ResolutionRecord, ResolutionSource, ResolutionState, SymbolExposure};
use super::provider::SymbolMapProvider;
use super::resolve::{resolve_observation, Observation};

/// Compact per-investigation symbol summary (§32). Counts only — the detailed
/// per-symbol records are the normalized `exposures` vector.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolSummary {
    pub unique_symbols_exposed: u64,
    pub declarations_exposed: u64,
    pub references_exposed: u64,
    pub enclosing_exposed: u64,
    pub symbols_explicitly_read: u64,
    pub symbols_via_search_snippet: u64,
}

/// Per-investigation symbol exposure document (normalized, §75).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolExposureDoc {
    pub investigation_id: String,
    /// Investigation-level provenance — once, not per exposure (§75).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_head: Option<String>,
    pub summary: SymbolSummary,
    pub exposures: Vec<SymbolExposure>,
    pub resolutions: Vec<ResolutionRecord>,
    /// Exposed symbol identities whose name appears as a whole token in the
    /// investigation's final answer — bounded exact-token match only (§36).
    #[serde(default)]
    pub final_answer_symbol_mentions: Vec<String>,
    /// Concatenated final-answer text for this investigation (for mention join).
    #[serde(default, skip_serializing)]
    pub answer_text: String,
}

impl SymbolExposureDoc {
    /// Recompute the compact summary from the normalized exposures.
    pub fn finalize_summary(&mut self) {
        let mut syms = std::collections::BTreeSet::new();
        let mut read = std::collections::BTreeSet::new();
        let mut snip = std::collections::BTreeSet::new();
        let mut s = SymbolSummary::default();
        for x in &self.exposures {
            syms.insert(x.symbol_id.clone());
            match x.exposure_kind {
                super::model::ExposureKind::DeclarationOccurrence => s.declarations_exposed += 1,
                super::model::ExposureKind::ReferenceOccurrence => s.references_exposed += 1,
                super::model::ExposureKind::EnclosingDeclaration => s.enclosing_exposed += 1,
            }
            match x.observation_provenance {
                ObservationKind::ExplicitRead => {
                    read.insert(x.symbol_id.clone());
                }
                ObservationKind::SearchSnippet => {
                    snip.insert(x.symbol_id.clone());
                }
                _ => {}
            }
        }
        s.unique_symbols_exposed = syms.len() as u64;
        s.symbols_explicitly_read = read.len() as u64;
        s.symbols_via_search_snippet = snip.len() as u64;
        self.summary = s;

        // Bounded final-answer symbol mention: an exposed symbol's exact name
        // appears as a whole identifier token in the answer text (§36). Never
        // fuzzy-matched; symbol identity is already established by exposure.
        if !self.answer_text.is_empty() {
            let mut mentioned = std::collections::BTreeSet::new();
            for id in &syms {
                // symbol_id is `decl:path#id` or a written name; match the name.
                let name = self
                    .exposures
                    .iter()
                    .find(|x| &x.symbol_id == id)
                    .map(|x| x.symbol_name.clone())
                    .unwrap_or_default();
                if name.len() >= 3 && contains_token(&self.answer_text, &name) {
                    mentioned.insert(id.clone());
                }
            }
            self.final_answer_symbol_mentions = mentioned.into_iter().collect();
        }
    }
}

/// Whole-identifier-token containment (word boundaries), not substring.
fn contains_token(text: &str, name: &str) -> bool {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|t| t == name)
}

/// Aggregate resolution diagnostics across a derive run (§80).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ResolutionDiag {
    pub source_observed_events: u64,
    pub resolved_indexed_hit: u64,
    pub resolved_cached_version: u64,
    pub resolved_new_parse: u64,
    pub resolved_assumed_indexed: u64,
    pub unresolved: u64,
    pub unresolved_missing_source_version: u64,
    pub unresolved_unsupported_language: u64,
    pub unresolved_no_symbol_overlap: u64,
    pub exposures: u64,
    pub unique_symbols: u64,
}

/// Persisted symbol-exposure store.
#[derive(Debug, Default)]
pub struct SymbolExposureStore {
    pub dir: PathBuf,
    /// investigation_id -> doc (in-memory working set for a derive run).
    pub docs: BTreeMap<String, SymbolExposureDoc>,
    pub diag: ResolutionDiag,
}

impl SymbolExposureStore {
    pub fn open(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            ..Default::default()
        }
    }

    fn doc_path(&self, investigation_id: &str) -> PathBuf {
        self.dir
            .join(format!("{}.json", sanitize(investigation_id)))
    }

    /// Persist all derived docs (per investigation).
    pub fn save(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        for doc in self.docs.values() {
            let p = self.doc_path(&doc.investigation_id);
            fs::write(p, serde_json::to_string_pretty(doc)?)?;
        }
        Ok(())
    }

    /// Load a doc for one investigation.
    pub fn load(&self, investigation_id: &str) -> Option<SymbolExposureDoc> {
        let p = self.doc_path(investigation_id);
        let s = fs::read_to_string(p).ok()?;
        serde_json::from_str(&s).ok()
    }

    /// All persisted investigation ids.
    pub fn investigation_ids(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Ok(rd) = fs::read_dir(&self.dir) {
            for e in rd.flatten() {
                if let Some(stem) = e.path().file_stem().and_then(|s| s.to_str()) {
                    v.push(stem.to_string());
                }
            }
        }
        v.sort();
        v
    }
}

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Build the `Observation` from a canonical `source_observed` event.
fn observation(e: &AgentEvent) -> Option<Observation> {
    let d = &e.data;
    let path = d.get("path").and_then(|v| v.as_str())?;
    if path.is_empty() {
        return None;
    }
    Some(Observation {
        investigation_id: e.investigation_id.clone().unwrap_or_default(),
        session_id: e.session_id.clone(),
        repository_id: e.repository_id.clone().unwrap_or_default(),
        repo_head: e.repo_head.clone(),
        source_event_sequence: e.sequence,
        path: path.to_string(),
        line_start: d.get("line_start").and_then(|v| v.as_u64()),
        line_end: d.get("line_end").and_then(|v| v.as_u64()),
        file_content_digest: d
            .get("file_content_digest")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        kind: ObservationKind::parse(
            d.get("observation_kind")
                .and_then(|v| v.as_str())
                .unwrap_or("other"),
        ),
    })
}

/// Derive symbol exposures from a stream of canonical events. Only
/// `source_observed` events drive resolution (§29). Returns the populated
/// store + diagnostics.
pub fn derive_symbol_exposures(
    events: &[AgentEvent],
    provider: &SymbolMapProvider,
    store: &mut SymbolExposureStore,
) {
    let mut symbols = std::collections::BTreeSet::new();
    let mut touched = std::collections::BTreeSet::new();
    // collect final-answer text per investigation for the mention join (§36).
    for e in events {
        if e.event_type == "final_answer" {
            if let Some(inv) = e.investigation_id.clone() {
                touched.insert(inv.clone());
                let doc = store
                    .docs
                    .entry(inv.clone())
                    .or_insert_with(|| SymbolExposureDoc {
                        investigation_id: inv,
                        ..Default::default()
                    });
                if let Some(c) = e.data.get("content").and_then(|v| v.as_str()) {
                    doc.answer_text.push_str(c);
                    doc.answer_text.push('\n');
                }
            }
        }
    }
    for e in events {
        if e.event_type != "source_observed" {
            continue;
        }
        store.diag.source_observed_events += 1;
        let obs = match observation(e) {
            Some(o) => o,
            None => continue,
        };
        let (exps, rec) = resolve_observation(provider, &obs);
        let inv = obs.investigation_id.clone();
        touched.insert(inv.clone());
        match rec.source {
            ResolutionSource::IndexedHit => store.diag.resolved_indexed_hit += 1,
            ResolutionSource::CachedVersion => store.diag.resolved_cached_version += 1,
            ResolutionSource::NewParse => store.diag.resolved_new_parse += 1,
            ResolutionSource::AssumedIndexed => store.diag.resolved_assumed_indexed += 1,
            ResolutionSource::Unresolved => store.diag.unresolved += 1,
        }
        match rec.resolution {
            ResolutionState::UnresolvedMissingSourceVersion => {
                store.diag.unresolved_missing_source_version += 1
            }
            ResolutionState::UnresolvedUnsupportedLanguage => {
                store.diag.unresolved_unsupported_language += 1
            }
            ResolutionState::UnresolvedNoSymbolOverlap => {
                store.diag.unresolved_no_symbol_overlap += 1
            }
            _ => {}
        }
        store.diag.exposures += exps.len() as u64;
        for x in &exps {
            symbols.insert(x.symbol_id.clone());
        }
        let doc = store
            .docs
            .entry(inv.clone())
            .or_insert_with(|| SymbolExposureDoc {
                investigation_id: inv,
                repository_id: Some(obs.repository_id.clone()),
                repo_head: obs.repo_head.clone(),
                ..Default::default()
            });
        doc.exposures.extend(exps);
        doc.resolutions.push(rec);
    }
    // Finalize only the investigations touched this call (incremental, §29).
    for inv in touched {
        if let Some(doc) = store.docs.get_mut(&inv) {
            doc.finalize_summary();
        }
    }
    store.diag.unique_symbols = symbols.len() as u64;
}

/// Report for a store build (digests + diagnostics).
#[derive(Debug, Serialize)]
pub struct SymbolExposureReport {
    pub investigations: usize,
    pub diag: ResolutionDiag,
    pub store_digest: String,
}
