"""Aggregate session metrics -> artifact tables."""
import csv
from collections import defaultdict
from classify import classify_tool


def session_row(tr):
    tools = tr.get("tool_categories", [])
    def n(c): return tools.count(c)
    fm = tr.get("final_metrics", {})
    return [tr["session"], tr["task"], tr["arm"], tr["rep"],
            tr["execution_status"], tr["validation_status"], int(tr["correct"]),
            fm.get("total_prompt_tokens", ""), fm.get("total_completion_tokens", ""),
            tr.get("model_calls", ""), len(tools), n("repodex"), n("native_search"),
            n("source_read"), n("edit"), n("test_build"), n("enumeration"),
            tr.get("agent_session_wall_ms"), tr.get("repodex_prepare_wall_ms"),
            tr.get("combined_wall_ms"), tr.get("packet_bytes")]


HEADER = ["session", "task", "arm", "rep", "execution_status", "validation_status",
          "correct", "input_tok", "output_tok", "model_calls", "tool_calls",
          "repodex_calls", "native_search", "source_read", "edit", "test_build",
          "enumeration", "agent_wall_ms", "repodex_prepare_wall_ms",
          "combined_wall_ms", "packet_bytes"]
