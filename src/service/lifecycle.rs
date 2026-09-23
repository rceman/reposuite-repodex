//! Service lifecycle — serve/start/stop/restart/status (§1-§5, §12, §22-§23).
//!
//! `start` reuses `serve`: it spawns a detached `repodex serve` then waits for
//! the runtime descriptor + a live PID (§3). There is exactly one service
//! implementation — no daemon installer, no OS service manager (§2).

use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::config::ServiceConfig;
use super::derived::{self, DerivedHot};
use super::http::{self, HttpServer};
use super::runtime::{self, RuntimeDescriptor};
use super::segments::DurableEventLog;
use super::state::ServiceState;
use super::token;

/// Held while a service instance runs; released on stop (§5).
struct InstanceLock {
    path: PathBuf,
}
impl Drop for InstanceLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Acquire the single-instance lock (`service.lock`, §5). Errors if another
/// live instance holds it; clears a stale lock left by a crash.
fn acquire_instance_lock(state: &Path) -> Result<InstanceLock, String> {
    let p = ServiceConfig::lock_path(state);
    fs::create_dir_all(state).map_err(|e| e.to_string())?;
    match fs::OpenOptions::new().create_new(true).write(true).open(&p) {
        Ok(mut f) => {
            use std::io::Write;
            let _ = writeln!(f, "{}", std::process::id());
            Ok(InstanceLock { path: p })
        }
        Err(_) => {
            // Lock exists — is the recorded PID still alive?
            let stale = fs::read_to_string(&p)
                .ok()
                .and_then(|t| t.trim().parse::<u32>().ok())
                .map(|pid| !runtime::pid_alive(pid))
                .unwrap_or(true);
            if stale {
                let _ = fs::remove_file(&p);
                return acquire_instance_lock(state);
            }
            Err("another RepoDex service instance is already running".to_string())
        }
    }
}

/// `repodex serve` — run the service in the foreground (§1).
pub fn serve(state_dir: &Path) -> Result<u8, String> {
    let config = ServiceConfig::load(state_dir)?;
    let token = token::load_or_generate(state_dir)?;
    let _instance = acquire_instance_lock(state_dir)?;

    // Durable event log + recovery: replay committed events to rebuild the
    // dedup index and the HOT derived state (§53, §77).
    let events = Arc::new(DurableEventLog::open(state_dir, &config.ingest)?);
    let committed = events.replay_committed().unwrap_or_default();
    let state = Arc::new(ServiceState::new(
        state_dir.to_path_buf(),
        config.clone(),
        token,
        events.clone(),
    ));
    let hot = Arc::new(DerivedHot::default());
    {
        // Rebuild dedup + derived from committed history (deterministic replay).
        let mut d = state.dedup.lock().unwrap();
        let mut idx = hot.activity.lock().unwrap();
        let mut sessions = hot.sessions.lock().unwrap();
        let mut applied = 0u64;
        for e in &committed {
            d.insert(
                e.event_id.clone(),
                super::segments::event_semantic_digest(e),
            );
            idx.add_event(e, e.investigation_id.as_deref());
            if !sessions.iter().any(|s| s == &e.session_id) {
                sessions.push(e.session_id.clone());
            }
            applied += 1;
        }
        hot.applied_events.store(applied, Ordering::SeqCst);
        state
            .metrics
            .events_persisted
            .store(applied, Ordering::SeqCst);
    }

    // Async derived worker (§45).
    let dtx = derived::spawn(state.clone(), hot.clone());
    *state.derived_tx.lock().unwrap() = Some(dtx);

    // Durable-batch flusher (§46).
    {
        let ev = events.clone();
        std::thread::spawn(move || ev.flusher_loop());
    }

    // Bind loopback + discover the actual port (§8-§10).
    let server = HttpServer::bind(&config.host, config.port)?;
    let (host, port) = server.local_addr()?;
    let instance_id = format!("repodex-{}", std::process::id());
    runtime::publish(
        state_dir,
        &RuntimeDescriptor::new(&host, port, &instance_id),
    )?;
    eprintln!(
        "repodex serve: http://{host}:{port} (pid {})",
        std::process::id()
    );

    // Route + serve until shutdown (stop flag set by /v1/shutdown).
    let stop = server.stop_handle();
    {
        let st = state.clone();
        std::thread::spawn(move || loop {
            if st.shutdown.load(Ordering::SeqCst) {
                stop.store(true, Ordering::SeqCst);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        });
    }
    server.run(super::api::handler(state.clone(), hot.clone()), 16);

    // Graceful shutdown: final durable flush + derived checkpoint + cleanup.
    events.shutdown();
    let _ = events.flush();
    derived::checkpoint(state_dir, &hot).ok();
    runtime::remove(state_dir);
    Ok(0)
}

/// `repodex start` — launch `serve` detached, wait for readiness, return (§3).
pub fn start(state_dir: &Path) -> Result<u8, String> {
    // Already running? idempotent no-op success (§5).
    if let Some(d) = runtime::live(state_dir) {
        println!(
            "already running: http://{}:{} (pid {})",
            d.host, d.port, d.pid
        );
        return Ok(0);
    }
    let _ = runtime::read(state_dir).map(|_| runtime::remove(state_dir)); // stale
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("serve")
        .env("REPODEX_STATE_DIR", state_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    spawn_detached(&mut cmd)?;
    // Wait for the runtime descriptor + live PID (§11-§12).
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(d) = runtime::live(state_dir) {
            println!("started: http://{}:{} (pid {})", d.host, d.port, d.pid);
            return Ok(0);
        }
        if Instant::now() > deadline {
            return Err("service did not become ready within 15s".to_string());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(unix)]
fn spawn_detached(cmd: &mut Command) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    // New session + detached: survives the parent shell.
    unsafe {
        cmd.pre_exec(|| {
            libc_setsid();
            Ok(())
        });
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(unix)]
unsafe fn libc_setsid() {
    extern "C" {
        fn setsid() -> i32;
    }
    setsid();
}
#[cfg(not(unix))]
fn spawn_detached(cmd: &mut Command) -> Result<(), String> {
    // Windows: CREATE_DETACHED + breakaway would be ideal; best-effort spawn —
    // the child is not a console service so it outlives the parent shell for V1.
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// `repodex stop` — graceful authenticated shutdown (§1, §15).
pub fn stop(state_dir: &Path) -> Result<u8, String> {
    let d = match runtime::live(state_dir) {
        Some(d) => d,
        None => {
            runtime::remove(state_dir);
            println!("stopped (no live service)");
            return Ok(0);
        }
    };
    let token = token::load_or_generate(state_dir)?;
    let (st, _body) = http::post_json(
        &d.host,
        d.port,
        "/v1/shutdown",
        Some(&token),
        &json!({}),
        5000,
    )
    .map_err(|e| format!("shutdown request failed: {e}"))?;
    if st != 200 {
        return Err(format!("shutdown rejected (status {st})"));
    }
    // Wait for the process to exit + descriptor removed.
    let deadline = Instant::now() + Duration::from_secs(10);
    while runtime::pid_alive(d.pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    runtime::remove(state_dir);
    println!("stopped pid {}", d.pid);
    Ok(0)
}

/// `repodex restart` — stop + start (§1).
pub fn restart(state_dir: &Path) -> Result<u8, String> {
    let _ = stop(state_dir);
    start(state_dir)
}

/// `repodex status [--json]` — authoritative status (§22-§23).
pub fn status(state_dir: &Path, json_out: bool) -> Result<u8, String> {
    let d = runtime::read(state_dir);
    let live = runtime::live(state_dir);
    match (d, live) {
        (Some(desc), Some(_)) => {
            // Probe the authenticated /v1/status for live metrics.
            let token = token::load_or_generate(state_dir)?;
            let metrics = http::get_json(&desc.host, desc.port, "/v1/status", Some(&token), 3000)
                .ok()
                .map(|(_, v)| v);
            if json_out {
                let mut o = json!({
                    "schema":"reposuite.repodex.service-status.v1",
                    "running":true,"pid":desc.pid,"endpoint":format!("http://{}:{}",desc.host,desc.port),
                    "instance_id":desc.instance_id,"started_at":desc.started_at,
                    "version":desc.repodex_version,
                });
                if let Some(m) = metrics {
                    o["live"] = m;
                }
                println!("{}", serde_json::to_string_pretty(&o).unwrap());
            } else {
                println!("RepoDex service: RUNNING");
                println!("  pid:      {}", desc.pid);
                println!("  endpoint: http://{}:{}", desc.host, desc.port);
                println!("  instance: {}", desc.instance_id);
                println!("  started:  {}", desc.started_at);
                println!("  version:  {}", desc.repodex_version);
                if let Some(m) = metrics {
                    let up = m["uptime_s"].as_f64().unwrap_or(0.0);
                    let ev = m["events"]["persisted"].as_u64().unwrap_or(0);
                    let q = m["queries"]["count"].as_u64().unwrap_or(0);
                    let mem = m["memory"]["current_mb"].as_f64().unwrap_or(0.0);
                    let budget = m["memory"]["budget_mb"].as_u64().unwrap_or(0);
                    let views = m["active_views"].as_array().map(|a| a.len()).unwrap_or(0);
                    let lag = m["derived"]["lag_events"].as_u64().unwrap_or(0);
                    println!("  uptime:   {:.1}s", up);
                    println!("  memory:   {:.0} MiB used / {} MiB budget", mem, budget);
                    println!("  events:   {} persisted (derived lag {})", ev, lag);
                    println!("  queries:  {}", q);
                    println!("  views:    {}", views);
                }
            }
            Ok(0)
        }
        (Some(desc), None) => {
            if json_out {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "schema":"reposuite.repodex.service-status.v1",
                        "running":false,"stale_descriptor":true,"pid":desc.pid,
                    }))
                    .unwrap()
                );
            } else {
                println!(
                    "RepoDex service: STOPPED (stale runtime descriptor, pid {} dead)",
                    desc.pid
                );
            }
            Ok(1)
        }
        (None, _) => {
            if json_out {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "schema":"reposuite.repodex.service-status.v1","running":false,
                    }))
                    .unwrap()
                );
            } else {
                println!("RepoDex service: STOPPED");
            }
            Ok(0)
        }
    }
}
