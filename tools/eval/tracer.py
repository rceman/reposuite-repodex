"""Full-fidelity trace export from a devin session export.

Preserves every observable event: full tool args, full tool results,
untruncated visible messages, per-call token metrics, timestamps.
Hidden reasoning_content is stripped. No 2000-char caps.
"""
import hashlib
from classify import classify_tool_ops, is_rejected_result, exit_code_of

SECRET_KEYS = ("token", "bearer", "apikey", "authorization", "password", "secret", "cookie")


def _clean(o):
    if isinstance(o, dict):
        return {k: _clean(v) for k, v in o.items()
                if k != "reasoning_content" and not any(p in k.lower() for p in SECRET_KEYS)}
    if isinstance(o, list):
        return [_clean(v) for v in o]
    return o


def extract(export):
    """Return (events, summary)."""
    evs = []
    rejected = 0
    model_calls = 0
    tools = []
    for i, s in enumerate(export.get("steps", [])):
        base = {"seq": i, "src": s.get("source"), "ts": s.get("timestamp")}
        if s.get("message"):
            evs.append({**base, "ev": "MODEL_VISIBLE_RESPONSE", "text": s["message"]})
        for tc in s.get("tool_calls", []) or []:
            if not isinstance(tc, dict):
                continue
            cats = classify_tool_ops(tc.get("function_name"), tc.get("arguments"))
            tools.extend(cats)  # §39: one category per executed operation
            evs.append({**base, "ev": "TOOL_REQUEST", "id": tc.get("tool_call_id"),
                        "tool": tc.get("function_name"), "category": cats[0],
                        "categories": cats,
                        "args": _clean(tc.get("arguments"))})
        obs = s.get("observation") or {}
        for res in obs.get("results", []) or []:
            content = res.get("content")
            st = "SUCCESS"
            if is_rejected_result(content):
                st = "REJECTED"; rejected += 1
            evs.append({**base, "ev": "TOOL_RESULT", "id": res.get("source_call_id"),
                        "tool_transport_status": st, "process_exit_code": exit_code_of(content),
                        "content": _clean(content),
                        "result_sha256": hashlib.sha256(str(content).encode()).hexdigest()})
        m = s.get("metrics") or {}
        if m:
            model_calls += 1
            evs.append({**base, "ev": "MODEL_TOKENS", "in": m.get("prompt_tokens"),
                        "out": m.get("completion_tokens"), "cached": m.get("cached_tokens")})
    return evs, {"rejected": rejected, "model_calls": model_calls, "tools": tools}


def last_answer(export):
    a = ""
    for s in export.get("steps", []):
        if s.get("source") == "agent" and s.get("message", "").strip():
            a = s["message"]
    return a
