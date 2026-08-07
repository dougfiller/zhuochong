#!/usr/bin/env python3
"""Static fail-closed checks for task 25 mandatory-RAG M2 wiring."""

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[2]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def ordered(text: str, markers: list[str]) -> bool:
    position = -1
    for marker in markers:
        position = text.find(marker, position + 1)
        if position < 0:
            return False
    return True


def main() -> int:
    contract = read("desktop/src-tauri/src/wechat/model_contract.rs")
    client = read("desktop/src-tauri/src/wechat/model_client.rs").split("#[cfg(test)]\nmod tests", 1)[0]
    flow = read("desktop/src-tauri/src/wechat/reply_flow.rs")
    commands = read("desktop/src-tauri/src/wechat/commands.rs")
    module = read("desktop/src-tauri/src/wechat/mod.rs")
    main_rs = read("desktop/src-tauri/src/main.rs")
    settings = read("desktop/src/routes/settings/components/SettingsKnowledge.svelte")

    require(contract.count("fn build_model_context(") == 1, "context builder must be unique")
    require("pub(crate) struct ModelKnowledgeContext" in contract, "private context type missing")
    require("Deserialize" not in contract.split("pub(crate) struct ModelKnowledgeContext", 1)[1].split("impl ModelKnowledgeContext", 1)[0], "context must not deserialize")
    require("wechat-rag-context-v1" in contract, "versioned context hash missing")
    require("selected.pop()" in contract, "tail-only hit trimming missing")

    require(
        ordered(
            flow,
            [
                "async fn finish_captured_m2_reply",
                "BindingStage::BeforeRetrieval",
                ".retrieve(",
                "build_model_context(retrieval)",
                "complete_retrieval",
                "BindingStage::BeforeModelTransport",
                "authorize_model_call",
                ".generate_m2_reply_with_client",
            ],
        ),
        "mandatory M2 order is incomplete",
    )
    require(
        ordered(
            flow,
            [
                "generate_m2_wechat_reply",
                "begin_m2_binding_request",
                "ReplyState::Retrieving",
                "finish_captured_m2_reply(",
            ],
        ),
        "Windows front half does not hand off to the verified M2 tail",
    )
    require(".knowledge_retrieve(request)" in flow, "production Store retrieval port missing")
    require("generate_m1_wechat_reply" in flow, "isolated M1 build helper missing")
    require("#[cfg(feature = \"wechat-m1\")]\nuse super::types::M1ReplyInput" in flow, "M1 input is not feature-isolated")
    require("generate_m1_wechat_reply" in module, "M2-to-M1 compile probe missing")
    require("feature = \"wechat-m2\"" in commands, "M2 command branch missing")

    for forbidden in ("chat_with_tools", "commands::ask", "send_with_retry", "tool_choice"):
        require(forbidden not in client, f"RAG client imported forbidden path: {forbidden}")
    require("generate_rag_reply" in client, "RAG-only client entry missing")
    require("request.clone()" in client, "frozen retry clone missing")

    source_dto = re.search(r"struct ReplySourceItemDto \{(?P<body>.*?)\n\}", commands, re.S)
    require(source_dto is not None, "safe source DTO missing")
    for forbidden in ("hit_id", "score", "path", "conversation_id", "message_id"):
        require(forbidden not in source_dto.group("body"), f"source DTO leaks {forbidden}")
    require("get_wechat_reply_sources" in main_rs, "source command not registered")
    require("showReplySources" in settings, "explicit source viewer missing")
    on_mount = re.search(r"onMount\(\(\) => \{(?P<body>.*?)\}\);", settings, re.S)
    require(on_mount is not None and "showReplySources" not in on_mount.group("body"), "source text must not auto-load")

    production_files = [contract, client, flow, commands]
    for forbidden in ("clipboard", "paste", "auto_send", "微信数据库", "MCP"):
        require(all(forbidden not in text for text in production_files), f"forbidden M2 capability found: {forbidden}")

    print("TASK25_M2_RAG_STATIC_OK")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as error:
        print(f"TASK25_M2_RAG_STATIC_FAILED: {error}", file=sys.stderr)
        raise SystemExit(1)
