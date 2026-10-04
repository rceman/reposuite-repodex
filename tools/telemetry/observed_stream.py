#!/usr/bin/env python3
"""Production-path replay stream: runs REAL RepoDex queries with
--emit-observation and wraps each observation's metadata in a canonical
`context_artifact_presented` AgentEvent, then appends a deterministic
follow-up schedule exercising every operation class.

This is the closest replay to production wiring available without Relay:
the artifact metadata is produced by RepoDex itself, not fabricated.

Usage: observed_stream.py <binary> <fixture-root>:<label> [<...>] > events.jsonl
"""
import json, subprocess, sys, tempfile, os, glob

SCHEMA = "reposuite.agent-event.v1"
# deterministic follow-up ops applied per artifact, cycling:
OPS = [
    ("exec", "shell", "discovery_search"),      # shell rg
    ("grep", "search", None),                   # legacy discovery
    ("read", "file_read", "source_read"),
    ("exec", "shell", "test"),                  # shell cargo test -> task action
    ("repo_search", "other_repository_tool", "discovery_search"),
    ("ls", "directory_list", None),
    ("exec", "shell", None),                    # legacy unclassified shell
]

def ev(sid, seq, ty, data):
    return {"schema": SCHEMA, "event_id": f"{sid}:{seq}", "session_id": sid,
            "repository_id": "fixture", "sequence": seq,
            "timestamp": "2026-10-04T00:00:00Z",
            "source": {"runtime": "replay", "adapter": "observed-stream", "adapter_version": "1"},
            "type": ty, "data": data}

QUERIES = ["callers method base", "callers method save", "callers method run",
           "related class D", "callees method over", "callers method persist"]

def main():
    binary, specs = sys.argv[1], sys.argv[2:]
    out = []
    gi = 0
    for spec in specs:
        root, label = spec.split(":", 1)
        for qi, q in enumerate(QUERIES):
            op_idx = gi; gi += 1
            sid = f"obs-{label}-{qi}"
            with tempfile.TemporaryDirectory() as td:
                obsdir = os.path.join(td, "obs")
                st = os.path.join(td, "st")
                p = subprocess.run([binary, "query", "--root", root, "--state-dir", st,
                                    "--nav", "adaptive", "--query", q,
                                    "--emit-observation", obsdir, "--direct"],
                                   capture_output=True)
                files = glob.glob(os.path.join(obsdir, "obs-*.json"))
                assert files, f"no observation for {root} {q}"
                obs = json.load(open(files[0]))
            seq = 0
            out.append(ev(sid, seq, "session_started", {})); seq += 1
            out.append(ev(sid, seq, "tool_call_started",
                          {"tool_call_id": "tc-q", "tool_name": "exec", "category": "shell"})); seq += 1
            out.append(ev(sid, seq, "tool_call_completed",
                          {"tool_call_id": "tc-q", "ok": p.returncode == 0,
                           "output_bytes": obs["packet_bytes"],
                           "output_digest": obs["packet_digest"]})); seq += 1
            out.append(ev(sid, seq, "context_artifact_presented", {
                "artifact_id": obs["packet_digest"], "artifact_kind": "rdx1_packet",
                "producer": "repodex", "tool_call_id": "tc-q",
                "presentation": "adaptive", "content_digest": obs["packet_digest"],
                "content_bytes": obs["packet_bytes"],
                "metadata": obs if False else {
                    "query_intent": obs["intent"],
                    "navigation_profile": obs["nav_profile"],
                    "evidence_complete": obs["seed_selection_complete"],
                    "gap_signatures": obs["gap_signatures"],
                    "seed_count": obs["seed_count"],
                    "candidate_count": obs["candidate_count"],
                    "relation_count": obs["relation_count"],
                    "packet_bytes": obs["packet_bytes"],
                    "query_digest": obs["query_digest"],
                }})); seq += 1
            name, cat, op = OPS[op_idx % len(OPS)]
            d = {"tool_call_id": f"fu-{qi}", "tool_name": name, "category": cat}
            if op is not None:
                d["repository_operation"] = op
            out.append(ev(sid, seq, "tool_call_started", d)); seq += 1
            out.append(ev(sid, seq, "session_completed", {}))
    for e in out:
        print(json.dumps(e, separators=(",", ":")))

main()
