#!/usr/bin/env python3
"""Validate the evidence-only M1 Windows and Work Review release gate.

The runner never starts the application, WeChat, a network client, or a model.
It reads a sanitized after-gate JSON document and returns exactly one verdict:
``pass``, ``fail``, or ``blocked``.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path


BASELINE_ID = "work-review-v1.1.0-before-wechat-rag-20260805"
SOURCE_COMMIT = "500f9d2cb3027392cfcc32ad18395dfe348fb4a1"
SOURCE_MANIFEST_SHA256 = "31dd2192f602ee0b4d6f659311186d2230416e42357744ac8c57e778f20cb14a"
BASE_IDS = {*(f"BASE-{number:02d}" for number in range(1, 11)), "BASE-AUTO-FE", "BASE-AUTO-BUILD", "BASE-AUTO-RUST-CHECK", "BASE-AUTO-RUST-CLIPPY", "BASE-AUTO-RUST-TEST"}
M1_REQUIRED_AC_IDS = {
    *(f"AC-WX-{number:02d}" for number in range(1, 7)),
    "AC-PET-01",
    "AC-PET-02",
}
SCENARIO_IDS = {"success", "capture-failed", "timeout", "cancel"}
FORBIDDEN_COUNTERS = {
    "mcp", "bot", "upload", "search", "localhostApi", "agentActionTool",
    "wechatInput", "clipboardRustApi", "processStart", "network", "syntheticInput",
    "petOcr", "petProcessStart", "petNetwork", "petInput",
    "resourceOcr", "resourceProcessStart", "resourceNetwork", "resourceInput",
}
REQUIRED_COUNTERS = FORBIDDEN_COUNTERS | {"replyModelNonLoopback", "ocrBackendLocalProcess"}
SHA256 = re.compile(r"^[0-9a-f]{64}$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")


def text(value: object) -> bool:
    return isinstance(value, str) and bool(value.strip())


def sha256(value: object) -> bool:
    return isinstance(value, str) and bool(SHA256.fullmatch(value))


def commit(value: object) -> bool:
    return isinstance(value, str) and bool(COMMIT.fullmatch(value))


class Verdict:
    def __init__(self) -> None:
        self.failures: list[str] = []
        self.blockers: list[str] = []

    def fail(self, message: str) -> None:
        self.failures.append(message)

    def block(self, message: str) -> None:
        self.blockers.append(message)

    @property
    def status(self) -> str:
        if self.failures:
            return "fail"
        if self.blockers:
            return "blocked"
        return "pass"


def require_mapping(value: object, label: str, result: Verdict) -> dict[str, object]:
    if isinstance(value, dict):
        return value
    result.fail(f"{label} must be an object")
    return {}


def check_candidate(document: dict[str, object], result: Verdict) -> tuple[str | None, str | None, str | None]:
    candidate = require_mapping(document.get("candidate"), "candidate", result)
    candidate_commit = candidate.get("git_commit")
    candidate_hash = candidate.get("nsis_sha256")
    batch_id = candidate.get("batch_id")
    if not commit(candidate_commit):
        result.block("candidate.git_commit is missing or invalid")
    if not sha256(candidate_hash):
        result.block("candidate.nsis_sha256 is missing or invalid")
    if not text(batch_id):
        result.block("candidate.batch_id is missing")
    return candidate_commit if isinstance(candidate_commit, str) else None, candidate_hash if isinstance(candidate_hash, str) else None, batch_id if isinstance(batch_id, str) else None


def check_baseline(document: dict[str, object], result: Verdict) -> None:
    baseline = require_mapping(document.get("baseline"), "baseline", result)
    expected = {
        "baseline_id": BASELINE_ID,
        "source_commit": SOURCE_COMMIT,
        "source_manifest_sha256": SOURCE_MANIFEST_SHA256,
    }
    for key, value in expected.items():
        if baseline.get(key) != value:
            result.fail(f"baseline.{key} does not match the frozen before baseline")


def linked(record: dict[str, object], candidate_commit: str | None, candidate_hash: str | None, batch_id: str | None, label: str, result: Verdict) -> None:
    for key, expected in (("candidate_commit", candidate_commit), ("nsis_sha256", candidate_hash), ("batch_id", batch_id)):
        if expected is None or record.get(key) != expected:
            result.block(f"{label}.{key} does not match the candidate")


def check_automated(document: dict[str, object], candidate_commit: str | None, candidate_hash: str | None, batch_id: str | None, result: Verdict) -> set[str]:
    records = document.get("automated")
    if not isinstance(records, list) or not records:
        result.block("automated evidence is missing")
        return set()
    identifiers: set[str] = set()
    for index, value in enumerate(records):
        record = require_mapping(value, f"automated[{index}]", result)
        identifier = record.get("id")
        if not text(identifier) or identifier in identifiers:
            result.fail(f"automated[{index}] has a missing or duplicate id")
        else:
            identifiers.add(identifier)
        if not text(record.get("command")) or not isinstance(record.get("exit_code"), int) or not sha256(record.get("log_sha256")):
            result.block(f"automated[{index}] lacks a command, exit code, or log hash")
        linked(record, candidate_commit, candidate_hash, batch_id, f"automated[{index}]", result)
        status = record.get("status")
        if status == "fail":
            result.fail(f"automated[{index}] reported fail")
        elif status == "blocked":
            result.block(f"automated[{index}] reported blocked")
        elif status == "known-upstream-failure":
            issue = record.get("known_issue")
            if issue != "UPSTREAM-RUST-001" or record.get("before_status") != "fail":
                result.fail(f"automated[{index}] has an invalid known upstream failure attribution")
            else:
                result.fail("UPSTREAM-RUST-001 remains a release blocker")
        elif status != "pass" or record.get("exit_code") != 0:
            result.fail(f"automated[{index}] must be a zero-exit pass")
    return identifiers


def check_matrix(document: dict[str, object], evidence_ids: set[str], result: Verdict) -> None:
    rows = document.get("after_matrix")
    if not isinstance(rows, list):
        result.fail("after_matrix must be a list")
        return
    identifiers: set[str] = set()
    for index, value in enumerate(rows):
        row = require_mapping(value, f"after_matrix[{index}]", result)
        identifier = row.get("id")
        if not text(identifier) or identifier in identifiers:
            result.fail(f"after_matrix[{index}] has a missing or duplicate id")
            continue
        identifiers.add(identifier)
        if row.get("before_ref") != identifier:
            result.fail(f"after_matrix[{index}].before_ref must equal its stable id")
        status = row.get("status")
        if str(identifier).startswith(("AC-KB-", "AC-RAG-")):
            if status != "conditional-not-enabled":
                result.fail(f"after_matrix[{index}] must keep an M2-only row conditional-not-enabled")
        elif status == "fail":
            result.fail(f"after_matrix[{index}] reported fail")
        elif status == "blocked":
            result.block(f"after_matrix[{index}] reported blocked")
        elif status == "conditional-not-enabled":
            result.fail(f"after_matrix[{index}] uses conditional-not-enabled outside M2")
        elif status != "pass":
            result.fail(f"after_matrix[{index}] has an invalid status")
        elif not isinstance(row.get("evidence_ids"), list) or not row["evidence_ids"]:
            result.block(f"after_matrix[{index}] pass lacks evidence")
        elif not set(row["evidence_ids"]).issubset(evidence_ids):
            result.block(f"after_matrix[{index}] cites unknown automated evidence")
    missing = BASE_IDS - identifiers
    if missing:
        result.block(f"after_matrix is missing base rows: {', '.join(sorted(missing))}")
    missing_m1_ac = M1_REQUIRED_AC_IDS - identifiers
    if missing_m1_ac:
        result.block(f"after_matrix is missing required M1 AC rows: {', '.join(sorted(missing_m1_ac))}")
    required_ac = document.get("required_ac_ids")
    if not isinstance(required_ac, list) or any(not text(item) for item in required_ac):
        result.fail("required_ac_ids must be a list of stable AC ids")
    elif set(required_ac) != M1_REQUIRED_AC_IDS or len(required_ac) != len(M1_REQUIRED_AC_IDS):
        result.block("required_ac_ids must declare the fixed M1 AC set")


def check_windows(document: dict[str, object], candidate_commit: str | None, candidate_hash: str | None, batch_id: str | None, result: Verdict) -> None:
    windows = require_mapping(document.get("windows"), "windows", result)
    linked(windows, candidate_commit, candidate_hash, batch_id, "windows", result)
    for key in ("host", "wechat_profile_fingerprint"):
        if not text(windows.get(key)):
            result.block(f"windows.{key} is missing")
    scenarios = windows.get("scenarios")
    if not isinstance(scenarios, list):
        result.block("windows.scenarios is missing")
        return
    found: set[str] = set()
    for index, value in enumerate(scenarios):
        scenario = require_mapping(value, f"windows.scenarios[{index}]", result)
        identifier = scenario.get("id")
        if not text(identifier) or identifier in found:
            result.fail(f"windows.scenarios[{index}] has a missing or duplicate id")
            continue
        found.add(identifier)
        if scenario.get("status") == "fail":
            result.fail(f"windows scenario {identifier} failed")
        elif scenario.get("status") != "pass":
            result.block(f"windows scenario {identifier} is not passed")
        if scenario.get("focus_unchanged") is not True or scenario.get("overlay_restored") is not True:
            result.fail(f"windows scenario {identifier} did not preserve focus or restore the overlay")
        expected_calls = 1 if identifier == "success" else 0
        if scenario.get("model_calls") != expected_calls:
            result.fail(f"windows scenario {identifier} has an invalid model call count")
    missing = SCENARIO_IDS - found
    if missing:
        result.block(f"windows is missing scenarios: {', '.join(sorted(missing))}")


def check_counters(document: dict[str, object], candidate_commit: str | None, candidate_hash: str | None, batch_id: str | None, result: Verdict) -> None:
    counters = require_mapping(document.get("capability_counters"), "capability_counters", result)
    linked(counters, candidate_commit, candidate_hash, batch_id, "capability_counters", result)
    values = counters.get("counts")
    if not isinstance(values, dict):
        result.block("capability_counters.counts is missing")
        return
    missing = REQUIRED_COUNTERS - set(values)
    if missing:
        result.block(f"capability counters are missing: {', '.join(sorted(missing))}")
    for key, value in values.items():
        if not isinstance(value, int) or value < 0:
            result.fail(f"capability counter {key} is not a non-negative integer")
    if values.get("replyModelNonLoopback", 0) > 1:
        result.fail("replyModelNonLoopback exceeds one request")
    for key in FORBIDDEN_COUNTERS:
        if values.get(key, 0) != 0:
            result.fail(f"forbidden capability {key} was used")


def check_assets(document: dict[str, object], candidate_commit: str | None, candidate_hash: str | None, batch_id: str | None, result: Verdict) -> None:
    ledger = require_mapping(document.get("asset_ledger_review"), "asset_ledger_review", result)
    linked(ledger, candidate_commit, candidate_hash, batch_id, "asset_ledger_review", result)
    if ledger.get("status") == "fail":
        result.fail("asset ledger review failed")
    elif ledger.get("status") != "pass":
        result.block("asset ledger review is not passed")
    if not isinstance(ledger.get("evidence_ids"), list) or not ledger["evidence_ids"]:
        result.block("asset ledger review has no evidence")


def validate(document: object) -> Verdict:
    result = Verdict()
    root = require_mapping(document, "after-gate document", result)
    if root.get("schema_version") != 1:
        result.fail("schema_version must be 1")
    if not text(root.get("gate_id")):
        result.fail("gate_id is missing")
    check_baseline(root, result)
    candidate_commit, candidate_hash, batch_id = check_candidate(root, result)
    evidence_ids = check_automated(root, candidate_commit, candidate_hash, batch_id, result)
    check_matrix(root, evidence_ids, result)
    check_windows(root, candidate_commit, candidate_hash, batch_id, result)
    check_counters(root, candidate_commit, candidate_hash, batch_id, result)
    check_assets(root, candidate_commit, candidate_hash, batch_id, result)
    declared = root.get("verdict")
    if declared not in {"pass", "fail", "blocked"}:
        result.fail("verdict must be pass, fail, or blocked")
    elif declared != result.status:
        result.fail(f"declared verdict {declared} does not match computed {result.status}")
    return result


def run_file(path: Path, verbose: bool) -> str:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        print(f"M1_RELEASE_GATE: fail")
        if verbose:
            print(f"detail: cannot read {path}: {error}")
        return "fail"
    result = validate(document)
    print(f"M1_RELEASE_GATE: {result.status}")
    if verbose:
        for message in result.failures + result.blockers:
            print(f"detail: {message}")
    return result.status


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project-root", type=Path, default=Path.cwd())
    parser.add_argument("--input", type=Path, help="sanitized after-gate JSON (defaults to the baseline document)")
    parser.add_argument("--verbose", action="store_true")
    arguments = parser.parse_args()
    path = arguments.input or arguments.project_root / "desktop/docs/baselines/work-review-m1-after-gate.json"
    status = run_file(path, arguments.verbose)
    return {"pass": 0, "fail": 1, "blocked": 2}[status]


if __name__ == "__main__":
    sys.exit(main())
