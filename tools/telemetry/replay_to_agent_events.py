#!/usr/bin/env python3
"""REPLAY-ONLY converter: committed eval traces -> canonical AgentEvent v1 JSONL.

This script lives in TEST/EVAL tooling, not production: it reads the RDX text
inside committed harness TOOL_RESULT events to fabricate
`context_artifact_presented` metadata for replay fixtures. Production Relay
must NEVER do this — it receives the structured RepoQueryObservation metadata
from the query producer directly (RELAY_PARSES_RDX_FOR_TELEMETRY = false).

Usage: replay_to_agent_events.py <trace.json> [<trace.json> ...] > events.jsonl
"""
import json, re, sys, hashlib

SCHEMA = "reposuite.agent-event.v1"

CAT_MAP = {  # harness category -> AgentEvent ToolCategory
    "repodex": "shell",          # producer call; correlated via tool_call_id
    "native_search": "search",
    "enumeration": "directory_list",
    "source_read": "file_read",
    "edit": "shell",
    "test_build": "shell",
    "other_exec": "shell",
    "git": "git_inspection",
    "todo_write": "non_repository_tool",
}

# normalized repository_operation for streams that carry it (correction V1):
# the converter plays the future Relay role — it observed the action class.
OP_MAP = {
    "repodex": None,             # correlated producer call; left absent
    "native_search": "discovery_search",
    "enumeration": "discovery_list",
    "source_read": "source_read",
    "edit": "edit",
    "test_build": "test",
    "other_exec": "runtime",
    "git": "git_inspection",
    "todo_write": "other",
}

G_MAP = {  # RDX G-line -> gap signature family (replay fixture construction)
    "NO_RESULT": ("no_result", None),
    "RESULT_LIMIT": ("result_limit", "seed_bound"),
    "AMBIGUOUS_RESULT": ("ambiguous_result", "no_confident_intent"),
    "RELATIONSHIP_LIMIT": ("result_limit", "relationship_limit"),
    "NO_ROUTE": ("bounded_no_route", "no_route"),
    "TRUNCATED_CONTINUATION": ("evidence_truncated", "truncated_continuation"),
    "UPSTREAM_TRUNCATED": ("evidence_truncated", None),
}


def sha(t):
    return "sha256:" + hashlib.sha256(str(t).encode()).hexdigest()


def ev(session, seq, ts, etype, data, repo=None):
    return {
        "schema": SCHEMA,
        "event_id": f"{session}:{seq}",
        "session_id": session,
        "repository_id": repo,
        "sequence": seq,
        "timestamp": ts,
        "source": {"runtime": "replay", "adapter": "eval-trace", "adapter_version": "1"},
        "type": etype,
        "data": data,
    }


def gaps_from_rdx(text):
    out = []
    intent = None
    for line in str(text).splitlines():
        if line.startswith("I "):
            intent = line[2:].strip()
        if line.startswith("G "):
            head = line[2:].split()[0]
            reason = None
            m = re.search(r"reason=(\S+)", line)
            fam, code = G_MAP.get(head, ("unsupported_evidence_class", head.lower()))
            if m:
                code = m.group(1)
            out.append({"family": fam, "reason_code": code})
        if line.startswith("S "):
            pass
    return out, intent


def convert(path):
    t = json.load(open(path))
    # namespace by suite dir: session ids may repeat across eval packages
    suite = path.split("evals/")[-1].split("/")[0] if "evals/" in path else "x"
    sid = suite + "." + t["session"]
    repo = t.get("task", "")
    events = []
    seq = 0
    ts = "1970-01-01T00:00:00Z"
    tevs = t.get("events", [])
    events.append(ev(sid, seq, ts, "session_started", {"repository": repo}, repo)); seq += 1
    calls = {}
    for e in tevs:
        if e.get("ev") == "TOOL_REQUEST":
            cid = e["id"]; cats = e.get("categories") or [e.get("category", "other_exec")]
            cat = cats[0] if cats else "other_exec"
            calls[cid] = cat
            data = {
                "tool_call_id": cid, "tool_name": e.get("tool", "exec"),
                "category": CAT_MAP.get(cat, "non_repository_tool")}
            op = OP_MAP.get(cat, "other")
            if op is not None:
                data["repository_operation"] = op
            events.append(ev(sid, seq, ts, "tool_call_started", data, repo)); seq += 1
        elif e.get("ev") == "TOOL_RESULT":
            cid = e["id"]; cat = calls.get(cid, "other_exec")
            content = e.get("content", "")
            events.append(ev(sid, seq, ts, "tool_call_completed", {
                "tool_call_id": cid, "ok": e.get("tool_transport_status") == "SUCCESS",
                "output_bytes": len(str(content).encode()),
                "output_digest": e.get("result_sha256")}, repo)); seq += 1
            if cat == "repodex" and "#RDX1" in content:
                gaps, intent = gaps_from_rdx(content)
                events.append(ev(sid, seq, ts, "context_artifact_presented", {
                    "artifact_id": sha(content)[:23],
                    "artifact_kind": "rdx1_packet",
                    "producer": "repodex",
                    "tool_call_id": cid,
                    "presentation": "adaptive",
                    "content_digest": e.get("result_sha256"),
                    "content_bytes": len(str(content).encode()),
                    "metadata": {
                        "query_intent": intent or "unknown",
                        "navigation_profile": "adaptive",
                        "evidence_complete": not any(g["family"] in ("no_result", "evidence_truncated") for g in gaps),
                        "gap_signatures": gaps,
                        "packet_bytes": len(str(content).encode()),
                    }}, repo)); seq += 1
            elif cat == "source_read":
                m = re.search(r'<file-view path="([^"]+)', str(content))
                path_v = None
                if m:
                    p = m.group(1)
                    path_v = p[p.find("/src/"):] if "/src/" in p else p
                events.append(ev(sid, seq, ts, "source_observed", {
                    "path": path_v, "observation_kind": "explicit_read",
                    "tool_call_id": cid,
                    "bytes": len(str(content).encode()),
                    "content_digest": e.get("result_sha256")}, repo)); seq += 1
    events.append(ev(sid, seq, ts, "session_completed", {"reason": "replay"}, repo))
    return events


if __name__ == "__main__":
    out = []
    for f in sys.argv[1:]:
        out.extend(convert(f))
    for e in out:
        print(json.dumps(e, separators=(",", ":")))
