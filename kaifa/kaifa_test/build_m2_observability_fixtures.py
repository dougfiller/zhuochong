#!/usr/bin/env python3
"""Build deterministic, file-bound fixtures for the M2 evidence verifier."""

from __future__ import annotations

import copy
import hashlib
import json
import shutil
import uuid
from pathlib import Path

from verify_m2_observability_gate import (
    AC_IDS,
    CAPABILITY_KEYS,
    FAULT_IDS,
    REQUIRED_COLLECTORS,
    RUNTIME_SCENARIO_PREFIXES,
    scenario_contract,
)


ROOT = Path(__file__).parent / "fixtures" / "m2_observability"
ARTIFACTS = ROOT / "artifacts"
BATCH_ID = "fixture-batch-v2"


def encoded(value: object) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode()


def write(path: Path, value: object) -> str:
    payload = encoded(value)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)
    return hashlib.sha256(payload).hexdigest()


def write_text(path: Path, text: str) -> str:
    payload = text.encode()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)
    return hashlib.sha256(payload).hexdigest()


def manifest_file(
    identifier: str,
    kind: str,
    path: Path,
    digest: str,
    request_evidence_id: str | None = None,
) -> dict[str, object]:
    return {
        "id": identifier,
        "kind": kind,
        "path": path.relative_to(ROOT).as_posix(),
        "sha256": digest,
        "batch_id": BATCH_ID,
        "request_evidence_id": request_evidence_id,
    }


def request_evidence(scenario_id: str) -> tuple[dict[str, object], list[dict[str, object]]]:
    contract = scenario_contract(scenario_id, BATCH_ID)
    evidence_id = f"E-{scenario_id}"
    request_id = str(uuid.uuid5(uuid.NAMESPACE_URL, f"aich8/{scenario_id}"))
    context_hash = hashlib.sha256(f"context/{scenario_id}".encode()).hexdigest()
    model_request_id = hashlib.sha256(f"model/{scenario_id}".encode()).hexdigest()[:32]
    request_bytes_hash = hashlib.sha256(f"bytes/{scenario_id}".encode()).hexdigest()
    events: list[dict[str, object]] = []
    if contract["retrieval_outcome"] != "none":
        events.append(
            {
                "kind": "retrieval_completed",
                "request_id": request_id,
                "binding_generation": 7,
                "stage_seq": 5,
                "outcome": contract["retrieval_outcome"],
                "context_hash": context_hash,
                "model_request_id": model_request_id,
                "selected_hit_count": contract["selected_hit_count"],
            }
        )
        for attempt in range(1, contract["model_attempts"] + 1):
            events.append(
                {
                    "kind": "model_transport_started",
                    "request_id": request_id,
                    "binding_generation": 7,
                    "stage_seq": 5,
                    "attempt": attempt,
                    "context_hash": context_hash,
                    "model_request_id": model_request_id,
                    "request_bytes_sha256": request_bytes_hash,
                }
            )
    events.append(
        {
            "kind": "terminal",
            "request_id": request_id,
            "stage_seq": 6 if contract["retrieval_outcome"] != "none" else 2,
            "outcome": contract["terminal"],
            "error_code": contract["error_code"],
        }
    )
    counts = {key: 0 for key in CAPABILITY_KEYS}
    counts["replyModelNonLoopback"] = contract["model_attempts"]
    counts["knowledgeEmbeddingLoopback"] = contract["embedding_attempts"]
    counts["ocrBackendLocalProcess"] = contract["ocr_local_process"]
    trace = {
        "schema_version": 1,
        "batch_id": BATCH_ID,
        "request_id": request_id,
        "events": events,
    }
    audit = {
        "schema_version": 1,
        "batch_id": BATCH_ID,
        "request_id": request_id,
        "counts": counts,
        "logical_model_requests": int(contract["model_attempts"] > 0),
        "upload_enqueue_count": contract["upload_enqueue_count"],
        "upload_attempt_count": contract["upload_attempt_count"],
        "collectors": sorted(REQUIRED_COLLECTORS),
    }
    trace_path = ARTIFACTS / f"{scenario_id.lower()}-trace.json"
    audit_path = ARTIFACTS / f"{scenario_id.lower()}-audit.json"
    trace_hash = write(trace_path, trace)
    audit_hash = write(audit_path, audit)
    row = {
        "id": evidence_id,
        "request_id": request_id,
        "trace_sha256": trace_hash,
        "audit_sha256": audit_hash,
        "events": events,
        "counts": counts,
        "logical_model_requests": audit["logical_model_requests"],
        "upload_enqueue_count": audit["upload_enqueue_count"],
        "upload_attempt_count": audit["upload_attempt_count"],
    }
    files = [
        manifest_file(f"TRACE-{scenario_id}", "request_trace", trace_path, trace_hash, evidence_id),
        manifest_file(f"AUDIT-{scenario_id}", "request_audit", audit_path, audit_hash, evidence_id),
    ]
    return row, files


def build_pass() -> dict[str, object]:
    if ARTIFACTS.exists():
        shutil.rmtree(ARTIFACTS)
    ARTIFACTS.mkdir(parents=True)
    files: list[dict[str, object]] = []
    requests: list[dict[str, object]] = []
    scenarios: list[dict[str, object]] = []
    for scenario_id in sorted(FAULT_IDS | AC_IDS):
        runtime = scenario_id.startswith(RUNTIME_SCENARIO_PREFIXES)
        cited: list[str] = []
        if runtime:
            request, request_files = request_evidence(scenario_id)
            requests.append(request)
            files.extend(request_files)
            cited.append(request["id"])
        contract = scenario_contract(scenario_id, BATCH_ID)
        scenarios.append(
            {
                "id": f"SC-{scenario_id}",
                "scenario_id": scenario_id,
                "status": "pass",
                "request_evidence_ids": cited,
                "expected": contract,
                "observed": copy.deepcopy(contract),
            }
        )

    source_path = ARTIFACTS / "source-tree.tar.zst"
    nsis_path = ARTIFACTS / "candidate-installer.exe"
    source_hash = write_text(source_path, "synthetic frozen source fixture\n")
    nsis_hash = write_text(nsis_path, "synthetic NSIS fixture\n")
    files.extend(
        [
            manifest_file("SOURCE", "source_tree", source_path, source_hash),
            manifest_file("NSIS", "nsis_package", nsis_path, nsis_hash),
        ]
    )
    candidate = {
        "git_commit": "4" * 40,
        "source_tree_sha256": source_hash,
        "nsis_sha256": nsis_hash,
        "batch_id": BATCH_ID,
    }
    identity_path = ARTIFACTS / "candidate-identity.json"
    identity_hash = write(
        identity_path,
        {
            "schema_version": 1,
            "batch_id": BATCH_ID,
            "git_commit": candidate["git_commit"],
            "source_tree_file_id": "SOURCE",
            "nsis_file_id": "NSIS",
        },
    )
    files.append(manifest_file("CANDIDATE", "candidate_identity", identity_path, identity_hash))

    environment = {
        "os_build": "Windows 11 synthetic fixture",
        "windows_host": "fixture-host",
        "wechat_profile_fingerprint": "fixture-profile",
        "real_reply_model": True,
        "real_embedding_model": True,
        "evidence_type": "synthetic",
    }
    environment_path = ARTIFACTS / "environment-attestation.json"
    environment_hash = write(
        environment_path,
        {
            "schema_version": 1,
            "batch_id": BATCH_ID,
            "collector": "m2-fixture-collector-v1",
            "environment": environment,
        },
    )
    files.append(
        manifest_file("ENVIRONMENT", "environment_attestation", environment_path, environment_hash)
    )

    supporting: dict[str, str] = {}
    for identifier, kind in (
        ("CREDENTIALS", "credentials_test"),
        ("PRIVACY", "privacy_scan"),
        ("SENTINEL", "sentinel_observation"),
        ("ASSETS", "assets_review"),
    ):
        path = ARTIFACTS / f"{identifier.lower()}.json"
        digest = write(
            path,
            {
                "schema_version": 1,
                "batch_id": BATCH_ID,
                "kind": kind,
                "status": "pass",
            },
        )
        supporting[kind] = digest
        files.append(manifest_file(identifier, kind, path, digest))

    scenarios_path = ARTIFACTS / "scenario-contracts.json"
    scenarios_hash = write(
        scenarios_path,
        {"schema_version": 1, "batch_id": BATCH_ID, "scenarios": scenarios},
    )
    files.append(manifest_file("SCENARIOS", "scenario_contracts", scenarios_path, scenarios_hash))
    return {
        "schema_version": 1,
        "gate_id": "m2-observability-fixture-v2",
        "generated_at": "2026-08-08T00:00:00Z",
        "candidate": candidate,
        "environment": environment,
        "request_evidence": requests,
        "fault_matrix": [
            {"id": identifier, "status": "pass", "evidence_ids": [f"SC-{identifier}"]}
            for identifier in sorted(FAULT_IDS)
        ],
        "ac_matrix": [
            {"id": identifier, "status": "pass", "evidence_ids": [f"SC-{identifier}"]}
            for identifier in sorted(AC_IDS)
        ],
        "credentials_mock_matrix": [
            {
                "id": "MOCK-CREDENTIALS",
                "status": "pass",
                "evidence_sha256": supporting["credentials_test"],
            }
        ],
        "privacy_scan": {
            "source_hits": 0,
            "evidence_hits": 0,
            "package_hits": 0,
            "evidence_sha256": supporting["privacy_scan"],
        },
        "sentinel_counters": {
            "status": "pass",
            "counts": {
                "fileAccess": 0,
                "moduleLoad": 0,
                "childProcess": 0,
                "network": 0,
                "syntheticInput": 0,
            },
            "evidence_sha256": supporting["sentinel_observation"],
        },
        "reference_assets_review": {
            "status": "pass",
            "evidence_sha256": supporting["assets_review"],
        },
        "verdict": "pass",
        "blockers": [],
        "evidence_manifest": {
            "schema_version": 1,
            "batch_id": BATCH_ID,
            "collector": "m2-fixture-collector-v1",
            "evidence_type": "synthetic",
            "files": files,
            "scenarios": scenarios,
        },
    }


def main() -> None:
    document = build_pass()
    write(ROOT / "pass.json", document)

    variant = copy.deepcopy(document)
    variant["request_evidence"] = variant["request_evidence"][:1]
    variant["verdict"] = "blocked"
    variant["blockers"] = ["single success request cannot prove the matrix"]
    write(ROOT / "blocked-single-success.json", variant)

    variant = copy.deepcopy(document)
    variant["request_evidence"][0]["counts"]["mcp"] = 1
    variant["verdict"] = "fail"
    write(ROOT / "fail-capability.json", variant)

    variant = copy.deepcopy(document)
    variant["default_pass_requirements"] = ["forbidden"]
    variant["verdict"] = "fail"
    write(ROOT / "fail-default-pass.json", variant)

    variant = copy.deepcopy(document)
    variant["ac_matrix"] = variant["ac_matrix"][:-1]
    variant["verdict"] = "blocked"
    variant["blockers"] = ["one AC row is missing"]
    write(ROOT / "blocked-missing-ac.json", variant)

    variant = copy.deepcopy(document)
    variant["evidence_manifest"]["files"][0]["sha256"] = "0" * 64
    variant["verdict"] = "fail"
    write(ROOT / "fail-hash-mismatch.json", variant)

    variant = copy.deepcopy(document)
    variant["evidence_manifest"]["files"][0]["batch_id"] = "other-batch"
    variant["verdict"] = "fail"
    write(ROOT / "fail-cross-batch.json", variant)

    variant = copy.deepcopy(document)
    variant["candidate"]["git_commit"] = "5" * 40
    variant["verdict"] = "fail"
    write(ROOT / "fail-candidate-identity.json", variant)

    variant = copy.deepcopy(document)
    variant["evidence_manifest"]["files"][0]["path"] = "artifacts/missing.json"
    variant["verdict"] = "fail"
    write(ROOT / "fail-missing-file.json", variant)

    variant = copy.deepcopy(document)
    variant["evidence_manifest"]["files"][0]["path"] = "../outside.json"
    variant["verdict"] = "fail"
    write(ROOT / "fail-path-escape.json", variant)

    variant = copy.deepcopy(document)
    variant["evidence_manifest"]["files"][1]["path"] = variant["evidence_manifest"]["files"][0]["path"]
    variant["evidence_manifest"]["files"][1]["sha256"] = variant["evidence_manifest"]["files"][0]["sha256"]
    variant["verdict"] = "fail"
    write(ROOT / "fail-duplicate-reference.json", variant)

    symlink = ARTIFACTS / "symlink-evidence.json"
    symlink.symlink_to(Path(document["evidence_manifest"]["files"][0]["path"]).name)
    variant = copy.deepcopy(document)
    variant["evidence_manifest"]["files"][0]["path"] = symlink.relative_to(ROOT).as_posix()
    variant["verdict"] = "fail"
    write(ROOT / "fail-symlink.json", variant)


if __name__ == "__main__":
    main()
