#!/usr/bin/env python3
"""Phase D §57 — repeatable query benchmark.

Runs a fixed set of deterministic queries against a graph and reports latency
+ result counts. Usage:

  query_bench.py <repodex-binary> <graph-dir> [label]

Covers: exact lookup, ambiguous lookup, 1-term + 3-term free-text, callers,
callees, path depth-2/4, neighborhood, exhaustive, RDX1-budgeted.
"""
import json
import subprocess
import sys
import time

BIN = sys.argv[1]
G = sys.argv[2]
LABEL = sys.argv[3] if len(sys.argv) > 3 else "corpus"


def run(args):
    t = time.perf_counter()
    p = subprocess.run([BIN, *args], capture_output=True, text=True)
    return (time.perf_counter() - t) * 1000, p


def bench(name, args, parser=None):
    ms, p = run(args)
    detail = ""
    if p.returncode != 0:
        detail = f"ERROR {p.stderr.strip()[:60]}"
    elif parser:
        detail = parser(p.stdout)
    print(f"{LABEL:8} {name:22} {ms:7.1f} ms  {detail}")


def seeds_count(out):
    try:
        return f"seeds={json.loads(out)['total']} shown={json.loads(out)['shown']}"
    except Exception:
        return f"{len(out.splitlines())} lines"


# --- exact + ambiguous + free-text -------------------------------------------
bench("exact-lookup", ["graph", "node", G, "file:app/run.go"], lambda o: "ok")
bench("ambig-name", ["query", "--graph", G, "--json", "--exhaustive", "Validate"], seeds_count)
bench("free-text-1", ["query", "--graph", G, "--json", "token"], seeds_count)
bench("free-text-3", ["query", "--graph", G, "--json", "auth token handler"], seeds_count)

# --- intents ------------------------------------------------------------------
bench("callers", ["query", "--graph", G, "--json", "--intent", "callers",
                  "--target", "ValidateToken", "ValidateToken"],
      lambda o: f"related={len(json.loads(o)['related'])}")
bench("callees", ["query", "--graph", G, "--json", "--intent", "callees",
                  "--target", "run", "run"],
      lambda o: f"related={len(json.loads(o)['related'])}")

# --- bounded paths ------------------------------------------------------------
bench("paths-d2", ["graph", "paths", G, "file:app/run.go", "decl:auth/token.go#1",
                   "--depth", "2"], lambda o: f"{len(o.splitlines())-1} paths")
bench("paths-d4", ["graph", "paths", G, "file:app/run.go", "decl:auth/token.go#1",
                   "--depth", "4"], lambda o: f"{len(o.splitlines())-1} paths")
bench("neighborhood", ["graph", "neighborhood", G, "file:app/run.go"],
      lambda o: f"{len(o.splitlines())-1} neighbors")

# --- exhaustive + RDX1 ---------------------------------------------------------
bench("exhaustive", ["query", "--graph", G, "--exhaustive", "--json", "Validate"], seeds_count)
bench("rdx1-budget", ["query", "--graph", G, "--tokens", "400", "token"],
      lambda o: f"{len(o.splitlines())} lines")
