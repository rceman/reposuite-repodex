//! Merge per-session `SessionPart`s into one `InvestigationEpisode` (§4-§22),
//! and extract conservative final-answer path mentions joined to observed/read
//! state.

use std::collections::{BTreeMap, BTreeSet};

use crate::repository::digest;

use super::model::*;
use super::session::{normalize_query, SessionPart};

/// Strip a trailing `:N` or `:N-M` line reference from a token.
fn strip_line_ref(t: &str) -> &str {
    if let Some(i) = t.rfind(':') {
        let tail = &t[i + 1..];
        if !tail.is_empty()
            && tail
                .chars()
                .all(|c| c.is_ascii_digit() || c == '-' || c == ',')
        {
            return &t[..i];
        }
    }
    t
}

/// Extract canonical repo-path mentions from answer text (§17-§18): only exact
/// matches against `known_paths` count. `repo_root` lets absolute paths like
/// `/repo/x.go` normalize to `x.go`. Returns (path_mentions, evidence_mentions,
/// unresolved). Conservative — no fuzzy inference. `evidence_mentions` are
/// paths inside a structured `EVIDENCE:` block (stricter §21 semantics).
pub fn extract_path_mentions(
    text: &str,
    known_paths: &BTreeSet<String>,
    repo_root: Option<&str>,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut found = BTreeSet::new();
    let mut evidence = BTreeSet::new();
    let mut unresolved = BTreeSet::new();
    let mut in_evidence = false;
    for line in text.lines() {
        let lt = line.trim();
        if lt.starts_with("EVIDENCE") || lt.starts_with("Evidence") {
            in_evidence = true;
        } else if lt.ends_with(':') && !lt.contains('/') {
            // a new non-evidence section header ends the evidence block
            in_evidence = false;
        }
        for tok in line.split(|c: char| c.is_whitespace() || "`\"'()[]{},;*".contains(c)) {
            let t = tok.trim_matches(|c: char| !(c.is_alphanumeric() || "/._-:".contains(c)));
            if t.is_empty() {
                continue;
            }
            // strip sentence-ending punctuation + line refs
            let t = strip_line_ref(t).trim_end_matches('.');
            if t.is_empty() {
                continue;
            }
            // candidate = has '/' + a '.' extension, not a URL
            if !(t.contains('/') && t.contains('.') && t.len() > 4 && !t.starts_with("http")) {
                continue;
            }
            // try to resolve to a repo-relative canonical path
            let mut cands = vec![t.to_string()];
            if let Some(root) = repo_root {
                let root = root.trim_end_matches('/');
                if let Some(rel) = t.strip_prefix(root) {
                    cands.push(rel.trim_start_matches('/').to_string());
                }
            }
            if let Some(rel) = t.strip_prefix("./") {
                cands.push(rel.to_string());
            }
            if let Some(rel) = t.strip_prefix('/') {
                cands.push(rel.to_string());
            }
            let mut matched = false;
            for c in &cands {
                if known_paths.contains(c.as_str()) {
                    found.insert(c.clone());
                    if in_evidence {
                        evidence.insert(c.clone());
                    }
                    matched = true;
                    break;
                }
            }
            if !matched {
                unresolved.insert(t.to_string());
            }
        }
    }
    (
        found.into_iter().collect(),
        evidence.into_iter().collect(),
        unresolved.into_iter().collect(),
    )
}

/// Merge session parts (one investigation) into an episode.
pub fn merge_episode(
    investigation_id: &str,
    parts: &[SessionPart],
    known_paths: &BTreeSet<String>,
    repo_root: Option<&str>,
) -> InvestigationEpisode {
    let mut ep = InvestigationEpisode {
        schema: "reposuite.investigation-episode.v1".into(),
        schema_version: DERIVED_SCHEMA_VERSION,
        investigation_id: investigation_id.to_string(),
        session_ids: Vec::new(),
        project_id: None,
        repository_id: None,
        repo_heads: Vec::new(),
        started_at: None,
        completed_at: None,
        state: InvestigationState::Open,
        query_text: None,
        normalized_query: None,
        query_digest: None,
        model_calls: 0,
        input_tokens: None,
        output_tokens: None,
        cached_input_tokens: None,
        cached_output_tokens: None,
        reasoning_tokens: None,
        usage_is_actual: true,
        per_model: BTreeMap::new(),
        tools: ToolTotals::default(),
        source_exposures: Vec::new(),
        unique_source_files_observed: 0,
        unique_source_files_explicitly_read: 0,
        source_observation_events: 0,
        source_exposure_bytes_total: 0,
        final_answers: Vec::new(),
        final_answer_path_mentions: Vec::new(),
        evidence_path_mentions: Vec::new(),
        unresolved_path_mentions: Vec::new(),
        session_outcomes: Vec::new(),
        task_outcomes: Vec::new(),
        quality_outcome: None,
    };
    let mut saw_tokens = false;
    let mut exposures: BTreeMap<String, PathExposure> = BTreeMap::new();
    let mut all_completed = !parts.is_empty();
    let mut any_completed = false;
    let mut any_failed = false;
    for p in parts {
        ep.session_ids.push(p.session_id.clone());
        if ep.project_id.is_none() {
            ep.project_id = p.project_id.clone();
        }
        if ep.repository_id.is_none() {
            ep.repository_id = p.repository_id.clone();
        }
        if let Some(h) = &p.repo_head {
            if !ep.repo_heads.contains(h) {
                ep.repo_heads.push(h.clone());
            }
        }
        // times
        if let Some(t) = &p.started_at {
            if ep.started_at.as_ref().is_none_or(|x| t < x) {
                ep.started_at = Some(t.clone());
            }
        }
        if let Some(t) = &p.completed_at {
            if ep.completed_at.as_ref().is_none_or(|x| t > x) {
                ep.completed_at = Some(t.clone());
            }
        }
        // query: deterministic preference = earliest session's query (§6)
        if ep.query_text.is_none() {
            if let Some(q) = &p.query_text {
                ep.query_text = Some(q.clone());
            }
        }
        // usage (§9: sum authoritative per-call actuals, no double count)
        ep.model_calls += p.model_calls;
        ep.input_tokens = Some(ep.input_tokens.unwrap_or(0) + p.input_tokens);
        ep.output_tokens = Some(ep.output_tokens.unwrap_or(0) + p.output_tokens);
        ep.cached_input_tokens = Some(ep.cached_input_tokens.unwrap_or(0) + p.cached_input_tokens);
        ep.cached_output_tokens =
            Some(ep.cached_output_tokens.unwrap_or(0) + p.cached_output_tokens);
        ep.reasoning_tokens = Some(ep.reasoning_tokens.unwrap_or(0) + p.reasoning_tokens);
        saw_tokens |= p.saw_token_fields;
        ep.usage_is_actual &= p.usage_is_actual;
        for (m, u) in &p.per_model {
            let d = ep.per_model.entry(m.clone()).or_default();
            d.calls += u.calls;
            if let Some(v) = u.input_tokens {
                *d.input_tokens.get_or_insert(0) += v;
            }
            if let Some(v) = u.output_tokens {
                *d.output_tokens.get_or_insert(0) += v;
            }
            if let Some(v) = u.cached_input_tokens {
                *d.cached_input_tokens.get_or_insert(0) += v;
            }
            if let Some(v) = u.cached_output_tokens {
                *d.cached_output_tokens.get_or_insert(0) += v;
            }
            if let Some(v) = u.reasoning_tokens {
                *d.reasoning_tokens.get_or_insert(0) += v;
            }
        }
        // tools
        let t = &p.tools;
        ep.tools.tool_calls_total += t.tool_calls_total;
        ep.tools.search_calls += t.search_calls;
        ep.tools.file_read_calls += t.file_read_calls;
        ep.tools.directory_list_calls += t.directory_list_calls;
        ep.tools.git_inspection_calls += t.git_inspection_calls;
        ep.tools.shell_calls += t.shell_calls;
        ep.tools.other_repository_tool_calls += t.other_repository_tool_calls;
        ep.tools.non_repository_tool_calls += t.non_repository_tool_calls;
        ep.tools.failed_tool_calls += t.failed_tool_calls;
        for (k, v) in &t.native_tool_counts {
            *ep.tools.native_tool_counts.entry(k.clone()).or_insert(0) += v;
        }
        // merge exposures by path (§12)
        for (path, pe) in &p.exposures {
            let e = exposures
                .entry(path.clone())
                .or_insert_with(|| PathExposure {
                    path: path.clone(),
                    first_observed_sequence: pe.first_observed_sequence,
                    last_observed_sequence: pe.last_observed_sequence,
                    first_observed_at: pe.first_observed_at.clone(),
                    last_observed_at: pe.last_observed_at.clone(),
                    ..Default::default()
                });
            e.observation_event_count += pe.observation_event_count;
            e.explicit_read_count += pe.explicit_read_count;
            e.search_snippet_count += pe.search_snippet_count;
            e.symbol_preview_count += pe.symbol_preview_count;
            e.diff_observation_count += pe.diff_observation_count;
            e.other_observation_count += pe.other_observation_count;
            e.observed_bytes_total += pe.observed_bytes_total;
            if pe.first_observed_at < e.first_observed_at {
                e.first_observed_at = pe.first_observed_at.clone();
                e.first_observed_sequence = pe.first_observed_sequence;
            }
            if pe.last_observed_at > e.last_observed_at {
                e.last_observed_at = pe.last_observed_at.clone();
                e.last_observed_sequence = pe.last_observed_sequence;
            }
            for (a, b) in &pe.line_ranges {
                e.line_ranges.push((*a, *b));
            }
        }
        // final answers (keep each session's; §16 multi-session documented)
        for (sid, content) in &p.final_answers {
            let (pm, evm, un) = extract_path_mentions(content, known_paths, repo_root);
            ep.final_answers.push(FinalAnswerRef {
                session_id: sid.clone(),
                content_digest: Some(digest::sha256_text(content)),
                content_bytes: Some(content.len() as u64),
                path_mentions: pm.clone(),
                unresolved_path_mentions: un.clone(),
            });
            for e in evm {
                if !ep.evidence_path_mentions.contains(&e) {
                    ep.evidence_path_mentions.push(e);
                }
            }
            for m in pm {
                if !ep.final_answer_path_mentions.iter().any(|x| x.path == m) {
                    ep.final_answer_path_mentions.push(PathMention {
                        path: m,
                        observed_during_investigation: false,
                        explicitly_read: false,
                        first_observed_sequence: None,
                    });
                }
            }
            ep.unresolved_path_mentions.extend(un);
        }
        ep.session_outcomes
            .extend(p.session_outcomes.iter().cloned());
        ep.task_outcomes.extend(p.task_outcomes.iter().cloned());
        all_completed &= p.completed;
        any_completed |= p.completed;
        if p.task_outcomes
            .iter()
            .any(|o| o.contains("fail") || o.contains("timeout"))
        {
            any_failed = true;
        }
    }
    ep.session_ids.sort();
    ep.session_ids.dedup();
    ep.repo_heads.sort();
    ep.unresolved_path_mentions.sort();
    ep.unresolved_path_mentions.dedup();
    // observed/read join on mentions (§20)
    for m in &mut ep.final_answer_path_mentions {
        if let Some(e) = exposures.get(&m.path) {
            m.observed_during_investigation = true;
            m.explicitly_read = e.explicit_read_count > 0;
            m.first_observed_sequence = Some(e.first_observed_sequence);
        }
    }
    // episode exposure summary (§13)
    let mut exps: Vec<PathExposure> = exposures.into_values().collect();
    for e in &mut exps {
        e.line_ranges.sort_unstable();
    }
    ep.unique_source_files_observed = exps.len() as u64;
    ep.unique_source_files_explicitly_read =
        exps.iter().filter(|e| e.explicit_read_count > 0).count() as u64;
    ep.source_observation_events = exps.iter().map(|e| e.observation_event_count).sum();
    ep.source_exposure_bytes_total = exps.iter().map(|e| e.observed_bytes_total).sum();
    ep.source_exposures = exps;
    // state (§5)
    ep.state = if any_failed {
        InvestigationState::Failed
    } else if all_completed && any_completed {
        InvestigationState::Completed
    } else if any_completed {
        InvestigationState::Partial
    } else {
        InvestigationState::Open
    };
    // query digest + normalized (§6-§7)
    if let Some(q) = &ep.query_text {
        ep.query_digest = Some(digest::sha256_text(q));
        ep.normalized_query = Some(normalize_query(q));
    }
    if !saw_tokens {
        ep.usage_is_actual = false;
    }
    ep
}
