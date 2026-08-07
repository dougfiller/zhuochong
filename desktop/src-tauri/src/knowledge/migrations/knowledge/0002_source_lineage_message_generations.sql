ALTER TABLE knowledge_sources ADD COLUMN exported_at_ms INTEGER;
ALTER TABLE knowledge_sources ADD COLUMN coverage_kind TEXT;

CREATE INDEX knowledge_sources_account_export_coverage_idx
  ON knowledge_sources(account_stable_id, exported_at_ms, coverage_kind, manifest_hash, coverage_hash, export_id);
CREATE UNIQUE INDEX knowledge_message_versions_identity_content_idx
  ON knowledge_message_versions(message_id, content_hash);
CREATE INDEX knowledge_source_lineage_successor_idx
  ON knowledge_source_lineage(successor_source_id, predecessor_source_id, relation_kind);
CREATE INDEX knowledge_import_generation_members_version_idx
  ON knowledge_import_generation_members(message_version_id, import_generation_id);
