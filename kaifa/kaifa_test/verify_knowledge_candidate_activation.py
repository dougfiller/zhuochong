#!/usr/bin/env python3
"""Static fail-closed boundary check for step 22; never opens user data."""

from __future__ import annotations

import argparse
from pathlib import Path


def require(text: str, needles: list[str], label: str) -> None:
    missing = [needle for needle in needles if needle not in text]
    if missing:
        raise SystemExit(f"KNOWLEDGE_CANDIDATE_ACTIVATION_GATE: fail {label} missing={missing}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-root", default=".")
    args = parser.parse_args()
    root = Path(args.project_root).resolve()
    migrations = (root / "desktop/src-tauri/src/knowledge/migrations.rs").read_text()
    migration = (root / "desktop/src-tauri/src/knowledge/migrations/knowledge/0005_candidate_activation_diagnostics.sql").read_text()
    store = (root / "desktop/src-tauri/src/knowledge/store.rs").read_text()
    embedding = (root / "desktop/src-tauri/src/knowledge/embedding.rs").read_text()

    require(migrations, ["SCHEMA_HEAD: i32 = 5", "0005_candidate_activation_diagnostics.sql", "invalid_generation_state"], "migration")
    require(migration, ["error_summary TEXT", "knowledge_index_generations_status_created_idx"], "migration-resource")
    require(store, [
        "pub(crate) fn validate_candidate_index",
        "pub(crate) fn activate_validated_candidate",
        "validate_candidate_connection(&transaction",
        "transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)",
        "catalog_generation_seq=?1",
        "AND catalog_generation_seq=?5",
        "status='ready',completed_at_ms=?1",
        "generation.snapshot_hash=catalog.active_snapshot_hash",
        "mapping.import_generation_id=conversation.active_import_generation_id",
        "decode_unit_embedding",
        "PRAGMA foreign_key_check",
        "DENIAL_INVALIDATED_CANDIDATE",
    ], "store")
    if "UPDATE knowledge_catalog_state SET active_index_generation_id=NULL" in store:
        raise SystemExit("KNOWLEDGE_CANDIDATE_ACTIVATION_GATE: fail denial-clears-catalog")
    require(embedding, ["validate_candidate_index(index_generation_id)", "activate_validated_candidate(index_generation_id, &validation)"], "embedding-orchestration")
    print("KNOWLEDGE_CANDIDATE_ACTIVATION_GATE: pass schema_head=5 activation=begin_immediate_catalog_cas")


if __name__ == "__main__":
    main()
