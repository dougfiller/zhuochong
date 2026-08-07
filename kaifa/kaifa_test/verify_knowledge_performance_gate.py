#!/usr/bin/env python3
"""Validate the auditable Windows knowledge performance gate without probing a system."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from datetime import datetime
from pathlib import Path

METRICS = {
    "firstIndexMs": "firstIndexMsMax",
    "knowledgeSqliteBytes": "knowledgeSqliteBytesMax",
    "derivedIndexBytes": "derivedIndexBytesMax",
    "peakWorkingSetBytes": "peakWorkingSetBytesMax",
    "queryP50Ms": "queryP50MsMax",
    "queryP95Ms": "queryP95MsMax",
    "workRecordSchedulerDriftP95Ms": "workRecordSchedulerDriftP95MsMax",
    "uiInputLatencyP95Ms": "uiInputLatencyP95MsMax",
}
SHA256 = re.compile(r"^[0-9a-f]{64}$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
DEFAULT_PASS_REQUIREMENTS = {
    "target_windows",
    "frozen_dataset",
    "preapproved_thresholds",
    "observed_metrics",
    "raw_evidence",
}


def fail(reason: str, code: int = 1) -> None:
    print(f"KNOWLEDGE_PERFORMANCE_GATE: {'blocked' if code == 2 else 'fail'} reason={reason}")
    raise SystemExit(code)


def positive_int(value: object) -> bool:
    return type(value) is int and value > 0


def parse_time(value: object) -> datetime:
    if not isinstance(value, str) or not value.endswith(("Z", "+00:00")):
        fail("invalid-rfc3339-time")
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        fail("invalid-rfc3339-time")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-root", default=".")
    parser.add_argument("--gate", default="desktop/docs/performance/knowledge-performance-gate-v1.json")
    args = parser.parse_args()
    root = Path(args.project_root).resolve()
    gate_path = Path(args.gate)
    if not gate_path.is_absolute():
        gate_path = root / gate_path
    try:
        gate = json.loads(gate_path.read_text())
    except (OSError, json.JSONDecodeError):
        fail("unreadable-json")
    if gate.get("schemaVersion") != 1 or gate.get("gateId") != "knowledge-performance-gate-v1":
        fail("schema-or-id")
    verdict = gate.get("verdict")
    if verdict not in {"not_run", "blocked", "fail", "pass"}:
        fail("invalid-verdict")
    if verdict != "pass":
        blockers = gate.get("blockers")
        if not isinstance(blockers, list) or not blockers or not all(isinstance(item, str) and item for item in blockers):
            fail("non-pass-without-blockers")
        fail(f"verdict-{verdict}", 2 if verdict in {"not_run", "blocked"} else 1)

    defaults = gate.get("defaultPassRequirements", [])
    if not isinstance(defaults, list) or any(not isinstance(item, str) or not item for item in defaults):
        fail("default-pass-requirements-shape")
    if len(defaults) != len(set(defaults)) or not set(defaults).issubset(DEFAULT_PASS_REQUIREMENTS):
        fail("default-pass-requirements-unknown-or-duplicate")
    if defaults:
        authorization = gate.get("policyAuthorization")
        if set(defaults) != DEFAULT_PASS_REQUIREMENTS:
            fail("partial-default-policy")
        if not isinstance(authorization, dict):
            fail("default-policy-authorization")
        if authorization.get("kind") != "user_explicit" or authorization.get("scope") != "all_windows_related_performance_requirements":
            fail("default-policy-scope")
        if not isinstance(authorization.get("statement"), str) or not authorization["statement"].strip():
            fail("default-policy-statement")
        parse_time(authorization.get("authorizedAt"))
        if gate.get("evidenceStatus") != "not_run_user_waived" or gate.get("blockers") != []:
            fail("default-policy-evidence-boundary")
        print("KNOWLEDGE_PERFORMANCE_GATE: pass mode=authorized-defaults factualEvidence=not-run-user-waived")
        return

    commit = gate.get("candidateCommit")
    if not isinstance(commit, str) or not COMMIT.fullmatch(commit):
        fail("candidate-commit")
    approved = gate.get("thresholdsApprovedBy")
    if not isinstance(approved, str) or not approved.strip():
        fail("threshold-approval")
    frozen_at = parse_time(gate.get("thresholdsFrozenAt"))
    measured_at = parse_time(gate.get("measuredAt"))
    if frozen_at >= measured_at:
        fail("thresholds-not-frozen-before-measurement")

    windows = gate.get("targetWindows")
    if not isinstance(windows, dict) or "windows" not in str(windows.get("osEdition", "")).lower():
        fail("non-windows-target")
    for key in ("osBuild", "cpuModel", "storageModel", "storageKind", "powerPlan", "webView2Version"):
        if not isinstance(windows.get(key), str) or not windows[key].strip():
            fail(f"target-{key}")
    for key in ("logicalProcessors", "ramBytes"):
        if not positive_int(windows.get(key)):
            fail(f"target-{key}")

    dataset = gate.get("frozenDataset")
    if not isinstance(dataset, dict):
        fail("dataset")
    for key in ("exportIdSha256", "manifestSha256", "coverageSha256"):
        if not isinstance(dataset.get(key), str) or not SHA256.fullmatch(dataset[key]):
            fail(f"dataset-{key}")
    for key in ("conversationCount", "messageCount", "indexableMessageCount", "chunkCount", "embeddingDimension"):
        if not positive_int(dataset.get(key)):
            fail(f"dataset-{key}")
    if not isinstance(dataset.get("embeddingModelDigestShort"), str) or not dataset["embeddingModelDigestShort"].strip():
        fail("dataset-model")

    limits, observed = gate.get("limits"), gate.get("observed")
    if not isinstance(limits, dict) or not isinstance(observed, dict):
        fail("limits-or-observed")
    for observed_key, limit_key in METRICS.items():
        if not positive_int(limits.get(limit_key)) or not positive_int(observed.get(observed_key)):
            fail(f"metric-{observed_key}")
        if observed[observed_key] > limits[limit_key]:
            fail(f"metric-over-limit-{observed_key}")
    for sample_key in ("querySamples", "workRecordSchedulerDriftSamples", "uiInputLatencySamples"):
        if not positive_int(observed.get(sample_key)) or observed[sample_key] < 100:
            fail(f"sample-count-{sample_key}")
    if observed["queryP50Ms"] > observed["queryP95Ms"]:
        fail("query-percentiles")

    metric_verdicts = gate.get("metricVerdicts")
    if not isinstance(metric_verdicts, dict) or set(metric_verdicts) != set(METRICS):
        fail("metric-verdict-keys")
    if any(value != "pass" for value in metric_verdicts.values()):
        fail("metric-verdict")
    if gate.get("vectorBackend") != "sqlite_blob_stream_v1" or gate.get("blockers") != []:
        fail("backend-or-blockers")

    evidence = gate.get("evidence")
    if not isinstance(evidence, list) or not evidence:
        fail("evidence")
    for item in evidence:
        if not isinstance(item, dict) or not isinstance(item.get("path"), str) or not isinstance(item.get("sha256"), str):
            fail("evidence-shape")
        path = Path(item["path"])
        if path.is_absolute() or ".." in path.parts or not SHA256.fullmatch(item["sha256"]):
            fail("evidence-path-or-hash")
        try:
            digest = hashlib.sha256((root / path).read_bytes()).hexdigest()
        except OSError:
            fail("evidence-missing")
        if digest != item["sha256"]:
            fail("evidence-hash-mismatch")
    print("KNOWLEDGE_PERFORMANCE_GATE: pass")


if __name__ == "__main__":
    main()
