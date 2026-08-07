#!/usr/bin/env python3
"""Static boundary gate for the step-23 knowledge retrieval facade."""

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[2]
KNOWLEDGE = ROOT / "desktop/src-tauri/src/knowledge"
RETRIEVE = KNOWLEDGE / "retrieve.rs"
STORE = KNOWLEDGE / "store.rs"
TYPES = KNOWLEDGE / "types.rs"
MOD = KNOWLEDGE / "mod.rs"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def main() -> int:
    retrieve = RETRIEVE.read_text(encoding="utf-8")
    store = STORE.read_text(encoding="utf-8")
    types = TYPES.read_text(encoding="utf-8")
    module = MOD.read_text(encoding="utf-8")
    knowledge_sources = {
        path: path.read_text(encoding="utf-8")
        for path in KNOWLEDGE.glob("*.rs")
    }

    definitions = [
        path.name
        for path, source in knowledge_sources.items()
        if re.search(r"\basync\s+fn\s+knowledge_retrieve\s*\(", source)
    ]
    require(definitions == ["retrieve.rs"], f"facade definitions={definitions}")
    require("fn knowledge_retrieve" not in types, "legacy types.rs facade remains")
    require("mod retrieve;" in module, "retrieve module is not registered")

    use_block = "\n".join(
        line for line in retrieve.splitlines() if line.startswith("use ")
    ).lower()
    for forbidden in ["model", "tauri", "upload", "mcp", "bot", "agent"]:
        require(forbidden not in use_block, f"forbidden retriever dependency: {forbidden}")
    require("work_review_core::semantic::reciprocal_rank_fusion" in retrieve,
            "shared RRF is not reused")
    require("query_active_vector" in retrieve and "KnowledgeStore" in retrieve,
            "retriever dependency direction is incomplete")

    for wire in ["conversation", "selected_conversations", "global_user_selected"]:
        require(f'#[serde(rename = "{wire}")]' in types, f"scope wire missing: {wire}")
    for wire in ["KB_NOT_READY", "KB_SCOPE_UNRESOLVED", "KB_RETRIEVAL_FAILED"]:
        require(f'#[serde(rename = "{wire}")]' in retrieve, f"error wire missing: {wire}")
    for wire in ["RetrievalStatus", "RetrievalMode", "Success", "NoHit", "FtsFallback"]:
        require(wire in retrieve, f"result contract missing: {wire}")
    require("fn success(" in retrieve and "fn no_hit(" in retrieve,
            "private success/no_hit constructors are missing")
    require("pub(crate) struct RetrievedReply" in retrieve,
            "RetrievedReply is not owned by retrieve.rs")
    require("pub " not in re.search(
        r"struct RetrievedReply\s*\{(?P<body>.*?)\n\}", retrieve, re.S
    ).group("body"), "RetrievedReply exposes public fields")

    for sql_token in [
        "generation.status='ready'",
        "generation.completed_at_ms IS NOT NULL",
        "generation.snapshot_hash=catalog.active_snapshot_hash",
        "knowledge_index_generation_imports mapping",
        "mapping.import_generation_id=conversation.active_import_generation_id",
        "knowledge_denials denial",
        "knowledge_chunk_messages member",
        "knowledge_message_sources provenance",
        "knowledge_import_generation_sources active_source_map",
        "source.source_state='active'",
    ]:
        require(sql_token in store, f"authorized SQL token missing: {sql_token}")
    for method in [
        "begin_authorized_retrieval",
        "search_authorized_fts",
        "search_authorized_vectors",
        "read_authorized_hit_payloads",
        "ensure_retrieval_still_active",
    ]:
        require(f"fn {method}" in store, f"typed store method missing: {method}")
    for authorization_token in [
        "authorization_epoch",
        "catalog_generation_seq=catalog_generation_seq+1",
        "unchecked_transaction()",
    ]:
        require(authorization_token in store,
                f"authorization freeze token missing: {authorization_token}")

    for canonical_result_field in [
        "knowledge-retrieval-result-v1",
        "bound_conversation_id",
        "same_conversation_boost",
        "source_message_range.first",
        "source_time_range.started_at_ms",
        "source_paths.len()",
        "hit.excerpt",
        "hit.token_count",
    ]:
        require(canonical_result_field in retrieve,
                f"canonical result hash field missing: {canonical_result_field}")

    trace = re.search(
        r"struct RetrievalTraceSummary\s*\{(?P<body>.*?)\n\}", retrieve, re.S
    )
    require(trace is not None, "trace summary is missing")
    trace_body = trace.group("body")
    for forbidden in ["query", "excerpt", "path", "provenance", "sender", "embedding"]:
        require(forbidden not in trace_body.lower(), f"trace leaks field: {forbidden}")
    require("request_audit_tag" in trace_body and "hit_ids" in trace_body,
            "trace lacks bounded identifiers")

    print("KNOWLEDGE_RETRIEVAL_FACADE_GATE status=passed scope=static-boundary")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as error:
        print(f"KNOWLEDGE_RETRIEVAL_FACADE_GATE status=failed reason={error}", file=sys.stderr)
        raise SystemExit(1)
