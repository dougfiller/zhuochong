#!/usr/bin/env python3
"""Validate the strict, metadata-only M2 evidence gate.

The verifier reads one sanitized gate document and its controlled relative
manifest files. It never starts WeChat, the application, a model, a database,
a package, or a network client. Exit codes are 0=pass, 1=fail, and 2=blocked.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


AC_IDS = {
    *(f"AC-BASE-{number:02d}" for number in range(1, 11)),
    *(f"AC-WX-{number:02d}" for number in range(1, 11)),
    *(f"AC-PET-{number:02d}" for number in range(1, 6)),
    *(f"AC-KB-{number:02d}" for number in range(1, 9)),
    *(f"AC-RAG-{number:02d}" for number in range(1, 12)),
}
FAULT_IDS = {
    *(f"M2-OBS-{number:02d}" for number in range(1, 7)),
    *(f"M2-CAP-{number:02d}" for number in range(1, 4)),
    *(f"M2-RET-{number:02d}" for number in range(1, 11)),
    *(f"M2-MOD-{number:02d}" for number in range(1, 8)),
    *(f"M2-KB-{number:02d}" for number in range(1, 9)),
    *(f"M2-PRIV-{number:02d}" for number in range(1, 8)),
    *(f"M2-PET-{number:02d}" for number in range(1, 5)),
}
CAPABILITY_KEYS = {
    "replyModelNonLoopback",
    "knowledgeEmbeddingLoopback",
    "ocrBackendLocalProcess",
    "mcp",
    "bot",
    "localhostApi",
    "remoteUpload",
    "search",
    "action",
    "cloudEmbedding",
    "petResourceExternalProcess",
    "petResourceNetwork",
    "petResourceModuleLoad",
    "petResourceSyntheticInput",
}
FORBIDDEN_CAPABILITIES = CAPABILITY_KEYS - {
    "replyModelNonLoopback",
    "knowledgeEmbeddingLoopback",
    "ocrBackendLocalProcess",
}
TOP_LEVEL_KEYS = {
    "schema_version",
    "gate_id",
    "generated_at",
    "candidate",
    "environment",
    "request_evidence",
    "fault_matrix",
    "ac_matrix",
    "credentials_mock_matrix",
    "privacy_scan",
    "sentinel_counters",
    "reference_assets_review",
    "verdict",
    "blockers",
    "evidence_manifest",
}
REQUIRED_TOP_LEVEL_KEYS = TOP_LEVEL_KEYS - {"evidence_manifest"}
FORBIDDEN_POLICY_KEYS = {
    "default_pass_requirements",
    "waiver",
    "waivers",
    "authorized_defaults",
    "authorized-defaults",
}
SHA256 = re.compile(r"^[0-9a-f]{64}$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
UUID = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$"
)
REQUIRED_COLLECTORS = {
    "terminal",
    "capability",
    "upload_queue",
    "ocr",
    "pet_resource",
}
RUNTIME_SCENARIO_PREFIXES = ("M2-OBS-", "M2-CAP-", "M2-RET-", "M2-MOD-")
MANIFEST_FILE_KINDS = {
    "candidate_identity",
    "source_tree",
    "nsis_package",
    "environment_attestation",
    "request_trace",
    "request_audit",
    "credentials_test",
    "privacy_scan",
    "sentinel_observation",
    "assets_review",
    "scenario_contracts",
}
SUPPORTING_FILE_KINDS = {
    "credentials_test",
    "privacy_scan",
    "sentinel_observation",
    "assets_review",
}


class Result:
    def __init__(self) -> None:
        self.failures: list[str] = []
        self.blockers: list[str] = []

    def fail(self, message: str) -> None:
        self.failures.append(message)

    def block(self, message: str) -> None:
        self.blockers.append(message)

    @property
    def verdict(self) -> str:
        if self.failures:
            return "fail"
        if self.blockers:
            return "blocked"
        return "pass"


def mapping(value: object, label: str, result: Result) -> dict[str, Any]:
    if isinstance(value, dict):
        return value
    result.fail(f"{label} must be an object")
    return {}


def nonempty(value: object) -> bool:
    return isinstance(value, str) and bool(value.strip())


def sha256(value: object) -> bool:
    return isinstance(value, str) and bool(SHA256.fullmatch(value))


def digest_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while chunk := handle.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def load_json_file(path: Path, label: str, result: Result) -> object | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        result.fail(f"{label} is not readable canonical JSON")
        return None


def safe_evidence_path(root: Path, relative: object, label: str, result: Result) -> Path | None:
    if not nonempty(relative):
        result.fail(f"{label}.path must be non-empty relative text")
        return None
    relative_path = Path(relative)
    if relative_path.is_absolute() or ".." in relative_path.parts:
        result.fail(f"{label}.path escapes the evidence directory")
        return None
    candidate = root / relative_path
    cursor = candidate
    while cursor != root:
        if cursor.is_symlink():
            result.fail(f"{label}.path must not contain a symlink")
            return None
        cursor = cursor.parent
    try:
        resolved = candidate.resolve(strict=True)
        resolved.relative_to(root.resolve(strict=True))
    except (OSError, ValueError):
        result.fail(f"{label}.path is missing or outside the evidence directory")
        return None
    if not resolved.is_file():
        result.fail(f"{label}.path is not a regular file")
        return None
    return resolved


def check_manifest(
    document: dict[str, Any],
    evidence_path: Path,
    allow_synthetic_fixture: bool,
    result: Result,
) -> dict[str, Any] | None:
    manifest = document.get("evidence_manifest")
    if manifest is None:
        result.block("evidence_manifest has not been collected")
        return None
    manifest = mapping(manifest, "evidence_manifest", result)
    required = {"schema_version", "batch_id", "collector", "evidence_type", "files", "scenarios"}
    if set(manifest) != required:
        result.fail("evidence_manifest fields are missing or unknown")
    if manifest.get("schema_version") != 1:
        result.fail("evidence_manifest.schema_version must be 1")
    batch_id = manifest.get("batch_id")
    if not nonempty(batch_id):
        result.fail("evidence_manifest.batch_id is required")
    evidence_type = manifest.get("evidence_type")
    if evidence_type not in {"real", "synthetic"}:
        result.fail("evidence_manifest.evidence_type must be real or synthetic")
    fixture_root = evidence_path.parent.resolve()
    expected_fixture_root = (Path(__file__).parent / "fixtures").resolve()
    fixture_allowed = False
    if allow_synthetic_fixture:
        try:
            fixture_root.relative_to(expected_fixture_root)
            fixture_allowed = True
        except ValueError:
            result.fail("synthetic fixture mode is restricted to kaifa_test/fixtures")
    if evidence_type == "synthetic" and not fixture_allowed:
        result.block("synthetic evidence cannot satisfy the production gate")
    if evidence_type == "real" and manifest.get("collector") != "m2-evidence-collector-v1":
        result.block("real evidence was not emitted by the trusted collector")
    if evidence_type == "synthetic" and manifest.get("collector") != "m2-fixture-collector-v1":
        result.fail("synthetic evidence must identify the fixture collector")

    files = manifest.get("files")
    if not isinstance(files, list) or not files:
        result.block("evidence_manifest.files is missing")
        files = []
    files_by_id: dict[str, dict[str, Any]] = {}
    used_paths: set[Path] = set()
    used_digests: set[tuple[object, object]] = set()
    for index, raw in enumerate(files):
        item = mapping(raw, f"evidence_manifest.files[{index}]", result)
        if set(item) != {"id", "kind", "path", "sha256", "batch_id", "request_evidence_id"}:
            result.fail(f"evidence_manifest.files[{index}] fields are missing or unknown")
        identifier = item.get("id")
        if not nonempty(identifier) or identifier in files_by_id:
            result.fail(f"evidence_manifest.files[{index}].id is missing or duplicate")
            continue
        if item.get("batch_id") != batch_id:
            result.fail(f"evidence_manifest.files[{index}] belongs to another batch")
        if item.get("kind") not in MANIFEST_FILE_KINDS:
            result.fail(f"evidence_manifest.files[{index}].kind is unknown")
        if item.get("request_evidence_id") is not None and not nonempty(
            item.get("request_evidence_id")
        ):
            result.fail(f"evidence_manifest.files[{index}].request_evidence_id is invalid")
        path = safe_evidence_path(
            evidence_path.parent,
            item.get("path"),
            f"evidence_manifest.files[{index}]",
            result,
        )
        if path is None:
            continue
        if path in used_paths:
            result.fail(f"evidence_manifest.files[{index}] reuses another manifest path")
            continue
        used_paths.add(path)
        if not sha256(item.get("sha256")):
            result.fail(f"evidence_manifest.files[{index}].sha256 is invalid")
        elif digest_file(path) != item["sha256"]:
            result.fail(f"evidence_manifest.files[{index}] SHA-256 does not match the file")
        digest_key = (item.get("kind"), item.get("sha256"))
        if digest_key in used_digests:
            result.fail(f"evidence_manifest.files[{index}] duplicates another evidence payload")
        used_digests.add(digest_key)
        item = dict(item)
        item["resolved_path"] = path
        files_by_id[identifier] = item
        if item.get("kind") in SUPPORTING_FILE_KINDS:
            payload = load_json_file(path, f"evidence_manifest.files[{index}]", result)
            expected_payload = {
                "schema_version": 1,
                "batch_id": batch_id,
                "kind": item.get("kind"),
                "status": "pass",
            }
            if payload is not None and payload != expected_payload:
                result.fail(f"evidence_manifest.files[{index}] supporting evidence is invalid")

    scenarios = manifest.get("scenarios")
    if not isinstance(scenarios, list):
        result.fail("evidence_manifest.scenarios must be a list")
        scenarios = []
    scenario_files = [item for item in files_by_id.values() if item.get("kind") == "scenario_contracts"]
    if len(scenario_files) != 1:
        result.block("exactly one scenario_contracts file is required")
    else:
        payload = load_json_file(scenario_files[0]["resolved_path"], "scenario_contracts", result)
        expected_payload = {
            "schema_version": 1,
            "batch_id": batch_id,
            "scenarios": scenarios,
        }
        if payload is not None and payload != expected_payload:
            result.fail("scenario contracts do not match their hash-bound file")

    return {
        "batch_id": batch_id,
        "evidence_type": evidence_type,
        "fixture_allowed": fixture_allowed,
        "files_by_id": files_by_id,
        "scenarios": scenarios,
    }


def files_of_kind(manifest: dict[str, Any] | None, kind: str) -> list[dict[str, Any]]:
    if manifest is None:
        return []
    return [item for item in manifest["files_by_id"].values() if item.get("kind") == kind]


def unique_file(
    manifest: dict[str, Any] | None,
    kind: str,
    label: str,
    result: Result,
) -> dict[str, Any] | None:
    files = files_of_kind(manifest, kind)
    if len(files) != 1:
        result.block(f"{label} requires exactly one {kind} file")
        return None
    return files[0]


def check_candidate(
    document: dict[str, Any],
    project_root: Path,
    manifest: dict[str, Any] | None,
    result: Result,
) -> None:
    candidate = mapping(document.get("candidate"), "candidate", result)
    if set(candidate) != {"git_commit", "source_tree_sha256", "nsis_sha256", "batch_id"}:
        result.fail("candidate must contain exactly the four frozen identity fields")
    if not isinstance(candidate.get("git_commit"), str) or not COMMIT.fullmatch(
        candidate["git_commit"]
    ):
        result.block("candidate.git_commit is missing or invalid")
    for key in ("source_tree_sha256", "nsis_sha256"):
        value = candidate.get(key)
        if value is not None and not sha256(value):
            result.fail(f"candidate.{key} must be null or SHA-256")
        if value is None:
            result.block(f"candidate.{key} has not been observed")
    if candidate.get("batch_id") is None:
        result.block("candidate.batch_id has not been observed")
    elif not nonempty(candidate.get("batch_id")):
        result.fail("candidate.batch_id must be null or non-empty text")

    if manifest is None:
        return
    if candidate.get("batch_id") != manifest["batch_id"]:
        result.fail("candidate.batch_id does not match the evidence manifest")
    identity_file = unique_file(manifest, "candidate_identity", "candidate identity", result)
    source_file = unique_file(manifest, "source_tree", "source tree", result)
    nsis_file = unique_file(manifest, "nsis_package", "NSIS package", result)
    if identity_file is None or source_file is None or nsis_file is None:
        return
    identity = load_json_file(identity_file["resolved_path"], "candidate identity", result)
    expected_identity = {
        "schema_version": 1,
        "batch_id": manifest["batch_id"],
        "git_commit": candidate.get("git_commit"),
        "source_tree_file_id": source_file.get("id"),
        "nsis_file_id": nsis_file.get("id"),
    }
    if identity is not None and identity != expected_identity:
        result.fail("candidate identity does not match its hash-bound file")
    if candidate.get("source_tree_sha256") != source_file.get("sha256"):
        result.fail("candidate.source_tree_sha256 does not match the frozen source artifact")
    if candidate.get("nsis_sha256") != nsis_file.get("sha256"):
        result.fail("candidate.nsis_sha256 does not match the frozen NSIS artifact")
    if manifest["evidence_type"] == "real":
        try:
            head = subprocess.run(
                ["git", "-C", str(project_root), "rev-parse", "HEAD"],
                check=True,
                capture_output=True,
                text=True,
                timeout=10,
            ).stdout.strip()
            frozen_tree = subprocess.run(
                ["git", "-C", str(project_root), "ls-tree", "-r", "--full-tree", "HEAD"],
                check=True,
                capture_output=True,
                timeout=10,
            ).stdout
        except (OSError, subprocess.SubprocessError):
            result.block("current Git candidate identity could not be read")
        else:
            if candidate.get("git_commit") != head:
                result.fail("candidate.git_commit is not the current checked-out candidate")
            try:
                source_bytes = source_file["resolved_path"].read_bytes()
                nsis_prefix = nsis_file["resolved_path"].read_bytes()[:2]
            except OSError:
                result.fail("candidate artifacts became unreadable during verification")
            else:
                if source_bytes != frozen_tree:
                    result.fail("frozen source manifest is not the current commit tree")
                if nsis_prefix != b"MZ":
                    result.fail("frozen NSIS artifact is not a Windows PE package")


def check_environment(
    document: dict[str, Any], manifest: dict[str, Any] | None, result: Result
) -> None:
    environment = mapping(document.get("environment"), "environment", result)
    required = {
        "os_build",
        "windows_host",
        "wechat_profile_fingerprint",
        "real_reply_model",
        "real_embedding_model",
        "evidence_type",
    }
    if set(environment) != required:
        result.fail("environment fields are missing or unknown")
    for key in ("os_build", "windows_host", "wechat_profile_fingerprint"):
        if environment.get(key) is None:
            result.block(f"environment.{key} has not been observed")
        elif not nonempty(environment.get(key)):
            result.fail(f"environment.{key} must be null or non-empty text")
    for key in ("real_reply_model", "real_embedding_model"):
        if not isinstance(environment.get(key), bool):
            result.fail(f"environment.{key} must be boolean")
        elif environment[key] is False:
            result.block(f"environment.{key} is not real-environment evidence")
    if environment.get("evidence_type") not in {"real", "synthetic", "mixed", "not-run"}:
        result.fail("environment.evidence_type is invalid")
    elif environment["evidence_type"] != "real":
        if manifest is None or not manifest.get("fixture_allowed"):
            result.block("environment evidence is not a complete real batch")

    if manifest is None:
        return
    expected_type = manifest["evidence_type"]
    if environment.get("evidence_type") != expected_type:
        result.fail("environment.evidence_type does not match the evidence manifest")
    attestation_file = unique_file(
        manifest, "environment_attestation", "environment attestation", result
    )
    if attestation_file is None:
        return
    attestation = load_json_file(
        attestation_file["resolved_path"], "environment attestation", result
    )
    expected_attestation = {
        "schema_version": 1,
        "batch_id": manifest["batch_id"],
        "collector": (
            "m2-evidence-collector-v1"
            if expected_type == "real"
            else "m2-fixture-collector-v1"
        ),
        "environment": environment,
    }
    if attestation is not None and attestation != expected_attestation:
        result.fail("environment does not match its collector attestation")


def check_request_evidence(
    document: dict[str, Any], manifest: dict[str, Any] | None, result: Result
) -> dict[str, dict[str, Any]]:
    evidence = document.get("request_evidence")
    if not isinstance(evidence, list) or not evidence:
        result.block("request_evidence is missing")
        return {}
    evidence_ids: set[str] = set()
    evidence_by_id: dict[str, dict[str, Any]] = {}
    for index, item in enumerate(evidence):
        row = mapping(item, f"request_evidence[{index}]", result)
        allowed = {
            "id",
            "request_id",
            "trace_sha256",
            "audit_sha256",
            "events",
            "counts",
            "logical_model_requests",
            "upload_enqueue_count",
            "upload_attempt_count",
        }
        if set(row) != allowed:
            result.fail(f"request_evidence[{index}] has missing or unknown fields")
        identifier = row.get("id")
        if not nonempty(identifier) or identifier in evidence_ids:
            result.fail(f"request_evidence[{index}].id is missing or duplicate")
            continue
        evidence_ids.add(identifier)
        evidence_by_id[identifier] = row
        if not isinstance(row.get("request_id"), str) or not UUID.fullmatch(row["request_id"]):
            result.fail(f"request_evidence[{index}].request_id is not an opaque UUID")
        for key in ("trace_sha256", "audit_sha256"):
            if not sha256(row.get(key)):
                result.block(f"request_evidence[{index}].{key} is missing")
        events = row.get("events")
        if not isinstance(events, list) or not events:
            result.block(f"request_evidence[{index}].events is missing")
            events = []
        retrievals = [event for event in events if isinstance(event, dict) and event.get("kind") == "retrieval_completed"]
        attempts = [event for event in events if isinstance(event, dict) and event.get("kind") == "model_transport_started"]
        terminals = [event for event in events if isinstance(event, dict) and event.get("kind") == "terminal"]
        if len(retrievals) + len(attempts) + len(terminals) != len(events):
            result.fail(f"request_evidence[{index}] contains an unknown event kind")
        if len(terminals) != 1:
            result.fail(f"request_evidence[{index}] must contain exactly one terminal event")
        terminal_position = -1
        if terminals:
            terminal = terminals[0]
            terminal_fields = {"kind", "request_id", "stage_seq", "outcome", "error_code"}
            if set(terminal) != terminal_fields:
                result.fail(f"request_evidence[{index}] terminal fields are incomplete or unknown")
            if terminal.get("request_id") != row.get("request_id"):
                result.fail(f"request_evidence[{index}] terminal request id does not join")
            if terminal.get("outcome") not in {"reply_ready", "failed", "cancelled"}:
                result.fail(f"request_evidence[{index}] terminal outcome is invalid")
            if type(terminal.get("stage_seq")) is not int or not 1 <= terminal["stage_seq"] <= 6:
                result.fail(f"request_evidence[{index}] terminal stage is invalid")
            if terminal.get("outcome") == "reply_ready" and terminal.get("error_code") is not None:
                result.fail(f"request_evidence[{index}] successful terminal must not have an error code")
            if terminal.get("outcome") != "reply_ready" and not nonempty(terminal.get("error_code")):
                result.fail(f"request_evidence[{index}] failed terminal must have an error code")
            terminal_position = events.index(terminal)
        logical_requests = row.get("logical_model_requests")
        if type(logical_requests) is not int or logical_requests not in {0, 1}:
            result.fail(f"request_evidence[{index}].logical_model_requests must be 0 or 1")
        if len(retrievals) > 1:
            result.fail(f"request_evidence[{index}] has duplicate retrieval permits")
        if retrievals and [attempt.get("attempt") for attempt in attempts] not in (
            [],
            [1],
            [1, 2],
        ):
            result.fail(f"request_evidence[{index}] physical attempts must be [], [1] or [1,2]")
        if not retrievals:
            if attempts:
                result.fail(f"request_evidence[{index}] transport started without a retrieval permit")
            if logical_requests != 0:
                result.fail(f"request_evidence[{index}] retrieval failure must have zero logical model requests")
            if terminals and terminals[0].get("outcome") not in {"failed", "cancelled"}:
                result.fail(f"request_evidence[{index}] retrieval failure must terminate failed or cancelled")
        if retrievals:
            retrieval = retrievals[0]
            retrieval_fields = {
                "kind",
                "request_id",
                "binding_generation",
                "stage_seq",
                "outcome",
                "context_hash",
                "model_request_id",
                "selected_hit_count",
            }
            if set(retrieval) != retrieval_fields:
                result.fail(f"request_evidence[{index}] retrieval fields are incomplete or unknown")
            if retrieval.get("stage_seq") != 5 or retrieval.get("outcome") not in {
                "success",
                "no_hit",
                "fts_fallback",
            }:
                result.fail(f"request_evidence[{index}] retrieval permit is invalid")
            joined = (
                retrieval.get("request_id"),
                retrieval.get("binding_generation"),
                retrieval.get("stage_seq"),
                retrieval.get("context_hash"),
                retrieval.get("model_request_id"),
            )
            if retrieval.get("request_id") != row.get("request_id"):
                result.fail(f"request_evidence[{index}] request id does not join")
            if not sha256(retrieval.get("context_hash")):
                result.fail(f"request_evidence[{index}] context hash is invalid")
            if not isinstance(retrieval.get("model_request_id"), str) or not re.fullmatch(
                r"[0-9a-f]{32}", retrieval["model_request_id"]
            ):
                result.fail(f"request_evidence[{index}] model request id is invalid")
            if type(retrieval.get("binding_generation")) is not int or retrieval["binding_generation"] < 0:
                result.fail(f"request_evidence[{index}] binding generation is invalid")
            if type(retrieval.get("selected_hit_count")) is not int or not 0 <= retrieval["selected_hit_count"] <= 20:
                result.fail(f"request_evidence[{index}] selected hit count is invalid")
            retrieval_position = events.index(retrieval)
            expected_logical_requests = int(bool(attempts))
            if logical_requests != expected_logical_requests:
                result.fail(
                    f"request_evidence[{index}] logical model count does not match transport"
                )
            if not attempts and terminals and terminals[0].get("outcome") not in {
                "failed",
                "cancelled",
            }:
                result.fail(
                    f"request_evidence[{index}] retrieval without transport must fail or cancel"
                )
            byte_hashes: set[str] = set()
            for attempt in attempts:
                attempt_fields = {
                    "kind",
                    "request_id",
                    "binding_generation",
                    "stage_seq",
                    "attempt",
                    "context_hash",
                    "model_request_id",
                    "request_bytes_sha256",
                }
                if set(attempt) != attempt_fields:
                    result.fail(f"request_evidence[{index}] attempt fields are incomplete or unknown")
                if events.index(attempt) <= retrieval_position:
                    result.fail(f"request_evidence[{index}] transport precedes retrieval evidence")
                attempt_join = (
                    attempt.get("request_id"),
                    attempt.get("binding_generation"),
                    attempt.get("stage_seq"),
                    attempt.get("context_hash"),
                    attempt.get("model_request_id"),
                )
                if attempt_join != joined:
                    result.fail(f"request_evidence[{index}] attempt does not match permit")
                if not sha256(attempt.get("request_bytes_sha256")):
                    result.fail(f"request_evidence[{index}] request bytes hash is invalid")
                else:
                    byte_hashes.add(attempt["request_bytes_sha256"])
            if len(byte_hashes) > 1:
                result.fail(f"request_evidence[{index}] retry bytes were not frozen")
            if terminal_position <= max([retrieval_position, *[events.index(attempt) for attempt in attempts]]):
                result.fail(f"request_evidence[{index}] terminal event precedes request activity")
        counts = mapping(row.get("counts"), f"request_evidence[{index}].counts", result)
        if set(counts) != CAPABILITY_KEYS:
            result.fail(f"request_evidence[{index}] capability keys are incomplete or unknown")
        for key, value in counts.items():
            if type(value) is not int or value < 0:
                result.fail(f"request_evidence[{index}] capability {key} is not a non-negative integer")
        expected_attempts = len(attempts)
        if counts.get("replyModelNonLoopback") != expected_attempts:
            result.fail(f"request_evidence[{index}] physical model counter does not match attempts")
        if counts.get("replyModelNonLoopback", 0) > 2:
            result.fail(f"request_evidence[{index}] physical model counter exceeds retry bound")
        if counts.get("ocrBackendLocalProcess", 0) > 1:
            result.fail(f"request_evidence[{index}] OCR process counter exceeds fallback bound")
        for key in FORBIDDEN_CAPABILITIES:
            if counts.get(key) != 0:
                result.fail(f"request_evidence[{index}] forbidden capability {key} is non-zero")
        for key in ("upload_enqueue_count", "upload_attempt_count"):
            if row.get(key) != 0:
                result.fail(f"request_evidence[{index}].{key} must be explicit zero")
        if manifest is not None:
            trace_files = [
                item
                for item in files_of_kind(manifest, "request_trace")
                if item.get("request_evidence_id") == identifier
            ]
            audit_files = [
                item
                for item in files_of_kind(manifest, "request_audit")
                if item.get("request_evidence_id") == identifier
            ]
            if len(trace_files) != 1 or len(audit_files) != 1:
                result.block(
                    f"request_evidence[{index}] requires one trace and one audit file"
                )
            else:
                trace_file = trace_files[0]
                audit_file = audit_files[0]
                if row.get("trace_sha256") != trace_file.get("sha256"):
                    result.fail(f"request_evidence[{index}] trace hash is not file-bound")
                if row.get("audit_sha256") != audit_file.get("sha256"):
                    result.fail(f"request_evidence[{index}] audit hash is not file-bound")
                trace = load_json_file(
                    trace_file["resolved_path"], f"request_evidence[{index}] trace", result
                )
                expected_trace = {
                    "schema_version": 1,
                    "batch_id": manifest["batch_id"],
                    "request_id": row.get("request_id"),
                    "events": row.get("events"),
                }
                if trace is not None and trace != expected_trace:
                    result.fail(f"request_evidence[{index}] trace summary was hand-edited")
                audit = load_json_file(
                    audit_file["resolved_path"], f"request_evidence[{index}] audit", result
                )
                expected_audit = {
                    "schema_version": 1,
                    "batch_id": manifest["batch_id"],
                    "request_id": row.get("request_id"),
                    "counts": row.get("counts"),
                    "logical_model_requests": row.get("logical_model_requests"),
                    "upload_enqueue_count": row.get("upload_enqueue_count"),
                    "upload_attempt_count": row.get("upload_attempt_count"),
                    "collectors": sorted(REQUIRED_COLLECTORS),
                }
                if audit is not None and audit != expected_audit:
                    result.fail(
                        f"request_evidence[{index}] audit counts or collector presence were hand-edited"
                    )
    return evidence_by_id


def scenario_contract(identifier: str, batch_id: str) -> dict[str, Any]:
    contract: dict[str, Any] = {
        "fault_injection": f"acceptance_{identifier.lower().replace('-', '_')}",
        "retrieval_outcome": "none",
        "selected_hit_count": None,
        "terminal": "not_applicable",
        "error_code": None,
        "model_attempts": 0,
        "embedding_attempts": 0,
        "ocr_local_process": 0,
        "upload_enqueue_count": 0,
        "upload_attempt_count": 0,
        "forbidden_capability_total": 0,
        "pet_sentinel_total": 0,
        "candidate_batch_id": batch_id,
    }
    runtime: dict[str, tuple[str, str, int | None, str, str | None, int, int, int]] = {
        "M2-OBS-01": ("normal_success", "success", 1, "reply_ready", None, 1, 0, 0),
        "M2-OBS-02": ("no_hit", "no_hit", 0, "reply_ready", None, 1, 0, 0),
        "M2-OBS-03": ("embedding_unavailable_fts", "fts_fallback", 1, "reply_ready", None, 1, 1, 0),
        "M2-OBS-04": ("model_timeout_then_success", "success", 1, "reply_ready", None, 2, 0, 0),
        "M2-OBS-05": ("concurrent_trigger", "none", None, "failed", "WX_BUSY", 0, 0, 0),
        "M2-OBS-06": ("cancel_late_result", "success", 1, "cancelled", "WX_REQUEST_CANCELLED", 1, 0, 0),
        "M2-CAP-01": ("all_global_capabilities_enabled", "success", 1, "reply_ready", None, 1, 0, 0),
        "M2-CAP-02": ("approved_local_ocr_fallback", "success", 1, "reply_ready", None, 1, 0, 1),
        "M2-CAP-03": ("non_loopback_embedding", "fts_fallback", 1, "reply_ready", None, 1, 1, 0),
        "M2-RET-01": ("kb_not_ready", "none", None, "failed", "KB_NOT_READY", 0, 0, 0),
        "M2-RET-02": ("scope_unresolved", "none", None, "failed", "KB_SCOPE_UNRESOLVED", 0, 0, 0),
        "M2-RET-03": ("embedding_unavailable_fts", "fts_fallback", 1, "reply_ready", None, 1, 1, 0),
        "M2-RET-04": ("sqlite_initial_busy", "none", None, "failed", "KB_RETRIEVAL_FAILED", 0, 0, 0),
        "M2-RET-05": ("sqlite_mid_retrieval_busy", "none", None, "failed", "KB_RETRIEVAL_FAILED", 0, 0, 0),
        "M2-RET-06": ("corrupt_retrieval_payload", "none", None, "failed", "KB_RETRIEVAL_FAILED", 0, 0, 0),
        "M2-RET-07": ("context_assembly_overflow", "none", None, "failed", "KB_RETRIEVAL_FAILED", 0, 0, 0),
        "M2-RET-08": ("binding_invalid_before_retrieval", "none", None, "failed", "WX_REQUEST_STALE", 0, 0, 0),
        "M2-RET-09": ("binding_invalid_before_transport", "success", 1, "failed", "WX_REQUEST_STALE", 0, 0, 0),
        "M2-RET-10": ("generation_switch_during_retrieval", "none", None, "failed", "KB_RETRIEVAL_FAILED", 0, 0, 0),
        "M2-MOD-01": ("timeout_then_success", "success", 1, "reply_ready", None, 2, 0, 0),
        "M2-MOD-02": ("two_timeouts", "success", 1, "failed", "LLM_FAILED", 2, 0, 0),
        "M2-MOD-03": ("retryable_provider_then_success", "success", 1, "reply_ready", None, 2, 0, 0),
        "M2-MOD-04": ("non_retryable_provider_error", "success", 1, "failed", "LLM_FAILED", 1, 0, 0),
        "M2-MOD-05": ("invalid_model_output", "success", 1, "failed", "LLM_FAILED", 1, 0, 0),
        "M2-MOD-06": ("tool_call_response", "success", 1, "failed", "LLM_FAILED", 1, 0, 0),
        "M2-MOD-07": ("late_model_result", "success", 1, "cancelled", "WX_REQUEST_CANCELLED", 1, 0, 0),
    }
    if identifier in runtime:
        (
            fault,
            retrieval,
            hits,
            terminal,
            error,
            model_attempts,
            embedding_attempts,
            ocr_processes,
        ) = runtime[identifier]
        contract.update(
            fault_injection=fault,
            retrieval_outcome=retrieval,
            selected_hit_count=hits,
            terminal=terminal,
            error_code=error,
            model_attempts=model_attempts,
            embedding_attempts=embedding_attempts,
            ocr_local_process=ocr_processes,
        )
    elif identifier.startswith("M2-KB-"):
        contract["fault_injection"] = f"knowledge_generation_{identifier[-2:]}"
    elif identifier.startswith("M2-PRIV-"):
        contract["fault_injection"] = f"privacy_retention_{identifier[-2:]}"
    elif identifier.startswith("M2-PET-"):
        contract["fault_injection"] = f"pet_resource_sentinel_{identifier[-2:]}"
    return contract


def observed_request_contract(row: dict[str, Any], batch_id: str) -> dict[str, Any]:
    events = row.get("events", [])
    retrievals = [event for event in events if event.get("kind") == "retrieval_completed"]
    terminals = [event for event in events if event.get("kind") == "terminal"]
    counts = row.get("counts", {})
    retrieval = retrievals[0] if retrievals else {}
    terminal = terminals[0] if terminals else {}
    return {
        "retrieval_outcome": retrieval.get("outcome", "none"),
        "selected_hit_count": retrieval.get("selected_hit_count"),
        "terminal": terminal.get("outcome"),
        "error_code": terminal.get("error_code"),
        "model_attempts": sum(
            event.get("kind") == "model_transport_started" for event in events
        ),
        "embedding_attempts": counts.get("knowledgeEmbeddingLoopback"),
        "ocr_local_process": counts.get("ocrBackendLocalProcess"),
        "upload_enqueue_count": row.get("upload_enqueue_count"),
        "upload_attempt_count": row.get("upload_attempt_count"),
        "forbidden_capability_total": sum(counts.get(key, 0) for key in FORBIDDEN_CAPABILITIES),
        "candidate_batch_id": batch_id,
    }


def check_scenarios(
    manifest: dict[str, Any] | None,
    evidence_by_id: dict[str, dict[str, Any]],
    result: Result,
) -> dict[str, dict[str, Any]]:
    if manifest is None:
        return {}
    scenarios_by_id: dict[str, dict[str, Any]] = {}
    scenario_ids: set[str] = set()
    used_runtime_evidence: set[str] = set()
    for index, raw in enumerate(manifest["scenarios"]):
        item = mapping(raw, f"evidence_manifest.scenarios[{index}]", result)
        required = {"id", "scenario_id", "status", "request_evidence_ids", "expected", "observed"}
        if set(item) != required:
            result.fail(f"evidence_manifest.scenarios[{index}] fields are missing or unknown")
        identifier = item.get("id")
        scenario_id = item.get("scenario_id")
        if not nonempty(identifier) or identifier in scenarios_by_id:
            result.fail(f"evidence_manifest.scenarios[{index}].id is missing or duplicate")
            continue
        if scenario_id not in FAULT_IDS | AC_IDS or scenario_id in scenario_ids:
            result.fail(f"evidence_manifest.scenarios[{index}].scenario_id is unknown or duplicate")
            continue
        scenarios_by_id[identifier] = item
        scenario_ids.add(scenario_id)
        expected = scenario_contract(scenario_id, manifest["batch_id"])
        if item.get("expected") != expected:
            result.fail(f"scenario {scenario_id} does not use the verifier-owned contract")
        if item.get("observed") != expected:
            result.fail(f"scenario {scenario_id} observed facts do not satisfy its contract")
        if item.get("status") != "pass":
            result.block(f"scenario {scenario_id} is not a collected pass")
        cited = item.get("request_evidence_ids")
        if not isinstance(cited, list) or any(not nonempty(value) for value in cited):
            result.fail(f"scenario {scenario_id} request_evidence_ids must be a list")
            cited = []
        if scenario_id.startswith(RUNTIME_SCENARIO_PREFIXES):
            if len(cited) != 1 or cited[0] not in evidence_by_id:
                result.block(f"scenario {scenario_id} lacks its own request evidence")
                continue
            evidence_id = cited[0]
            if evidence_id in used_runtime_evidence:
                result.fail(f"scenario {scenario_id} reuses request evidence from another scenario")
                continue
            used_runtime_evidence.add(evidence_id)
            observed = observed_request_contract(evidence_by_id[evidence_id], manifest["batch_id"])
            expected_request = {key: value for key, value in expected.items() if key != "fault_injection" and key != "pet_sentinel_total"}
            if observed != expected_request:
                result.fail(f"scenario {scenario_id} request evidence proves different facts")
        elif cited:
            result.fail(f"scenario {scenario_id} must use its non-request collector artifact")
    return scenarios_by_id


def check_matrix(
    document: dict[str, Any],
    key: str,
    required_ids: set[str],
    scenarios_by_id: dict[str, dict[str, Any]],
    result: Result,
) -> None:
    rows = document.get(key)
    if not isinstance(rows, list):
        result.fail(f"{key} must be a list")
        return
    found: set[str] = set()
    for index, item in enumerate(rows):
        row = mapping(item, f"{key}[{index}]", result)
        allowed = {"id", "status", "evidence_ids", "reason"}
        if set(row) - allowed:
            result.fail(f"{key}[{index}] has unknown fields")
        identifier = row.get("id")
        if identifier not in required_ids or identifier in found:
            result.fail(f"{key}[{index}].id is unknown or duplicate")
            continue
        found.add(identifier)
        status = row.get("status")
        cited = row.get("evidence_ids")
        if not isinstance(cited, list) or any(not nonempty(value) for value in cited):
            result.fail(f"{key}[{index}].evidence_ids must be a list")
            cited = []
        if not set(cited).issubset(scenarios_by_id):
            result.block(f"{key}[{index}] cites unknown scenario evidence")
        if status == "pass":
            matching = [
                evidence_id
                for evidence_id in cited
                if scenarios_by_id.get(evidence_id, {}).get("scenario_id") == identifier
            ]
            if len(cited) != 1 or len(matching) != 1:
                result.block(
                    f"{key}[{index}] pass lacks its exact scenario evidence contract"
                )
        elif status == "fail":
            result.fail(f"{key}[{index}] reported fail")
        elif status in {"blocked", "not-run"}:
            result.block(f"{key}[{index}] is {status}")
        elif status == "conditional-not-applicable":
            if identifier not in {"AC-PET-04", "AC-PET-05"} or not nonempty(row.get("reason")):
                result.fail(f"{key}[{index}] has invalid conditional applicability")
        else:
            result.fail(f"{key}[{index}].status is invalid")
    missing = required_ids - found
    if missing:
        result.block(f"{key} is missing rows: {', '.join(sorted(missing))}")


def hash_is_bound(
    manifest: dict[str, Any] | None, kind: str, value: object
) -> bool:
    return any(item.get("sha256") == value for item in files_of_kind(manifest, kind))


def check_supporting_sections(
    document: dict[str, Any], manifest: dict[str, Any] | None, result: Result
) -> None:
    credentials = document.get("credentials_mock_matrix")
    if not isinstance(credentials, list) or not credentials:
        result.block("credentials_mock_matrix is missing")
    else:
        for index, row in enumerate(credentials):
            item = mapping(row, f"credentials_mock_matrix[{index}]", result)
            if item.get("status") == "fail":
                result.fail(f"credentials_mock_matrix[{index}] failed")
            elif item.get("status") != "pass" or not sha256(item.get("evidence_sha256")):
                result.block(f"credentials_mock_matrix[{index}] is not evidenced")
            elif not hash_is_bound(manifest, "credentials_test", item.get("evidence_sha256")):
                result.block(f"credentials_mock_matrix[{index}] hash is not file-bound")

    privacy = mapping(document.get("privacy_scan"), "privacy_scan", result)
    if privacy.get("source_hits") != 0 or privacy.get("evidence_hits") != 0:
        result.fail("privacy scan found sensitive content")
    if privacy.get("package_hits") is None:
        result.block("privacy package scan has not run")
    elif privacy.get("package_hits") != 0:
        result.fail("privacy package scan found sensitive content")
    if not sha256(privacy.get("evidence_sha256")):
        result.block("privacy scan evidence hash is missing")
    elif not hash_is_bound(manifest, "privacy_scan", privacy.get("evidence_sha256")):
        result.block("privacy scan hash is not file-bound")

    sentinels = mapping(document.get("sentinel_counters"), "sentinel_counters", result)
    required_sentinels = {
        "fileAccess",
        "moduleLoad",
        "childProcess",
        "network",
        "syntheticInput",
    }
    if set(sentinels.get("counts", {})) != required_sentinels:
        result.fail("sentinel counter keys are incomplete or unknown")
    elif any(value != 0 for value in sentinels["counts"].values()):
        result.fail("a pet/resource sentinel counter is non-zero")
    if sentinels.get("status") != "pass" or not sha256(sentinels.get("evidence_sha256")):
        result.block("sentinel observation is not real, hashed pass evidence")
    elif not hash_is_bound(manifest, "sentinel_observation", sentinels.get("evidence_sha256")):
        result.block("sentinel observation hash is not file-bound")

    review = mapping(document.get("reference_assets_review"), "reference_assets_review", result)
    if review.get("status") == "fail":
        result.fail("reference/assets review failed")
    elif review.get("status") != "pass" or not sha256(review.get("evidence_sha256")):
        result.block("reference/assets review remains incomplete")
    elif not hash_is_bound(manifest, "assets_review", review.get("evidence_sha256")):
        result.block("reference/assets review hash is not file-bound")


def check_source_boundaries(project_root: Path, result: Result) -> None:
    paths = {
        "observability": project_root / "desktop/src-tauri/src/wechat/observability.rs",
        "model": project_root / "desktop/src-tauri/src/wechat/model_client.rs",
        "reply": project_root / "desktop/src-tauri/src/wechat/reply_flow.rs",
        "embedding": project_root / "desktop/src-tauri/src/knowledge/embedding.rs",
        "retrieve": project_root / "desktop/src-tauri/src/knowledge/retrieve.rs",
    }
    sources: dict[str, str] = {}
    for label, path in paths.items():
        try:
            sources[label] = path.read_text(encoding="utf-8")
        except (OSError, UnicodeError):
            result.fail(f"required source boundary is unreadable: {label}")
    if len(sources) != len(paths):
        return
    required_observability = {
        "deny_unknown_fields",
        "MAX_JSONL_LINE_BYTES",
        "M2AuditKind",
        "CapabilitySnapshot",
        "M2EvidenceCollector",
        "terminal_observed",
        "upload_queue_observed_zero",
        "ocr_local_process_started",
        "WxAuditPersistFailed",
    }
    if not required_observability.issubset(set(re.findall(r"[A-Za-z0-9_]+", sources["observability"]))):
        result.fail("metadata audit schema markers are incomplete")
    transport_impl = sources["model"].split("impl RagSingleTurnTransport for SingleTurnRagTransport", 1)[-1]
    transport_impl = transport_impl.split("pub(super) struct WechatReplyModelClient", 1)[0]
    if transport_impl.find(".record(") < 0 or transport_impl.find(".inner") < 0:
        result.fail("physical transport audit boundary is missing")
    elif transport_impl.find(".record(") > transport_impl.find(".inner"):
        result.fail("physical transport audit is not before the provider transport")
    trait_body = sources["reply"].split("trait M2ReplyTailPort", 1)[-1]
    trait_body = trait_body.split("async fn finish_captured_m2_reply", 1)[0]
    for forbidden in ("AppState", "remote_upload", "reqwest", "shell", "process", "input"):
        if forbidden in trait_body:
            result.fail(f"M2 tail capability boundary contains forbidden dependency {forbidden}")
    for marker in (
        "new_with_m2_audit",
        "m2_audit_sink",
        "retrieval_completed",
        "M2OcrAudit",
        "terminal_observed",
        "upload_queue_observed_zero",
    ):
        if marker not in sources["reply"]:
            result.fail(f"M2 orchestration audit marker is missing: {marker}")
    if "knowledge_retrieve_with_audit" not in sources["retrieve"] or "request.request_id" not in sources["retrieve"]:
        result.fail("retrieval request correlation is not explicit")
    if "record_m2_embedding" not in sources["embedding"]:
        result.fail("embedding request correlation adapter is missing")


def validate(
    document: dict[str, Any],
    project_root: Path,
    evidence_path: Path,
    allow_synthetic_fixture: bool,
) -> Result:
    result = Result()
    if set(document) & FORBIDDEN_POLICY_KEYS:
        result.fail("default-pass, waiver, and authorized-default fields are forbidden")
    unknown = set(document) - TOP_LEVEL_KEYS
    missing = REQUIRED_TOP_LEVEL_KEYS - set(document)
    if unknown:
        result.fail(f"unknown top-level fields: {', '.join(sorted(unknown))}")
    if missing:
        result.fail(f"missing top-level fields: {', '.join(sorted(missing))}")
    if document.get("schema_version") != 1:
        result.fail("schema_version must be 1")
    if not nonempty(document.get("gate_id")) or not nonempty(document.get("generated_at")):
        result.fail("gate_id and generated_at are required")
    manifest = check_manifest(document, evidence_path, allow_synthetic_fixture, result)
    check_candidate(document, project_root, manifest, result)
    check_environment(document, manifest, result)
    evidence_by_id = check_request_evidence(document, manifest, result)
    scenarios_by_id = check_scenarios(manifest, evidence_by_id, result)
    check_matrix(document, "fault_matrix", FAULT_IDS, scenarios_by_id, result)
    check_matrix(document, "ac_matrix", AC_IDS, scenarios_by_id, result)
    check_supporting_sections(document, manifest, result)
    check_source_boundaries(project_root, result)
    declared_blockers = document.get("blockers")
    if not isinstance(declared_blockers, list) or any(not nonempty(item) for item in declared_blockers):
        result.fail("blockers must be a list of non-empty strings")
    if document.get("verdict") not in {"pass", "fail", "blocked"}:
        result.fail("verdict is invalid")
    if document.get("verdict") != result.verdict:
        result.fail(f"declared verdict {document.get('verdict')!r} does not match {result.verdict!r}")
    if result.verdict == "pass" and declared_blockers:
        result.fail("pass verdict cannot declare blockers")
    if result.verdict == "blocked" and not declared_blockers:
        result.fail("blocked verdict must declare blockers")
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-root", type=Path, default=Path("."))
    parser.add_argument("--evidence", type=Path)
    parser.add_argument("--allow-synthetic-fixture", action="store_true")
    args = parser.parse_args()
    evidence = args.evidence or Path("desktop/docs/baselines/work-review-m2-after-gate.json")
    path = evidence if evidence.is_absolute() else args.project_root / evidence
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        print(f"M2_OBSERVABILITY_GATE status=fail error={type(error).__name__}")
        return 1
    if not isinstance(document, dict):
        print("M2_OBSERVABILITY_GATE status=fail error=root-must-be-object")
        return 1
    result = validate(
        document,
        args.project_root.resolve(),
        path.resolve(),
        args.allow_synthetic_fixture,
    )
    print(
        "M2_OBSERVABILITY_GATE "
        f"status={result.verdict} failures={len(result.failures)} blockers={len(result.blockers)}"
    )
    for message in result.failures:
        print(f"failure: {message}")
    for message in result.blockers:
        print(f"blocker: {message}")
    return {"pass": 0, "fail": 1, "blocked": 2}[result.verdict]


if __name__ == "__main__":
    sys.exit(main())
