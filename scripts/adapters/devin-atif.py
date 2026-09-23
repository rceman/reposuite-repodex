#!/usr/bin/env python3
"""Devin ATIF -> canonical reposuite.agent-event.v1 converter.

Prototype/test/replay adapter. NOT part of RepoDex runtime semantics (see
scripts/adapters/README.md). RepoDex ingests only canonical AgentEvent v1 —
this script produces that canonical JSONL from a native ATIF trajectory.

Usage:
  python3 scripts/adapters/devin-atif.py --input traj.json --output ev.jsonl
  cat traj.json | python3 scripts/adapters/devin-atif.py > ev.jsonl

Options supply canonical identity (benchmark replay):
  --investigation-id --project-id --repository-id --repo-head --repo-root
"""

import argparse
import hashlib
import json
import re
import sys

SCHEMA = "reposuite.agent-event.v1"
ADAPTER = "devin-atif"
ADAPTER_VERSION = "1"


def sha(s):
    return hashlib.sha256(s.encode("utf-8")).hexdigest()


def tool_category(name):
    """Native tool name -> harness-neutral category bucket."""
    if name in ("grep", "find_file_by_name", "find", "glob", "search", "read_many"):
        return "search"
    if name in ("read", "read_file"):
        return "file_read"
    if name in ("ls", "list_dir", "list_files"):
        return "directory_list"
    if name in ("git", "git_log", "git_blame", "git_diff", "git_show"):
        return "git_inspection"
    if name in ("exec", "bash", "shell", "run", "sh"):
        return "shell"
    if name in ("write", "edit", "edit_file", "apply_patch", "notebook_edit"):
        return "other_repository_tool"
    if name in (
        "web_search", "webfetch", "browser_preview", "ask_user_question",
        "mcp_call_tool", "mcp_list_tools", "run_subagent", "skill", "todo_write",
    ):
        return "non_repository_tool"
    return "other_repository_tool"


def to_repo_path(abs_path, repo_root):
    """Absolute -> repo-relative; None for external/non-repo (never fabricate)."""
    if not repo_root:
        return None
    root = repo_root.rstrip("/")
    if abs_path.startswith(root):
        rel = abs_path[len(root):].lstrip("/")
        if rel and ".." not in rel:
            return rel
    return None


def parse_file_view(content):
    """Parse `<file-view path=.. start_line=.. end_line=..>` header of `read`."""
    hdr = content.split("\n", 1)[0]
    if "<file-view" not in hdr:
        return None
    def attr(k):
        m = re.search(re.escape(k) + r'="([^"]*)"', hdr)
        return m.group(1) if m else None
    path = attr("path")
    if not path:
        return None
    sl = attr("start_line")
    el = attr("end_line")
    return (path, int(sl) if sl and sl.isdigit() else None,
            int(el) if el and el.isdigit() else None)


def parse_rdx1_packet(text):
    """Parse an RDX1 <RETRIEVAL_PACKET> block -> (references, block)."""
    s = text.find("<RETRIEVAL_PACKET>")
    if s < 0:
        return None
    e = text.find("</RETRIEVAL_PACKET>")
    block = text[s:e if e >= 0 else len(text)]
    refs = []
    for line in block.splitlines():
        l = line.strip()
        if not l.startswith("F "):
            continue
        parts = l[2:].split(" ", 3)
        if len(parts) < 3:
            continue
        rank = int(parts[0]) if parts[0].isdigit() else None
        kind = parts[1]
        key = parts[2]
        label = parts[3] if len(parts) > 3 else ""
        path = key.split(":", 1)[-1].split("#", 1)[0] if ":" in key else key
        if not path:
            continue
        refs.append({
            "reference_kind": kind,
            "path": path,
            "entity": label.strip('"') or None,
            "rank": rank,
            "relation": "presented",
        })
    return (refs, block) if refs else None


def emit_source_observed(out, sid, inv, ts, tool_name, args, tool_call_id,
                         content, seq, repo_root, mk):
    """read -> explicit_read; grep content -> search_snippet per file."""
    if tool_name in ("read", "read_file"):
        fv = parse_file_view(content)
        if fv:
            path, sl, el = fv
            mk(out, sid, inv, ts, seq, "source_observed", {
                "path": to_repo_path(path, repo_root),
                "observation_kind": "explicit_read",
                "tool_call_id": tool_call_id,
                "bytes": len(content),
                "line_start": sl, "line_end": el,
                "content_digest": "sha256:" + sha(content),
            })
            return seq + 1
    elif tool_name in ("grep", "search"):
        if (args or {}).get("output_mode") != "content":
            return seq
        per_file = []  # [path, bytes, min_ln, max_ln]
        for line in content.splitlines():
            if line.startswith("--"):
                m = re.search(r" matches in ", line)
                if m:
                    per_file.append([line[m.end():].strip(), 0, None, None])
                    continue
            if per_file:
                t = line.lstrip()
                digits = ""
                for ch in t:
                    if ch.isdigit():
                        digits += ch
                    else:
                        break
                if digits and t[len(digits):len(digits)+1] in ("|", "-"):
                    ln = int(digits)
                    per_file[-1][1] += len(line)
                    per_file[-1][2] = ln if per_file[-1][2] is None else min(per_file[-1][2], ln)
                    per_file[-1][3] = ln if per_file[-1][3] is None else max(per_file[-1][3], ln)
        for path, by, mn, mx in per_file:
            if by == 0:
                continue
            mk(out, sid, inv, ts, seq, "source_observed", {
                "path": to_repo_path(path, repo_root),
                "observation_kind": "search_snippet",
                "tool_call_id": tool_call_id, "bytes": by,
                "line_start": mn, "line_end": mx,
            })
            seq += 1
    return seq


def from_atif(traj, ctx):
    sid = traj.get("session_id")
    if not sid:
        raise ValueError("missing session_id")
    steps = traj.get("steps")
    if not isinstance(steps, list):
        raise ValueError("missing steps")
    first_ts = (steps[0].get("timestamp") if steps else None) or "1970-01-01T00:00:00Z"
    last_ts = (steps[-1].get("timestamp") if steps else None) or first_ts
    model = next((s.get("model_name") for s in steps if s.get("model_name")), None)
    inv = ctx.get("investigation_id")
    out = []

    def mk(o, sid, inv, ts, seq, etype, data):
        o.append({
            "schema": SCHEMA,
            "event_id": f"{sid}:{seq}",
            "session_id": sid,
            "investigation_id": inv,
            "project_id": ctx.get("project_id"),
            "repository_id": ctx.get("repository_id"),
            "repo_head": ctx.get("repo_head"),
            "sequence": seq,
            "timestamp": ts,
            "source": {"runtime": "devin", "adapter": ADAPTER,
                       "adapter_version": ADAPTER_VERSION, "native_session_id": sid},
            "type": etype,
            "data": data,
        })

    seq = 0
    mk(out, sid, inv, first_ts, seq, "session_started",
       {"model": model, "repository": ctx.get("repository_id"),
        "repo_head": ctx.get("repo_head"), "task": inv})
    seq += 1

    # first user message -> agent_message (+ optional context_artifact_presented)
    user_msg = None
    for s in steps:
        if s.get("source") == "user":
            user_msg = s.get("message") or ""
            ts = s.get("timestamp") or first_ts
            mk(out, sid, inv, ts, seq, "agent_message",
               {"role": "user", "content": user_msg, "content_bytes": len(user_msg),
                "content_digest": "sha256:" + sha(user_msg)})
            seq += 1
            break

    # context_artifact_presented from a <RETRIEVAL_PACKET> in the user prompt
    if user_msg and "<RETRIEVAL_PACKET>" in user_msg:
        pk = parse_rdx1_packet(user_msg)
        if pk:
            refs, block = pk
            root = (ctx.get("repo_root") or "").rstrip("/")
            for r in refs:
                if r.get("path") and root and r["path"].startswith(root):
                    r["path"] = r["path"][len(root):].lstrip("/")
            mk(out, sid, inv, first_ts, seq, "context_artifact_presented", {
                "artifact_id": sha(block)[:16],
                "artifact_kind": "rdx1_packet",
                "producer": "repodex",
                "content_digest": "sha256:" + sha(block),
                "content_bytes": len(block),
                "presented_at": first_ts,
                "references": refs,
            })
            seq += 1

    final = None
    for s in steps:
        ts = s.get("timestamp") or first_ts
        is_agent = s.get("source") == "agent"
        step_id = s.get("step_id", 0)
        if is_agent and s.get("metrics") is not None:
            mc = f"mc-{step_id}"
            m = s["metrics"]
            mk(out, sid, inv, ts, seq, "model_call_started",
               {"model_call_id": mc, "model": model})
            seq += 1
            mk(out, sid, inv, ts, seq, "model_call_completed", {
                "model_call_id": mc,
                "model": s.get("model_name") or model,
                "input_tokens": m.get("prompt_tokens"),
                "output_tokens": m.get("completion_tokens"),
                "cached_input_tokens": m.get("cached_tokens"),
                "status": "ok", "usage_estimated": False})
            seq += 1
        obs = (s.get("observation") or {}).get("results")
        for tc in s.get("tool_calls") or []:
            tid = tc.get("tool_call_id", "")
            name = tc.get("function_name", "unknown")
            args = tc.get("arguments")
            mk(out, sid, inv, ts, seq, "tool_call_started", {
                "tool_call_id": tid, "tool_name": name,
                "category": tool_category(name), "arguments": args})
            seq += 1
            for r in obs or []:
                if r.get("source_call_id") == tid:
                    content = r.get("content", "")
                    mk(out, sid, inv, ts, seq, "tool_call_completed", {
                        "tool_call_id": tid, "ok": True, "status": "ok",
                        "output_bytes": len(content),
                        "output_digest": "sha256:" + sha(content)})
                    seq += 1
                    seq = emit_source_observed(out, sid, inv, ts, name, args,
                                               tid, content, seq,
                                               ctx.get("repo_root"), mk)
        if is_agent and (s.get("message") or "").strip():
            final = (ts, s["message"])

    if final:
        ts, text = final
        mk(out, sid, inv, ts, seq, "final_answer",
           {"content": text, "content_bytes": len(text),
            "content_digest": "sha256:" + sha(text)})
        seq += 1
    fm = traj.get("final_metrics") or {}
    mk(out, sid, inv, last_ts, seq, "session_completed",
       {"reason": "runtime_ended", "total_steps": fm.get("total_steps")})
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--input")
    ap.add_argument("--output")
    ap.add_argument("--investigation-id")
    ap.add_argument("--project-id")
    ap.add_argument("--repository-id")
    ap.add_argument("--repo-head")
    ap.add_argument("--repo-root")
    a = ap.parse_args()
    raw = open(a.input).read() if a.input else sys.stdin.read()
    traj = json.loads(raw)
    events = from_atif(traj, {
        "investigation_id": a.investigation_id,
        "project_id": a.project_id,
        "repository_id": a.repository_id,
        "repo_head": a.repo_head,
        "repo_root": a.repo_root,
    })
    text = "".join(json.dumps(e) + "\n" for e in events)
    if a.output:
        open(a.output, "w").write(text)
    else:
        sys.stdout.write(text)


if __name__ == "__main__":
    main()
