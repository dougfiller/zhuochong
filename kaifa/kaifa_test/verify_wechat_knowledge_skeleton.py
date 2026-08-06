#!/usr/bin/env python3
"""Static acceptance gate for the Step 5 empty WeChat/knowledge skeleton."""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
TAURI = ROOT / "desktop" / "src-tauri" / "src"


def require(path: Path, fragments: list[str]) -> list[str]:
    if not path.is_file():
        return [f"missing: {path.relative_to(ROOT)}"]
    text = path.read_text(encoding="utf-8")
    return [f"{path.relative_to(ROOT)} missing: {fragment}" for fragment in fragments if fragment not in text]


def main() -> int:
    failures: list[str] = []
    failures += require(TAURI / "main.rs", [
        ".manage(wechat::WechatReplyRuntime::default())",
        ".manage(wechat::CaptureCoordinator::default())",
        ".manage(knowledge::KnowledgeStore::default())",
        "get_wechat_settings_status",
        "get_knowledge_settings_status",
        "validate_knowledge_local_embedding",
    ])
    failures += require(TAURI / "wechat" / "runtime.rs", ["WX_NOT_READY", "WX_TEXT_MODEL_UNAVAILABLE"])
    failures += require(TAURI / "knowledge" / "runtime.rs", ["KB_NOT_READY"])
    failures += require(TAURI / "knowledge" / "config.rs", ["ollama_loopback", "KB_EMBEDDING_ENDPOINT_NOT_LOOPBACK"])
    failures += require(ROOT / "desktop" / "crates" / "core" / "src" / "config.rs", [
        "pub wechat: WechatConfig", "pub knowledge: KnowledgeConfig",
        "normalize_wechat_config", "normalize_knowledge_config",
    ])
    if failures:
        print("Step 5 static gate failed:")
        print("\n".join(f"- {failure}" for failure in failures))
        return 1
    print("Step 5 static gate passed: safe states, commands, config defaults, and loopback validation are wired.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
