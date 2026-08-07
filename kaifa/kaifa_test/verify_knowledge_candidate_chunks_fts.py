#!/usr/bin/env python3
"""Static boundary gate for step 20; it never opens user data or a database."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys


def require(text: str, needles: tuple[str, ...], label: str) -> list[str]:
    return [f"missing:{label}:{needle}" for needle in needles if needle not in text]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-root", type=Path, default=Path("."))
    root = parser.parse_args().project_root.resolve()
    knowledge = root / "desktop/src-tauri/src/knowledge"
    paths = {
        "migration": knowledge / "migrations/knowledge/0004_candidate_index_chunks_fts.sql",
        "migrations": knowledge / "migrations.rs",
        "store": knowledge / "store.rs",
        "chunk": knowledge / "chunk.rs",
        "module": knowledge / "mod.rs",
    }
    missing = [f"missing-file:{path}" for path in paths.values() if not path.is_file()]
    if missing:
        print("KNOWLEDGE_CANDIDATE_CHUNKS_FTS_GATE: fail")
        print("\n".join(missing))
        return 1
    text = {name: path.read_text(encoding="utf-8") for name, path in paths.items()}
    failures: list[str] = []
    failures += require(text["migrations"], (
        "SCHEMA_HEAD: i32 = 4", "CANDIDATE_INDEX_CHUNKS_FTS",
        "preflight_v3_chunk_members", "validate_schema",
    ), "migrations")
    failures += require(text["migration"], (
        "knowledge_index_generations_one_building_idx", "token_counter_version",
        "fts_pretoken_version", "retrieval_token_budget", "knowledge_chunk_messages_v4",
        "message_version_id", "message_index", "direction",
    ), "migration")
    failures += require(text["module"], ("mod chunk;",), "module")
    failures += require(text["chunk"], (
        "CHUNK_SCHEMA_VERSION", "TOKEN_COUNTER_VERSION", "FTS_PRETOKEN_VERSION",
        "ChunkerState", "push_page", "finish", "token_count_v1", "chunk_messages",
        "fts_pretoken", "fts_match_query",
        "MAX_GAP_MS", "OVERLAP_MESSAGES",
    ), "chunk")
    failures += require(text["store"], (
        "begin_or_resume_index_build", "list_build_conversations",
        "read_build_message_page", "reset_build_chunks", "write_chunk_batch",
        "search_active_fts", "status='building'", "status='ready'",
        "fts_pretoken_version", "resolved != request.scope.len() as i64",
        "WITH requested(account_stable_id,conversation_stable_id)",
        "knowledge_chunks_fts MATCH ?", "knowledge_catalog_state",
    ), "store")
    for forbidden in ("rusqlite", "Connection", "::open("):
        if forbidden in text["chunk"]:
            failures.append(f"forbidden-chunk-capability:{forbidden}")
    if "CREATE TRIGGER" in text["migration"].upper():
        failures.append("forbidden-cross-table-trigger")
    active_query = text["store"].split("pub(crate) fn search_active_fts", 1)[-1]
    if " LIKE " in active_query.upper():
        failures.append("forbidden-fts-like-fallback")
    for helper in ("register_ready_index", "register_ready_index_set"):
        marker = f"#[cfg(test)]\n    pub(crate) fn {helper}"
        if marker not in text["store"]:
            failures.append(f"production-ready-shortcut:{helper}")
    if failures:
        print("KNOWLEDGE_CANDIDATE_CHUNKS_FTS_GATE: fail")
        print("\n".join(sorted(set(failures))))
        return 1
    print("KNOWLEDGE_CANDIDATE_CHUNKS_FTS_GATE: pass")
    print("frozen_column=fts_pretoken_version supported_pretoken=fts-pretoken-v1 token_counter=v1 tokenizer=fts5-unicode61")
    return 0


if __name__ == "__main__":
    sys.exit(main())
