#!/usr/bin/env python3
"""Behavior tests for the task-27 final release verifier."""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
VERIFIER = ROOT / "kaifa/kaifa_test/verify_final_m2_release_gate.py"
FIXTURE = ROOT / "kaifa/kaifa_test/fixtures/final_release/pass"
COLLECTOR = ROOT / "desktop/scripts/release/collect-final-evidence.ps1"


class FinalReleaseGateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(prefix="task27-final-gate-")
        self.root = Path(self.temp.name)
        for source in FIXTURE.iterdir():
            if source.is_file():
                shutil.copyfile(source, self.root / source.name)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def read(self, name: str) -> dict:
        return json.loads((self.root / name).read_text(encoding="utf-8"))

    def write(self, name: str, value: dict) -> None:
        (self.root / name).write_text(
            json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
        )

    def refresh_hash(self, name: str) -> None:
        digest = hashlib.sha256((self.root / name).read_bytes()).hexdigest()
        manifest = self.read("manifest.json")
        reference = next(ref for ref in manifest["evidenceRefs"] if ref["path"] == name)
        reference["sha256"] = digest
        self.write("manifest.json", manifest)

    def restore(self) -> None:
        for source in FIXTURE.iterdir():
            if source.is_file():
                shutil.copyfile(source, self.root / source.name)

    def run_gate(self, project_root: Path | None = None) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                "python3", "-B", str(VERIFIER),
                "--project-root", str(project_root or self.root),
                "--freeze", str(self.root / "freeze.json"),
                "--manifest", str(self.root / "manifest.json"),
            ],
            check=False,
            capture_output=True,
            text=True,
        )

    def mutate(self, name: str, callback) -> subprocess.CompletedProcess[str]:
        value = self.read(name)
        callback(value)
        self.write(name, value)
        self.refresh_hash(name)
        return self.run_gate()

    def test_pass_fixture_recomputes_raw_evidence(self) -> None:
        result = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_freeze_change_invalidates_join_and_approval(self) -> None:
        freeze = self.read("freeze.json")
        freeze["targetWindows"] = None
        self.write("freeze.json", freeze)
        result = self.run_gate()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("RELEASE_FREEZE_HASH_MISMATCH", result.stdout)

    def test_default_pass_policy_is_failed(self) -> None:
        freeze = self.read("freeze.json")
        freeze["default_pass_requirements"] = True
        self.write("freeze.json", freeze)
        self.assertEqual(self.run_gate().returncode, 1)

    def test_hash_mismatch_is_failed(self) -> None:
        runtime = self.read("runtime.json")
        runtime["runtimeForbiddenHits"] = 1
        self.write("runtime.json", runtime)
        result = self.run_gate()
        self.assertEqual(result.returncode, 1)
        self.assertIn("EVIDENCE_HASH_MISMATCH", result.stdout)

    def test_non_m2_and_double_feature_candidates_are_failed(self) -> None:
        for features in (["custom-protocol"], ["custom-protocol", "wechat-m1"], ["custom-protocol", "wechat-m1", "wechat-m2"]):
            with self.subTest(features=features):
                self.restore()
                result = self.mutate("candidate.json", lambda value: value.update(features=features))
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def test_late_limits_and_tampered_aggregate_are_failed(self) -> None:
        freeze = self.read("freeze.json")
        freeze["approvedAt"] = "2026-08-08T03:00:00Z"
        self.write("freeze.json", freeze)
        self.assertEqual(self.run_gate().returncode, 1)
        self.restore()
        result = self.mutate("evidence.json", lambda value: value["performanceSummary"].update(queryP95Ms=1.0))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def test_missing_uat_category_is_failed(self) -> None:
        result = self.mutate("uat.json", lambda value: value["questions"].pop())
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("BUSINESS_UAT_COVERAGE_INVALID", result.stdout)

    def test_non_object_incomplete_and_self_reported_joins_are_failed(self) -> None:
        mutations = [
            lambda value: value.update(rows=["not-an-object"]),
            lambda value: value["rows"][0].pop("retrievalTerminal"),
            lambda value: value["rows"][0].update(retrievalPhysicalAttempts=0),
        ]
        for mutate in mutations:
            with self.subTest(mutation=mutate):
                self.restore()
                result = self.mutate("audit.json", mutate)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def test_self_reported_release_summary_is_rejected_by_strict_schema(self) -> None:
        result = self.mutate("evidence.json", lambda value: value.update(signatures=True, rebuildDeterministic=True))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("FINAL_EVIDENCE_SCHEMA_INVALID", result.stdout)

    def test_signature_asset_runtime_recovery_rebuild_and_rollback_are_failed(self) -> None:
        mutations = [
            ("signatures.json", lambda value: value.update(updaterSignatureStatus="Invalid")),
            ("assets.json", lambda value: value.update(pendingAssetCount=1)),
            ("runtime.json", lambda value: value.update(runtimeForbiddenHits=1)),
            ("recovery.json", lambda value: value.update(quarantinePreserved=False)),
            ("rebuild.json", lambda value: value["runs"][1].update(logicalDigest="drift")),
            ("rollback.json", lambda value: value.update(targetContract="m1")),
        ]
        for name, mutate in mutations:
            with self.subTest(name=name):
                self.restore()
                result = self.mutate(name, mutate)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def test_any_rehashed_evidence_change_invalidates_approval(self) -> None:
        result = self.mutate("runtime.json", lambda value: value.update(runtimeForbiddenHits=1))
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("RELEASE_APPROVAL_INVALIDATED", result.stdout)

    def test_fixture_cannot_be_used_outside_declared_project_root(self) -> None:
        result = self.run_gate(ROOT)
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("RELEASE_MANIFEST_PATH_INVALID", result.stdout)

    def test_path_escape_and_symlink_are_failed(self) -> None:
        manifest = self.read("manifest.json")
        manifest["evidenceRefs"][0]["path"] = "../evidence.json"
        self.write("manifest.json", manifest)
        self.assertEqual(self.run_gate().returncode, 1)
        self.restore()
        (self.root / "evidence.json").unlink()
        (self.root / "evidence.json").symlink_to(FIXTURE / "evidence.json")
        self.assertEqual(self.run_gate().returncode, 1)

    def test_duplicate_evidence_kind_is_failed(self) -> None:
        manifest = self.read("manifest.json")
        duplicate = dict(next(ref for ref in manifest["evidenceRefs"] if ref["kind"] == "runtimeAudit"))
        duplicate["path"] = "runtime-copy.json"
        shutil.copyfile(self.root / "runtime.json", self.root / duplicate["path"])
        manifest["evidenceRefs"].append(duplicate)
        self.write("manifest.json", manifest)
        result = self.run_gate()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("EVIDENCE_DUPLICATE", result.stdout)

    def test_windows_collector_source_is_picker_owned_recursive_and_atomic(self) -> None:
        source = COLLECTOR.read_text(encoding="utf-8")
        self.assertNotIn("[string]$EvidenceRoot", source)
        for required in (
            "BrowseForFolder", "ReleaseBatchId -notmatch", "Test-RelatedPath",
            "ReparsePoint", "Test-SafeMetadata", "OrdinalIgnoreCase",
            "MetadataInput exceeds maximum depth", ".pending-", "Flush($true)",
            "Move-Item -LiteralPath $pending -Destination $batchRoot",
        ):
            self.assertIn(required, source)


if __name__ == "__main__":
    unittest.main()
