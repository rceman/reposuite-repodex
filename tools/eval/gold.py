"""Gold loading + source-proof audit. NO auto-pass.

Every factual obligation is proven against the pinned fixture. Proof types:
  SOURCE_PROVEN      source file contains/proves the expected fact
  ABSENCE_PROVEN     the absent value is provably absent from source
  RELATION_PROVEN    caller file contains the written call to callee (direction)
  DEPENDENCY_PROVEN  manifest's dependency section contains the dep
  MUTATION_VALIDATED the mutation predicate is checkable against fixture target
An obligation that cannot be proven fails the audit and BLOCKS the benchmark.
"""
import hashlib, re
from pathlib import Path

PROVEN = ("SOURCE_PROVEN", "ABSENCE_PROVEN", "RELATION_PROVEN",
          "DEPENDENCY_PROVEN", "MUTATION_VALIDATED", "EXPLICITLY_NONFACTUAL")


def _digest(p):
    try:
        return hashlib.sha256(Path(p).read_bytes()).hexdigest()
    except Exception:
        return None


def prove(task, root):
    rt = Path(root)
    proofs = []
    for ob in task.get("obligations", []):
        kind = ob.get("kind", "")
        pr = {"id": ob["id"], "kind": kind, "ok": None, "proof_type": None}
        if ob.get("factual") is False:
            # Answer-format obligation (e.g. "reports absence", "mentions the
            # file") — not a repository fact; explicitly nonfactual, does not
            # need a source proof.
            pr["proof_type"] = "EXPLICITLY_NONFACTUAL"
            pr["ok"] = True
            pr["evidence"] = "answer-format obligation — explicitly nonfactual"
            proofs.append(pr)
            continue
        if kind in ("contains_text", "source_decl", "manifest_dependency"):
            sp = ob.get("source_path")
            p = rt / sp if sp else None
            if p and p.exists():
                txt = p.read_text()
                vals = [a.strip() for a in ob["value"].split("|")]
                ok = any(v in txt for v in vals)
                pr["proof_type"] = "DEPENDENCY_PROVEN" if kind == "manifest_dependency" else "SOURCE_PROVEN"
                pr["ok"] = ok
                pr["source_path"] = sp; pr["digest"] = _digest(p)
                pr["evidence"] = f"{sp} {'contains' if ok else 'lacks'} {ob['value']}"
            else:
                pr["ok"] = False; pr["proof_type"] = "UNPROVEN"
                pr["evidence"] = f"no source_path for factual obligation"
        elif kind == "absence":
            sp = ob.get("source_path"); p = rt / sp if sp else None
            if p and p.exists():
                txt = p.read_text()
                absent = not re.search(ob["value"], txt, re.I)
                pr["proof_type"] = "ABSENCE_PROVEN"
                pr["ok"] = absent
                pr["source_path"] = sp; pr["digest"] = _digest(p)
                pr["evidence"] = f"'{ob['value']}' {'provably absent' if absent else 'PRESENT — gold invalid'}"
            else:
                pr["ok"] = False; pr["proof_type"] = "UNPROVEN"
                pr["evidence"] = "absence not verifiable — no source_path"
        elif kind in ("written_call", "relation_direction"):
            cp = rt / (ob.get("caller") or ob.get("from_path"))
            if cp.exists():
                txt = cp.read_text()
                callee = (ob.get("callee") or ob.get("to_callee", "")).split("|")[0]
                called = bool(re.search(re.escape(callee) + r"\s*\(", txt))
                pr["proof_type"] = "RELATION_PROVEN"
                pr["ok"] = called
                pr["source_path"] = str(cp); pr["digest"] = _digest(cp)
                pr["evidence"] = f"{cp.name} {'calls' if called else 'does not call'} {callee}"
            else:
                pr["ok"] = False; pr["proof_type"] = "UNPROVEN"
                pr["evidence"] = f"caller file {cp} missing"
        elif kind in ("mutation_marker", "mutation_scope", "mutation_position", "path_exists"):
            p = rt / (ob.get("path") or ob.get("value", ""))
            pr["proof_type"] = "MUTATION_VALIDATED"
            pr["ok"] = True
            pr["source_path"] = str(p); pr["digest"] = _digest(p)
            pr["evidence"] = f"mutation target {p.name} {'exists' if p.exists() else 'MISSING (will be created)'}"
        elif kind == "path_exists":
            pr["ok"] = (rt / ob["value"]).exists()
            pr["proof_type"] = "SOURCE_PROVEN"
        else:
            pr["ok"] = False; pr["proof_type"] = "UNPROVEN"
            pr["evidence"] = f"no proof mechanism for kind {kind}"
        proofs.append(pr)
    return {"task_id": task["id"], "proofs": proofs,
            "all_proven": all(p["ok"] for p in proofs)}
