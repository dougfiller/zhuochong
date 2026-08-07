CREATE TABLE knowledge_message_normalizations (
  message_version_id TEXT PRIMARY KEY REFERENCES knowledge_message_versions(id) ON DELETE CASCADE,
  created_at_ms INTEGER NOT NULL,
  source_ordinal INTEGER NOT NULL CHECK(source_ordinal >= 0),
  sort_key TEXT NOT NULL,
  message_kind TEXT NOT NULL,
  render_kind TEXT NOT NULL,
  sender_key TEXT NOT NULL,
  text_hash TEXT NOT NULL,
  reference_json TEXT,
  extra_json TEXT,
  canonical_hash TEXT NOT NULL,
  UNIQUE(message_version_id, canonical_hash)
);
CREATE INDEX knowledge_message_normalizations_sort_idx
  ON knowledge_message_normalizations(message_version_id, created_at_ms, source_ordinal, sort_key);

ALTER TABLE knowledge_media_refs ADD COLUMN ordinal INTEGER NOT NULL DEFAULT 0;
ALTER TABLE knowledge_media_refs ADD COLUMN media_kind TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE knowledge_media_refs ADD COLUMN exists_state TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE knowledge_media_refs ADD COLUMN metadata_json TEXT;
CREATE UNIQUE INDEX knowledge_media_refs_version_source_ordinal_idx
  ON knowledge_media_refs(message_version_id, source_id, ordinal);

CREATE TABLE knowledge_import_generation_input_keys (
  import_generation_id TEXT NOT NULL REFERENCES knowledge_import_generations(id) ON DELETE CASCADE,
  identity_key TEXT NOT NULL,
  PRIMARY KEY(import_generation_id, identity_key)
);

ALTER TABLE knowledge_sources ADD COLUMN member_audit_count INTEGER;
ALTER TABLE knowledge_sources ADD COLUMN member_audit_digest TEXT;
