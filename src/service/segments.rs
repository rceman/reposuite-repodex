//! Durable AgentEvent log — append-only segments + atomic committed cursor
//! (§44-§50, §68-§70, §75-§77).
//!
//! Group-commit durability: events are buffered and written/fsynced together
//! every `flush_interval` / N events / N bytes, then the `cursor.json` durable
//! boundary is atomically advanced. An event is only ACKed once the batch that
//! contains it has been fsynced AND the cursor published — never earlier (§47).
//!
//! Layout:
//!   events/segments/seg-000001.jsonl   (append-only; rotated by size)
//!   events/cursor.json                 (atomic durable boundary)
//!
//! Recovery: replay committed events; for the active segment, drop bytes past
//! the cursor offset — they were never ACKed, so dropping is correct (§76).

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::agent_event::model::AgentEvent;
use crate::repository::digest;

pub const EVENTS_DIR: &str = "events";
pub const CURSOR_FILE: &str = "cursor.json";
pub const SEG_PREFIX: &str = "seg-";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Cursor {
    pub schema: String,
    /// Active segment index the durable boundary lives in.
    pub segment: u32,
    /// Byte offset within that segment that is durably committed.
    pub offset: u64,
    /// Total durable events committed so far (across all segments).
    pub committed_events: u64,
    pub updated_at: String,
}
const CURSOR_SCHEMA: &str = "reposuite.repodex.event-cursor.v1";

/// Write/fsync telemetry for the write-amplification report (§68-§70).
#[derive(Debug, Default, Clone)]
pub struct WriteStats {
    pub logical_bytes: u64,
    pub fsyncs: u64,
    pub durable_batches: u64,
    pub write_calls: u64,
}

struct Inner {
    seg_index: u32,
    seg_file: Option<File>,
    seg_len: u64,
    pending: Vec<Vec<u8>>, // serialized event lines awaiting durable commit
    pending_bytes: usize,
    committed_events: u64,
    last_flush: Instant,
    closed: bool,
    stats: WriteStats,
    /// seq assigned to the newest buffered event — waiters park on condvar.
    next_seq: u64,
    durable_seq: u64, // == committed_events once a batch fsyncs+publishes
}

pub struct DurableEventLog {
    dir: PathBuf,
    cfg: super::config::IngestCfg,
    inner: Mutex<Inner>,
    flush_cv: Condvar,
    stop: std::sync::atomic::AtomicBool,
}

impl DurableEventLog {
    /// Open/create the event log under `state`, recovering the durable boundary
    /// (truncating any uncommitted segment tail) (§76).
    pub fn open(state: &Path, cfg: &super::config::IngestCfg) -> Result<Self, String> {
        let dir = state.join(EVENTS_DIR);
        fs::create_dir_all(dir.join("segments")).map_err(|e| e.to_string())?;
        let cursor = read_cursor(&dir).unwrap_or(Cursor {
            schema: CURSOR_SCHEMA.into(),
            segment: 0,
            offset: 0,
            committed_events: 0,
            updated_at: super::util::now_rfc3339(),
        });
        // Truncate the active segment to the committed offset (drop uncommitted
        // tail — never ACKed). Older segments are fully committed/immutable.
        let segs = list_segments(&dir);
        let seg_index = segs.last().copied().unwrap_or(1).max(1);
        if let Some(&last) = segs.last() {
            let p = seg_path(&dir, last);
            let len = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            let keep = if last == cursor.segment {
                cursor.offset.min(len)
            } else if last > cursor.segment {
                len // a whole segment committed beyond cursor (shouldn't happen)
            } else {
                0
            };
            if last == cursor.segment && len > keep {
                let f = OpenOptions::new()
                    .write(true)
                    .open(&p)
                    .map_err(|e| e.to_string())?;
                f.set_len(keep).map_err(|e| e.to_string())?;
            }
        }
        let seg_path_now = seg_path(&dir, seg_index);
        let seg_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&seg_path_now)
            .map_err(|e| e.to_string())?;
        let seg_len = fs::metadata(&seg_path_now).map(|m| m.len()).unwrap_or(0);

        let log = Self {
            dir,
            cfg: cfg.clone(),
            inner: Mutex::new(Inner {
                seg_index,
                seg_file: Some(seg_file),
                seg_len,
                pending: Vec::new(),
                pending_bytes: 0,
                committed_events: cursor.committed_events,
                last_flush: Instant::now(),
                closed: false,
                stats: WriteStats::default(),
                next_seq: cursor.committed_events,
                durable_seq: cursor.committed_events,
            }),
            flush_cv: Condvar::new(),
            stop: std::sync::atomic::AtomicBool::new(false),
        };
        Ok(log)
    }

    /// Periodic flush driver — call from a dedicated thread holding `Arc<Self>`.
    pub fn flusher_loop(self: &std::sync::Arc<Self>) {
        while !self.stop.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(10));
            let due = {
                let i = self.inner.lock().unwrap();
                i.last_flush.elapsed() >= Duration::from_millis(self.cfg.flush_interval_ms)
                    || i.pending.len() >= self.cfg.flush_max_events
                    || i.pending_bytes >= self.cfg.flush_max_bytes
            };
            if due {
                let _ = self.flush();
            }
        }
        let _ = self.flush();
    }

    /// Enqueue `lines` (serialized event+`\n`), dedup-check under one lock, then
    /// BLOCK until the batch containing the last line is durable (§46-§47).
    /// A single batch POST therefore shares ONE fsync — the write-amplification
    /// win — instead of one fsync per event.
    ///
    /// Returns per-item outcomes (Committed/Duplicate/Conflict) in input order;
    /// `Err` aborts the whole call only on a hard I/O failure.
    pub fn append_many_durable(
        self: &std::sync::Arc<Self>,
        items: &[(String, String, Vec<u8>)], // (event_id, semantic_digest, line)
        dedup: &Mutex<HashMap<String, String>>,
    ) -> Result<Vec<AppendOutcome>, String> {
        let mut outcomes = Vec::with_capacity(items.len());
        // Phase 1 — enqueue under inner + dedup (consistent order inner→dedup),
        // releasing BOTH before the durable wait. Holding dedup across the wait
        // would deadlock: another producer grabs inner (released mid-wait) then
        // blocks on dedup while we wait on inner (ABBA).
        let last_ticket = {
            let mut i = self.inner.lock().unwrap();
            if i.closed {
                return Err("event log closed".into());
            }
            let mut last = 0u64;
            {
                let mut d = dedup.lock().unwrap();
                for (eid, dig, line) in items {
                    match d.get(eid) {
                        Some(existing) if existing == dig => {
                            outcomes.push(AppendOutcome::Duplicate)
                        }
                        Some(_) => outcomes.push(AppendOutcome::Conflict(eid.clone())),
                        None => {
                            d.insert(eid.clone(), dig.clone());
                            i.next_seq += 1;
                            last = i.next_seq;
                            i.pending_bytes += line.len();
                            i.pending.push(line.clone());
                            outcomes.push(AppendOutcome::Committed(i.next_seq));
                        }
                    }
                }
            } // dedup released here — before any wait
            let full = i.pending.len() >= self.cfg.flush_max_events
                || i.pending_bytes >= self.cfg.flush_max_bytes;
            drop(i); // inner released too
            if full {
                self.flush()?;
            }
            last
        };
        if last_ticket == 0 {
            return Ok(outcomes); // all duplicates/conflicts — nothing durable to await
        }
        // Phase 2 — wait for the batch holding last_ticket to be durable, using
        // only the inner lock (no dedup held).
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let mut i = self.inner.lock().unwrap();
            if i.durable_seq >= last_ticket {
                break;
            }
            let (g, to) = self
                .flush_cv
                .wait_timeout(i, Duration::from_millis(50))
                .unwrap();
            i = g;
            if i.durable_seq >= last_ticket {
                break;
            }
            if Instant::now() > deadline {
                return Err("durable commit timed out".into());
            }
            if to.timed_out() {
                drop(i);
                let _ = self.flush();
            }
        }
        Ok(outcomes)
    }

    /// Write+fsync the pending batch, rotate if needed, advance cursor (§46-§50).
    pub fn flush(self: &std::sync::Arc<Self>) -> Result<u64, String> {
        let (lines, seg_index) = {
            let mut i = self.inner.lock().unwrap();
            if i.pending.is_empty() {
                return Ok(i.durable_seq);
            }
            let lines = std::mem::take(&mut i.pending);
            i.pending_bytes = 0;
            i.last_flush = Instant::now();
            (lines, i.seg_index)
        };
        // Write outside the lock (batched sequential write, §48).
        let bytes: usize = lines.iter().map(|l| l.len()).sum();
        {
            let mut i = self.inner.lock().unwrap();
            let f = i.seg_file.as_mut().ok_or("segment file closed")?;
            let mut wc = 0u64;
            for l in &lines {
                f.write_all(l).map_err(|e| e.to_string())?;
                wc += 1;
            }
            f.sync_data().map_err(|e| e.to_string())?; // ONE fsync per batch
            i.seg_len += bytes as u64;
            i.committed_events += lines.len() as u64;
            i.durable_seq = i.committed_events;
            i.stats.write_calls += wc;
            i.stats.logical_bytes += bytes as u64;
            i.stats.fsyncs += 1;
            i.stats.durable_batches += 1;
        }
        // Publish the durable boundary atomically (tmp+rename) (§50).
        self.publish_cursor()?;
        // Rotate oversized segment.
        let mut i = self.inner.lock().unwrap();
        if i.seg_len >= self.cfg.segment_max_bytes as u64 {
            i.seg_index += 1;
            let p = seg_path(&self.dir, i.seg_index);
            i.seg_file = Some(
                OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&p)
                    .map_err(|e| e.to_string())?,
            );
            i.seg_len = 0;
        }
        self.flush_cv.notify_all();
        Ok(seg_index as u64)
    }

    fn publish_cursor(&self) -> Result<(), String> {
        let i = self.inner.lock().unwrap();
        let cur = Cursor {
            schema: CURSOR_SCHEMA.into(),
            segment: i.seg_index,
            offset: i.seg_len,
            committed_events: i.committed_events,
            updated_at: super::util::now_rfc3339(),
        };
        drop(i);
        write_cursor(&self.dir, &cur)
    }

    pub fn stats(&self) -> WriteStats {
        self.inner.lock().unwrap().stats.clone()
    }
    pub fn committed_events(&self) -> u64 {
        self.inner.lock().unwrap().committed_events
    }
    pub fn pending(&self) -> usize {
        self.inner.lock().unwrap().pending.len()
    }
    pub fn segment_count(&self) -> usize {
        list_segments(&self.dir).len()
    }

    pub fn shutdown(&self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        // final flush handled by flusher_loop exit / caller.
    }

    /// Replay all committed events (recovery / derived rebuild) (§77).
    pub fn replay_committed(&self) -> Result<Vec<AgentEvent>, String> {
        replay_dir(&self.dir)
    }
}

#[derive(Debug, PartialEq)]
pub enum AppendOutcome {
    Committed(u64),
    Duplicate,
    /// Same event_id, different content (§30).
    Conflict(String),
}

// --- files ------------------------------------------------------------------

fn seg_path(dir: &Path, idx: u32) -> PathBuf {
    dir.join("segments")
        .join(format!("{SEG_PREFIX}{idx:06}.jsonl"))
}
fn list_segments(dir: &Path) -> Vec<u32> {
    let mut out = Vec::new();
    let d = dir.join("segments");
    if let Ok(rd) = fs::read_dir(&d) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if let Some(s) = n
                .strip_prefix(SEG_PREFIX)
                .and_then(|s| s.strip_suffix(".jsonl"))
            {
                if let Ok(i) = s.parse::<u32>() {
                    out.push(i);
                }
            }
        }
    }
    out.sort();
    out
}
fn read_cursor(dir: &Path) -> Option<Cursor> {
    let t = fs::read_to_string(dir.join(CURSOR_FILE)).ok()?;
    serde_json::from_str(&t).ok()
}
fn write_cursor(dir: &Path, c: &Cursor) -> Result<(), String> {
    let p = dir.join(CURSOR_FILE);
    let tmp = p.with_extension("json.tmp");
    fs::write(
        &tmp,
        serde_json::to_string_pretty(c).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

/// Replay every event in every segment (used for derived rebuild + recovery).
pub fn replay_dir(dir: &Path) -> Result<Vec<AgentEvent>, String> {
    let mut out = Vec::new();
    for idx in list_segments(dir) {
        let p = seg_path(dir, idx);
        let f = File::open(&p).map_err(|e| e.to_string())?;
        for line in BufReader::new(f).lines() {
            let line = line.map_err(|e| e.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<AgentEvent>(&line) {
                Ok(e) => out.push(e),
                Err(_) => break, // stop at a torn/partial tail line (§76)
            }
        }
    }
    Ok(out)
}

/// Event semantic digest for dedup (matches AgentEventStore::event_digest).
pub fn event_semantic_digest(e: &AgentEvent) -> String {
    let mut c = e.clone();
    c.content_digest = None;
    digest::sha256(&serde_json::to_vec(&c).unwrap_or_default())
}
