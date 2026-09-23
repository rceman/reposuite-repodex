//! Symbol-level AgentActivity (§34-§35, §77).
//!
//! A separate view from file-level AgentActivity — not blended. Hot records
//! hold counts only; unbounded per-contribution identity lives in a disk-backed
//! `contrib.jsonl` (same scalable pattern as file AgentActivity).

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::model::{ExposureKind, SymbolExposure};

/// Bounded hot record for one symbol (§35). Counts only — no id arrays.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolActivity {
    /// `decl:{path}#{id}` (declarations) or a referenced name/candidate.
    pub symbol_id: String,
    pub symbol_name: String,
    pub path: String,
    pub symbol_kind: String,
    pub observation_event_count_total: u64,
    pub explicit_read_exposure_count: u64,
    pub search_snippet_exposure_count: u64,
    pub enclosing_declaration_count: u64,
    pub declaration_occurrence_count: u64,
    pub reference_occurrence_count: u64,
    pub sessions_exposed_count: u64,
    pub investigations_exposed_count: u64,
    pub final_answer_symbol_mention_count: u64,
    pub structured_evidence_symbol_mention_count: u64,
    pub first_exposed_at: Option<String>,
    pub last_exposed_at: Option<String>,
}

/// Disk-backed contribution record (one line per exposure) — build-only.
#[derive(Debug, Clone, Serialize)]
pub struct SymbolContribution<'a> {
    pub symbol_id: &'a str,
    pub session_id: &'a str,
    pub investigation_id: &'a str,
    pub exposure_kind: &'a str,
    pub observation_provenance: &'a str,
    pub certainty: &'a str,
    pub source_event_sequence: u64,
}

/// Symbol activity store: hot `symbols.jsonl` + disk-backed `contrib.jsonl`.
pub struct SymbolActivityStore {
    pub dir: PathBuf,
    pub symbols: BTreeMap<String, SymbolActivity>,
    /// per-symbol dedup sets for session/investigation counts (build-time only).
    sessions: BTreeMap<String, std::collections::BTreeSet<String>>,
    investigations: BTreeMap<String, std::collections::BTreeSet<String>>,
    contrib_path: PathBuf,
}

impl SymbolActivityStore {
    pub fn open(dir: &std::path::Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            symbols: BTreeMap::new(),
            sessions: BTreeMap::new(),
            investigations: BTreeMap::new(),
            contrib_path: dir.join("contrib.jsonl"),
        }
    }

    /// Fold one exposure into the hot record + append its contribution line.
    /// `investigation_id` comes from the parent doc (per-exposure provenance).
    pub fn fold(
        &mut self,
        x: &SymbolExposure,
        investigation_id: &str,
        contrib_out: &mut impl Write,
    ) {
        let a = self
            .symbols
            .entry(x.symbol_id.clone())
            .or_insert_with(|| SymbolActivity {
                symbol_id: x.symbol_id.clone(),
                symbol_name: x.symbol_name.clone(),
                path: x.path.clone(),
                symbol_kind: x.symbol_kind.clone(),
                ..Default::default()
            });
        a.observation_event_count_total += 1;
        match x.observation_provenance.as_str() {
            "explicit_read" => a.explicit_read_exposure_count += 1,
            "search_snippet" => a.search_snippet_exposure_count += 1,
            _ => {}
        }
        match x.exposure_kind {
            ExposureKind::EnclosingDeclaration => a.enclosing_declaration_count += 1,
            ExposureKind::DeclarationOccurrence => a.declaration_occurrence_count += 1,
            ExposureKind::ReferenceOccurrence => a.reference_occurrence_count += 1,
        }
        self.sessions
            .entry(x.symbol_id.clone())
            .or_default()
            .insert(x.session_id.clone());
        self.investigations
            .entry(x.symbol_id.clone())
            .or_default()
            .insert(investigation_id.to_string());
        let _ = writeln!(
            contrib_out,
            "{}",
            serde_json::to_string(&SymbolContribution {
                symbol_id: &x.symbol_id,
                session_id: &x.session_id,
                investigation_id,
                exposure_kind: x.exposure_kind.as_str(),
                observation_provenance: x.observation_provenance.as_str(),
                certainty: match x.certainty {
                    super::model::Certainty::Fact => "fact",
                    super::model::Certainty::Candidate => "candidate",
                },
                source_event_sequence: x.source_event_sequence,
            })
            .unwrap_or_default()
        );
    }

    /// Finalize session/investigation counts + persist hot records.
    pub fn save(&mut self) -> std::io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        for (sid, set) in &self.sessions {
            if let Some(a) = self.symbols.get_mut(sid) {
                a.sessions_exposed_count = set.len() as u64;
            }
        }
        for (sid, set) in &self.investigations {
            if let Some(a) = self.symbols.get_mut(sid) {
                a.investigations_exposed_count = set.len() as u64;
            }
        }
        let mut f = fs::File::create(self.dir.join("symbols.jsonl"))?;
        for a in self.symbols.values() {
            writeln!(f, "{}", serde_json::to_string(a)?)?;
        }
        Ok(())
    }

    /// The contribution sink path (caller opens/writes).
    pub fn contrib_path(&self) -> &std::path::Path {
        &self.contrib_path
    }
}
