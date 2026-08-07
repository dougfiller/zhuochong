#!/usr/bin/env python3
"""Static gate for the step-18 streaming-message persistence boundary."""
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
IMPORTER = ROOT / "desktop/src-tauri/src/knowledge/archive_importer.rs"
STORE = ROOT / "desktop/src-tauri/src/knowledge/store.rs"
MIGRATION = ROOT / "desktop/src-tauri/src/knowledge/migrations/knowledge/0003_streaming_message_normalization_media.sql"


def require(path: Path, fragments: list[str]) -> None:
    text = path.read_text(encoding="utf-8")
    missing = [fragment for fragment in fragments if fragment not in text]
    if missing:
        raise SystemExit(f"{path.relative_to(ROOT)} missing: {', '.join(missing)}")


def main() -> None:
    require(MIGRATION, [
        "knowledge_message_normalizations",
        "knowledge_import_generation_input_keys",
        "exists_state TEXT NOT NULL DEFAULT 'unknown'",
        "knowledge_media_refs_version_source_ordinal_idx",
    ])
    require(IMPORTER, [
        "MessagesEnvelopeSeed",
        "MessageArrayVisitor",
        "stream_manifest",
        "SourceAuditAccumulator",
        "normalize_message",
        "normalize_member_path(&path)",
        "messages must follow a complete envelope header",
        "import_conversation",
        "failed_conversations",
    ])
    require(STORE, [
        "append_staging_messages",
        "knowledge_import_generation_input_keys",
        "knowledge_message_normalizations",
        "exists_state,metadata_json",
    ])
    forbidden = ["open_declared(&media", "metadata(&media", "read(&media"]
    importer = IMPORTER.read_text(encoding="utf-8")
    if "Vec<(StagingImport" in importer or "collect::<Vec<_>>()" in importer.split("impl<'a> WechatJsonArchiveImporter", 1)[1].split("fn usable_verdict", 1)[0]:
        raise SystemExit("importer must not retain completed staging generations")
    if "serde_json::from_reader(self.guard.open_manifest()?)" in importer:
        raise SystemExit("manifest conversations must be consumed through the streaming manifest cursor")
    if any(token in importer for token in forbidden):
        raise SystemExit("media references must remain metadata-only")
    print("STREAMING_MESSAGE_IMPORT_GATE=passed")


if __name__ == "__main__":
    main()
