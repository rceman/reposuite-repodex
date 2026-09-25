//! Service API — /v1/* routes (§17-§23, §78-§79). Business semantics live here,
//! separated from the HTTP transport so a future socket/pipe transport can
//! reuse them (§60). Harness-neutral — no Task/Gateway knowledge (§41-§42).

use serde_json::json;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use crate::agent_event::ingest::validate_event;
use crate::agent_event::model::AgentEvent;
use crate::view as v;

use super::derived::DerivedHot;
use super::http::{Request, Response};
use super::segments::{event_semantic_digest, AppendOutcome};
use super::state::ServiceState;

/// Build the route handler for this service instance.
pub fn handler(state: Arc<ServiceState>, hot: Arc<DerivedHot>) -> super::http::Handler {
    Arc::new(move |req: &Request| route(&state, &hot, req))
}

fn authed(state: &ServiceState, req: &Request) -> Result<(), Response> {
    match &req.bearer {
        Some(t) if super::token::token_matches(&state.token, t) => Ok(()),
        Some(_) => Err(Response::err(401, "UNAUTHORIZED", "invalid token")),
        None => Err(Response::err(401, "UNAUTHORIZED", "missing bearer token")),
    }
}

fn route(state: &Arc<ServiceState>, hot: &Arc<DerivedHot>, req: &Request) -> Response {
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/v1/status") => {
            if let Err(r) = authed(state, req) {
                return r;
            }
            status(state, hot)
        }
        ("POST", "/v1/query") => {
            if let Err(r) = authed(state, req) {
                return r;
            }
            query(state, req)
        }
        ("POST", "/v1/events") => {
            if let Err(r) = authed(state, req) {
                return r;
            }
            events(state, req, false)
        }
        ("POST", "/v1/events/batch") => {
            if let Err(r) = authed(state, req) {
                return r;
            }
            events(state, req, true)
        }
        ("POST", "/v1/shutdown") => {
            if let Err(r) = authed(state, req) {
                return r;
            }
            state.shutdown.store(true, Ordering::SeqCst);
            Response::json(
                200,
                &json!({"schema":"reposuite.repodex.shutdown.v1","ok":true}),
            )
        }
        _ => Response::err(404, "NOT_FOUND", "unknown route"),
    }
}

// ---- /v1/status (§22-§23, §61) ----------------------------------------------

fn status(state: &Arc<ServiceState>, hot: &Arc<DerivedHot>) -> Response {
    let ws = state.events.stats();
    let ql = state.metrics.query_latency.lock().unwrap().summary();
    let il = state.metrics.ingest_latency.lock().unwrap().summary();
    let body = json!({
        "schema":"reposuite.repodex.service.status.v1",
        "uptime_s": state.uptime_s(),
        "version": env!("CARGO_PKG_VERSION"),
        "memory": {
            "budget_mb": state.config.memory.max_mb,
            "current_mb": (state.memory_current_bytes() as f64)/(1024.0*1024.0),
        },
        "active_views": *state.active_views.lock().unwrap(),
        "queries": {
            "count": state.metrics.queries.load(Ordering::SeqCst),
            "latency_ms": {"mean":ql.mean,"p50":ql.p50,"p95":ql.p95,"p99":ql.p99,"max":ql.max},
        },
        "events": {
            "received": state.metrics.events_received.load(Ordering::SeqCst),
            "persisted": state.metrics.events_persisted.load(Ordering::SeqCst),
            "duplicates": state.metrics.events_duplicate.load(Ordering::SeqCst),
            "rejected": state.metrics.events_rejected.load(Ordering::SeqCst),
            "queue_depth": state.events.pending(),
            "ingest_latency_ms": {"mean":il.mean,"p50":il.p50,"p95":il.p95,"p99":il.p99,"max":il.max},
        },
        "derived": {
            "applied": state.metrics.derived_events_applied.load(Ordering::SeqCst),
            "lag_events": state.metrics.derived_lag_events.load(Ordering::SeqCst),
            "checkpoints": state.metrics.checkpoints.load(Ordering::SeqCst),
            "sessions": hot.sessions.lock().unwrap().len(),
        },
        "storage": {
            "segments": state.events.segment_count(),
            "committed_events": state.events.committed_events(),
            "logical_bytes": ws.logical_bytes,
            "fsyncs": ws.fsyncs,
            "durable_batches": ws.durable_batches,
            "write_calls": ws.write_calls,
        },
        "file_analysis": {
            "cache_size": state.hot.file_analysis.lock().unwrap().len(),
            "hits": state.metrics.fa_cache_hit.load(Ordering::SeqCst),
            "misses": state.metrics.fa_cache_miss.load(Ordering::SeqCst),
            "unique_versions_parsed": state.metrics.unique_versions_parsed.load(Ordering::SeqCst),
        },
    });
    Response::json(200, &body)
}

// ---- /v1/query (§18-§19) -----------------------------------------------------

fn query(state: &Arc<ServiceState>, req: &Request) -> Response {
    let started = Instant::now();
    let parsed: Result<v::QueryRequest, _> = serde_json::from_slice(&req.body);
    let qreq = match parsed {
        Ok(q) => q,
        Err(e) => return Response::err(400, "INVALID_REQUEST", &format!("malformed request: {e}")),
    };
    let locator = match v::service::locator_from_request(&qreq) {
        Ok(l) => l,
        Err((k, m)) => return Response::err(400, k.as_str(), &m),
    };
    let text = match &qreq.query {
        Some(t) => t.clone(),
        None => return Response::err(400, "INVALID_REQUEST", "missing query"),
    };
    let intent = qreq
        .intent
        .as_deref()
        .and_then(crate::query::QueryIntent::parse);
    let params = v::service::ViewQueryParams {
        query_text: &text,
        mode: crate::query::QueryMode::Ranked,
        intent,
        target: qreq.target.clone(),
        to: qreq.to.clone(),
        max_results: qreq.max_results.unwrap_or(50),
        token_budget: None,
        depth: qreq.depth.unwrap_or(4),
        memory_mode: crate::memory::compose::MemoryMode::parse(
            qreq.memory_mode.as_deref().unwrap_or("off"),
        ),
        context_policy: crate::context::ContextPolicy::parse(
            qreq.context_policy.as_deref().unwrap_or("static"),
        ),
        recipes: crate::recipe::RecipePolicy::parse(qreq.recipes.as_deref().unwrap_or("off")),
        utility_policy: crate::utility::UtilityPolicy::parse(
            qreq.utility_policy.as_deref().unwrap_or("off"),
        ),
        source_witness: crate::witness::SourceWitnessPolicy::parse(
            qreq.source_witness.as_deref().unwrap_or("off"),
        ),
        vocab_bridge: matches!(qreq.vocab_bridge.as_deref(), Some("on")),
        state_override: Some(&state.state_dir),
    };
    let outcome = match v::run_view_query(&locator, &params) {
        Ok(o) => o,
        Err((k, m)) => return Response::err(400, k.as_str(), &m),
    };
    state.note_view(&outcome.view.canonical_root.display().to_string());
    state.enforce_memory_budget();
    state.metrics.queries.fetch_add(1, Ordering::SeqCst);
    state
        .metrics
        .query_latency
        .lock()
        .unwrap()
        .add(started.elapsed().as_secs_f64() * 1000.0);
    let body = json!({
        "schema": v::QUERY_RESPONSE_SCHEMA,
        "repository_view": view_meta_json(&outcome.view),
        "query": text,
        "result": query_result_json(&outcome, Some(&outcome.ensure.index_dir())),
        "index": {"fingerprint":outcome.ensure.fingerprint,
                  "reused":outcome.ensure.index_reused,
                  "analyses_reused":outcome.ensure.analyses_reused,
                  "analyses_parsed":outcome.ensure.analyses_parsed},
        "ensure_ms": outcome.ensure.ensure_ms,
    });
    Response::json(200, &body)
}

fn view_meta_json(view: &v::RepositoryView) -> serde_json::Value {
    json!({
        "repository_id":view.repository_id,"view_id":view.view_id,
        "root":view.canonical_root,"head":view.head,"branch":view.branch,
        "detached":view.detached,"dirty":view.dirty,
        "git_common_dir":view.git_common_dir,
        "view_fingerprint":view.view_fingerprint,"locator":view.locator,
    })
}

/// Canonical machine `result` = the ONE EvidenceProjection (§6) — same shape as
/// direct `--json`, with rank/path/ranges/provenance preserved (no parallel
/// service renderer). `index_dir` enables current-range materialization.
fn query_result_json(
    outcome: &v::service::ViewQueryOutcome,
    index_dir: Option<&Path>,
) -> serde_json::Value {
    let mut p = crate::query::projection::build(&outcome.result, index_dir);
    let m = &outcome.memory;
    if m.mode != crate::memory::compose::MemoryMode::Off.as_str()
        && !m.degraded
        && m.matched_investigations > 0
    {
        p.memory = serde_json::to_value(m).ok();
    }
    if let Some(ut) = &outcome.utility_trace {
        p.utility = Some(serde_json::to_value(ut).unwrap_or_default());
    }
    // §16: source-witness delivery — exact current bytes for bounded ranges.
    if outcome.source_witness == crate::witness::SourceWitnessPolicy::Bounded {
        let wp = crate::witness::materialize(&p, &outcome.view_root, index_dir);
        p.source_exposure = Some(
            serde_json::json!({"witness_bytes":wp.witness_bytes,"witness_count":wp.witnesses.len(),"unavailable":wp.unavailable}),
        );
        p.witnesses = wp
            .witnesses
            .iter()
            .map(|w| serde_json::to_value(w).unwrap_or_default())
            .collect();
    }
    if let Some(rc) = &outcome.recipe {
        p.recipe = Some(
            serde_json::json!({"recipe_id":rc.recipe_id,"family":rc.family,"produced":rc.produced.len(),"fell_back":rc.fell_back}),
        );
    }
    if outcome.context_policy == crate::context::ContextPolicy::Adaptive {
        crate::context::compile_into(
            &mut p,
            outcome.shape,
            outcome.memory.mode_parse(),
            &crate::context::ContextBudget::default(),
        );
    }
    p.to_json()
}

// ---- /v1/events + /v1/events/batch (§41-§45) --------------------------------

fn events(state: &Arc<ServiceState>, req: &Request, batch: bool) -> Response {
    let t0 = Instant::now();
    let events: Vec<AgentEvent> = if batch {
        match serde_json::from_slice(&req.body) {
            Ok(v) => v,
            Err(e) => {
                return Response::err(400, "INVALID_REQUEST", &format!("malformed batch: {e}"))
            }
        }
    } else {
        match serde_json::from_slice(&req.body) {
            Ok(e) => vec![e],
            Err(e) => {
                return Response::err(400, "INVALID_REQUEST", &format!("malformed event: {e}"))
            }
        }
    };
    let mut accepted = 0u64;
    let mut duplicates = 0u64;
    let mut rejected = 0u64;
    let mut errors: Vec<String> = Vec::new();
    // Validate + serialize all first (small ACK path, §44), then ONE durable
    // batch enqueue — a whole POST shares a single group-commit fsync.
    let mut items: Vec<(String, String, Vec<u8>)> = Vec::with_capacity(events.len());
    let mut ok_events: Vec<AgentEvent> = Vec::with_capacity(events.len());
    for e in &events {
        state.metrics.events_received.fetch_add(1, Ordering::SeqCst);
        if let Err(err) = validate_event(e) {
            rejected += 1;
            state.metrics.events_rejected.fetch_add(1, Ordering::SeqCst);
            errors.push(format!("{}: {}", e.event_id, err));
            continue;
        }
        match serde_json::to_vec(e) {
            Ok(mut l) => {
                l.push(b'\n');
                items.push((e.event_id.clone(), event_semantic_digest(e), l));
                ok_events.push(e.clone());
            }
            Err(er) => {
                rejected += 1;
                errors.push(format!("serialize: {er}"));
            }
        }
    }
    let outcomes = match state
        .events
        .clone()
        .append_many_durable(&items, &state.dedup)
    {
        Ok(o) => o,
        Err(m) => {
            state
                .metrics
                .ingest_latency
                .lock()
                .unwrap()
                .add(t0.elapsed().as_secs_f64() * 1000.0);
            return Response::err(500, "QUERY_FAILED", &m);
        }
    };
    for (e, o) in ok_events.iter().zip(outcomes.iter()) {
        match o {
            AppendOutcome::Committed(_) => {
                accepted += 1;
                state
                    .metrics
                    .events_persisted
                    .fetch_add(1, Ordering::SeqCst);
                if let Some(tx) = state.derived_tx.lock().unwrap().as_ref() {
                    let _ = tx.try_send(e.clone());
                }
            }
            AppendOutcome::Duplicate => {
                duplicates += 1;
                state
                    .metrics
                    .events_duplicate
                    .fetch_add(1, Ordering::SeqCst);
            }
            AppendOutcome::Conflict(id) => {
                rejected += 1;
                state.metrics.events_rejected.fetch_add(1, Ordering::SeqCst);
                errors.push(format!("conflicting event_id {id}"));
            }
        }
    }
    state
        .metrics
        .ingest_latency
        .lock()
        .unwrap()
        .add(t0.elapsed().as_secs_f64() * 1000.0);
    Response::json(
        200,
        &json!({
            "schema":"reposuite.repodex.ingest.v1",
            "accepted":accepted,"duplicates":duplicates,"rejected":rejected,
            "committed_events":state.events.committed_events(),
            "errors":errors,
        }),
    )
}
