#!/usr/bin/env python3
"""Fail-closed final M2 release evidence verifier (stdlib only).

Exit codes are part of the release contract: 0=pass, 1=fail, 2=blocked.
The verifier never starts the product, follows symlinks, or trusts precomputed
performance summaries.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from datetime import datetime
from pathlib import Path
from typing import Any


REQUIRED_FREEZE = {
    "approvedAt",
    "approvedBy",
    "targetWindows",
    "wechatProfile",
    "dataset",
    "performanceLimits",
    "productIdentity",
    "assetLedgerSha256",
    "replyModel",
    "embedding",
    "contentRetentionDays",
    "authenticode",
    "updater",
    "releaseChannel",
}
REQUIRED_SECTIONS = {
    "candidate",
    "environment",
    "performance",
    "businessUat",
    "m2Audit",
    "rebuild",
    "recovery",
    "installer",
    "signatures",
    "runtimeAudit",
    "assetAudit",
    "rollback",
    "releaseNotes",
    "approvals",
}
PERFORMANCE_METRICS = {
    "firstIndexMs",
    "stableKnowledgeBytes",
    "derivedTotalBytes",
    "peakWorkingSetBytes",
    "queryP50Ms",
    "queryP95Ms",
    "schedulerDriftP95Ms",
    "uiInputLatencyP95Ms",
}
UAT_CATEGORIES = {
    "singleConversation",
    "selectedConversations",
    "globalConfirmed",
    "noHit",
    "chineseKeyword",
    "chineseSemantic",
    "synonym",
    "sourceReview",
}
FORBIDDEN_POLICY_KEYS = {
    "defaultpassrequirements",
    "default_pass_requirements",
    "authorizeddefaults",
    "authorized_defaults",
    "waiver",
    "selfsigned",
    "self_signed",
    "placeholder",
}
ALLOWED_STATUS = {"pass", "fail", "blocked", "not-run"}
REQUIRED_EVIDENCE_KINDS = {
    "finalEvidence",
    "m2ContractFreeze",
    "candidateMetadata",
    "candidateArtifact",
    "environmentAttestation",
    "businessUat",
    "m2Audit",
    "rebuildSnapshots",
    "recoveryReceipt",
    "signatureOutput",
    "installerLifecycle",
    "runtimeAudit",
    "assetAudit",
    "rollbackReceipt",
    "approvals",
    "releaseNotesReview",
}
JOIN_FIELDS = {
    "requestId",
    "releaseBatchId",
    "candidateCommit",
    "candidateTree",
    "retrievalStageSeq",
    "modelPermitStageSeq",
    "retrievalTerminal",
    "retrievalPhysicalAttempts",
    "retrievalFailureModelCalls",
    "modelCalls",
}


class Gate:
    def __init__(self) -> None:
        self.failures: list[str] = []
        self.blockers: list[str] = []

    def fail(self, code: str) -> None:
        if code not in self.failures:
            self.failures.append(code)

    def block(self, code: str) -> None:
        if code not in self.blockers:
            self.blockers.append(code)


def load_json(path: Path, gate: Gate, code: str) -> Any | None:
    try:
        if path.is_symlink() or not path.is_file():
            gate.fail(f"{code}_PATH_INVALID")
            return None
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError):
        gate.fail(f"{code}_INVALID_JSON")
        return None


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def exact_object(value: Any, fields: set[str], gate: Gate, code: str) -> bool:
    if not isinstance(value, dict) or set(value) != fields:
        gate.fail(code)
        return False
    return True


def sha256_canonical(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def safe_input_path(project_root: Path, path: Path, gate: Gate, code: str) -> Path | None:
    try:
        root = project_root.resolve(strict=True)
        resolved = path.resolve(strict=True)
    except OSError:
        gate.fail(f"{code}_PATH_INVALID")
        return None
    if path.is_symlink() or not path.is_file() or resolved != root and root not in resolved.parents:
        gate.fail(f"{code}_PATH_INVALID")
        return None
    return resolved


def parse_time(value: Any) -> datetime | None:
    if not isinstance(value, str):
        return None
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None


def has_forbidden_policy(value: Any) -> bool:
    if isinstance(value, dict):
        for key, nested in value.items():
            normalized = str(key).replace("-", "").lower()
            if normalized in {item.replace("-", "") for item in FORBIDDEN_POLICY_KEYS}:
                return True
            if has_forbidden_policy(nested):
                return True
    elif isinstance(value, list):
        return any(has_forbidden_policy(item) for item in value)
    elif isinstance(value, str):
        lowered = value.lower()
        return any(token in lowered for token in FORBIDDEN_POLICY_KEYS)
    return False


def nearest_rank(values: list[float], percentile: float) -> float:
    ordered = sorted(values)
    return ordered[max(0, math.ceil(percentile * len(ordered)) - 1)]


def validate_freeze(freeze: Any, gate: Gate) -> None:
    if not exact_object(freeze, REQUIRED_FREEZE | {"schemaVersion"}, gate, "RELEASE_FREEZE_SCHEMA_INVALID"):
        return
    if freeze.get("schemaVersion") != 1:
        gate.fail("RELEASE_FREEZE_SCHEMA_INVALID")
        return
    missing = REQUIRED_FREEZE - freeze.keys()
    if missing:
        gate.fail("RELEASE_FREEZE_FIELDS_MISSING")
    for key in REQUIRED_FREEZE:
        value = freeze.get(key)
        if value is None or value == "" or value == [] or value == {}:
            gate.block(f"RELEASE_INPUT_UNFROZEN_{key.upper()}")
    approved_at = parse_time(freeze.get("approvedAt"))
    if freeze.get("approvedAt") is not None and approved_at is None:
        gate.fail("RELEASE_APPROVAL_TIME_INVALID")
    if has_forbidden_policy(freeze):
        gate.fail("RELEASE_FREEZE_POLICY_BYPASS")
    limits = freeze.get("performanceLimits")
    if isinstance(limits, dict):
        if set(limits) != PERFORMANCE_METRICS or any(
            not isinstance(value, (int, float)) or isinstance(value, bool) or value <= 0
            for value in limits.values()
        ):
            gate.fail("PERFORMANCE_LIMITS_INVALID")


def load_evidence(
    manifest_path: Path, manifest: dict[str, Any], gate: Gate
) -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    root = manifest_path.parent.resolve()
    payloads: dict[str, Any] = {}
    references: dict[str, dict[str, Any]] = {}
    seen_paths: set[Path] = set()
    seen_hashes: set[str] = set()
    refs = manifest.get("evidenceRefs")
    if not isinstance(refs, list) or not refs:
        gate.block("RELEASE_EVIDENCE_MISSING")
        return payloads, references
    if {str(ref.get("kind")) for ref in refs if isinstance(ref, dict)} != REQUIRED_EVIDENCE_KINDS:
        gate.fail("EVIDENCE_KIND_SET_INVALID")
    for ref in refs:
        if not isinstance(ref, dict) or set(ref) != {"kind", "path", "sha256"}:
            gate.fail("EVIDENCE_REFERENCE_INVALID")
            continue
        relative = Path(str(ref["path"]))
        if relative.is_absolute() or ".." in relative.parts:
            gate.fail("EVIDENCE_PATH_ESCAPE")
            continue
        candidate = manifest_path.parent / relative
        try:
            resolved = candidate.resolve(strict=True)
        except OSError:
            gate.fail("EVIDENCE_FILE_MISSING")
            continue
        if candidate.is_symlink() or root not in resolved.parents:
            gate.fail("EVIDENCE_PATH_ESCAPE")
            continue
        if resolved in seen_paths or str(ref["sha256"]) in seen_hashes:
            gate.fail("EVIDENCE_DUPLICATE_PAYLOAD")
            continue
        seen_paths.add(resolved)
        seen_hashes.add(str(ref["sha256"]))
        if sha256_file(resolved) != ref["sha256"]:
            gate.fail("EVIDENCE_HASH_MISMATCH")
            continue
        kind = str(ref["kind"])
        if kind in references:
            gate.fail("EVIDENCE_DUPLICATE_KIND")
            continue
        references[kind] = {"kind": kind, "path": str(ref["path"]), "sha256": str(ref["sha256"])}
        if kind == "candidateArtifact":
            payloads[kind] = {"sha256": str(ref["sha256"]), "bytes": resolved.stat().st_size}
            continue
        payload = load_json(resolved, gate, "EVIDENCE")
        if payload is not None:
            payloads[kind] = payload
    return payloads, references


def validate_performance(freeze: dict[str, Any], evidence: dict[str, Any], gate: Gate) -> None:
    samples = evidence.get("performanceSamples")
    limits = freeze.get("performanceLimits")
    if not isinstance(samples, dict) or not isinstance(limits, dict):
        gate.block("PERFORMANCE_RAW_SAMPLES_MISSING")
        return
    if set(samples) != PERFORMANCE_METRICS:
        gate.fail("PERFORMANCE_SAMPLE_SET_INVALID")
        return
    computed: dict[str, float] = {}
    for metric, raw in samples.items():
        if not isinstance(raw, list) or not raw or any(
            not isinstance(value, (int, float)) or isinstance(value, bool) or value < 0
            for value in raw
        ):
            gate.fail("PERFORMANCE_RAW_SAMPLES_INVALID")
            return
        values = [float(value) for value in raw]
        if metric.endswith("P50Ms"):
            computed[metric] = nearest_rank(values, 0.50)
        elif metric.endswith("P95Ms"):
            computed[metric] = nearest_rank(values, 0.95)
        else:
            computed[metric] = max(values)
    summary = evidence.get("performanceSummary")
    if summary != computed:
        gate.fail("PERFORMANCE_AGGREGATE_MISMATCH")
    if any(computed[key] > float(limits[key]) for key in PERFORMANCE_METRICS):
        gate.fail("PERFORMANCE_LIMIT_EXCEEDED")
    if parse_time(freeze.get("approvedAt")) is None or parse_time(evidence.get("capturedAt")) is None:
        gate.fail("PERFORMANCE_TIMESTAMPS_INVALID")
    elif parse_time(freeze["approvedAt"]) > parse_time(evidence["capturedAt"]):
        gate.fail("PERFORMANCE_LIMITS_APPROVED_LATE")


def validate_release(
    freeze: dict[str, Any],
    freeze_sha256: str,
    manifest: dict[str, Any],
    payloads: dict[str, Any],
    references: dict[str, dict[str, Any]],
    gate: Gate,
) -> None:
    batch = manifest.get("releaseBatchId")
    if not isinstance(batch, str) or not batch:
        gate.fail("RELEASE_BATCH_INVALID")
    final = payloads.get("finalEvidence")
    if not exact_object(
        final,
        {"schemaVersion", "releaseBatchId", "capturedAt", "releaseFreezeSha256", "performanceSamples", "performanceSummary"},
        gate,
        "FINAL_EVIDENCE_SCHEMA_INVALID",
    ):
        return
    if final.get("schemaVersion") != 1 or final.get("releaseBatchId") != batch:
        gate.fail("EVIDENCE_CROSS_BATCH")
    if final.get("releaseFreezeSha256") != freeze_sha256:
        gate.fail("RELEASE_FREEZE_HASH_MISMATCH")
    validate_performance(freeze, final, gate)

    contract = payloads.get("m2ContractFreeze")
    contract_fields = {
        "schemaVersion", "candidateCommit", "candidateTree", "featureMarker",
        "wechatProfileSha256", "promptSchemaSha256", "embeddingFingerprint", "contractHash",
    }
    if not exact_object(contract, contract_fields, gate, "M2_CONTRACT_FREEZE_SCHEMA_INVALID"):
        return
    candidate = payloads.get("candidateMetadata")
    candidate_fields = {
        "schemaVersion", "releaseBatchId", "candidateCommit", "candidateTree", "featureMarker",
        "features", "m2Contract", "contractHash", "artifactSha256", "nsisSha256", "updaterSha256",
    }
    if not exact_object(candidate, candidate_fields, gate, "CANDIDATE_SCHEMA_INVALID"):
        return
    artifact = payloads.get("candidateArtifact")
    if candidate.get("releaseBatchId") != batch:
        gate.fail("EVIDENCE_CROSS_BATCH")
    if candidate.get("features") != ["custom-protocol", "wechat-m2"]:
        gate.fail("CANDIDATE_FEATURE_SET_INVALID")
    if candidate.get("m2Contract") != "forced-rag" or not candidate.get("contractHash"):
        gate.fail("M2_CONTRACT_IDENTITY_INVALID")
    for field in ("candidateCommit", "candidateTree", "featureMarker", "contractHash"):
        if candidate.get(field) != contract.get(field):
            gate.fail("M2_CONTRACT_IDENTITY_INVALID")
    if not isinstance(artifact, dict) or candidate.get("artifactSha256") != artifact.get("sha256"):
        gate.fail("CANDIDATE_ARTIFACT_HASH_MISMATCH")
    embedding = freeze.get("embedding")
    profile = freeze.get("wechatProfile")
    if (
        not isinstance(embedding, dict)
        or contract.get("embeddingFingerprint") != embedding.get("fingerprint")
        or not isinstance(profile, dict)
        or contract.get("wechatProfileSha256") != profile.get("sha256")
    ):
        gate.fail("M2_CONTRACT_FREEZE_MISMATCH")

    environment = payloads.get("environmentAttestation")
    environment_fields = {"schemaVersion", "releaseBatchId", "candidateCommit", "targetWindowsSha256", "datasetSha256", "wechatProfileSha256"}
    if not exact_object(environment, environment_fields, gate, "ENVIRONMENT_ATTESTATION_SCHEMA_INVALID") or (
        environment.get("releaseBatchId") != batch
        or environment.get("candidateCommit") != candidate.get("candidateCommit")
        or environment.get("targetWindowsSha256") != sha256_canonical(freeze.get("targetWindows"))
        or environment.get("datasetSha256") != sha256_canonical(freeze.get("dataset"))
        or environment.get("wechatProfileSha256") != sha256_canonical(freeze.get("wechatProfile"))
    ):
        gate.fail("ENVIRONMENT_ATTESTATION_INVALID")

    uat = payloads.get("businessUat")
    if not exact_object(uat, {"schemaVersion", "releaseBatchId", "questions"}, gate, "BUSINESS_UAT_SCHEMA_INVALID"):
        return
    questions = uat.get("questions")
    question_fields = {"questionId", "category", "requestId", "result"}
    if (
        uat.get("releaseBatchId") != batch
        or not isinstance(questions, list)
        or not questions
        or any(not isinstance(row, dict) or set(row) != question_fields for row in questions)
        or len({row["questionId"] for row in questions if isinstance(row, dict)}) != len(questions)
        or {row.get("category") for row in questions if isinstance(row, dict)} != UAT_CATEGORIES
        or any(row.get("result") != "pass" for row in questions if isinstance(row, dict))
    ):
        gate.fail("BUSINESS_UAT_COVERAGE_INVALID")

    audit = payloads.get("m2Audit")
    if not exact_object(audit, {"schemaVersion", "releaseBatchId", "rows"}, gate, "M2_AUDIT_SCHEMA_INVALID"):
        return
    joins = audit.get("rows")
    if not isinstance(joins, list) or not joins:
        gate.block("M2_REQUEST_AUDIT_MISSING")
    elif any(not isinstance(row, dict) or set(row) != JOIN_FIELDS for row in joins):
        gate.fail("M2_REQUEST_JOIN_SCHEMA_INVALID")
    elif (
        audit.get("releaseBatchId") != batch
        or {row.get("requestId") for row in joins} != {row.get("requestId") for row in questions}
        or any(
            row.get("releaseBatchId") != batch
            or row.get("candidateCommit") != candidate.get("candidateCommit")
            or row.get("candidateTree") != candidate.get("candidateTree")
            or row.get("retrievalStageSeq") != 5
            or row.get("modelPermitStageSeq") != 6
            or row.get("retrievalTerminal") is not True
            or not isinstance(row.get("retrievalPhysicalAttempts"), int)
            or row.get("retrievalPhysicalAttempts") < 1
            or row.get("retrievalFailureModelCalls") != 0
            or row.get("modelCalls") != 1
            for row in joins
        )
    ):
        gate.fail("M2_REQUEST_JOIN_INVALID")

    rebuild = payloads.get("rebuildSnapshots")
    if not exact_object(rebuild, {"schemaVersion", "releaseBatchId", "runs"}, gate, "REBUILD_SCHEMA_INVALID"):
        return
    runs = rebuild.get("runs")
    run_fields = {"mode", "run", "logicalDigest", "sourcePolicyDigest", "indexSpecDigest"}
    grouped: dict[str, list[dict[str, Any]]] = {}
    if isinstance(runs, list):
        for row in runs:
            if not isinstance(row, dict) or set(row) != run_fields:
                gate.fail("REBUILD_SCHEMA_INVALID")
                break
            grouped.setdefault(str(row.get("mode")), []).append(row)
    if rebuild.get("releaseBatchId") != batch or set(grouped) != {"selected", "full", "incremental"} or any(
        len(rows) != 2
        or {row.get("run") for row in rows} != {1, 2}
        or len({(row.get("logicalDigest"), row.get("sourcePolicyDigest"), row.get("indexSpecDigest")) for row in rows}) != 1
        for rows in grouped.values()
    ):
        gate.fail("REBUILD_DETERMINISM_FAILED")

    recovery = payloads.get("recoveryReceipt")
    recovery_fields = {"schemaVersion", "releaseBatchId", "candidateCommit", "reopened", "integrity", "configSha256", "databaseSha256", "quarantinePreserved"}
    if not exact_object(recovery, recovery_fields, gate, "RECOVERY_RECEIPT_SCHEMA_INVALID"):
        gate.block("RECOVERY_BUNDLE_UNVERIFIED")
    elif (
        recovery.get("releaseBatchId") != batch
        or recovery.get("candidateCommit") != candidate.get("candidateCommit")
        or recovery.get("reopened") is not True
        or recovery.get("integrity") != "ok"
        or recovery.get("quarantinePreserved") is not True
        or not recovery.get("configSha256")
        or not recovery.get("databaseSha256")
    ):
        gate.fail("RECOVERY_BACKUP_FAILED")

    signatures = payloads.get("signatureOutput")
    signature_fields = {"schemaVersion", "releaseBatchId", "candidateSha256", "nsisSha256", "updaterSha256", "authenticodeStatus", "signerThumbprint", "timestampStatus", "updaterSignatureStatus"}
    if not exact_object(signatures, signature_fields, gate, "SIGNATURE_OUTPUT_SCHEMA_INVALID"):
        gate.block("SIGNATURE_CHAIN_UNVERIFIED")
    elif (
        signatures.get("releaseBatchId") != batch
        or signatures.get("candidateSha256") != candidate.get("artifactSha256")
        or signatures.get("nsisSha256") != candidate.get("nsisSha256")
        or signatures.get("updaterSha256") != candidate.get("updaterSha256")
        or signatures.get("authenticodeStatus") != "Valid"
        or signatures.get("timestampStatus") != "Valid"
        or signatures.get("updaterSignatureStatus") != "Valid"
        or not isinstance(freeze.get("authenticode"), dict)
        or signatures.get("signerThumbprint") != freeze["authenticode"].get("thumbprint")
    ):
        gate.fail("SIGNATURE_CHAIN_INVALID")

    installer = payloads.get("installerLifecycle")
    installer_fields = {"schemaVersion", "releaseBatchId", "candidateSha256", "nsisSha256", "updaterSha256", "checks"}
    required_checks = {"cleanInstall", "upgrade", "updaterUpgrade", "singleInstance", "autostart", "uninstall", "reinstall"}
    if not exact_object(installer, installer_fields, gate, "INSTALLER_LIFECYCLE_SCHEMA_INVALID") or (
        installer.get("releaseBatchId") != batch
        or installer.get("candidateSha256") != candidate.get("artifactSha256")
        or installer.get("nsisSha256") != candidate.get("nsisSha256")
        or installer.get("updaterSha256") != candidate.get("updaterSha256")
        or not isinstance(installer.get("checks"), list)
        or {row.get("name") for row in installer.get("checks", []) if isinstance(row, dict)} != required_checks
        or any(not isinstance(row, dict) or set(row) != {"name", "result"} or row.get("result") != "pass" for row in installer.get("checks", []))
    ):
        gate.fail("INSTALLER_LIFECYCLE_INVALID")

    runtime = payloads.get("runtimeAudit")
    if not exact_object(runtime, {"schemaVersion", "releaseBatchId", "candidateSha256", "runtimeForbiddenHits", "packageForbiddenHits"}, gate, "RUNTIME_AUDIT_SCHEMA_INVALID"):
        return
    if runtime.get("releaseBatchId") != batch or runtime.get("candidateSha256") != candidate.get("artifactSha256") or runtime.get("runtimeForbiddenHits") != 0 or runtime.get("packageForbiddenHits") != 0:
        gate.fail("RELEASE_ISOLATION_FAILED")

    assets = payloads.get("assetAudit")
    if not exact_object(assets, {"schemaVersion", "releaseBatchId", "assetLedgerSha256", "pendingAssetCount"}, gate, "ASSET_AUDIT_SCHEMA_INVALID"):
        return
    if assets.get("releaseBatchId") != batch or assets.get("assetLedgerSha256") != freeze.get("assetLedgerSha256") or assets.get("pendingAssetCount") != 0:
        gate.fail("ASSET_AUTHORIZATION_INCOMPLETE")

    rollback = payloads.get("rollbackReceipt")
    rollback_fields = {"schemaVersion", "releaseBatchId", "targetContract", "lkgGatePassed", "candidateCommit", "candidateTree", "nsisSha256", "updaterSha256"}
    if not exact_object(rollback, rollback_fields, gate, "ROLLBACK_RECEIPT_SCHEMA_INVALID") or (
        rollback.get("releaseBatchId") != batch
        or rollback.get("targetContract") != "forced-rag"
        or rollback.get("lkgGatePassed") is not True
        or rollback.get("candidateCommit") != candidate.get("candidateCommit")
        or rollback.get("candidateTree") != candidate.get("candidateTree")
        or rollback.get("nsisSha256") != candidate.get("nsisSha256")
        or rollback.get("updaterSha256") != candidate.get("updaterSha256")
    ):
        gate.fail("ROLLBACK_TARGET_INVALID")

    approval_payload = payloads.get("approvals")
    if not exact_object(approval_payload, {"schemaVersion", "releaseBatchId", "approvals"}, gate, "APPROVAL_SCHEMA_INVALID"):
        return
    approvals = approval_payload.get("approvals")
    binding = sha256_canonical({
        "releaseBatchId": batch,
        "releaseFreezeSha256": freeze_sha256,
        "evidenceRefs": [references[kind] for kind in sorted(references) if kind != "approvals"],
    })
    if not isinstance(approvals, list) or not approvals:
        gate.block("RELEASE_APPROVALS_MISSING")
    elif approval_payload.get("releaseBatchId") != batch or any(
        not isinstance(row, dict)
        or set(row) != {"role", "decision", "evidenceSetSha256"}
        or row.get("decision") != "pass"
        or row.get("evidenceSetSha256") != binding
        for row in approvals
    ):
        gate.fail("RELEASE_APPROVAL_INVALIDATED")

    notes = payloads.get("releaseNotesReview")
    if not exact_object(notes, {"schemaVersion", "releaseBatchId", "candidateCommit", "reviewed"}, gate, "RELEASE_NOTES_SCHEMA_INVALID") or (
        notes.get("releaseBatchId") != batch
        or notes.get("candidateCommit") != candidate.get("candidateCommit")
        or notes.get("reviewed") is not True
    ):
        gate.fail("RELEASE_NOTES_REVIEW_INVALID")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-root", required=True, type=Path)
    parser.add_argument("--freeze", required=True, type=Path)
    parser.add_argument("--manifest", required=True, type=Path)
    args = parser.parse_args()
    gate = Gate()
    freeze_path = safe_input_path(args.project_root, args.freeze, gate, "RELEASE_FREEZE")
    manifest_path = safe_input_path(args.project_root, args.manifest, gate, "RELEASE_MANIFEST")
    freeze = load_json(freeze_path, gate, "RELEASE_FREEZE") if freeze_path else None
    manifest = load_json(manifest_path, gate, "RELEASE_MANIFEST") if manifest_path else None
    if freeze is not None:
        validate_freeze(freeze, gate)
    if isinstance(manifest, dict):
        if not exact_object(
            manifest,
            {"schemaVersion", "releaseBatchId", "verdict", "blockers", "sections", "evidenceRefs"},
            gate,
            "RELEASE_MANIFEST_SCHEMA_INVALID",
        ) or manifest.get("schemaVersion") != 1:
            gate.fail("RELEASE_MANIFEST_SCHEMA_INVALID")
        if has_forbidden_policy(manifest):
            gate.fail("RELEASE_MANIFEST_POLICY_BYPASS")
        sections = manifest.get("sections")
        if not isinstance(sections, dict) or set(sections) != REQUIRED_SECTIONS:
            gate.fail("RELEASE_SECTION_SET_INVALID")
        else:
            for status in sections.values():
                if status not in ALLOWED_STATUS:
                    gate.fail("RELEASE_SECTION_STATUS_INVALID")
                elif status == "fail":
                    gate.fail("RELEASE_SECTION_FAILED")
                elif status != "pass":
                    gate.block("RELEASE_SECTION_INCOMPLETE")
        if manifest.get("verdict") == "pass" and manifest.get("blockers"):
            gate.fail("RELEASE_VERDICT_CONTRADICTS_BLOCKERS")
        evidence_payloads, evidence_references = load_evidence(manifest_path, manifest, gate)
        evidence = evidence_payloads.get("finalEvidence")
        if isinstance(freeze, dict) and isinstance(evidence, dict) and freeze_path:
            validate_release(
                freeze,
                sha256_file(freeze_path),
                manifest,
                evidence_payloads,
                evidence_references,
                gate,
            )
        elif not gate.failures:
            gate.block("FINAL_EVIDENCE_MISSING")
    result = "fail" if gate.failures else "blocked" if gate.blockers else "pass"
    expected_verdict = manifest.get("verdict") if isinstance(manifest, dict) else None
    if expected_verdict in ALLOWED_STATUS and expected_verdict != result:
        gate.fail("RELEASE_VERDICT_MISMATCH")
        result = "fail"
    output = {"schemaVersion": 1, "verdict": result, "failures": gate.failures, "blockers": gate.blockers}
    print(json.dumps(output, ensure_ascii=False, sort_keys=True))
    return {"pass": 0, "fail": 1, "blocked": 2}[result]


if __name__ == "__main__":
    sys.exit(main())
