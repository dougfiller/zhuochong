#!/usr/bin/env python3
"""Validate the Work Review pre-change regression baseline without running the product."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path, PurePosixPath


EXPECTED_ITEMS = {
    *(f"BASE-{number:02d}" for number in range(1, 11)),
    "BASE-AUTO-FE",
    "BASE-AUTO-BUILD",
    "BASE-AUTO-RUST-CHECK",
    "BASE-AUTO-RUST-CLIPPY",
    "BASE-AUTO-RUST-TEST",
}
EXPECTED_COMMANDS = {
    "AUTO-FE",
    "AUTO-BUILD",
    "AUTO-RUST-CHECK",
    "AUTO-RUST-CLIPPY",
    "AUTO-RUST-TEST",
}
ALLOWED_STATUSES = {"pass", "fail", "blocked", "conditional-pass", "not-run"}
MATRIX_FIELDS = {
    "ac_base",
    "capability",
    "entry_and_core_files",
    "data_dependencies",
    "before_method",
    "before_evidence",
    "before_status",
    "before_reason",
    "after_method",
    "after_evidence",
    "after_status",
    "credential_mode",
    "wechat_rag_risk",
    "release_blocker",
    "known_issue",
}
EXCLUDED_DIRECTORY_NAMES = {".git", ".cache", "__pycache__", "dist", "node_modules", "runtime-data", "target"}
EXCLUDED_FILE_SUFFIXES = {".db", ".dll", ".key", ".log", ".p12", ".pem", ".pfx", ".sqlite", ".sqlite3"}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_sha256(value: object) -> str:
    encoded = json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode("utf-8")
    return hashlib.sha256(encoded).hexdigest()


def is_excluded(relative: PurePosixPath) -> bool:
    return (
        relative.parts[:2] == ("docs", "baselines")
        or any(part in EXCLUDED_DIRECTORY_NAMES for part in relative.parts[:-1])
        or relative.name == ".env"
        or relative.name.startswith(".env.")
        or relative.suffix.lower() in EXCLUDED_FILE_SUFFIXES
    )


def frozen_source_files(desktop: Path) -> list[dict[str, object]]:
    files: list[dict[str, object]] = []
    for path in desktop.rglob("*"):
        if not path.is_file() or path.is_symlink():
            continue
        relative = PurePosixPath(path.relative_to(desktop).as_posix())
        if is_excluded(relative):
            continue
        files.append({"path": str(relative), "bytes": path.stat().st_size, "sha256": sha256(path)})
    return sorted(files, key=lambda item: str(item["path"]).encode("utf-8"))


def section_text(matrix: str, identifier: str) -> str:
    marker = f"## {identifier}\n"
    start = matrix.find(marker)
    if start < 0:
        return ""
    end = matrix.find("\n## ", start + len(marker))
    return matrix[start:] if end < 0 else matrix[start:end]


def matrix_fields(section: str) -> dict[str, str]:
    fields: dict[str, str] = {}
    for line in section.splitlines():
        if not line.startswith("- **") or "**: " not in line:
            continue
        field, value = line[4:].split("**: ", 1)
        fields[field] = value
    return fields


def matrix_evidence(item: dict[str, object], commands: dict[str, dict[str, object]]) -> list[str]:
    evidence_ids = item.get("before_evidence", [])
    if not isinstance(evidence_ids, list):
        return []
    paths: list[str] = []
    for command_id in evidence_ids:
        command = commands.get(command_id)
        if command is None:
            continue
        evidence = command.get("evidence")
        if isinstance(evidence, str) and evidence.startswith("docs/baselines/"):
            paths.append(evidence.removeprefix("docs/baselines/"))
    return paths


def validate_matrix_consistency(
    items: list[dict[str, object]], matrix: str, commands: dict[str, dict[str, object]]
) -> list[str]:
    errors: list[str] = []
    for item in items:
        identifier = item["id"]
        section = section_text(matrix, identifier)
        if not section:
            errors.append(f"matrix row {identifier} is missing")
            continue
        fields = matrix_fields(section)
        missing_fields = [field for field in MATRIX_FIELDS if field not in fields]
        if missing_fields:
            errors.append(f"matrix row {identifier} misses fields: {', '.join(sorted(missing_fields))}")
            continue
        for field in ("before_status", "after_status", "release_blocker", "known_issue"):
            if fields[field] != item.get(field):
                errors.append(f"matrix row {identifier} {field} differs from the baseline JSON")
        expected_evidence = matrix_evidence(item, commands)
        actual_evidence = [] if fields["before_evidence"].startswith("无") else [fields["before_evidence"].strip("`")]
        if actual_evidence != expected_evidence:
            errors.append(f"matrix row {identifier} before_evidence or command ID differs from the baseline JSON")
    return errors


def validate(project_root: Path, matrix_override: Path | None = None) -> list[str]:
    errors: list[str] = []
    desktop = project_root / "desktop"
    baseline_dir = desktop / "docs" / "baselines"
    manifest_path = baseline_dir / "work-review-source.json"
    matrix_path = matrix_override or baseline_dir / "work-review-inheritance-matrix.md"
    result_path = baseline_dir / "work-review-regression-baseline.json"
    for path in (manifest_path, matrix_path, result_path):
        if not path.is_file():
            errors.append(f"missing required artifact: {path.relative_to(project_root)}")
    if errors:
        return errors

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    result = json.loads(result_path.read_text(encoding="utf-8"))
    matrix = matrix_path.read_text(encoding="utf-8")
    source = result.get("source", {})
    manifest_source = manifest.get("source", {})
    manifest_files = manifest.get("snapshot", {}).get("files", [])
    if result.get("schema_version") != 1:
        errors.append("baseline schema_version must be 1")
    if result.get("baseline_id") != "work-review-v1.1.0-before-wechat-rag-20260805":
        errors.append("baseline_id is not the fixed pre-WeChat/RAG baseline")
    for key, expected in {
        "tag": "v1.1.0",
        "commit": "500f9d2cb3027392cfcc32ad18395dfe348fb4a1",
        "source_file_count": 580,
        "source_total_bytes": 44961912,
        "files_sha256": "31dd2192f602ee0b4d6f659311186d2230416e42357744ac8c57e778f20cb14a",
    }.items():
        if source.get(key) != expected:
            errors.append(f"baseline source.{key} is not the frozen value")
    if manifest_source.get("resolved_tag") != source.get("tag") or manifest_source.get("resolved_commit") != source.get("commit"):
        errors.append("baseline source identity does not match the source manifest")
    if len(manifest_files) != source.get("source_file_count") or sum(item.get("bytes", -1) for item in manifest_files) != source.get("source_total_bytes"):
        errors.append("source manifest count or byte total does not match baseline")
    if canonical_sha256(manifest_files) != source.get("files_sha256"):
        errors.append("source manifest file-list hash does not match baseline")
    if frozen_source_files(desktop) != manifest_files:
        errors.append("desktop frozen source differs from work-review-source.json")

    commands_by_id: dict[str, dict[str, object]] = {}
    commands = result.get("commands")
    if not isinstance(commands, list) or {item.get("id") for item in commands} != EXPECTED_COMMANDS:
        errors.append("commands must contain exactly the five native CI baseline IDs")
    else:
        for command in commands:
            evidence = desktop / command.get("evidence", "")
            if not evidence.is_file():
                errors.append(f"command {command['id']} evidence file is missing")
                continue
            if command.get("log_sha256") != sha256(evidence):
                errors.append(f"command {command['id']} evidence hash does not match")
        commands_by_id = {item["id"]: item for item in commands}
        if commands_by_id["AUTO-RUST-TEST"].get("exit_code") != 101:
            errors.append("AUTO-RUST-TEST must retain the known failing exit code 101")
        for command_id in EXPECTED_COMMANDS - {"AUTO-RUST-TEST"}:
            if commands_by_id[command_id].get("exit_code") != 0:
                errors.append(f"{command_id} must retain its observed zero exit code")

    items = result.get("items")
    if not isinstance(items, list) or {item.get("id") for item in items} != EXPECTED_ITEMS or len(items) != len(EXPECTED_ITEMS):
        errors.append("items must cover BASE-01 through BASE-10 and five automation rows exactly once")
    else:
        for item in items:
            identifier = item["id"]
            before_status = item.get("before_status")
            after_status = item.get("after_status")
            if before_status not in ALLOWED_STATUSES or after_status not in ALLOWED_STATUSES:
                errors.append(f"{identifier} has an invalid status")
            if after_status != "not-run":
                errors.append(f"{identifier} after_status must remain not-run in the pre-change baseline")
            if before_status in {"pass", "conditional-pass", "fail"} and not item.get("before_evidence"):
                errors.append(f"{identifier} requires before_evidence for its recorded status")
            if before_status in {"fail", "blocked", "not-run"} and not item.get("before_reason"):
                errors.append(f"{identifier} requires before_reason for its recorded status")
        errors.extend(validate_matrix_consistency(items, matrix, commands_by_id))

    known_issues = result.get("known_issues")
    if not isinstance(known_issues, list) or len(known_issues) != 1:
        errors.append("baseline must retain exactly one documented known issue")
    else:
        issue = known_issues[0]
        if issue.get("id") != "UPSTREAM-RUST-001" or issue.get("command_id") != "AUTO-RUST-TEST" or issue.get("before_status") != "fail":
            errors.append("UPSTREAM-RUST-001 does not retain its test failure attribution")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project-root", type=Path, default=Path.cwd())
    parser.add_argument("--matrix-path", type=Path, help="validate a candidate inheritance matrix")
    arguments = parser.parse_args()
    try:
        errors = validate(arguments.project_root.resolve(), arguments.matrix_path)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"baseline validation failed: {error}", file=sys.stderr)
        return 1
    if errors:
        print("baseline validation failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("baseline validation passed: frozen source, matrix, command evidence, and known issue are consistent")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
