#!/usr/bin/env python3
"""TASK 3A performance validation.

Measures, for one representative repository per language:

  fresh full snapshot build
  load/verify an existing snapshot
  no-change update
  single-file-change update
  fresh full rebuild of the single-file-change final state

For each run it records processed files, reused files, reparsed files, bytes
hashed, bytes reparsed, wall time, snapshot artifact size and peak RSS.

The single-file change is applied by appending a comment line to one supported
source file and is always restored, verified by SHA-256, before the script
finishes. No corpus is left modified.

Nothing here is a performance budget. The numbers are observations on this
machine, with a warm page cache, and are reported as such.
"""

import hashlib
import json
import os
import statistics
import subprocess
import sys
import time
from pathlib import Path

HOME = Path.home()
CORPORA = HOME / "reposuite" / "repodex" / "corpora"
REPO = Path(__file__).resolve().parent.parent
BIN = REPO / "target" / "release" / "reposuite-repodex"

REPETITIONS = 3

# (label, language, corpus directory, comment prefix)
TARGETS = [
    ("rust", "rust", "tokio-rs_tokio", "//"),
    ("go", "go", "gohugoio_hugo", "//"),
    ("python", "python", "django_django", "#"),
    ("php", "php", "laravel_framework", "//"),
]

EXTENSIONS = {"rust": ".rs", "go": ".go", "python": ".py", "php": ".php"}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def run_timed(args, expect_json=True):
    """Run a command under /usr/bin/time -v and parse wall time and peak RSS."""
    completed = subprocess.run(
        ["/usr/bin/time", "-v", str(BIN)] + [str(a) for a in args],
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"command failed ({completed.returncode}): {' '.join(map(str, args))}\n"
            f"{completed.stderr}"
        )
    wall_ms = None
    elapsed_raw = None
    peak_rss_kb = None
    for line in completed.stderr.splitlines():
        if "Maximum resident set size" in line:
            peak_rss_kb = int(line.split(":")[1].strip())
        elif "Elapsed (wall clock)" in line:
            elapsed_raw = line.rsplit(":", 1)[1].strip()
            wall_ms = parse_elapsed(elapsed_raw)
    payload = None
    if expect_json:
        payload = json.loads(completed.stdout)
    return {
        "wall_ms": wall_ms,
        "elapsed_raw": elapsed_raw,
        "peak_rss_kb": peak_rss_kb,
        "stdout": completed.stdout,
        "json": payload,
    }


def parse_elapsed(text: str) -> float:
    """Parse `mm:ss.ss` or `hh:mm:ss.ss` from /usr/bin/time into milliseconds."""
    parts = text.split(":")
    seconds = 0.0
    for part in parts:
        seconds = seconds * 60 + float(part)
    return seconds * 1000.0


def pick_change_target(root: Path, extension: str) -> Path:
    """Deterministically choose a small supported file to modify."""
    candidates = []
    for path in sorted(root.rglob(f"*{extension}")):
        if not path.is_file():
            continue
        if any(part in {"target", "vendor", "node_modules", ".git"} for part in path.parts):
            continue
        size = path.stat().st_size
        if 200 <= size <= 20_000:
            candidates.append(path)
    if not candidates:
        raise RuntimeError(f"no candidate {extension} file found under {root}")
    return candidates[0]


def snapshot_size(directory: Path) -> int:
    total = 0
    for path in directory.rglob("*"):
        if path.is_file():
            total += path.stat().st_size
    return total


def record(rows, corpus, scenario, repetition, result, extra=None):
    row = {
        "corpus": corpus,
        "scenario": scenario,
        "repetition": repetition,
        "wall_ms": result["wall_ms"],
        "elapsed_raw": result["elapsed_raw"],
        "peak_rss_kb": result["peak_rss_kb"],
    }
    payload = result["json"]
    if payload:
        stats = payload.get("stats")
        if stats:
            row.update(
                {
                    "files_total": stats["files_total"],
                    "reused_files": stats["reused_files"],
                    "reparsed_files": stats["reparsed_files"],
                    "reextracted_files": stats["reextracted_files"],
                    "added": stats["added"],
                    "changed": stats["changed"],
                    "deleted": stats["deleted"],
                    "bytes_hashed": stats["bytes_hashed"],
                    "bytes_reparsed": stats["bytes_reparsed"],
                    "reported_ms": stats["duration_ms"],
                }
            )
        if "snapshot_digest" in payload:
            row["snapshot_digest"] = payload["snapshot_digest"]
        if "files" in payload and "checked_artifacts" in payload:
            row["verified_files"] = payload["files"]
            row["artifact_bytes"] = payload["artifact_bytes"]
        if "totals" in payload:
            row["source_bytes"] = payload["totals"]["source_bytes"]
    if extra:
        row.update(extra)
    rows.append(row)
    return row


def summarise(values):
    if not values:
        return None
    return {
        "min": min(values),
        "median": statistics.median(values),
        "max": max(values),
    }


def main():
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    out_root = HOME / "reposuite" / "repodex" / "benchmarks" / f"task3a-perf-{stamp}"
    out_root.mkdir(parents=True, exist_ok=True)
    rows = []
    equivalence = []

    for label, language, corpus_name, comment in TARGETS:
        root = CORPORA / corpus_name
        if not root.is_dir():
            print(f"skipping {corpus_name}: not present", file=sys.stderr)
            continue
        extension = EXTENSIONS[language]
        target = pick_change_target(root, extension)
        original = target.read_bytes()
        original_digest = hashlib.sha256(original).hexdigest()
        print(f"== {label} ({corpus_name}) change target: "
              f"{target.relative_to(root)} ({len(original)} bytes)")

        try:
            for repetition in range(REPETITIONS):
                base = out_root / corpus_name / f"rep{repetition}"
                previous = base / "previous"
                nochange = base / "nochange"
                change = base / "change"
                changefresh = base / "changefresh"

                # 1. fresh full build
                result = run_timed(
                    ["index", "build", root, "--output", previous, "--json"]
                )
                fresh_row = record(rows, corpus_name, "fresh_build", repetition, result)

                # 2. load/verify the existing snapshot
                result = run_timed(["index", "verify", previous, "--json"])
                record(rows, corpus_name, "verify", repetition, result)

                # 3. no-change update
                result = run_timed(
                    [
                        "index", "update", root,
                        "--previous", previous,
                        "--output", nochange,
                        "--json",
                    ]
                )
                nochange_row = record(
                    rows, corpus_name, "no_change_update", repetition, result
                )

                # 4. single-file-change update
                target.write_bytes(original + f"\n{comment} repodex t3a probe\n".encode())
                result = run_timed(
                    [
                        "index", "update", root,
                        "--previous", previous,
                        "--output", change,
                        "--json",
                    ]
                )
                change_row = record(
                    rows, corpus_name, "one_file_change_update", repetition, result
                )

                # 5. fresh full rebuild of the same final state
                result = run_timed(
                    ["index", "build", root, "--output", changefresh, "--json"]
                )
                rebuild_row = record(
                    rows, corpus_name, "fresh_rebuild_of_changed_state",
                    repetition, result,
                )

                # Restore immediately after the measurement.
                target.write_bytes(original)
                restored = sha256_file(target)
                equivalence.append(
                    {
                        "corpus": corpus_name,
                        "repetition": repetition,
                        "changed_path": str(target.relative_to(root)),
                        "restored_ok": restored == original_digest,
                        "incremental_digest": change_row.get("snapshot_digest"),
                        "fresh_rebuild_digest": rebuild_row.get("snapshot_digest"),
                        "digests_equal": change_row.get("snapshot_digest")
                        == rebuild_row.get("snapshot_digest"),
                        "no_change_reparsed": nochange_row.get("reparsed_files"),
                        "no_change_reused": nochange_row.get("reused_files"),
                        "one_change_reparsed": change_row.get("reparsed_files"),
                        "one_change_reused": change_row.get("reused_files"),
                        "fresh_artifact_bytes": fresh_row.get("artifact_bytes")
                        or snapshot_size(previous),
                    }
                )
                assert restored == original_digest, "corpus file was not restored"
        finally:
            target.write_bytes(original)
            assert sha256_file(target) == original_digest, "restore failed"

        # Artifact size for the fresh snapshot.
        for row in rows:
            if row["corpus"] == corpus_name and row["scenario"] == "fresh_build":
                row["artifact_bytes_measured"] = snapshot_size(
                    out_root / corpus_name / f"rep{row['repetition']}" / "previous"
                )

    with open(out_root / "perf-raw.jsonl", "w") as handle:
        for row in rows:
            handle.write(json.dumps(row, sort_keys=True) + "\n")

    with open(out_root / "equivalence.jsonl", "w") as handle:
        for item in equivalence:
            handle.write(json.dumps(item, sort_keys=True) + "\n")

    lines = ["TASK 3A performance validation", f"binary: {BIN}", f"repetitions: {REPETITIONS}", ""]
    scenarios = [
        "fresh_build",
        "verify",
        "no_change_update",
        "one_file_change_update",
        "fresh_rebuild_of_changed_state",
    ]
    for label, language, corpus_name, _ in TARGETS:
        corpus_rows = [r for r in rows if r["corpus"] == corpus_name]
        if not corpus_rows:
            continue
        lines.append(f"## {label} ({corpus_name})")
        for scenario in scenarios:
            subset = [r for r in corpus_rows if r["scenario"] == scenario]
            if not subset:
                continue
            wall = summarise([r["wall_ms"] for r in subset if r["wall_ms"] is not None])
            rss = summarise(
                [r["peak_rss_kb"] for r in subset if r["peak_rss_kb"] is not None]
            )
            sample = subset[0]
            lines.append(
                f"  {scenario:<32} "
                f"wall_ms min/med/max {wall['min']:.1f}/{wall['median']:.1f}/{wall['max']:.1f}  "
                f"peak_rss_mb min/med/max "
                f"{rss['min']/1024:.1f}/{rss['median']/1024:.1f}/{rss['max']/1024:.1f}  "
                f"files {sample.get('files_total')} "
                f"reused {sample.get('reused_files')} "
                f"reparsed {sample.get('reparsed_files')} "
                f"bytes_hashed {sample.get('bytes_hashed')} "
                f"bytes_reparsed {sample.get('bytes_reparsed')} "
                f"source_bytes {sample.get('source_bytes')} "
                f"artifact_bytes {sample.get('artifact_bytes_measured') or sample.get('artifact_bytes')}"
            )
        lines.append("")

    all_ok = all(
        item["restored_ok"] and item["digests_equal"] for item in equivalence
    )
    lines.append(f"all corpora restored: {all(i['restored_ok'] for i in equivalence)}")
    lines.append(f"all incremental digests equal fresh rebuild: "
                 f"{all(i['digests_equal'] for i in equivalence)}")
    lines.append(f"overall: {'PASS' if all_ok else 'FAIL'}")

    text = "\n".join(lines) + "\n"
    with open(out_root / "perf-summary.txt", "w") as handle:
        handle.write(text)
    print(text)
    print(f"evidence root: {out_root}")
    return 0 if all_ok else 1


if __name__ == "__main__":
    sys.exit(main())
