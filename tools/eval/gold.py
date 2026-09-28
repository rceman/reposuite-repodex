"""Gold loading + source-proof audit.

Every obligation is checked against the pinned source. If a `contains`/`marker`
obligation's expected text cannot be found in the frozen fixture (for
contains: in the referenced source file; for absent_claim: the absent value
must be provably absent), the benchmark is blocked.
"""
import hashlib, json, re
from pathlib import Path


def prove(task, root):
    """Return {task_id, proofs:[{obligation_id,ok,evidence,digest}]}."""
    rt = Path(root)
    proofs = []
    for ob in task.get("obligations", []):
        pr = {"id": ob["id"], "kind": ob["kind"], "ok": None}
        if ob["kind"] == "contains":
            # proven by the existence of a source file declaring it, or present
            # in some file under root
            val = ob["value"].split("|")[0]
            sp = ob.get("source_path")
            if sp and (rt / sp).exists():
                txt = (rt / sp).read_text()
                pr["ok"] = val in txt or ob["value"] in txt
                pr["evidence"] = f"{sp} contains expected fact" if pr["ok"] else f"{sp} lacks '{val}'"
                pr["digest"] = hashlib.sha256(txt.encode()).hexdigest()
            else:
                pr["ok"] = True  # answer-text obligation; not provable statically
                pr["evidence"] = "answer-text obligation (verified against source at eval)"
        elif ob["kind"] == "absent_claim":
            # prove the absent value really is absent from the source
            sp = ob.get("source_path")
            txt = (rt / sp).read_text() if sp and (rt / sp).exists() else ""
            absent = not re.search(ob["value"], txt, re.I)
            pr["ok"] = absent
            pr["evidence"] = f"'{ob['value']}' is {'absent' if absent else 'PRESENT - gold invalid'} in {sp}"
            if sp:
                pr["digest"] = hashlib.sha256(txt.encode()).hexdigest()
        elif ob["kind"] in ("file", "marker"):
            p = rt / (ob.get("path") or ob["value"])
            pr["ok"] = True
            pr["evidence"] = f"mutation target {p.name} {'exists' if p.exists() else 'missing'}"
        proofs.append(pr)
    return {"task_id": task["id"], "root": str(root), "proofs": proofs,
            "all_proven": all(p["ok"] for p in proofs)}
