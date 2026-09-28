"""Obligation-based + worktree-grounded validation.

Two independent dimensions:
  execution_status: EXECUTION_VALID | INFRA_INVALID | MODEL_PROVIDER_FAILURE | TIMEOUT
  validation_status: TASK_SUCCESS | TASK_FAILURE | NOT_EVALUATED
"""
import json, subprocess, hashlib
from pathlib import Path

EXECUTION_VALID = "EXECUTION_VALID"
INFRA_INVALID = "INFRA_INVALID"
MODEL_PROVIDER_FAILURE = "MODEL_PROVIDER_FAILURE"
TIMEOUT = "TIMEOUT"
TASK_SUCCESS = "TASK_SUCCESS"
TASK_FAILURE = "TASK_FAILURE"
NOT_EVALUATED = "NOT_EVALUATED"


def _alts(value):
    return [a.strip().lower() for a in str(value).split("|") if a.strip()]


def _contains(answer, value):
    a = answer.lower()
    return any(alt in a for alt in _alts(value))


def eval_obligation(ob, answer, worktree):
    """Evaluate one structured obligation -> {id, kind, ok, why}.

    kind:
      contains     -> answer contains value (| alternation)
      absent_claim -> answer must NOT assert `value`; correct = reports absence
                      (or simply does not assert). 'value' is what a WRONG answer
                      would claim, e.g. a go version.
      file         -> worktree/<value> exists
      marker       -> worktree/<path> contains value
    """
    kind = ob.get("kind")
    val = ob.get("value", "")
    if kind == "contains":
        ok = _contains(answer, val)
        return {"id": ob["id"], "kind": kind, "ok": ok, "why": f"answer {'contains' if ok else 'lacks'} '{val}'"}
    if kind == "absent_claim":
        # Correct iff the answer does NOT assert the claimed fact.
        import re
        asserts = bool(re.search(val, answer, re.I))
        indicates_absent = bool(re.search(r"no |not |absent|does not|doesn't|none|missing|undeclared|no go|no version", answer, re.I))
        ok = (not asserts) or (indicates_absent and not asserts)
        return {"id": ob["id"], "kind": kind, "ok": ok,
                "why": "answer correctly avoids the false claim" if ok else "answer asserts a value the source does not contain"}
    if kind == "file":
        p = Path(worktree) / val
        return {"id": ob["id"], "kind": kind, "ok": p.exists(), "why": f"{val} {'exists' if p.exists() else 'missing'} in worktree"}
    if kind == "marker":
        p = Path(worktree) / ob["path"]
        if not p.exists():
            return {"id": ob["id"], "kind": kind, "ok": False, "why": f"{ob['path']} missing"}
        ok = str(val) in p.read_text()
        return {"id": ob["id"], "kind": kind, "ok": ok, "why": f"'{val}' {'present' if ok else 'absent'} in {ob['path']}"}
    return {"id": ob.get("id"), "kind": kind, "ok": False, "why": f"unknown obligation kind {kind}"}


def worktree_proof(task, worktree):
    """For mutating tasks: prove the required change exists in the worktree.

    Returns changed_paths, per-marker predicate, a diff vs the pristine fixture,
    and (if the task declares one) a validator-owned build/test command result.
    """
    res = {"changed_paths": [], "markers": [], "diff": "", "diff_digest": None,
           "validator_command": None, "validator_exit_code": None}
    wt = Path(worktree)
    # diff vs pristine copy stored alongside
    pristine = Path(task.get("pristine_root", task["root"]))
    try:
        diff = subprocess.run(["diff", "-ruN", str(pristine), str(wt)],
                              capture_output=True, text=True, timeout=60)
        res["diff"] = diff.stdout[:200000]
        res["diff_digest"] = hashlib.sha256(diff.stdout.encode()).hexdigest()
        res["changed_paths"] = sorted({l.split()[3] for l in diff.stdout.splitlines()
                                       if l.startswith("Only in") or l.startswith("diff")})
    except Exception as e:
        res["diff_error"] = str(e)
    for ob in task.get("obligations", []):
        if ob.get("kind") == "marker":
            res["markers"].append(eval_obligation(ob, "", worktree))
    if task.get("validator_command"):
        try:
            c = subprocess.run(task["validator_command"], shell=True, cwd=worktree,
                               capture_output=True, text=True, timeout=120)
            res["validator_command"] = task["validator_command"]
            res["validator_exit_code"] = c.returncode
            res["validator_stdout"] = c.stdout[:20000]
            res["validator_stderr"] = c.stderr[:20000]
        except Exception as e:
            res["validator_error"] = str(e)
    return res


def validate(task, answer, worktree, export_ok, rejected, timed_out):
    """Return {execution_status, validation_status, obligations, worktree}."""
    if not export_ok:
        return {"execution_status": MODEL_PROVIDER_FAILURE,
                "validation_status": NOT_EVALUATED}
    if timed_out:
        return {"execution_status": TIMEOUT, "validation_status": NOT_EVALUATED}
    if rejected:
        return {"execution_status": INFRA_INVALID, "validation_status": NOT_EVALUATED}
    obs = [eval_obligation(o, answer, worktree) for o in task.get("obligations", [])]
    out = {"execution_status": EXECUTION_VALID, "obligations": obs}
    if task.get("mutating"):
        proof = worktree_proof(task, worktree)
        out["worktree"] = proof
        # success = all marker obligations present in worktree AND validator cmd ok
        mark_ok = all(m["ok"] for m in proof["markers"]) if proof["markers"] else False
        cmd_ok = proof.get("validator_exit_code", 0) == 0 if proof.get("validator_command") else True
        out["validation_status"] = TASK_SUCCESS if (mark_ok and cmd_ok) else TASK_FAILURE
    else:
        out["validation_status"] = TASK_SUCCESS if all(o["ok"] for o in obs) else TASK_FAILURE
    return out
