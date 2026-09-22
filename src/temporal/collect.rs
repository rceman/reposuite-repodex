//! Streaming Git-history collector.
//!
//! One bounded `git log` subprocess emits commit/path events; stdout is parsed
//! line-by-line so memory stays O(tracked files), never O(commits x files).
//! No per-file `git log` is ever spawned (§5/§6).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

use super::model::{FileTemporal, CHANGE_TAIL_CAP};
use super::TemporalError;

/// The single streaming history command.
///
/// `git log <range> --no-renames --format=... --name-status`
///   RS(\x1e) starts a commit record: `sha US(\x1f) committer_ts US parents`.
///   Then `--name-status` lines: `A|C|D|M|T\tpath` (rename disabled).
///   Merge commits emit a header with >=2 parents and no file lines.
fn log_args(repo: &Path, range: Option<&str>) -> Vec<String> {
    let mut a = vec![
        "-C".to_string(),
        repo.to_string_lossy().to_string(),
        "log".to_string(),
    ];
    if let Some(r) = range {
        a.push(r.to_string());
    }
    a.push("--no-renames".to_string());
    a.push("--format=format:%x1e%H%x1f%ct%x1f%P".to_string());
    a.push("--name-status".to_string());
    a
}

/// A running `git log` we read incrementally.
struct LogProc {
    child: Child,
}

impl LogProc {
    fn spawn(repo: &Path, range: Option<&str>) -> Result<Self, TemporalError> {
        let args = log_args(repo, range);
        let child = Command::new("git")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| TemporalError::Git {
                reason: format!("spawn git log: {e}"),
            })?;
        Ok(LogProc { child })
    }

    fn wait(self) -> Result<(), TemporalError> {
        let out = self
            .child
            .wait_with_output()
            .map_err(|e| TemporalError::Git {
                reason: format!("wait git log: {e}"),
            })?;
        if !out.status.success() {
            return Err(TemporalError::Git {
                reason: format!(
                    "git log exited {}: {}",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            });
        }
        Ok(())
    }
}

/// Run one git command and return trimmed stdout.
fn git_out(repo: &Path, args: &[&str]) -> Result<String, TemporalError> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| TemporalError::Git {
            reason: format!("spawn git {}: {e}", args.join(" ")),
        })?;
    if !out.status.success() {
        return Err(TemporalError::Git {
            reason: format!(
                "git {} exited {}: {}",
                args.join(" "),
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `git rev-parse HEAD`.
pub fn head_commit(repo: &Path) -> Result<String, TemporalError> {
    git_out(repo, &["rev-parse", "HEAD"])
}

/// Committer unix seconds of a commit.
pub fn commit_ts(repo: &Path, rev: &str) -> Result<i64, TemporalError> {
    git_out(repo, &["show", "-s", "--format=%ct", rev])?
        .parse()
        .map_err(|_| TemporalError::Git {
            reason: format!("unparseable committer ts for {rev}"),
        })
}

/// Root commit of the current history (first parentless ancestor).
pub fn root_commit(repo: &Path) -> Result<String, TemporalError> {
    let out = git_out(repo, &["rev-list", "--max-parents=0", "HEAD"])?;
    out.lines()
        .next()
        .map(|s| s.to_string())
        .ok_or_else(|| TemporalError::Git {
            reason: "no root commit".to_string(),
        })
}

/// Remote origin url if configured ("" when absent — offline repos are fine).
pub fn remote_url(repo: &Path) -> String {
    git_out(repo, &["config", "--get", "remote.origin.url"]).unwrap_or_default()
}

/// True when `old` is an ancestor of `new` (safe fast-forward incremental).
pub fn is_ancestor(repo: &Path, old: &str, new: &str) -> Result<bool, TemporalError> {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["merge-base", "--is-ancestor", old, new])
        .status()
        .map_err(|e| TemporalError::Git {
            reason: format!("spawn merge-base: {e}"),
        })?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        other => Err(TemporalError::Git {
            reason: format!("merge-base --is-ancestor exited {other:?}"),
        }),
    }
}

/// Accumulated per-file temporal map plus build counters.
#[derive(Default)]
pub struct CollectResult {
    pub files: BTreeMap<String, FileTemporal>,
    pub commits_processed: u64,
    pub change_events: u64,
}

/// Stream `git log` over `range` (None = full history) into `files`.
///
/// Processes newest->oldest. The first touch of a path sets `last_changed_*`;
/// every touch also refreshes `first_seen_*` so the oldest wins by stream end.
/// `change_ts_tail` keeps the most-recent events, capped.
pub fn collect(repo: &Path, range: Option<&str>) -> Result<CollectResult, TemporalError> {
    let mut acc = CollectResult::default();
    collect_into(repo, range, &mut acc, true)?;
    Ok(acc)
}

/// Apply history events into an existing accumulator (used by incremental
/// update). `acc` may already hold records from a prior index.
///
/// `full_history` distinguishes the two cases:
/// * full build — every touch may update `first_seen` (oldest wins at end).
/// * incremental `OLD..NEW` — a path already in `acc` keeps its earlier
///   `first_seen`; only a path first created by this pass tracks its oldest
///   in-range touch.
pub fn collect_into(
    repo: &Path,
    range: Option<&str>,
    acc: &mut CollectResult,
    full_history: bool,
) -> Result<(), TemporalError> {
    let mut proc = LogProc::spawn(repo, range)?;
    let stdout = proc.child.stdout.take().ok_or_else(|| TemporalError::Git {
        reason: "git log stdout unavailable".to_string(),
    })?;
    let mut reader = BufReader::with_capacity(1 << 16, stdout);
    let mut line = String::new();
    let mut cur_sha = String::new();
    let mut cur_ts: i64 = 0;
    let mut created: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Incremental passes gather new events separately (bounded by range size)
    // then merge them ahead of the existing capped tail.
    let mut pending: std::collections::BTreeMap<String, Vec<i64>> =
        std::collections::BTreeMap::new();
    loop {
        line.clear();
        let n = reader.read_line(&mut line).map_err(|e| TemporalError::Io {
            path: repo.to_path_buf(),
            reason: e.to_string(),
        })?;
        if n == 0 {
            break;
        }
        let l = line.trim_end_matches(['\n', '\r']);
        if l.is_empty() {
            continue;
        }
        if let Some(rec) = l.strip_prefix('\x1e') {
            // commit header
            acc.commits_processed += 1;
            let mut f = rec.split('\x1f');
            cur_sha = f.next().unwrap_or("").to_string();
            cur_ts = f.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            continue;
        }
        // name-status line: "X\tpath"  (R## lines cannot appear with --no-renames)
        let mut it = l.splitn(2, '\t');
        let status = it.next().unwrap_or("");
        let path = match it.next() {
            Some(p) => p.trim(),
            None => continue,
        };
        if path.is_empty() || !matches!(status, "A" | "M" | "D" | "T" | "C") {
            continue;
        }
        acc.change_events += 1;
        let path = path.to_string();
        if !acc.files.contains_key(&path) {
            created.insert(path.clone());
        }
        let e = acc
            .files
            .entry(path.clone())
            .or_insert_with(|| FileTemporal {
                path: path.clone(),
                first_seen_commit: cur_sha.clone(),
                first_seen_at: cur_ts,
                last_changed_commit: cur_sha.clone(),
                last_changed_at: cur_ts,
                change_commit_count: 0,
                change_ts_tail: Vec::new(),
            });
        // first (newest) touch sets last_changed
        if e.change_commit_count == 0 || cur_ts > e.last_changed_at {
            e.last_changed_commit = cur_sha.clone();
            e.last_changed_at = cur_ts;
        }
        // refresh first_seen: always in a full build; in an incremental pass
        // only for paths first created here (their oldest in-range touch wins).
        if full_history || created.contains(&path) {
            e.first_seen_commit = cur_sha.clone();
            e.first_seen_at = cur_ts;
        }
        e.change_commit_count += 1;
        if full_history {
            // newest->oldest append; cap keeps the most recent.
            if e.change_ts_tail.len() < CHANGE_TAIL_CAP {
                e.change_ts_tail.push(cur_ts);
            }
        } else {
            pending.entry(path.clone()).or_default().push(cur_ts);
        }
    }
    proc.wait()?;
    if !full_history {
        // Merge in-range events (all newer than the stored tail) ahead of the
        // existing tail, then keep the most-recent `cap`.
        for (path, mut ts) in pending {
            let e = acc.files.get_mut(&path).expect("pending path exists");
            let mut merged = std::mem::take(&mut e.change_ts_tail);
            merged.append(&mut ts);
            merged.sort_unstable_by(|a, b| b.cmp(a));
            merged.truncate(CHANGE_TAIL_CAP);
            merged.sort_unstable();
            e.change_ts_tail = merged;
        }
    }
    // store ascending for stable serialization.
    for f in acc.files.values_mut() {
        f.change_ts_tail.sort_unstable();
    }
    Ok(())
}
