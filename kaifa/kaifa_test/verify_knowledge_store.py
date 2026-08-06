#!/usr/bin/env python3
"""Static boundary gate for step 16; it never opens a user knowledge database."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys


def require(text: str, needle: str, path: Path) -> None:
    if needle not in text:
        raise RuntimeError(f"missing {needle!r} in {path}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-root", type=Path, default=Path("."))
    args = parser.parse_args()
    root = args.project_root.resolve()
    knowledge = root / "desktop/src-tauri/src/knowledge"
    store = knowledge / "store.rs"
    migrations = knowledge / "migrations.rs"
    schema = knowledge / "migrations/knowledge/0001_initial.sql"
    importer = knowledge / "archive_importer.rs"
    main_rs = root / "desktop/src-tauri/src/main.rs"
    for path in (store, migrations, schema, importer, main_rs):
        if not path.is_file():
            raise RuntimeError(f"missing required file: {path}")
    store_text = store.read_text(encoding="utf-8")
    migration_text = migrations.read_text(encoding="utf-8")
    schema_text = schema.read_text(encoding="utf-8")
    importer_text = importer.read_text(encoding="utf-8")
    main_text = main_rs.read_text(encoding="utf-8")
    for needle in ("pub(crate) struct KnowledgeStore", "open_or_unavailable", "with_writer", "with_reader", "read_active_snapshot", "activate_candidate", "deny_or_delete"):
        require(store_text, needle, store)
    for needle in ("PRAGMA foreign_keys=ON", "PRAGMA journal_mode=WAL", "busy_timeout", "SCHEMA_HEAD", "integrity_check", "foreign_key_check"):
        require(migration_text, needle, migrations)
    for needle in ("knowledge_catalog_state", "knowledge_import_generations", "ready_candidate", "knowledge_chunks_fts", "ON DELETE RESTRICT"):
        require(schema_text, needle, schema)
    importer_production = importer_text.split("#[cfg(test)]", 1)[0]
    if "rusqlite" in importer_production or "WechatArchiveStore" in importer_production:
        raise RuntimeError("archive importer must not own a SQLite connection")
    require(main_text, "KnowledgeStore::open_or_unavailable(&data_dir)", main_rs)
    print("KNOWLEDGE_STORE_GATE: pass")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError) as error:
        print(f"KNOWLEDGE_STORE_GATE: fail: {error}", file=sys.stderr)
        raise SystemExit(1)
