#!/usr/bin/env python3
"""Static boundary gate for step 17; it never opens a user archive or database."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys


def require(text: str, needles: tuple[str, ...], path: Path) -> list[str]:
    return [f"missing:{needle}:{path.name}" for needle in needles if needle not in text]


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--project-root", type=Path, default=Path("."))
    root = parser.parse_args().project_root.resolve()
    knowledge = root / "desktop/src-tauri/src/knowledge"
    paths = {
        "migration": knowledge / "migrations/knowledge/0002_source_lineage_message_generations.sql",
        "migrations": knowledge / "migrations.rs",
        "store": knowledge / "store.rs",
        "importer": knowledge / "archive_importer.rs",
        "schema": knowledge / "archive_schema.rs",
    }
    failures: list[str] = []
    for path in paths.values():
        if not path.is_file():
            failures.append(f"missing-file:{path}")
    if failures:
        print("KNOWLEDGE_LINEAGE_GATE: fail")
        print("\n".join(failures))
        return 1
    text = {name: path.read_text(encoding="utf-8") for name, path in paths.items()}
    failures += require(text["migrations"], ("SCHEMA_HEAD: i32 = 5", "SOURCE_LINEAGE_MESSAGE_GENERATIONS", "STREAMING_MESSAGE_NORMALIZATION_MEDIA"), paths["migrations"])
    failures += require(text["migration"], ("exported_at_ms", "coverage_kind", "knowledge_message_versions_identity_content_idx", "knowledge_source_lineage_successor_idx"), paths["migration"])
    failures += require(text["store"], ("IncomingMessage", "append_staging_messages", "record_source_audits", "register_ready_index_set", "activate_candidates", "knowledge_source_lineage", "knowledge_message_versions", "parent_generation_id"), paths["store"])
    failures += require(text["importer"], ("Vec::with_capacity(256)", "append_staging_messages", "finalize_source_candidates", "discard_stagings"), paths["importer"])
    failures += require(text["schema"], ("exported_at_ms", "parse_from_rfc3339"), paths["schema"])
    production_importer = text["importer"].split("#[cfg(test)]", 1)[0]
    for forbidden in ("write_all(", "remove_file(", "rename(", "rusqlite"):
        if forbidden in production_importer:
            failures.append(f"forbidden-importer-capability:{forbidden}")
    if failures:
        print("KNOWLEDGE_LINEAGE_GATE: fail")
        print("\n".join(sorted(set(failures))))
        return 1
    print("KNOWLEDGE_LINEAGE_GATE: pass")
    return 0


if __name__ == "__main__":
    sys.exit(main())
