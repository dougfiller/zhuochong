#!/usr/bin/env python3
"""Static privacy and isolation gate for WeChat reply runtime metadata storage."""

from __future__ import annotations

import argparse
from pathlib import Path


def verify(project_root: Path) -> None:
    source = project_root / "desktop" / "src-tauri" / "src" / "wechat"
    trace = (source / "trace.rs").read_text(encoding="utf-8")
    content = (source / "content.rs").read_text(encoding="utf-8")
    runtime = (source / "runtime.rs").read_text(encoding="utf-8")
    commands = (source / "commands.rs").read_text(encoding="utf-8")

    for required in ["wechat_reply", "trace", "sync_data", "tail_recovered", "stale_result_rejected"]:
        assert required in trace, f"missing trace contract: {required}"
    for forbidden in ["remote_upload", "Database", "ScreenshotService", "localhost_api", "Agent", "Bot", "OcrReadyReply", "GeneratedReply"]:
        assert forbidden not in trace, f"trace crosses forbidden boundary: {forbidden}"
    for required in ["content", "create_new", "sync_all", "rename", "Uuid::parse_str", "delete_all", "delete_request", "symlink_metadata"]:
        assert required in content, f"missing content contract: {required}"
    for forbidden in ["remote_upload", "Database", "ScreenshotService", "localhost_api", "KnowledgeStore"]:
        assert forbidden not in content, f"content crosses forbidden boundary: {forbidden}"
    for required in ["begin_reply", "WxBusy", "transition", "complete_retrieval", "cancel_reply", "finish_reply", "watch::channel", "fail_closed", "retain_ocr_content"]:
        assert required in runtime, f"missing runtime contract: {required}"
    assert "generate_wechat_reply" not in runtime
    for required in ["RetrievalMode", "validate_m2_metadata", "Uuid::parse_str", "MAX_HITS"]:
        assert required in trace, f"missing trace schema guard: {required}"
    for required in ["list_wechat_reply_traces", "delete_wechat_reply_content", "TraceQueryInput"]:
        assert required in commands, f"missing restricted command: {required}"
    for forbidden in ["generate_wechat_reply", "upload_screenshot", "remote_upload"]:
        assert forbidden not in commands, f"command exposes forbidden capability: {forbidden}"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project-root", type=Path, default=Path(__file__).resolve().parents[2])
    args = parser.parse_args()
    verify(args.project_root.resolve())
    print("wechat reply runtime static gate: PASS (single-flight, metadata-only trace, isolated content)")


if __name__ == "__main__":
    main()
