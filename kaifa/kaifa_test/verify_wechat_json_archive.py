#!/usr/bin/env python3
"""Read-only acceptance gate for the synthetic wechat_archive_v1 fixture."""

import argparse
import json
from pathlib import Path
import sys


def require_text(path: Path, fragments: list[str], root: Path) -> list[str]:
    if not path.is_file():
        return [f"missing:{path.relative_to(root)}"]
    text = path.read_text(encoding="utf-8")
    return [f"missing:{path.relative_to(root)}:{fragment}" for fragment in fragments if fragment not in text]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    root = args.project_root.resolve()
    fixture = root / "desktop/src-tauri/tests/fixtures/wechat_archive_v1"
    failures: list[str] = []
    try:
        manifest = json.loads((fixture / "manifest.json").read_text(encoding="utf-8"))
        report = json.loads((fixture / "report.json").read_text(encoding="utf-8"))
        messages = json.loads((fixture / "conversations/conv_fixture_01/messages.json").read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        failures.append("fixture_invalid")
        manifest = report = messages = {}
    if manifest.get("schemaVersion") != "wechat_archive_v1" or report.get("schemaVersion") != "wechat_archive_v1":
        failures.append("schema_route_invalid")
    if manifest.get("scope", {}).get("kind") != "selected":
        failures.append("selected_coverage_missing")
    kinds = {message.get("type") for message in messages.get("messages", [])}
    required_kinds = {"text", "image", "video", "voice", "file", "location", "link", "emoji", "quote", "reply", "system", "recall"}
    if kinds != required_kinds:
        failures.append("message_fixture_coverage_invalid")
    fixture_text = "\n".join(path.read_text(encoding="utf-8") for path in fixture.rglob("*.json")) if fixture.is_dir() else ""
    if "acct_fixture_01" not in fixture_text or "synthetic" not in fixture_text or "/Users/" in fixture_text:
        failures.append("fixture_deidentification_invalid")
    tauri = root / "desktop/src-tauri/src"
    failures += require_text(tauri / "knowledge/archive_schema.rs", ["deny_unknown_fields", "WECHAT_ARCHIVE_V1", "CoverageKind", "unsupported_source_scope_and_media_contracts_fail_closed", 'source.kind == "user_selected"', "include_media"], root)
    failures += require_text(tauri / "knowledge/archive_importer.rs", ["WechatArchiveReadGuard", "open_messages_stream", "deserialize_seq", "KbSourceUnsupported", "MAX_JSON_STRING_BYTES", "JsonStringLimitReader", "open_member_no_follow", "oversized_message_string_is_rejected_without_recording_an_import", "symlinked_member_is_rejected_by_no_follow_open", "source_path_is_not_persisted_in_the_derived_database"], root)
    failures += require_text(tauri / "knowledge/archive_store.rs", ["wechat_knowledge", "archive_member_audits", "fast_verified"], root)
    importer = (tauri / "knowledge/archive_importer.rs").read_text(encoding="utf-8") if (tauri / "knowledge/archive_importer.rs").is_file() else ""
    importer_runtime = importer.split("#[cfg(test)]", 1)[0]
    forbidden = ["create(", "write_all(", "remove_file(", "rename("]
    failures += [f"source_write_api:{token}" for token in forbidden if token in importer_runtime]
    if "source_id" in (tauri / "knowledge/archive_store.rs").read_text(encoding="utf-8"):
        failures.append("source_id_persisted")
    if "/微信聊天记录知识库/liaotian/" not in (root / ".gitignore").read_text(encoding="utf-8"):
        failures.append("private_source_ignore_missing")
    config = json.loads((root / "desktop/src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    if config.get("bundle", {}).get("resources"):
        failures.append("bundle_resources_not_empty")
    if failures:
        print("WECHAT_JSON_ARCHIVE_GATE: fail")
        print("\n".join(sorted(set(failures))))
        return 1
    print("WECHAT_JSON_ARCHIVE_GATE: pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
