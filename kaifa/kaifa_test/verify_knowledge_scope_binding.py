#!/usr/bin/env python3
"""Static safety gate for task 24 knowledge-scope binding boundaries."""

from __future__ import annotations

import argparse
from pathlib import Path


def require(text: str, needle: str, label: str, failures: list[str]) -> None:
    if needle not in text:
        failures.append(f"missing {label}: {needle}")


def forbid(text: str, needle: str, label: str, failures: list[str]) -> None:
    if needle in text:
        failures.append(f"forbidden {label}: {needle}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-root", default=".")
    root = Path(parser.parse_args().project_root).resolve()
    binding = (root / "desktop/src-tauri/src/wechat/binding.rs").read_text(encoding="utf-8")
    store = (root / "desktop/src-tauri/src/knowledge/store.rs").read_text(encoding="utf-8")
    commands = (root / "desktop/src-tauri/src/wechat/commands.rs").read_text(encoding="utf-8")
    config = (root / "desktop/crates/core/src/config.rs").read_text(encoding="utf-8")
    capture = (root / "desktop/src-tauri/src/wechat/capture.rs").read_text(encoding="utf-8")
    runtime = (root / "desktop/src-tauri/src/wechat/runtime.rs").read_text(encoding="utf-8")
    profiles = (root / "desktop/src-tauri/src/wechat/profiles.rs").read_text(encoding="utf-8")
    picker = (root / "desktop/src/lib/components/KnowledgeScopePicker.svelte").read_text(encoding="utf-8")
    main_rs = (root / "desktop/src-tauri/src/main.rs").read_text(encoding="utf-8")
    failures: list[str] = []

    require(binding, "pub(crate) struct KnowledgeScopeBinding", "single binding owner", failures)
    require(main_rs, ".manage(wechat::binding::KnowledgeScopeBinding::default())", "one managed binding", failures)
    if main_rs.count(".manage(wechat::binding::KnowledgeScopeBinding::default())") != 1:
        failures.append("KnowledgeScopeBinding must be managed exactly once")
    forbid(binding, "Serialize, Deserialize", "binding serde", failures)
    forbid(binding, "title_hint", "window-title identity", failures)
    require(store, "knowledge-scope-key-v1\\0", "opaque scope key domain", failures)
    require(store, "conversation.account_stable_id", "account-scoped resolution", failures)
    require(store, "display_metadata_json", "active display metadata", failures)
    require(capture, "HeaderObservationFrame", "header-only frame", failures)
    require(runtime, "capture_header_identity_observation", "shared header capture", failures)
    require(runtime, ".try_acquire()", "shared capture coordinator", failures)
    require(profiles, 'reply_surface != "single_chat"', "single-chat profile gate", failures)
    require(commands, "begin_knowledge_scope_observation", "observation command", failures)
    require(commands, "confirm_knowledge_scope_binding", "confirmation command", failures)
    require(commands, "invalidate_current_capture", "capture invalidation", failures)
    require(config, "last_scope_hint_keys", "hint-only persistence", failures)
    require(config, "config.scope_mode = None", "legacy scope neutralization", failures)
    for forbidden in ["binding_generation:", "session_nonce:", "header_clue:", "window_token:"]:
        # These fields are allowed in the process-local binding module, but never in persisted config.
        forbid(config, forbidden, "persisted authority field", failures)
    for action in ["bindOne", "selectMany", "confirmGlobal"]:
        require(picker, action, f"explicit picker action {action}", failures)
    require(picker, "let selectedKeys = [];", "zero selection default", failures)
    require(picker, "formatWechatUserError", "safe binding error display", failures)
    forbid(picker, "selectedKeys = hintKeys", "hint auto-selection", failures)
    for forbidden in ["uiautomation", "SendInput", "keybd_event", "mouse_event", "auto_send", "wechat.sqlite"]:
        forbid(binding + commands + picker, forbidden, "automation capability", failures)

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}")
        return 1
    print("knowledge scope binding static gate: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
