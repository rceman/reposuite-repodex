//! Temporal foundation fixtures (§50-54): incremental correctness, divergence,
//! rename, delete/recreate, determinism. Uses disposable Git repos with
//! deterministic committer dates.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use repodex::temporal::{self, TemporalIndex};

static N: AtomicU32 = AtomicU32::new(0);

struct Repo(PathBuf);
impl Repo {
    fn new(name: &str) -> Self {
        let id = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("rt-tmp-{name}-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let r = Repo(dir);
        r.git(&["init", "-q"]);
        r.git(&["config", "user.email", "t@t"]);
        r.git(&["config", "user.name", "t"]);
        r.git(&["config", "commit.gpgsign", "false"]);
        r
    }
    fn git(&self, args: &[&str]) {
        let st = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(args)
            .status()
            .unwrap();
        assert!(st.success(), "git {:?} failed", args);
    }
    /// Commit with a deterministic committer/author date.
    fn commit(&self, date: &str, msg: &str) {
        let st = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .args(["commit", "-qm", msg])
            .status()
            .unwrap();
        assert!(st.success(), "commit {msg} failed");
    }
    fn write(&self, rel: &str, text: &str) {
        let p = self.0.join(rel);
        if let Some(par) = p.parent() {
            std::fs::create_dir_all(par).unwrap();
        }
        std::fs::write(p, text).unwrap();
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn unix(date: &str) -> i64 {
    // All fixture dates are "YYYY-MM-DDT00:00:00Z" at day granularity.
    let (y, rest) = date.split_once('-').unwrap();
    let (m, d) = rest.split_once('-').unwrap();
    let y: i64 = y.parse().unwrap();
    let m: i64 = m.parse().unwrap();
    let d: i64 = d[..2].parse().unwrap();
    // days-from-civil (Howard Hinnant)
    let yy = if m <= 2 { y - 1 } else { y };
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe - 719468) * 86400
}

fn load(dir: &Path) -> TemporalIndex {
    TemporalIndex::load(dir).expect("temporal index must load")
}

/// §50 incremental correctness: A created/changed, B created/changed, then an
/// incremental commit updates A and creates C.
#[test]
fn temporal_incremental_exact() {
    let r = Repo::new("incr");
    r.write("A.txt", "a1");
    r.git(&["add", "-A"]);
    r.commit("2020-01-01T00:00:00Z", "c1");
    r.write("A.txt", "a1a2");
    r.write("B.txt", "b1");
    r.git(&["add", "-A"]);
    r.commit("2020-02-01T00:00:00Z", "c2");
    r.write("B.txt", "b1b2");
    r.git(&["add", "-A"]);
    r.commit("2020-03-01T00:00:00Z", "c3");

    let out = std::env::temp_dir().join(format!("rt-ti-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    temporal::build_temporal(&r.0, &out).unwrap();

    let idx = load(&out);
    let a = idx.file("A.txt").unwrap();
    assert_eq!(a.change_commit_count, 2);
    assert_eq!(a.first_seen_at, unix("2020-01-01T00:00:00Z"));
    assert_eq!(a.last_changed_at, unix("2020-02-01T00:00:00Z"));
    let b = idx.file("B.txt").unwrap();
    assert_eq!(b.change_commit_count, 2);
    assert_eq!(b.first_seen_at, unix("2020-02-01T00:00:00Z"));

    // commit 4: A changed, C created
    r.write("A.txt", "a1a2a3");
    r.write("C.txt", "c1");
    r.git(&["add", "-A"]);
    r.commit("2020-04-01T00:00:00Z", "c4");

    let out2 = std::env::temp_dir().join(format!("rt-ti2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out2);
    let oc = temporal::update_temporal(&r.0, &out, &out2).unwrap();
    assert_eq!(
        oc.stats.commits_processed, 1,
        "only the new commit is scanned"
    );
    assert_eq!(oc.stats.change_events, 2);

    let idx2 = load(&out2);
    let a2 = idx2.file("A.txt").unwrap();
    assert_eq!(a2.change_commit_count, 3);
    assert_eq!(
        a2.first_seen_at,
        unix("2020-01-01T00:00:00Z"),
        "first_seen kept"
    );
    assert_eq!(a2.last_changed_at, unix("2020-04-01T00:00:00Z"));
    let c2 = idx2.file("C.txt").unwrap();
    assert_eq!(c2.change_commit_count, 1);
    assert_eq!(c2.first_seen_at, unix("2020-04-01T00:00:00Z"));
    let _ = std::fs::remove_dir_all(&out);
    let _ = std::fs::remove_dir_all(&out2);
}

/// §51 divergence: a non-descendant HEAD must refuse incremental update.
#[test]
fn temporal_divergence_refused() {
    let r = Repo::new("div");
    r.write("A.txt", "a1");
    r.git(&["add", "-A"]);
    r.commit("2020-01-01T00:00:00Z", "c1");
    r.write("A.txt", "a1a2");
    r.git(&["add", "-A"]);
    r.commit("2020-02-01T00:00:00Z", "c2");
    let out = std::env::temp_dir().join(format!("rt-td-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    temporal::build_temporal(&r.0, &out).unwrap();
    // rewind + a divergent commit -> new head is not a descendant of indexed.
    r.git(&["reset", "--hard", "HEAD~1"]);
    r.write("DIV.txt", "x");
    r.git(&["add", "-A"]);
    r.commit("2020-02-15T00:00:00Z", "div");
    let out2 = std::env::temp_dir().join(format!("rt-td2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out2);
    let err = temporal::update_temporal(&r.0, &out, &out2).unwrap_err();
    let s = err.to_string();
    assert!(s.contains("history_diverged"), "got: {s}");
    assert!(s.contains("rebuild_required"), "got: {s}");
    let _ = std::fs::remove_dir_all(&out);
    let _ = std::fs::remove_dir_all(&out2);
}

/// §52 rename: path-centric (--no-renames). New path first-seen at the rename;
/// old path's last change is the rename delete. History is not carried across.
#[test]
fn temporal_rename_path_centric() {
    let r = Repo::new("ren");
    r.write("old.txt", "x");
    r.git(&["add", "-A"]);
    r.commit("2021-01-01T00:00:00Z", "c1");
    r.write("old.txt", "xy");
    r.git(&["add", "-A"]);
    r.commit("2021-02-01T00:00:00Z", "c2");
    r.git(&["mv", "old.txt", "new.txt"]);
    r.commit("2021-03-01T00:00:00Z", "ren");
    let out = std::env::temp_dir().join(format!("rt-tr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    temporal::build_temporal(&r.0, &out).unwrap();
    let idx = load(&out);
    let old = idx.file("old.txt").unwrap();
    assert_eq!(old.change_commit_count, 3, "create+change+rename-delete");
    assert_eq!(old.last_changed_at, unix("2021-03-01T00:00:00Z"));
    let new = idx.file("new.txt").unwrap();
    assert_eq!(
        new.change_commit_count, 1,
        "no history carried across rename"
    );
    assert_eq!(new.first_seen_at, unix("2021-03-01T00:00:00Z"));
    let _ = std::fs::remove_dir_all(&out);
}

/// §53 delete/recreate: one path history; D and re-A are ordinary events.
#[test]
fn temporal_delete_recreate_one_path() {
    let r = Repo::new("del");
    r.write("f.txt", "a");
    r.git(&["add", "-A"]);
    r.commit("2021-04-01T00:00:00Z", "add");
    r.git(&["rm", "-q", "f.txt"]);
    r.commit("2021-05-01T00:00:00Z", "rm");
    r.write("f.txt", "b");
    r.git(&["add", "-A"]);
    r.commit("2021-06-01T00:00:00Z", "readd");
    let out = std::env::temp_dir().join(format!("rt-tx-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    temporal::build_temporal(&r.0, &out).unwrap();
    let f = load(&out).file("f.txt").unwrap().clone();
    assert_eq!(f.change_commit_count, 3);
    assert_eq!(f.first_seen_at, unix("2021-04-01T00:00:00Z"));
    assert_eq!(f.last_changed_at, unix("2021-06-01T00:00:00Z"));
    let _ = std::fs::remove_dir_all(&out);
}

/// §54 determinism: same repo+HEAD+schema => semantically identical records.
#[test]
fn temporal_deterministic() {
    let r = Repo::new("det");
    r.write("A.txt", "a");
    r.git(&["add", "-A"]);
    r.commit("2020-01-01T00:00:00Z", "c1");
    r.write("A.txt", "ab");
    r.git(&["add", "-A"]);
    r.commit("2020-02-01T00:00:00Z", "c2");
    let o1 = std::env::temp_dir().join(format!("rt-tz1-{}", std::process::id()));
    let o2 = std::env::temp_dir().join(format!("rt-tz2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&o1);
    let _ = std::fs::remove_dir_all(&o2);
    temporal::build_temporal(&r.0, &o1).unwrap();
    temporal::build_temporal(&r.0, &o2).unwrap();
    let d1 = repodex::temporal::artifact::records_digest(&o1).unwrap();
    let d2 = repodex::temporal::artifact::records_digest(&o2).unwrap();
    assert_eq!(d1, d2, "byte-identical record digest");
    let _ = std::fs::remove_dir_all(&o1);
    let _ = std::fs::remove_dir_all(&o2);
}
