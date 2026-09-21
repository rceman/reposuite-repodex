//! System One integration tests — all against a LOCAL protocol-compatible
//! HTTP server. No live TypeSafe/Jev credentials or network are used (§22/§47).

mod support;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use repodex::system_one::client::{HttpSystemOneModel, SystemOneModel};
use repodex::system_one::config;
use repodex::system_one::{
    Answer, Auth, ModelConfig, Question, SystemOne, SystemOneConfig, SystemOneRequest, PROTOCOL_V1,
};

// ---------------------------------------------------------------------------
// Minimal synchronous HTTP test server.
// ---------------------------------------------------------------------------

/// A captured request (method/path/auth-header/body).
#[derive(Debug, Clone, Default)]
struct Seen {
    path: String,
    auth: Option<String>,
    api_key: Option<String>,
    body: String,
}

/// Spin up a one-shot-per-connection HTTP server that always returns `body`.
/// Returns (base_url, handle to inspect captured requests, join handle).
fn serve<F>(responder: F) -> (String, Arc<Mutex<Vec<Seen>>>)
where
    F: Fn(&Seen) -> String + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut s = stream.unwrap();
            // Read until we have full headers + declared Content-Length body.
            let mut raw = Vec::new();
            let mut tmp = [0u8; 8192];
            loop {
                let n = match s.read(&mut tmp) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                raw.extend_from_slice(&tmp[..n]);
                let txt = String::from_utf8_lossy(&raw);
                if let Some(hp) = txt.find("\r\n\r\n") {
                    let cl: usize = txt[..hp]
                        .lines()
                        .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
                        .and_then(|l| l.split_once(':').unwrap().1.trim().parse().ok())
                        .unwrap_or(0);
                    if raw.len() >= hp + 4 + cl {
                        break; // full request received
                    }
                }
                if raw.len() > 1 << 20 {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&raw).to_string();
            // Parse a minimal request line + headers + body.
            let mut seen = Seen::default();
            if let Some(line) = text.lines().next() {
                seen.path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
            }
            let mut content_len = 0usize;
            for h in text.lines().skip(1) {
                let hl = h.to_ascii_lowercase();
                let val = || h.split_once(':').map(|(_, v)| v.trim().to_string());
                if hl.starts_with("authorization:") {
                    seen.auth = val();
                } else if hl.starts_with("x-api-key:") {
                    seen.api_key = val();
                } else if hl.starts_with("content-length:") {
                    content_len = val().and_then(|v| v.parse().ok()).unwrap_or(0);
                }
                if h.trim().is_empty() {
                    break;
                }
            }
            if let Some(pos) = text.find("\r\n\r\n") {
                let end = (pos + 4 + content_len).min(text.len());
                seen.body = text[pos + 4..end].to_string();
            }
            let resp_body = responder(&seen);
            seen2.lock().unwrap().push(seen);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                resp_body.len(),
                resp_body
            );
            let _ = s.write_all(resp.as_bytes());
            let _ = s.flush();
        }
    });
    (format!("http://127.0.0.1:{port}"), seen)
}

/// Spin up a server that returns a fixed HTTP status + body.
fn serve_status(status: u16, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let body = body.to_string();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut s = stream.unwrap();
            let mut buf = [0u8; 8192];
            let _ = s.read(&mut buf);
            let reason = match status {
                401 => "Unauthorized",
                429 => "Too Many Requests",
                500 => "Internal Server Error",
                _ => "OK",
            };
            let resp = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
        }
    });
    format!("http://127.0.0.1:{port}")
}

fn model_cfg(url: &str) -> ModelConfig {
    ModelConfig {
        protocol: PROTOCOL_V1.to_string(),
        url: url.to_string(),
        model: "test-model".to_string(),
        timeout_ms: 5000,
        auth: Auth::None,
    }
}

// ---------------------------------------------------------------------------
// Config (§5-§12)
// ---------------------------------------------------------------------------

#[test]
fn config_named_models_and_role_routing() {
    let t = support::TempDir::new("so-cfg");
    let path = t.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
[system_one]
enabled = true
[system_one.roles]
query = "local"
rerank = "jev"
[system_one.models.local]
protocol = "system-one-v1"
url = "http://127.0.0.1:8787/systemone"
model = "repodex-router"
timeout_ms = 2000
[system_one.models.local.auth]
type = "none"
[system_one.models.jev]
protocol = "system-one-v1"
url = "https://api.typesafe.ai/v1/systemone"
model = "jev-latest"
timeout_ms = 5000
[system_one.models.jev.auth]
type = "bearer"
token = "k"
"#,
    )
    .unwrap();
    let cfg = config::load(&path).unwrap();
    let so = cfg.system_one.unwrap();
    assert!(so.enabled);
    assert_eq!(so.roles.query.as_deref(), Some("local"));
    assert_eq!(so.roles.rerank.as_deref(), Some("jev"));
    assert_eq!(so.models.len(), 2);
    so.validate().unwrap();
    // role resolution routes to the right named model
    let (n, m) = so.model_for("query").unwrap();
    assert_eq!(n, "local");
    assert_eq!(m.model, "repodex-router");
    let (n, m) = so.model_for("rerank").unwrap();
    assert_eq!(n, "jev");
    assert_eq!(m.url, "https://api.typesafe.ai/v1/systemone");
}

#[test]
fn config_invalid_rejected() {
    // role references unknown model
    let so = SystemOneConfig {
        enabled: true,
        roles: repodex::system_one::Roles {
            query: Some("ghost".into()),
            rerank: None,
        },
        models: Default::default(),
    };
    assert!(so.validate().unwrap_err().contains("unknown model"));
    // unsupported protocol
    let mut bad = model_cfg("http://x");
    bad.protocol = "future-v9".into();
    let mut models = std::collections::BTreeMap::new();
    models.insert("m".to_string(), bad);
    let so = SystemOneConfig {
        enabled: true,
        roles: repodex::system_one::Roles {
            query: Some("m".into()),
            rerank: None,
        },
        models,
    };
    assert!(so.validate().unwrap_err().contains("unsupported protocol"));
}

#[test]
fn config_set_get_key() {
    // set keys incrementally on a raw doc — partial model config is allowed
    let t = support::TempDir::new("so-setget");
    let path = t.path().join("config.toml");
    let mut doc = config::load_value(&path).unwrap();
    config::set_key(&mut doc, "system-one.enabled", "true").unwrap();
    config::set_key(&mut doc, "system-one.models.jev.url", "https://x").unwrap();
    config::set_key(&mut doc, "system-one.models.jev.protocol", "system-one-v1").unwrap();
    config::set_key(&mut doc, "system-one.models.jev.model", "jev-latest").unwrap();
    config::set_key(&mut doc, "system-one.models.jev.timeout_ms", "5000").unwrap();
    config::set_key(&mut doc, "system-one.models.jev.auth.type", "bearer").unwrap();
    config::set_key(&mut doc, "system-one.models.jev.auth.token", "tok").unwrap();
    config::set_key(&mut doc, "system-one.roles.query", "jev").unwrap();
    config::save_value(&path, &doc).unwrap();
    assert_eq!(
        config::get_key(&doc, "system-one.enabled").as_deref(),
        Some("true")
    );
    assert_eq!(
        config::get_key(&doc, "system-one.models.jev.url").as_deref(),
        Some("https://x")
    );
    // the saved file deserializes into a valid typed config
    let cfg = config::load(&path).unwrap();
    let so = cfg.system_one.unwrap();
    assert!(so.enabled);
    assert_eq!(so.models["jev"].model, "jev-latest");
    assert_eq!(so.models["jev"].timeout_ms, 5000);
    assert_eq!(so.roles.query.as_deref(), Some("jev"));
    so.validate().unwrap();
}

// ---------------------------------------------------------------------------
// Protocol client (§15-§22)
// ---------------------------------------------------------------------------

#[test]
fn client_sends_model_state_and_parses_choice() {
    let (base, seen) = serve(|_| {
        r#"{"answers":{"intent":{"choice":"callers","confidence":0.9,"probabilities":{}}}}"#
            .to_string()
    });
    let url = format!("{base}/systemone");
    let m = HttpSystemOneModel::new("local", &model_cfg(&url)).unwrap();
    let mut req = SystemOneRequest::new("test-model", serde_json::json!({"q":"x"}));
    req.questions.insert(
        "intent".into(),
        Question::choice("pick", [("callers".to_string(), "c".to_string())].into()),
    );
    let resp = m.decide(&req).unwrap();
    let Answer::Choice(c) = &resp.answers["intent"] else {
        panic!("expected choice");
    };
    assert_eq!(c.choice, "callers");
    // the request hit the configured URL with model+state in the body
    let s = seen.lock().unwrap()[0].clone();
    assert_eq!(s.path, "/systemone");
    assert!(s.body.contains("\"model\":\"test-model\""));
    assert!(s.body.contains("\"q\":\"x\""));
    assert!(s.body.contains("\"type\":\"choice\""));
}

#[test]
fn client_auth_modes() {
    // bearer
    let (base, seen) = serve(|_| r#"{"answers":{}}"#.to_string());
    let mut cfg = model_cfg(&format!("{base}/s"));
    cfg.auth = Auth::Bearer {
        token: "sekrit".into(),
    };
    let m = HttpSystemOneModel::new("b", &cfg).unwrap();
    let req = SystemOneRequest::new("m", serde_json::json!({}));
    m.decide(&req).unwrap();
    assert_eq!(
        seen.lock().unwrap()[0].auth.as_deref(),
        Some("Bearer sekrit")
    );
    // custom header
    let (base2, seen2) = serve(|_| r#"{"answers":{}}"#.to_string());
    let mut cfg2 = model_cfg(&format!("{base2}/s"));
    cfg2.auth = Auth::Header {
        header: "X-API-Key".into(),
        token: "abc".into(),
    };
    let m2 = HttpSystemOneModel::new("h", &cfg2).unwrap();
    m2.decide(&req).unwrap();
    assert_eq!(seen2.lock().unwrap()[0].api_key.as_deref(), Some("abc"));
}

#[test]
fn client_error_taxonomy() {
    let req = SystemOneRequest::new("m", serde_json::json!({}));
    // 401 -> Auth
    let url = serve_status(401, "{}");
    let m = HttpSystemOneModel::new("x", &model_cfg(&url)).unwrap();
    assert!(matches!(
        m.decide(&req),
        Err(repodex::system_one::SystemOneError::Auth(_))
    ));
    // 429 -> RateLimited
    let url = serve_status(429, "{}");
    let m = HttpSystemOneModel::new("x", &model_cfg(&url)).unwrap();
    assert_eq!(
        m.decide(&req).unwrap_err(),
        repodex::system_one::SystemOneError::RateLimited
    );
    // 500 -> Server
    let url = serve_status(500, "{}");
    let m = HttpSystemOneModel::new("x", &model_cfg(&url)).unwrap();
    assert_eq!(
        m.decide(&req).unwrap_err(),
        repodex::system_one::SystemOneError::Server(500)
    );
    // malformed JSON -> InvalidJson
    let (url, _) = serve(|_| "not json".to_string());
    let m = HttpSystemOneModel::new("x", &model_cfg(&format!("{url}/s"))).unwrap();
    assert!(matches!(
        m.decide(&req),
        Err(repodex::system_one::SystemOneError::InvalidJson(_))
    ));
    // malformed typed answer -> InvalidResponse
    let (url, _) = serve(|_| r#"{"answers":{"a":{"noul":"notanumber"}}}"#.to_string());
    let m = HttpSystemOneModel::new("x", &model_cfg(&format!("{url}/s"))).unwrap();
    assert!(m.decide(&req).is_err());
}

#[test]
fn client_noul_and_score() {
    let (base, _) = serve(|_| {
        r#"{"answers":{"ok":{"noul":0.7},"s":{"score":3.0,"confidence":0.8,"legend":{},"probabilities":{}}}}"#
            .to_string()
    });
    let m = HttpSystemOneModel::new("x", &model_cfg(&format!("{base}/s"))).unwrap();
    let mut req = SystemOneRequest::new("m", serde_json::json!({}));
    req.questions.insert("ok".into(), Question::noul("yes?"));
    req.questions.insert(
        "s".into(),
        Question::score("how?", vec!["lo".into(), "hi".into()]),
    );
    let resp = m.decide(&req).unwrap();
    assert!(matches!(resp.answers["ok"], Answer::Noul(_)));
    assert!(matches!(resp.answers["s"], Answer::Score(_)));
}

// ---------------------------------------------------------------------------
// Multi-model routing (§49) + custom local URL (§50)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// End-to-end: build a tiny graph + drive SystemOne::run for each mode (§48-§50)
// ---------------------------------------------------------------------------

use repodex::candidates::build_candidates;
use repodex::graph::{build_graph, GraphIndex};
use repodex::links::build_links;
use repodex::query::{QueryEngine, QueryMode, QueryPlan};
use repodex::repository::{build_snapshot, BuildOptions};

fn engine_of(files: &[(&str, &str)]) -> (support::TempDir, GraphIndex) {
    let t = support::TempDir::new("so-graph");
    let repo = t.path().join("repo");
    for (rel, src) in files {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, src).unwrap();
    }
    let snap = t.path().join("snap");
    build_snapshot(&support::analyzer(), &repo, &snap, BuildOptions::default()).unwrap();
    let links = t.path().join("links");
    build_links(&snap, Some(&repo), &links).unwrap();
    let cand = t.path().join("cand");
    build_candidates(&snap, &links, &cand).unwrap();
    let g = t.path().join("graph");
    build_graph(&snap, &links, &cand, &g).unwrap();
    (t, GraphIndex::load(&g).unwrap())
}

fn graph_fixture() -> Vec<(&'static str, &'static str)> {
    vec![
        ("go.mod", "module example.com/m\n\ngo 1.22\n"),
        (
            "auth/token.go",
            "package auth\nfunc ValidateToken() {}\nfunc IssueToken() {}\n",
        ),
        (
            "util/cfg.go",
            "package util\nfunc Config() {}\nfunc ConfigPath() {}\n",
        ),
    ]
}

fn svc(query_model: &str, query_url: &str, rerank_model: &str, rerank_url: &str) -> SystemOne {
    let mut models = std::collections::BTreeMap::new();
    models.insert(query_model.to_string(), model_cfg(query_url));
    models.insert(rerank_model.to_string(), model_cfg(rerank_url));
    SystemOne::new(Some(SystemOneConfig {
        enabled: true,
        roles: repodex::system_one::Roles {
            query: Some(query_model.into()),
            rerank: Some(rerank_model.into()),
        },
        models,
    }))
}

#[test]
fn multi_model_routing_custom_urls() {
    // query -> local-a on /custom/systemone; rerank -> local-b on /other/path
    let (a_base, a_seen) = serve(|_| {
        r#"{"answers":{"intent":{"choice":"find","confidence":0.9,"probabilities":{}}}}"#
            .to_string()
    });
    let (b_base, b_seen) = serve(|s| {
        // answer a score for every rN question in the request
        let mut out = String::from(r#"{"answers":{"#);
        let body: serde_json::Value = serde_json::from_str(&s.body).unwrap();
        let mut first = true;
        for k in body["questions"].as_object().unwrap().keys() {
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&format!(
                r#""{k}":{{"score":5.0,"confidence":0.9,"legend":{{}},"probabilities":{{}}}}"#
            ));
        }
        out.push_str("}}");
        out
    });
    let a_url = format!("{a_base}/custom/systemone");
    let b_url = format!("{b_base}/other/path");
    let (_t, g) = engine_of(&graph_fixture());
    let engine = QueryEngine::new(&g);
    let so = svc("local-a", &a_url, "local-b", &b_url);
    let plan = QueryPlan::parse(
        "ValidateToken",
        QueryMode::Ranked,
        None,
        None,
        None,
        50,
        None,
    );
    let (res, prov) = so.run(&engine, &plan, false);
    // both roles used; query advice hit local-a, rerank hit local-b
    assert_eq!(res.so_query.as_deref(), Some("used"));
    assert_eq!(res.so_rerank.as_deref(), Some("used"));
    assert!(a_seen
        .lock()
        .unwrap()
        .iter()
        .all(|s| s.path == "/custom/systemone"));
    assert!(b_seen
        .lock()
        .unwrap()
        .iter()
        .all(|s| s.path == "/other/path"));
    assert!(!a_seen.lock().unwrap().is_empty());
    assert!(!b_seen.lock().unwrap().is_empty());
    let _ = prov;
}

#[test]
fn rerank_reorders_seeds_only_and_preserves_set() {
    // rerank model returns a fixed high score for the LAST seed -> it moves first.
    let (base, _) = serve(|s| {
        let body: serde_json::Value = serde_json::from_str(&s.body).unwrap();
        let n = body["questions"].as_object().unwrap().len();
        let mut out = String::from(r#"{"answers":{"#);
        for i in 0..n {
            if i > 0 {
                out.push(',');
            }
            // give the last question the top score
            let sc = if i == n - 1 { 9.0 } else { 1.0 };
            out.push_str(&format!(
                r#""r{i}":{{"score":{sc},"confidence":0.9,"legend":{{}},"probabilities":{{}}}}"#
            ));
        }
        out.push_str("}}");
        out
    });
    let url = format!("{base}/s");
    let (_t, g) = engine_of(&graph_fixture());
    let engine = QueryEngine::new(&g);
    // baseline deterministic order
    let base_plan = QueryPlan::parse("Token", QueryMode::Ranked, None, None, None, 50, None);
    let baseline = engine.run(&base_plan);
    // rerank-only service (no query role)
    let so = {
        let mut models = std::collections::BTreeMap::new();
        models.insert("r".to_string(), model_cfg(&url));
        SystemOne::new(Some(SystemOneConfig {
            enabled: true,
            roles: repodex::system_one::Roles {
                query: None,
                rerank: Some("r".into()),
            },
            models,
        }))
    };
    let (res, _) = so.run(&engine, &base_plan, false);
    // same SET of result identities, only order changed; last baseline is first
    let bk: std::collections::BTreeSet<_> =
        baseline.seeds.iter().map(|s| s.node.key.clone()).collect();
    let rk: std::collections::BTreeSet<_> = res.seeds.iter().map(|s| s.node.key.clone()).collect();
    assert_eq!(bk, rk, "rerank must not add/remove results");
    assert_eq!(res.so_rerank.as_deref(), Some("used"));
}

#[test]
fn forced_failure_falls_back_to_deterministic_baseline() {
    // a dead endpoint (port that refuses connections) -> every call fails.
    let (_t, g) = engine_of(&graph_fixture());
    let engine = QueryEngine::new(&g);
    let plan = QueryPlan::parse("Token", QueryMode::Ranked, None, None, None, 50, None);
    let baseline = engine.run(&plan);
    // configure both roles against an endpoint that drops every connection
    let dead = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for s in listener.incoming() {
                drop(s); // accept then immediately reset
            }
        });
        format!("http://127.0.0.1:{port}/dead")
    };
    let so = svc("q", &dead, "r", &dead);
    let (res, prov) = so.run(&engine, &plan, false);
    // identical result identities + order as the deterministic baseline
    let bk: Vec<_> = baseline.seeds.iter().map(|s| s.node.key.clone()).collect();
    let rk: Vec<_> = res.seeds.iter().map(|s| s.node.key.clone()).collect();
    assert_eq!(bk, rk, "fallback must equal deterministic baseline");
    assert_eq!(res.total, baseline.total);
    assert_eq!(res.complete, baseline.complete);
    // provenance marks both roles fallback
    assert_eq!(prov.query, repodex::system_one::RoleStatus::Fallback);
    assert_eq!(prov.rerank, repodex::system_one::RoleStatus::Fallback);
    assert_eq!(res.so_query.as_deref(), Some("fallback"));
    assert_eq!(res.so_rerank.as_deref(), Some("fallback"));
}

#[test]
fn disabled_produces_no_provenance() {
    let (_t, g) = engine_of(&graph_fixture());
    let engine = QueryEngine::new(&g);
    let plan = QueryPlan::parse("Token", QueryMode::Ranked, None, None, None, 50, None);
    let so = SystemOne::disabled();
    let (res, prov) = so.run(&engine, &plan, false);
    assert_eq!(prov.query, repodex::system_one::RoleStatus::Disabled);
    assert_eq!(prov.rerank, repodex::system_one::RoleStatus::Disabled);
    assert!(res.so_query.is_none());
    assert!(res.so_rerank.is_none());
    // RDX1 emits no so_* keys when disabled
    let rdx = repodex::rdx1::render(&res);
    assert!(!rdx.contains("so_query"));
    assert!(!rdx.contains("so_rerank"));
}

#[test]
fn explicit_intent_never_overridden_by_query_role() {
    // even if the query model says "callers", an explicit --intent find wins
    let (base, _) = serve(|_| {
        r#"{"answers":{"intent":{"choice":"callers","confidence":0.99,"probabilities":{}}}}"#
            .to_string()
    });
    let url = format!("{base}/s");
    let (_t, g) = engine_of(&graph_fixture());
    let engine = QueryEngine::new(&g);
    let so = {
        let mut models = std::collections::BTreeMap::new();
        models.insert("q".to_string(), model_cfg(&url));
        SystemOne::new(Some(SystemOneConfig {
            enabled: true,
            roles: repodex::system_one::Roles {
                query: Some("q".into()),
                rerank: None,
            },
            models,
        }))
    };
    let plan = QueryPlan::parse(
        "Token",
        QueryMode::Ranked,
        Some(repodex::query::QueryIntent::Find),
        None,
        None,
        50,
        None,
    );
    let (res, _) = so.run(&engine, &plan, true); // explicit_intent = true
                                                 // intent stays find (explicit control wins), query advice never asked
    assert_eq!(res.plan.intent.as_str(), "find");
    assert!(res.so_query.is_none()); // role not exercised
}
