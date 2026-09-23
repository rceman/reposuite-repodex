#!/usr/bin/env python3
"""Bounded tests for devin-atif.py (addendum §16): session identity, token
usage, tool calls, SourceObserved, final answer, determinism. Stdlib only."""

import importlib.util
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("devin_atif", os.path.join(HERE, "devin-atif.py"))
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

CTX = {"investigation_id": "inv-1", "project_id": "p", "repository_id": "gtw",
       "repo_head": "abc", "repo_root": "/repo"}

TRAJ = {
    "session_id": "sess-1",
    "steps": [
        {"step_id": 0, "timestamp": "2026-01-01T00:00:00Z", "source": "user",
         "message": "QUESTION: x\n\nINSTRUCTIONS: y"},
        {"step_id": 1, "timestamp": "2026-01-01T00:00:01Z", "source": "agent",
         "message": "", "model_name": "m",
         "metrics": {"prompt_tokens": 100, "completion_tokens": 10},
         "tool_calls": [{"tool_call_id": "t1", "function_name": "read",
                         "arguments": {"file_path": "/repo/src/a.go"}}],
         "observation": {"results": [{"source_call_id": "t1",
             "content": '<file-view path="/repo/src/a.go" start_line="1" end_line="3">\n 1|package a\n'}]}},
        {"step_id": 2, "timestamp": "2026-01-01T00:00:02Z", "source": "agent",
         "message": "", "metrics": {"prompt_tokens": 50, "completion_tokens": 5}},
        {"step_id": 3, "timestamp": "2026-01-01T00:00:03Z", "source": "agent",
         "message": "ANSWER: it is src/a.go"},
    ],
    "final_metrics": {"total_prompt_tokens": 150, "total_completion_tokens": 15,
                      "total_steps": 3},
}


def types(events):
    return [e["type"] for e in events]


def run():
    ev = mod.from_atif(TRAJ, CTX)
    t = types(ev)
    # session identity + lifecycle
    assert ev[0]["session_id"] == "sess-1"
    assert ev[0]["investigation_id"] == "inv-1"
    assert t[0] == "session_started" and t[-1] == "session_completed"
    # token conversion: two model_call_completed, actuals sum to final_metrics
    mc = [e for e in ev if e["type"] == "model_call_completed"]
    assert len(mc) == 2
    assert sum(e["data"]["input_tokens"] for e in mc) == 150
    assert sum(e["data"]["output_tokens"] for e in mc) == 15
    assert all(not e["data"]["usage_estimated"] for e in mc)
    # tool call conversion
    assert "tool_call_started" in t and "tool_call_completed" in t
    tc = next(e for e in ev if e["type"] == "tool_call_started")
    assert tc["data"]["tool_name"] == "read" and tc["data"]["category"] == "file_read"
    # SourceObserved: read -> explicit_read, repo-relative path
    so = [e for e in ev if e["type"] == "source_observed"]
    assert len(so) == 1
    assert so[0]["data"]["path"] == "src/a.go"
    assert so[0]["data"]["observation_kind"] == "explicit_read"
    # final answer
    fa = [e for e in ev if e["type"] == "final_answer"]
    assert len(fa) == 1 and "src/a.go" in fa[0]["data"]["content"]
    # monotonic sequence + stable event_id
    assert [e["sequence"] for e in ev] == list(range(len(ev)))
    # determinism
    assert mod.from_atif(TRAJ, CTX) == ev
    # context_artifact_presented from an RDX1 packet
    t2 = json.loads(json.dumps(TRAJ))
    t2["steps"][0]["message"] = (
        "q\n<RETRIEVAL_PACKET>\n#RDX1 v1\nF 1 decl decl:/repo/internal/x.go#3 Foo\n"
        "</RETRIEVAL_PACKET>")
    ev2 = mod.from_atif(t2, CTX)
    ca = [e for e in ev2 if e["type"] == "context_artifact_presented"]
    assert len(ca) == 1
    assert ca[0]["data"]["references"][0]["path"] == "internal/x.go"
    assert ca[0]["data"]["references"][0]["rank"] == 1
    print("devin-atif adapter: all checks passed")


if __name__ == "__main__":
    run()
