//! Service lifecycle/auth/ingest/recovery integration tests (§80).
//! Uses the real binary + an isolated REPODEX_STATE_DIR + real loopback HTTP.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_reposuite-repodex"))
}
fn state(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join("repodex-svc-test").join(name);
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}
fn run(state: &Path, args: &[&str]) -> (bool, String, String) {
    let o = Command::new(bin())
        .args(args)
        .env("REPODEX_STATE_DIR", state)
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).to_string(),
        String::from_utf8_lossy(&o.stderr).to_string(),
    )
}
fn runtime(state: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(state.join("service.runtime.json")).unwrap()).unwrap()
}
fn token(state: &Path) -> String {
    fs::read_to_string(state.join("service.token"))
        .unwrap()
        .trim()
        .to_string()
}
fn get(state: &Path, path: &str, tok: Option<&str>) -> (u16, serde_json::Value) {
    let d = runtime(state);
    repodex::service::http::get_json(
        d["host"].as_str().unwrap(),
        d["port"].as_u64().unwrap() as u16,
        path,
        tok,
        5000,
    )
    .unwrap_or((0, serde_json::Value::Null))
}
fn post(
    state: &Path,
    path: &str,
    tok: Option<&str>,
    body: &serde_json::Value,
) -> (u16, serde_json::Value) {
    let d = runtime(state);
    repodex::service::http::post_json(
        d["host"].as_str().unwrap(),
        d["port"].as_u64().unwrap() as u16,
        path,
        tok,
        body,
        30000,
    )
    .unwrap_or((0, serde_json::Value::Null))
}
fn event(sess: &str, i: u64) -> serde_json::Value {
    serde_json::json!({"schema":"reposuite.agent-event.v1","event_id":format!("{sess}:{i}"),"session_id":sess,
        "sequence":i,"timestamp":"2026-09-23T12:00:00Z","source":{"runtime":"t","adapter":"b","adapter_version":"1"},
        "type":"source_observed","data":{"path":"src/a.rs","bytes":10,"observation_kind":"read"}})
}

#[test]
fn service_start_status_stop_lifecycle() {
    let s = state("lifecycle");
    // stopped status
    let (_ok, out, _) = run(&s, &["status"]);
    assert!(out.contains("STOPPED"), "stopped status: {out}");
    // start -> running descriptor
    let (ok, out, _) = run(&s, &["start"]);
    assert!(
        ok && out.contains("started: http://127.0.0.1:"),
        "start: {out}"
    );
    let d = runtime(&s);
    assert_eq!(d["host"], "127.0.0.1");
    assert!(d["port"].as_u64().unwrap() > 0, "dynamic port assigned");
    assert!(d["pid"].as_u64().unwrap() > 0);
    assert_eq!(d["schema"], "reposuite.repodex.service.runtime.v1");
    assert!(d.get("token").is_none(), "token never in descriptor");
    // idempotent start
    let (ok, out, _) = run(&s, &["start"]);
    assert!(
        ok && out.contains("already running"),
        "idempotent start: {out}"
    );
    // running status probes live metrics
    let (ok, out, _) = run(&s, &["status"]);
    assert!(ok && out.contains("RUNNING"), "running status: {out}");
    // stop
    let (ok, out, _) = run(&s, &["stop"]);
    assert!(ok && out.contains("stopped"), "stop: {out}");
    assert!(
        !s.join("service.runtime.json").exists(),
        "descriptor removed"
    );
    // stopped again
    let (_, out, _) = run(&s, &["status"]);
    assert!(out.contains("STOPPED"));
}

#[test]
fn service_auth_required() {
    let s = state("auth");
    run(&s, &["start"]);
    // no token -> 401
    let (st, _) = get(&s, "/v1/status", None);
    assert_eq!(st, 401, "missing token rejected");
    // wrong token -> 401
    let (st, _) = get(&s, "/v1/status", Some("deadbeef"));
    assert_eq!(st, 401, "wrong token rejected");
    // right token -> 200
    let (st, body) = get(&s, "/v1/status", Some(&token(&s)));
    assert_eq!(st, 200);
    assert_eq!(body["schema"], "reposuite.repodex.service.status.v1");
    run(&s, &["stop"]);
}

#[test]
fn service_ingest_durable_batched() {
    let s = state("ingest");
    run(&s, &["start"]);
    let tok = token(&s);
    // batch of 200 events -> one group-commit
    let evs: Vec<_> = (0..200).map(|i| event("s1", i)).collect();
    let (st, r) = post(&s, "/v1/events/batch", Some(&tok), &serde_json::json!(evs));
    assert_eq!(st, 200);
    assert_eq!(r["accepted"], 200);
    // dedup: resend -> duplicates
    let (_st, r) = post(&s, "/v1/events/batch", Some(&tok), &serde_json::json!(evs));
    assert_eq!(r["duplicates"], 200);
    // fsync count is FAR less than per-event (group commit)
    let (_, st_body) = get(&s, "/v1/status", Some(&tok));
    let fsyncs = st_body["storage"]["fsyncs"].as_u64().unwrap();
    assert!(
        fsyncs < 50,
        "group-commit fsyncs ({fsyncs}) << events (200)"
    );
    assert!(s.join("events/segments").exists());
    assert!(s.join("events/cursor.json").exists());
    run(&s, &["stop"]);
}

#[test]
fn service_crash_recovery_replays_committed() {
    let s = state("crash");
    run(&s, &["start"]);
    let tok = token(&s);
    let evs: Vec<_> = (0..100).map(|i| event("c1", i)).collect();
    post(&s, "/v1/events/batch", Some(&tok), &serde_json::json!(evs));
    let committed = runtime(&s)["pid"].as_u64().unwrap();
    // kill -9 (abrupt) — not graceful stop
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-9", &committed.to_string()])
            .output();
    }
    std::thread::sleep(Duration::from_millis(300));
    // restart recovers committed events
    let (ok, _, _) = run(&s, &["start"]);
    assert!(ok);
    let (_, st_body) = get(&s, "/v1/status", Some(&token(&s)));
    assert_eq!(
        st_body["events"]["persisted"].as_u64().unwrap(),
        100,
        "committed events recovered after kill -9"
    );
    run(&s, &["stop"]);
}

#[test]
fn service_restart_and_stale_descriptor() {
    let s = state("restart");
    run(&s, &["start"]);
    let pid1 = runtime(&s)["pid"].as_u64().unwrap();
    // restart -> new pid
    let (ok, _, _) = run(&s, &["restart"]);
    assert!(ok);
    let pid2 = runtime(&s)["pid"].as_u64().unwrap();
    assert_ne!(pid1, pid2, "restart produced new instance");
    run(&s, &["stop"]);
}

#[test]
fn service_fixed_port_collision_fails() {
    let s = state("portcoll");
    // occupy a port, then configure service to require it
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    fs::write(
        s.join("service.toml"),
        format!("port = {port}\nhost = \"127.0.0.1\"\n"),
    )
    .unwrap();
    // foreground serve exits quickly with a bind error (child would block if ok)
    let mut child = Command::new(bin())
        .arg("serve")
        .env("REPODEX_STATE_DIR", &s)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait().unwrap() {
            Some(st) => {
                assert!(!st.success(), "serve must fail on port collision");
                break;
            }
            None => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    panic!("serve should have failed fast on port collision");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    drop(l);
}
