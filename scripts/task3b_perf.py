#!/usr/bin/env python3
"""TASK 3B performance and artifact-size validation.

Measures, for one representative repository per language:

    fresh full snapshot build (TASK 3A)
    link build (TASK 3B), with the phase breakdown the CLI reports
    link build repeated, to prove canonical determinism
    link build into a different output directory, to prove location independence
    snapshot bytes, link artifact bytes, and the ratio

and answers the one question that decides whether TASK 3B needs incremental
relationship mutation:

    Is a full relationship rebuild cheap enough that incremental link mutation
    is unnecessary for now?

Usage:

    scripts/task3b_perf.py [--json out.json] [--repeat N]
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HOME = Path.home()
CORPORA = HOME / "reposuite" / "repodex" / "corpora"
REPO = Path(__file__).resolve().parent.parent
BINARY = REPO / "target" / "release" / "reposuite-repodex"

# One representative repository per language, matching the TASK 3A corpus set.
TARGETS = [
    ("rust", "tokio-rs_tokio"),
    ("go", "gohugoio_hugo"),
    ("python", "django_django"),
    ("php", "laravel_framework"),
]


def run(args, measure=True):
    """Run a command, returning (stdout, wall seconds, peak RSS KB)."""
    if measure and Path("/usr/bin/time").exists():
        wrapper = ["/usr/bin/time", "-f", "@@%es@@%MKB"]
        started = time.monotonic()
        proc = subprocess.run(
            wrapper + args, capture_output=True, text=True, check=False
        )
        wall = time.monotonic() - started
        stderr = proc.stderr
        rss = 0
        for line in stderr.splitlines():
            match = re.search(r"@@([\d.]+)s@@(\d+)KB", line)
            if match:
                wall = float(match.group(1))
                rss = int(match.group(2))
        if proc.returncode != 0:
            raise SystemExit(f"{' '.join(args)} failed:\n{stderr}")
        return proc.stdout, wall, rss
    started = time.monotonic()
    proc = subprocess.run(args, capture_output=True, text=True, check=False)
    if proc.returncode != 0:
        raise SystemExit(f"{' '.join(args)} failed:\n{proc.stderr}")
    return proc.stdout, time.monotonic() - started, 0


def parse_key_values(text):
    out = {}
    for line in text.splitlines():
        match = re.match(r"^([a-z][a-z ]*[a-z]):\s+(.*)$", line)
        if match:
            out[match.group(1).strip()] = match.group(2).strip()
    return out


def parse_links_line(text):
    match = re.search(
        r"links:\s+(\d+) \(exact (\d+), ambiguous (\d+), unresolved (\d+), "
        r"out-of-scope (\d+)\)",
        text,
    )
    if not match:
        return {}
    return {
        "links": int(match.group(1)),
        "exact": int(match.group(2)),
        "ambiguous": int(match.group(3)),
        "unresolved": int(match.group(4)),
        "out_of_scope": int(match.group(5)),
    }


def parse_phases(text):
    match = re.search(
        r"elapsed:\s+([\d.]+) ms \(load ([\d.]+), metadata ([\d.]+), "
        r"structure ([\d.]+), derive ([\d.]+), serialize ([\d.]+), verify ([\d.]+)\)",
        text,
    )
    if not match:
        return {}
    keys = [
        "duration_ms",
        "snapshot_load_ms",
        "metadata_ms",
        "structure_ms",
        "derive_ms",
        "serialize_ms",
        "verify_ms",
    ]
    return {key: float(value) for key, value in zip(keys, match.groups())}


def parse_files_line(text):
    match = re.search(
        r"files:\s+(\d+) \(snapshot (\d+) bytes, links (\d+) bytes, ratio ([\d.]+)\)",
        text,
    )
    if not match:
        return {}
    return {
        "files": int(match.group(1)),
        "snapshot_bytes": int(match.group(2)),
        "link_bytes": int(match.group(3)),
        "size_ratio": float(match.group(4)),
    }


def dir_bytes(path: Path) -> int:
    total = 0
    for item in path.rglob("*"):
        if item.is_file():
            total += item.stat().st_size
    return total


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", default=None)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument(
        "--corpora", default=str(CORPORA), help="corpus root directory"
    )
    args = parser.parse_args()

    corpora = Path(args.corpora)
    if not BINARY.exists():
        raise SystemExit(f"build the release binary first: {BINARY}")

    work = Path(tempfile.mkdtemp(prefix="t3b-perf-"))
    results = []

    print(f"binary  : {BINARY}")
    print(f"corpora : {corpora}")
    print(f"scratch : {work}")
    print()

    for language, name in TARGETS:
        root = corpora / name
        if not root.is_dir():
            print(f"!! missing corpus {root}")
            continue
        print("=" * 78)
        print(f"{language}: {name}")

        snapshot_dir = work / f"snap-{name}"
        _, snapshot_wall, snapshot_rss = run(
            [str(BINARY), "index", "build", str(root), "--output", str(snapshot_dir)]
        )
        snapshot_bytes = dir_bytes(snapshot_dir)

        links_dir = work / f"links-{name}"
        stdout, link_wall, link_rss = run(
            [
                str(BINARY),
                "links",
                "build",
                str(snapshot_dir),
                "--repository",
                str(root),
                "--output",
                str(links_dir),
            ]
        )
        files = parse_files_line(stdout)
        links = parse_links_line(stdout)
        phases = parse_phases(stdout)
        link_bytes = dir_bytes(links_dir)
        digest = parse_key_values(stdout).get("link digest")

        # Determinism: repeat, and build into a different location.
        digests = {digest}
        for index in range(args.repeat - 1):
            other = work / f"links-{name}-repeat{index}"
            stdout_other, _, _ = run(
                [
                    str(BINARY),
                    "links",
                    "build",
                    str(snapshot_dir),
                    "--repository",
                    str(root),
                    "--output",
                    str(other),
                ]
            )
            digests.add(parse_key_values(stdout_other).get("link digest"))
            shutil.rmtree(other, ignore_errors=True)

        relocated = work / "relocated" / f"links-{name}"
        relocated.parent.mkdir(parents=True, exist_ok=True)
        stdout_relocated, _, _ = run(
            [
                str(BINARY),
                "links",
                "build",
                str(snapshot_dir),
                "--repository",
                str(root),
                "--output",
                str(relocated),
            ]
        )
        digests.add(parse_key_values(stdout_relocated).get("link digest"))
        shutil.rmtree(relocated.parent, ignore_errors=True)

        # A no-change TASK 3A update, then a link rebuild, must agree too.
        updated_snapshot = work / f"snap-updated-{name}"
        run(
            [
                str(BINARY),
                "index",
                "update",
                str(root),
                "--previous",
                str(snapshot_dir),
                "--output",
                str(updated_snapshot),
            ]
        )
        updated_links = work / f"links-updated-{name}"
        stdout_updated, _, _ = run(
            [
                str(BINARY),
                "links",
                "build",
                str(updated_snapshot),
                "--repository",
                str(root),
                "--output",
                str(updated_links),
            ]
        )
        updated_digest = parse_key_values(stdout_updated).get("link digest")
        shutil.rmtree(updated_snapshot, ignore_errors=True)
        shutil.rmtree(updated_links, ignore_errors=True)

        record = {
            "language": language,
            "corpus": name,
            "snapshot_wall_s": snapshot_wall,
            "snapshot_rss_kb": snapshot_rss,
            "snapshot_bytes": snapshot_bytes,
            "link_wall_s": link_wall,
            "link_rss_kb": link_rss,
            "link_bytes": link_bytes,
            "size_ratio": link_bytes / snapshot_bytes if snapshot_bytes else None,
            "link_build_vs_snapshot_build": (
                link_wall / snapshot_wall if snapshot_wall else None
            ),
            "digests": sorted(d for d in digests if d),
            "deterministic": len(digests) == 1,
            "update_then_link_digest": updated_digest,
            "update_matches_fresh": updated_digest == digest,
            "phases": phases,
            **links,
            **files,
        }
        results.append(record)

        print(f"  snapshot build : {snapshot_wall:8.2f} s   {snapshot_rss:>8} KB rss")
        print(f"  link build     : {link_wall:8.2f} s   {link_rss:>8} KB rss")
        print(
            f"  snapshot bytes : {snapshot_bytes:>12,}\n"
            f"  link bytes     : {link_bytes:>12,}  ({record['size_ratio']:.4f} of snapshot)"
        )
        print(
            f"  link / snapshot build time: {record['link_build_vs_snapshot_build']:.3f}"
        )
        print(
            f"  relationships  : {links.get('links')} "
            f"(exact {links.get('exact')}, ambiguous {links.get('ambiguous')}, "
            f"unresolved {links.get('unresolved')}, out-of-scope {links.get('out_of_scope')})"
        )
        print(f"  phases         : {phases}")
        print(f"  deterministic  : {record['deterministic']} ({len(digests)} distinct digest(s))")
        print(f"  update == fresh: {record['update_matches_fresh']}")
        print()

    if results:
        print("=" * 78)
        ratios = [r["size_ratio"] for r in results if r["size_ratio"] is not None]
        speedups = [
            r["link_build_vs_snapshot_build"]
            for r in results
            if r["link_build_vs_snapshot_build"] is not None
        ]
        print(f"link/snapshot size ratio : min {min(ratios):.4f} max {max(ratios):.4f}")
        print(
            f"link/snapshot build time : min {min(speedups):.3f} max {max(speedups):.3f}"
        )
        print(f"all deterministic        : {all(r['deterministic'] for r in results)}")
        print(
            f"all update == fresh      : {all(r['update_matches_fresh'] for r in results)}"
        )

    if args.json:
        Path(args.json).write_text(json.dumps(results, indent=2))
        print(f"\nwrote {args.json}")

    shutil.rmtree(work, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
