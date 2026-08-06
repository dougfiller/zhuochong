#!/usr/bin/env python3
"""Read-only boundary check for step 13's explicit WeChat trigger."""

from __future__ import annotations

import argparse
from pathlib import Path


def require(source: str, token: str, path: Path) -> None:
    if token not in source:
        raise SystemExit(f"missing required token in {path}: {token}")


def reject(source: str, token: str, path: Path) -> None:
    if token in source:
        raise SystemExit(f"forbidden token in {path}: {token}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-root", type=Path, default=Path("."))
    root = parser.parse_args().project_root.resolve()
    commands = root / "desktop/src-tauri/src/wechat/commands.rs"
    flow = root / "desktop/src-tauri/src/wechat/reply_flow.rs"
    window = root / "desktop/src/routes/avatar/AvatarWindow.svelte"
    popover = root / "desktop/src/lib/components/Avatar/AvatarPopover.svelte"
    errors = root / "desktop/src/lib/utils/errorDisplay.js"

    command_source = commands.read_text(encoding="utf-8")
    flow_source = flow.read_text(encoding="utf-8")
    window_source = window.read_text(encoding="utf-8")
    popover_source = popover.read_text(encoding="utf-8")
    error_source = errors.read_text(encoding="utf-8")

    require(command_source, "async fn generate_wechat_reply(", commands)
    require(command_source, "request_phase", commands)
    reject(command_source, "GenerateWechatReplyInput", commands)
    require(flow_source, "publish_generated_suggestion", flow)
    require(flow_source, "emit_avatar_bubble", flow)
    require(flow_source, "runtime.finish_reply(lease)", flow)
    require(window_source, "const WECHAT_PREPARE_SECONDS = 3", window)
    require(window_source, "clearWechatPrepareTimer", window)
    require(window_source, "invoke('generate_wechat_reply')", window)
    reject(window_source, "invoke('generate_wechat_reply', {", window)
    require(popover_source, "onCancelWechatGeneration", popover)
    require(error_source, "formatWechatUserError", errors)
    reject(command_source + flow_source, "clipboard", commands)
    print("verify_wechat_explicit_trigger: passed")


if __name__ == "__main__":
    main()
