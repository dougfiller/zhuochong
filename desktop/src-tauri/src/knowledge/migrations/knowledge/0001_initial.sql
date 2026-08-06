CREATE TABLE knowledge_sources (
  id TEXT PRIMARY KEY, account_stable_id TEXT NOT NULL, export_id TEXT NOT NULL,
  schema_version TEXT NOT NULL, manifest_hash TEXT NOT NULL, coverage_hash TEXT NOT NULL,
  snapshot_kind TEXT NOT NULL, scope_filters_json TEXT NOT NULL, integrity_json TEXT NOT NULL,
  priority INTEGER NOT NULL DEFAULT 0, source_state TEXT NOT NULL CHECK(source_state IN ('active','retired','missing')),
  import_status TEXT NOT NULL, checked_at_ms INTEGER NOT NULL, error_code TEXT, error_summary TEXT,
  UNIQUE(account_stable_id, export_id, schema_version, manifest_hash, coverage_hash)
);
CREATE TABLE knowledge_source_members (
  source_id TEXT NOT NULL REFERENCES knowledge_sources(id) ON DELETE CASCADE,
  member_path_token TEXT NOT NULL, member_kind TEXT NOT NULL, size_bytes INTEGER NOT NULL,
  mtime_ms INTEGER NOT NULL, declared_hash TEXT, checked INTEGER NOT NULL CHECK(checked IN (0,1)),
  PRIMARY KEY(source_id, member_path_token)
);
CREATE TABLE knowledge_source_lineage (
  predecessor_source_id TEXT NOT NULL REFERENCES knowledge_sources(id) ON DELETE RESTRICT,
  successor_source_id TEXT NOT NULL REFERENCES knowledge_sources(id) ON DELETE RESTRICT,
  relation_kind TEXT NOT NULL, verified_at_ms INTEGER NOT NULL, evidence_hash TEXT NOT NULL,
  UNIQUE(predecessor_source_id, successor_source_id, relation_kind)
);
CREATE TABLE knowledge_conversations (
  id TEXT PRIMARY KEY, account_stable_id TEXT NOT NULL, conversation_stable_id TEXT NOT NULL,
  display_metadata_json TEXT NOT NULL DEFAULT '{}', active_import_generation_id TEXT,
  UNIQUE(account_stable_id, conversation_stable_id),
  FOREIGN KEY(id, active_import_generation_id) REFERENCES knowledge_import_generations(conversation_id, id) DEFERRABLE INITIALLY DEFERRED
);
CREATE TABLE knowledge_import_generations (
  id TEXT PRIMARY KEY, trigger_source_id TEXT NOT NULL REFERENCES knowledge_sources(id) ON DELETE RESTRICT,
  conversation_id TEXT NOT NULL REFERENCES knowledge_conversations(id) ON DELETE RESTRICT,
  parent_generation_id TEXT REFERENCES knowledge_import_generations(id) ON DELETE RESTRICT,
  source_set_hash TEXT NOT NULL, merge_mode TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('staging','ready_candidate','active','superseded','failed')),
  message_count INTEGER NOT NULL DEFAULT 0, created_at_ms INTEGER NOT NULL, error_code TEXT,
  UNIQUE(conversation_id, id)
);
CREATE TABLE knowledge_import_generation_sources (
  import_generation_id TEXT NOT NULL REFERENCES knowledge_import_generations(id) ON DELETE CASCADE,
  source_id TEXT NOT NULL REFERENCES knowledge_sources(id) ON DELETE RESTRICT,
  precedence INTEGER NOT NULL, coverage_role TEXT NOT NULL, PRIMARY KEY(import_generation_id, source_id)
);
CREATE TABLE knowledge_messages (
  id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES knowledge_conversations(id) ON DELETE RESTRICT,
  message_stable_id TEXT, fallback_key TEXT, low_confidence INTEGER NOT NULL DEFAULT 0 CHECK(low_confidence IN (0,1)),
  CHECK((message_stable_id IS NOT NULL) != (fallback_key IS NOT NULL)),
  UNIQUE(conversation_id, message_stable_id), UNIQUE(conversation_id, fallback_key)
);
CREATE TABLE knowledge_message_versions (
  id TEXT PRIMARY KEY, message_id TEXT NOT NULL REFERENCES knowledge_messages(id) ON DELETE CASCADE,
  import_generation_id TEXT NOT NULL REFERENCES knowledge_import_generations(id) ON DELETE RESTRICT,
  content TEXT NOT NULL, normalized_content TEXT NOT NULL, content_hash TEXT NOT NULL,
  UNIQUE(import_generation_id, message_id), UNIQUE(message_id, id)
);
CREATE TABLE knowledge_import_generation_members (
  import_generation_id TEXT NOT NULL REFERENCES knowledge_import_generations(id) ON DELETE CASCADE,
  message_id TEXT NOT NULL REFERENCES knowledge_messages(id) ON DELETE CASCADE,
  message_version_id TEXT NOT NULL, selection_reason TEXT NOT NULL,
  PRIMARY KEY(import_generation_id, message_id),
  FOREIGN KEY(message_id, message_version_id) REFERENCES knowledge_message_versions(message_id, id)
);
CREATE TABLE knowledge_message_sources (
  message_version_id TEXT NOT NULL REFERENCES knowledge_message_versions(id) ON DELETE CASCADE,
  source_id TEXT NOT NULL REFERENCES knowledge_sources(id) ON DELETE RESTRICT,
  source_relative_path TEXT NOT NULL, PRIMARY KEY(message_version_id, source_id, source_relative_path)
);
CREATE TABLE knowledge_media_refs (
  id TEXT PRIMARY KEY, message_version_id TEXT NOT NULL REFERENCES knowledge_message_versions(id) ON DELETE CASCADE,
  source_id TEXT NOT NULL REFERENCES knowledge_sources(id) ON DELETE RESTRICT, source_relative_path TEXT NOT NULL
);
CREATE TABLE knowledge_denials (
  id TEXT PRIMARY KEY, source_id TEXT REFERENCES knowledge_sources(id) ON DELETE RESTRICT,
  conversation_id TEXT REFERENCES knowledge_conversations(id) ON DELETE RESTRICT,
  message_id TEXT REFERENCES knowledge_messages(id) ON DELETE RESTRICT,
  reason TEXT NOT NULL, effective_generation_id TEXT REFERENCES knowledge_import_generations(id) ON DELETE RESTRICT,
  created_at_ms INTEGER NOT NULL, CHECK(source_id IS NOT NULL OR conversation_id IS NOT NULL OR message_id IS NOT NULL)
);
CREATE TABLE knowledge_index_generations (
  id TEXT PRIMARY KEY, schema_version TEXT NOT NULL, embedding_metadata_json TEXT NOT NULL,
  snapshot_hash TEXT NOT NULL, status TEXT NOT NULL CHECK(status IN ('building','ready','failed')),
  chunk_count INTEGER NOT NULL DEFAULT 0, created_at_ms INTEGER NOT NULL
);
CREATE TABLE knowledge_index_generation_imports (
  index_generation_id TEXT NOT NULL REFERENCES knowledge_index_generations(id) ON DELETE RESTRICT,
  conversation_id TEXT NOT NULL REFERENCES knowledge_conversations(id) ON DELETE RESTRICT,
  import_generation_id TEXT NOT NULL REFERENCES knowledge_import_generations(id) ON DELETE RESTRICT,
  PRIMARY KEY(index_generation_id, conversation_id), UNIQUE(index_generation_id, conversation_id, import_generation_id)
);
CREATE TABLE knowledge_catalog_state (
  singleton_id INTEGER PRIMARY KEY CHECK(singleton_id=1), catalog_generation_seq INTEGER NOT NULL,
  active_snapshot_hash TEXT, active_index_generation_id TEXT REFERENCES knowledge_index_generations(id) ON DELETE RESTRICT,
  activated_at_ms INTEGER
);
INSERT INTO knowledge_catalog_state(singleton_id, catalog_generation_seq, active_snapshot_hash, active_index_generation_id, activated_at_ms) VALUES(1, 0, NULL, NULL, NULL);
CREATE TABLE knowledge_chunks (
  id TEXT PRIMARY KEY, index_generation_id TEXT NOT NULL REFERENCES knowledge_index_generations(id) ON DELETE RESTRICT,
  chunk_key TEXT NOT NULL, conversation_id TEXT NOT NULL REFERENCES knowledge_conversations(id) ON DELETE RESTRICT,
  first_message_version_id TEXT REFERENCES knowledge_message_versions(id) ON DELETE CASCADE,
  last_message_version_id TEXT REFERENCES knowledge_message_versions(id) ON DELETE CASCADE,
  content TEXT NOT NULL, content_hash TEXT NOT NULL, embedding BLOB, UNIQUE(index_generation_id, chunk_key)
);
CREATE TABLE knowledge_chunk_messages (
  chunk_id TEXT NOT NULL REFERENCES knowledge_chunks(id) ON DELETE CASCADE,
  message_id TEXT NOT NULL REFERENCES knowledge_messages(id) ON DELETE CASCADE, PRIMARY KEY(chunk_id, message_id)
);
CREATE VIRTUAL TABLE knowledge_chunks_fts USING fts5(content, chunk_id UNINDEXED, index_generation_id UNINDEXED);
