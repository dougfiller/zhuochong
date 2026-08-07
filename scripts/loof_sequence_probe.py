#!/usr/bin/env python3
"""Aich8 wrapper for the shared, read-only LOOF sequence probe."""

from __future__ import annotations

import importlib.util
import json
import re
import sys
from pathlib import Path


_SHARED_PROBE = Path("/Users/sky/aich7-keyanzhushou/scripts/loof_sequence_probe.py")
_SPEC = importlib.util.spec_from_file_location("_shared_loof_sequence_probe", _SHARED_PROBE)
if _SPEC is None or _SPEC.loader is None:
    raise RuntimeError("shared LOOF sequence probe is unavailable")
_PROBE = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_PROBE)
_SHARED_PARSE_ROLLOUT = _PROBE.parse_rollout

_RESULT_RE = re.compile(r"^LOOF_SEQUENCE_RESULT\s+(\{.*\})\s*$")
_MEMORY_CITATION_BEGIN = "<oai-mem-citation>"
_MEMORY_CITATION_END = "</oai-mem-citation>"


def parse_receipt(text: object) -> dict | None:
    """Accept one receipt followed only by the mandatory memory citation."""
    if not isinstance(text, str):
        return None
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    matches = [(index, _RESULT_RE.fullmatch(line)) for index, line in enumerate(lines)]
    matches = [(index, match) for index, match in matches if match]
    if len(matches) != 1:
        return None
    index, match = matches[0]
    trailing = lines[index + 1 :]
    if trailing and not (
        trailing[0] == _MEMORY_CITATION_BEGIN
        and trailing[-1] == _MEMORY_CITATION_END
        and trailing.count(_MEMORY_CITATION_BEGIN) == 1
        and trailing.count(_MEMORY_CITATION_END) == 1
    ):
        return None
    try:
        value = json.loads(match.group(1))
    except ValueError:
        return None
    return value if isinstance(value, dict) else None


def _superseded_active_turns(path: object, active_turn_ids: list[str], latest_turn_id: str) -> tuple[list[str], str | None]:
    """Return older unfinished turns superseded by the latest completed turn.

    Codex serializes work within a thread, but an interrupted turn may lack its
    own ``turn_aborted`` record.  A later ``task_complete`` for the same thread
    is then the authoritative terminal record; retaining that older turn as
    active would indefinitely block the outer sequence.
    """
    active = set(active_turn_ids)
    started_at: dict[str, int] = {}
    completed_at: int | None = None
    completed_message: str | None = None

    try:
        with Path(path).open("r", encoding="utf-8") as stream:
            for position, line in enumerate(stream, 1):
                if not line.endswith("\n"):
                    return [], None
                try:
                    record = json.loads(line)
                except ValueError:
                    return [], None
                if not isinstance(record, dict):
                    return [], None
                if record.get("type") != "event_msg":
                    continue
                payload = record.get("payload")
                if not isinstance(payload, dict):
                    continue
                event_type = payload.get("type")
                turn_id = payload.get("turn_id")
                if event_type == "task_started" and turn_id in active:
                    started_at[turn_id] = position
                elif event_type == "task_complete" and turn_id == latest_turn_id:
                    completed_at = position
                    message = payload.get("last_agent_message")
                    completed_message = message if isinstance(message, str) else None
    except (OSError, UnicodeError):
        return [], None

    if completed_at is None or any(started_at.get(turn_id, completed_at) >= completed_at for turn_id in active):
        return [], None
    return sorted(active), completed_message


def parse_rollout(path: object) -> dict:
    """Apply the shared parser, then reconcile provably superseded old turns."""
    result = _SHARED_PARSE_ROLLOUT(path)
    active_turn_ids = result.get("active_turn_ids")
    latest_turn = result.get("latest_turn")
    if not isinstance(active_turn_ids, list) or not isinstance(latest_turn, dict):
        return result
    latest_turn_id = latest_turn.get("id")
    if latest_turn.get("status") != "completed" or not isinstance(latest_turn_id, str):
        return result

    superseded, final_text = _superseded_active_turns(path, active_turn_ids, latest_turn_id)
    if not superseded:
        return result

    result["active_turn_ids"] = [turn_id for turn_id in active_turn_ids if turn_id not in superseded]
    result["superseded_turn_ids"] = superseded
    result["turn_statuses"] = [
        "superseded" if status == "inProgress" else status
        for status in result.get("turn_statuses", [])
    ]
    if not result["active_turn_ids"] and isinstance(final_text, str):
        result["receipt"] = parse_receipt(final_text)
    result["terminal"] = bool(
        latest_turn.get("status") == "completed"
        and not result["active_turn_ids"]
        and not result.get("malformed_lines")
        and not result.get("incomplete_tail")
        and not result.get("changed_during_read")
    )
    return result


_PROBE.parse_receipt = parse_receipt
_PROBE.parse_rollout = parse_rollout

if __name__ == "__main__":
    sys.exit(_PROBE.main())
