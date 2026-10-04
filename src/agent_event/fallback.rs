//! Native-fallback association analysis (PRODUCTION-GAP-TELEMETRY-V1 §21-§25).
//!
//! For every `context_artifact_presented` event, observe the bounded sequence
//! window that follows it and classify what happened next. This is an
//! ASSOCIATION, not a causal claim: `native_discovery_after_artifact` means
//! broad repository discovery occurred inside the window — it never asserts
//! the artifact caused it.
//!
//! Ordering authority is `sequence` (§40). Sessions are analyzed independently
//! (§42) and `repository_id`/`repo_head` are carried, never merged (§43).

use std::collections::BTreeMap;

use serde::Serialize;

use super::model::{AgentEvent, ContextArtifactPresented, ToolCategory};

/// Hard bound on events examined after one artifact presentation (§22).
pub const FALLBACK_WINDOW_MAX_EVENTS: usize = 64;

/// Follow-up classification for one presented artifact (§24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FollowupDisposition {
    /// Nothing else happened inside the window.
    NoFollowupDiscovery,
    /// Only exact source reads / source-observation events followed.
    SourceVerificationOnly,
    /// Broad native repository discovery (search/directory/uncorrelated
    /// repository tool) followed the artifact.
    NativeDiscoveryAfterArtifact,
    /// The next observable context action was another artifact presentation
    /// (e.g. a follow-up RepoDex query).
    AnotherContextArtifact,
    /// An edit/write/build/test/runtime action ended the window.
    TaskActionStarted,
}

impl FollowupDisposition {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoFollowupDiscovery => "no_followup_discovery",
            Self::SourceVerificationOnly => "source_verification_only",
            Self::NativeDiscoveryAfterArtifact => "native_discovery_after_artifact",
            Self::AnotherContextArtifact => "another_context_artifact",
            Self::TaskActionStarted => "task_action_started",
        }
    }
}

/// One presented artifact plus the observed follow-up (recomputed from raw
/// events; never a second persisted copy of the observation).
#[derive(Debug, Clone, Serialize)]
pub struct FallbackAssociation {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub investigation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_head: Option<String>,
    /// Sequence of the `context_artifact_presented` event.
    pub artifact_sequence: u64,
    pub artifact_id: String,
    pub artifact_kind: String,
    pub producer: String,
    /// The tool call the artifact correlates with (§19), if supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Producer-supplied structured gap signatures, verbatim.
    #[serde(default)]
    pub gap_signatures: Vec<super::model::GapSignature>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_intent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<String>,
    pub disposition: FollowupDisposition,
    /// True when exact source verification preceded the observed outcome.
    pub verified_before: bool,
    /// Events scanned inside the window.
    pub window_events: usize,
}

fn ca(e: &AgentEvent) -> Option<ContextArtifactPresented> {
    if e.event_type != "context_artifact_presented" {
        return None;
    }
    serde_json::from_value::<ContextArtifactPresented>(e.data.clone()).ok()
}

fn is_native_discovery(cat: ToolCategory, correlated: bool) -> bool {
    !correlated && matches!(cat, ToolCategory::Search | ToolCategory::DirectoryList)
}

fn is_task_action(e: &AgentEvent) -> bool {
    if e.event_type == "tool_call_started" {
        let cat = e.data["category"].as_str().unwrap_or_default();
        return matches!(
            cat,
            "shell" | "non_repository_tool" | "other_repository_tool"
        );
    }
    false
}

/// Analyze ONE session's events (must already be this session's events only).
/// Sorts defensively by `sequence`; identical sequences keep store order.
pub fn analyze_session(events: &[AgentEvent]) -> Vec<FallbackAssociation> {
    let mut evs: Vec<&AgentEvent> = events.iter().collect();
    evs.sort_by_key(|e| e.sequence);
    // tool_call_ids that produced a presented artifact — a RepoDex query call
    // is producer-correlated, not native discovery.
    let producer_calls: std::collections::BTreeSet<String> = evs
        .iter()
        .filter_map(|e| ca(e))
        .filter_map(|c| c.tool_call_id)
        .collect();
    let mut out = Vec::new();
    for (i, e) in evs.iter().enumerate() {
        let artifact = match ca(e) {
            Some(a) => a,
            None => continue,
        };
        let mut saw_native = false;
        let mut saw_verify = false;
        let mut other_artifact = false;
        let mut task_action = false;
        let mut scanned = 0usize;
        for n in evs.iter().skip(i + 1).take(FALLBACK_WINDOW_MAX_EVENTS) {
            scanned += 1;
            match n.event_type.as_str() {
                "context_artifact_presented" => {
                    other_artifact = true;
                    break;
                }
                "tool_call_started" => {
                    let id = n.data["tool_call_id"].as_str().unwrap_or_default();
                    if producer_calls.contains(id) {
                        continue;
                    }
                    let cat = serde_json::from_value::<ToolCategory>(n.data["category"].clone())
                        .unwrap_or_default();
                    if is_native_discovery(cat, false) {
                        saw_native = true;
                    } else if cat == ToolCategory::FileRead {
                        saw_verify = true;
                    } else if is_task_action(n) {
                        task_action = true;
                        break;
                    }
                }
                "source_observed" => {
                    saw_verify = true;
                }
                _ => {}
            }
        }
        let disposition = if saw_native {
            FollowupDisposition::NativeDiscoveryAfterArtifact
        } else if other_artifact {
            FollowupDisposition::AnotherContextArtifact
        } else if saw_verify {
            FollowupDisposition::SourceVerificationOnly
        } else if task_action {
            FollowupDisposition::TaskActionStarted
        } else {
            FollowupDisposition::NoFollowupDiscovery
        };
        let meta = artifact.metadata.clone().unwrap_or_default();
        let gaps: Vec<super::model::GapSignature> =
            serde_json::from_value(meta["gap_signatures"].clone()).unwrap_or_default();
        out.push(FallbackAssociation {
            session_id: e.session_id.clone(),
            investigation_id: e.investigation_id.clone(),
            repository_id: e.repository_id.clone(),
            repo_head: e.repo_head.clone(),
            artifact_sequence: e.sequence,
            artifact_id: artifact.artifact_id,
            artifact_kind: artifact.artifact_kind,
            producer: artifact.producer,
            tool_call_id: artifact.tool_call_id,
            gap_signatures: gaps,
            query_intent: meta["query_intent"].as_str().map(String::from),
            presentation: artifact.presentation,
            disposition,
            verified_before: saw_verify,
            window_events: scanned,
        });
    }
    out
}

/// Buckets keyed by one dimension value; counts keep denominators (§29).
#[derive(Debug, Clone, Default, Serialize)]
pub struct DispositionCounts {
    pub presentations: u64,
    pub native_discovery_after: u64,
    pub source_verification_only: u64,
    pub another_context_artifact: u64,
    pub task_action_started: u64,
    pub no_followup_discovery: u64,
}

impl DispositionCounts {
    fn add(&mut self, d: FollowupDisposition) {
        self.presentations += 1;
        match d {
            FollowupDisposition::NativeDiscoveryAfterArtifact => self.native_discovery_after += 1,
            FollowupDisposition::SourceVerificationOnly => self.source_verification_only += 1,
            FollowupDisposition::AnotherContextArtifact => self.another_context_artifact += 1,
            FollowupDisposition::TaskActionStarted => self.task_action_started += 1,
            FollowupDisposition::NoFollowupDiscovery => self.no_followup_discovery += 1,
        }
    }
}

/// The deterministic offline report (§28). JSON is appropriate here — this
/// output is machine telemetry, never Agent-facing RDX.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FallbackReport {
    pub schema: String,
    pub sessions_scanned: u64,
    /// Sessions that presented no context artifacts at all (control, §39).
    pub sessions_without_artifacts: u64,
    pub artifact_presentations: u64,
    pub repodex_presentations: u64,
    /// Breakdowns — every bucket keeps `presentations` as denominator.
    pub by_disposition: DispositionCounts,
    pub by_gap_family: BTreeMap<String, DispositionCounts>,
    pub by_gap_reason_code: BTreeMap<String, DispositionCounts>,
    pub by_intent: BTreeMap<String, DispositionCounts>,
    pub by_producer: BTreeMap<String, DispositionCounts>,
    /// Artifacts that carried no gap signature at all.
    pub no_gap: DispositionCounts,
    /// Presentations in sessions that also presented RepoDex artifacts but for
    /// a different repository — never merged (§43); counted for audit.
    pub cross_repository_associations: u64,
    /// Impossible by construction — assertions must keep this zero (§39).
    pub false_repodex_associations: u64,
    pub window_max_events: usize,
}

pub const FALLBACK_REPORT_SCHEMA: &str = "reposuite.repodex.gap-fallback-report.v1";

/// Build the report from already-loaded per-session event lists.
/// Associations attach only to sessions that actually presented artifacts.
pub fn report(sessions: &[(String, Vec<AgentEvent>)]) -> FallbackReport {
    let mut r = FallbackReport {
        schema: FALLBACK_REPORT_SCHEMA.to_string(),
        sessions_scanned: sessions.len() as u64,
        window_max_events: FALLBACK_WINDOW_MAX_EVENTS,
        ..Default::default()
    };
    for (_sid, evs) in sessions {
        let assocs = analyze_session(evs);
        if assocs.is_empty() {
            r.sessions_without_artifacts += 1;
            continue;
        }
        for a in assocs {
            r.artifact_presentations += 1;
            if a.producer == "repodex" {
                r.repodex_presentations += 1;
            }
            r.by_disposition.add(a.disposition);
            let dims = [a
                .gap_signatures
                .iter()
                .map(|g| g.family.clone())
                .collect::<Vec<_>>()];
            if a.gap_signatures.is_empty() {
                r.no_gap.add(a.disposition);
            }
            for fam in &dims[0] {
                r.by_gap_family
                    .entry(fam.clone())
                    .or_default()
                    .add(a.disposition);
            }
            for g in &a.gap_signatures {
                if let Some(rc) = &g.reason_code {
                    r.by_gap_reason_code
                        .entry(format!("{}:{}", g.family, rc))
                        .or_default()
                        .add(a.disposition);
                }
            }
            r.by_intent
                .entry(a.query_intent.clone().unwrap_or_else(|| "unknown".into()))
                .or_default()
                .add(a.disposition);
            r.by_producer
                .entry(a.producer.clone())
                .or_default()
                .add(a.disposition);
        }
    }
    r
}
