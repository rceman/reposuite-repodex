"""Obligation-based + worktree-grounded validation.

Two independent dimensions:
  execution_status: EXECUTION_VALID | INFRA_INVALID | MODEL_PROVIDER_FAILURE | TIMEOUT
  validation_status: TASK_SUCCESS | TASK_FAILURE | NOT_EVALUATED

Semantic obligation kinds:
  contains_text        answer contains value (| alternation)
  path_exists          worktree path exists
  source_decl          source file contains an exact declaration
  absence              source does NOT declare value; answer reports absence
  written_call         caller file contains a call to callee (direction proven)
  relation_direction   A calls B (not B calls A); reversed answer fails
  manifest_dependency  manifest file's [dependencies] contains dep
  mutation_marker      worktree file contains required source
  mutation_scope       marker present AND at required scope (e.g. top-level)
  mutation_position    marker present AND at required relative position
"""
import json, re, subprocess, hashlib
from pathlib import Path

EXECUTION_VALID = "EXECUTION_VALID"
INFRA_INVALID = "INFRA_INVALID"
MODEL_PROVIDER_FAILURE = "MODEL_PROVIDER_FAILURE"
TIMEOUT = "TIMEOUT"
TASK_SUCCESS = "TASK_SUCCESS"
TASK_FAILURE = "TASK_FAILURE"
NOT_EVALUATED = "NOT_EVALUATED"


def _alts(v):
    return [a.strip().lower() for a in str(v).split("|") if a.strip()]


def _contains(answer, v):
    a = str(answer).lower()
    return any(alt in a for alt in _alts(v))


def eval_obligation(ob, answer, worktree, pristine):
    kind, val = ob.get("kind"), ob.get("value", "")
    if kind in ("contains_text", "source_decl", "manifest_dependency"):
        ok = _contains(answer, val)
        return {"id": ob["id"], "kind": kind, "ok": ok, "why": f"answer {'contains' if ok else 'lacks'} '{val}'"}
    if kind == "absence":
        # source must not declare `value`; answer must not assert it either.
        sp = Path(worktree) / ob.get("source_path", "")
        src = sp.read_text() if sp.exists() else ""
        source_has = bool(re.search(val, src, re.I))
        asserts = bool(re.search(val, answer, re.I))
        neg = bool(re.search(r"no |not |absent|does not|doesn't|none|missing|undeclared|no go|no version", answer, re.I))
        ok = (not source_has) and (not asserts) and neg
        why = ("source lacks claim and answer reports absence" if ok else
               ("answer asserts a value the source lacks" if asserts else
                ("source unexpectedly contains the value" if source_has else "answer doesn't report absence")))
        return {"id": ob["id"], "kind": kind, "ok": ok, "why": why}
    if kind == "path_exists":
        p = Path(worktree) / val
        return {"id": ob["id"], "kind": kind, "ok": p.exists(), "why": f"{val} {'exists' if p.exists() else 'missing'}"}
    if kind == "written_call":
        # caller file contains a call-expression referencing callee -> direction.
        cp = Path(worktree) / ob["caller"]
        txt = cp.read_text() if cp.exists() else ""
        callee = re.escape(ob["callee"].split("|")[0])
        called = bool(re.search(callee + r"\s*\(", txt))
        # answer must also mention the callee to count it as stated
        stated = _contains(answer, ob["callee"])
        ok = called and stated
        return {"id": ob["id"], "kind": kind, "ok": ok,
                "why": f"{ob['caller']} {'calls' if called else 'does not call'} {ob['callee']} and answer {'states' if stated else 'omits'} it"}
    if kind == "relation_direction":
        # prove A calls B (B appears as a call in A); answer must give direction.
        fp = Path(worktree) / ob["from_path"]
        txt = fp.read_text() if fp.exists() else ""
        callee = ob["to_callee"].split("|")[0]
        a_calls_b = bool(re.search(re.escape(callee) + r"\s*\(", txt))
        # Direction in the answer: caller pkg must appear before the call verb
        # and the callee; a reversed "callee calls caller" answer must fail.
        caller_pkg = Path(ob["from_path"]).parent.name  # e.g. 'engine'
        callee_name = re.escape(callee.split(".")[0])   # e.g. 'util'
        a = str(answer).lower()
        correct_dir = bool(re.search(caller_pkg + r".{0,40}(call|use|invoke|depend|->|import).{0,40}" + callee_name, a))
        reversed_dir = bool(re.search(callee_name + r".{0,40}(call|use|invoke|depend|->|import).{0,40}" + caller_pkg, a))
        ok = a_calls_b and correct_dir and not reversed_dir
        return {"id": ob["id"], "kind": kind, "ok": ok,
                "why": f"{ob['from_path']} {'calls' if a_calls_b else 'no call to'} {callee}; answer direction {'correct' if ok else 'wrong/missing'}"}
    if kind == "mutation_marker":
        p = Path(worktree) / ob["path"]
        return _marker_result(ob, p.exists() and val in p.read_text(), ob["path"])
    if kind == "mutation_scope":
        return _scope_check(ob, worktree)
    if kind == "mutation_position":
        return _position_check(ob, worktree)
    return {"id": ob.get("id"), "kind": kind, "ok": False, "why": f"unknown kind {kind}"}


def _marker_result(ob, ok, path):
    return {"id": ob["id"], "kind": ob["kind"], "ok": ok, "why": f"marker {'present' if ok else 'absent'} in {path}"}


def _scope_check(ob, worktree):
    """marker must exist AND be at top-level scope (not inside a function)."""
    p = Path(worktree) / ob["path"]
    if not p.exists():
        return {"id": ob["id"], "kind": "mutation_scope", "ok": False, "why": "file missing"}
    lines = p.read_text().splitlines()
    for i, l in enumerate(lines):
        if ob["value"] not in l:
            continue
        # top-down: track whether line i sits inside a 'func ... {' block.
        depth = 0
        inside = False
        func_depth = None
        for j in range(i):
            s = lines[j].strip()
            if s.startswith("func ") and "{" in s:
                func_depth = depth + s.count("{") - s.count("}")
            depth += s.count("{") - s.count("}")
        # if a func opened and hasn't closed by line i -> inside function
        inside = func_depth is not None and depth >= (func_depth or 0) and depth > 0
        scope = "inside-function" if inside else "top-level"
        want = ob.get("scope", "top-level")
        ok = scope == want
        return {"id": ob["id"], "kind": "mutation_scope", "ok": ok,
                "why": f"marker is {scope}, required {want}"}
    return {"id": ob["id"], "kind": "mutation_scope", "ok": False, "why": "marker absent"}


def _position_check(ob, worktree):
    """comment/marker must be on the line immediately above `above` anchor."""
    p = Path(worktree) / ob["path"]
    if not p.exists():
        return {"id": ob["id"], "kind": "mutation_position", "ok": False, "why": "file missing"}
    lines = p.read_text().splitlines()
    for i, l in enumerate(lines):
        if ob["above"] in l:
            # walk up over blank lines/comments? require immediately above.
            j = i - 1
            # allow doc comment lines to sit between? Task says immediately above.
            if j >= 0 and ob["value"] in lines[j]:
                return {"id": ob["id"], "kind": "mutation_position", "ok": True, "why": "marker immediately above anchor"}
            return {"id": ob["id"], "kind": "mutation_position", "ok": False,
                    "why": f"line above anchor is '{lines[j][:40] if j>=0 else 'start of file'}', not marker"}
    return {"id": ob["id"], "kind": "mutation_position", "ok": False, "why": "anchor not found"}


def worktree_proof(task, worktree):
    """Prove the required change exists in the worktree."""
    res = {"changed_paths": [], "diff": "", "diff_digest": None,
           "validator_command": None, "validator_exit_code": None,
           "validator_stdout": None, "validator_stderr": None}
    wt = Path(worktree)
    pristine = Path(task["pristine_root"])
    try:
        diff = subprocess.run(["diff", "-ruN", str(pristine), str(wt)],
                              capture_output=True, text=True, timeout=60)
        res["diff"] = diff.stdout
        res["diff_digest"] = hashlib.sha256(diff.stdout.encode()).hexdigest()
        # normalize to deterministic repository-relative paths
        def _rel(pth):
            pth = pth.rsplit(" ", 1)[-1] if " " in pth else pth
            for base in (str(pristine), str(wt)):
                if pth.startswith(base.rstrip("/") + "/"):
                    return pth[len(base.rstrip("/")) + 1:]
            return pth.split("/")[-1]
        # `repo_query` is benchmark scaffolding injected by the harness (the
        # repo_query shim), not a source change — exclude it from the diff set.
        res["changed_paths"] = sorted({_rel(l.split()[-1]) for l in diff.stdout.splitlines()
                                       if l.startswith("diff ")} - {"repo_query"})
    except Exception as e:
        res["diff_error"] = str(e)
    if task.get("validator_command"):
        cmd = task["validator_command"].replace("%ROOT%", str(wt))
        try:
            c = subprocess.run(cmd, shell=True, cwd=worktree, capture_output=True,
                               text=True, timeout=120)
            res["validator_command"] = task["validator_command"]
            res["validator_exit_code"] = c.returncode
            res["validator_stdout"] = c.stdout[:20000]
            res["validator_stderr"] = c.stderr[:20000]
        except Exception as e:
            res["validator_error"] = str(e)
            res["validator_exit_code"] = -1
    return res


def validate(task, answer, worktree, export_ok, rejected, timed_out):
    if not export_ok:
        return {"execution_status": MODEL_PROVIDER_FAILURE, "validation_status": NOT_EVALUATED}
    if timed_out:
        return {"execution_status": TIMEOUT, "validation_status": NOT_EVALUATED}
    if rejected:
        return {"execution_status": INFRA_INVALID, "validation_status": NOT_EVALUATED}
    obs = [eval_obligation(o, answer, worktree, task["pristine_root"])
           for o in task.get("obligations", [])]
    out = {"execution_status": EXECUTION_VALID, "obligations": obs}
    if task.get("mutating"):
        proof = worktree_proof(task, worktree)
        out["worktree"] = proof
        has_diff = bool(proof["diff"].strip()) and len(proof["changed_paths"]) >= 1
        obs_ok = all(o["ok"] for o in obs)
        cmd_ok = proof.get("validator_exit_code", 0) == 0 if proof.get("validator_command") else True
        # every changed path must be in the allowed set; the required path must
        # itself have changed.
        allowed = set(task.get("allowed_changed_paths") or [])
        req_paths = {o["path"] for o in task.get("obligations", [])
                     if o.get("kind") in ("mutation_marker", "mutation_scope", "mutation_position") and o.get("path")}
        forbidden = sorted(p for p in proof["changed_paths"] if allowed and p not in allowed)
        req_changed = (not req_paths) or any(p in proof["changed_paths"] for p in req_paths)
        scope_ok = has_diff and not forbidden and req_changed
        out["validation_status"] = TASK_SUCCESS if (has_diff and obs_ok and cmd_ok and scope_ok) else TASK_FAILURE
        out["has_required_diff"] = has_diff
        out["changed_paths"] = proof["changed_paths"]
        out["forbidden_changed_paths"] = forbidden
        out["allowed_changed_paths"] = sorted(allowed)
    else:
        out["validation_status"] = TASK_SUCCESS if all(o["ok"] for o in obs) else TASK_FAILURE
    return out
