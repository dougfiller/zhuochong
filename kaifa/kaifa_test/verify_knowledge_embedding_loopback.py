#!/usr/bin/env python3
"""Static scope/privacy gate for step 21; it never opens databases or calls a network."""

from __future__ import annotations

import argparse
from pathlib import Path


def require(text: str, needles: tuple[str, ...], label: str) -> None:
    missing = [needle for needle in needles if needle not in text]
    if missing:
        raise SystemExit(f"KNOWLEDGE_EMBEDDING_GATE fail={label} missing={missing}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-root", default=".")
    args = parser.parse_args()
    root = Path(args.project_root).resolve()
    semantic = (root / "desktop/crates/core/src/semantic.rs").read_text(encoding="utf-8")
    wire = (root / "desktop/src-tauri/src/embedding.rs").read_text(encoding="utf-8")
    knowledge = (root / "desktop/src-tauri/src/knowledge/embedding.rs").read_text(
        encoding="utf-8"
    )
    store = (root / "desktop/src-tauri/src/knowledge/store.rs").read_text(encoding="utf-8")

    require(
        semantic,
        (
            "decode_embedding_exact",
            "checked_mul(4)",
            "StreamingCosineTopK",
            "reciprocal_rank_fusion",
            "DuplicateKey",
        ),
        "shared_semantic_primitives",
    )
    require(
        wire,
        ("OpenAiBatch", "OllamaLegacyPrompt", "OllamaBatchV1", "expected_count"),
        "wire_compatibility",
    )
    require(
        knowledge,
        (
            ".no_proxy()",
            ".redirect(Policy::none())",
            "build_client_with_timeouts(endpoint, CONNECT_TIMEOUT, REQUEST_TIMEOUT)",
            ".connect_timeout(connect_timeout)",
            ".timeout(request_timeout)",
            ".resolve_to_addrs(&endpoint.host, &endpoint.addresses)",
            'endpoint_url(endpoint, "api/tags")',
            'endpoint_url(endpoint, "api/embed")',
            "SystemEndpointResolver",
            "LocalhostPinned",
        ),
        "loopback_transport",
    )
    forbidden = (
        "embedding_api_key",
        "openai_api_key",
        "Authorization",
        "remote_upload",
        "agent::",
        "bot_",
    )
    present = [needle for needle in forbidden if needle in knowledge]
    if present:
        raise SystemExit(f"KNOWLEDGE_EMBEDDING_GATE fail=forbidden_boundary found={present}")
    require(
        store,
        (
            "read_building_embedding_config",
            "read_active_embedding_config",
            "list_pending_build_embeddings",
            "write_build_embeddings",
            "search_active_vectors",
            "embedding IS NULL",
            "decode_embedding_exact",
        ),
        "generation_store",
    )
    print(
        "KNOWLEDGE_EMBEDDING_GATE status=pass "
        "mode=static network_calls=0 database_opens=0 content_reads=0"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
