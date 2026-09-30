"""Aggregate session metrics -> artifact tables."""
import csv
from collections import defaultdict
from classify import classify_tool


DISCOVERY = ("repodex", "native_search", "enumeration", "source_read")


def discovery_flags(tools):
    """§40 first-discovery adherence — ordered op categories -> flags.
    `repodex_first` = the FIRST repository-discovery operation was RepoDex
    (not merely 'repodex used at some point')."""
    used = "repodex" in tools
    first_disc = next((c for c in tools if c in DISCOVERY), None)
    repodex_first = first_disc == "repodex"
    idx_rep = tools.index("repodex") if used else None
    native_before = any(c in ("native_search", "enumeration", "source_read")
                        for c in tools[:idx_rep]) if used else False
    source_after = "source_read" in tools[idx_rep:] if used else False
    return {"repodex_used": used, "repodex_first": repodex_first,
            "native_before_repodex": native_before,
            "source_read_after_repodex": source_after}


def session_row(tr):
    tools = tr.get("tool_categories", [])
    def n(c): return tools.count(c)
    fm = tr.get("final_metrics", {})
    fl = discovery_flags(tools)
    return [tr["session"], tr["task"], tr["arm"], tr["rep"],
            tr["execution_status"], tr["validation_status"], int(tr["correct"]),
            fm.get("total_prompt_tokens", ""), fm.get("total_completion_tokens", ""),
            tr.get("model_calls", ""), len(tools), n("repodex"), n("native_search"),
            n("source_read"), n("edit"), n("test_build"), n("enumeration"),
            int(fl["repodex_used"]), int(fl["repodex_first"]),
            int(fl["native_before_repodex"]), int(fl["source_read_after_repodex"]),
            tr.get("agent_session_wall_ms"), tr.get("repodex_prepare_wall_ms"),
            tr.get("combined_wall_ms"), tr.get("packet_bytes")]


HEADER = ["session", "task", "arm", "rep", "execution_status", "validation_status",
          "correct", "input_tok", "output_tok", "model_calls", "tool_calls",
          "repodex_calls", "native_search", "source_read", "edit", "test_build",
          "enumeration", "repodex_used", "repodex_first", "native_before_repodex",
          "source_read_after_repodex", "agent_wall_ms", "repodex_prepare_wall_ms",
          "combined_wall_ms", "packet_bytes"]
