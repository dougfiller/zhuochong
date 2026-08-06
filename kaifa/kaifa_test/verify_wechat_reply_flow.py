#!/usr/bin/env python3
"""Static scope gate for the Step 11 private M1 reply-flow wiring."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys


def require(source: str, needle: str, label: str, failures: list[str]) -> None:
    if needle not in source:
        failures.append(f"missing {label}: {needle}")


def reject(source: str, needle: str, label: str, failures: list[str]) -> None:
    if needle in source:
        failures.append(f"forbidden {label}: {needle}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project-root", type=Path, default=Path("."))
    args = parser.parse_args()
    root = args.project_root.resolve()
    wechat = root / "desktop/src-tauri/src/wechat"
    flow = (wechat / "reply_flow.rs").read_text(encoding="utf-8")
    runtime = (wechat / "runtime.rs").read_text(encoding="utf-8")
    module = (wechat / "mod.rs").read_text(encoding="utf-8")
    production_flow = flow.split("#[cfg(test)]", 1)[0]
    failures: list[str] = []

    require(module, '#[cfg(feature = "wechat-m1")]\nmod reply_flow;', "M1-only module gate", failures)
    for needle, label in [
        ("pub(crate) async fn generate_wechat_reply", "private M1 facade"),
        ("runtime.begin_reply(m1_snapshot(), ReplyTraceStore::new(data_dir))", "lease before capture"),
        ("lease.request_id().clone()", "capture request identity"),
        ("runtime.enter_ocr_after_capture(lease, capture_version)", "capture version handoff"),
        ("coordinator.is_current_capture(capture_version)", "stale capture gate"),
        ("M1ReplyInput::from(ocr)", "OCR-only M1 input"),
        (".generate_m1_reply_with_client(client, config, profiles, M1ReplyInput::from(ocr), lease)", "runtime-owned model call"),
        ("fn finish_failed", "failure terminal cleanup"),
        ("fn enter_ocr_after_capture", "atomic runtime OCR handoff"),
        ("active.capture_version = Some(capture_version);", "trace version bind"),
        ("request_id: super::types::RequestId,", "capture lease request argument"),
    ]:
        require(flow + "\n" + runtime, needle, label, failures)

    for needle, label in [
        ("#[tauri::command]", "Tauri command"),
        ("ModelKnowledgeContext", "M2 knowledge context"),
        ("RetrievedReply", "M2 retrieval reply"),
        ("knowledge::", "knowledge dependency"),
        ("emit_avatar_bubble", "avatar presentation"),
        ("clipboard", "clipboard access"),
        ("set_focus", "window focus control"),
        ("send_input", "input automation"),
        ("http://", "HTTP transport"),
        ("https://", "HTTP transport"),
    ]:
        reject(production_flow, needle, label, failures)

    capture_start = runtime.index("pub(crate) async fn capture_foreground_wechat")
    capture_end = runtime.index("\n}\n\n#[cfg(test)]", capture_start)
    reject(runtime[capture_start:capture_end], "RequestId::new()", "capture-created request id", failures)

    if failures:
        print("wechat reply-flow verification failed:", file=sys.stderr)
        print("\n".join(f"- {failure}" for failure in failures), file=sys.stderr)
        return 1
    print("wechat reply-flow verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
