#!/usr/bin/env python3
"""Static boundary check for the step-10 WeChat no-tools model transport."""

from pathlib import Path
import sys


def require(text: str, marker: str, path: Path) -> None:
    if marker not in text:
        raise SystemExit(f"missing {marker!r} in {path}")


def main() -> int:
    root = Path(__file__).resolve().parents[2]
    model = root / "desktop/src-tauri/src/agent/model.rs"
    ask = root / "desktop/src-tauri/src/commands/ask.rs"
    client = root / "desktop/src-tauri/src/wechat/model_client.rs"
    config = root / "desktop/src-tauri/src/wechat/config.rs"
    types = root / "desktop/src-tauri/src/wechat/types.rs"
    runtime = root / "desktop/src-tauri/src/wechat/runtime.rs"

    model_text = model.read_text(encoding="utf-8")
    ask_text = ask.read_text(encoding="utf-8")
    client_text = client.read_text(encoding="utf-8")
    config_text = config.read_text(encoding="utf-8")
    types_text = types.read_text(encoding="utf-8")
    runtime_text = runtime.read_text(encoding="utf-8")

    for marker in ("struct SingleTurnTextRequest", "trait SingleTurnTextTransport", "complete_single_turn_text", "single_turn_request_body"):
        require(model_text, marker, model)
    for forbidden in ('"tools"', '"tool_choice"', '"functionCall"'):
        if forbidden in model_text[model_text.index("fn single_turn_request_body"):model_text.index("async fn single_turn_openai_compatible")]:
            raise SystemExit(f"single-turn request body contains forbidden field {forbidden}")
    require(ask_text, "crate::agent::model::complete_single_turn_text", ask)
    for marker in ("WechatReplyModelClient", "WX_TEXT_MODEL_UNAVAILABLE", "WECHAT_SYSTEM_PROMPT", "M1ReplyInput", "ModelKnowledgeContext"):
        require(client_text + config_text + types_text, marker, client)
    if "#[tauri::command]" in client_text:
        raise SystemExit("WeChat model client must not expose a Tauri command")
    for marker in ("complete_generated_reply", "generate_m1_reply", "generate_m2_reply", "fail_model_generation", "WechatReplyModelClient::new()"):
        require(runtime_text, marker, runtime)
    print("step-10 WeChat no-tools transport boundary: PASS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
