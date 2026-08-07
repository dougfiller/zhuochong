ALTER TABLE knowledge_index_generations ADD COLUMN token_counter_version TEXT;
ALTER TABLE knowledge_index_generations ADD COLUMN fts_pretoken_version TEXT;
ALTER TABLE knowledge_index_generations ADD COLUMN retrieval_token_budget INTEGER;
ALTER TABLE knowledge_index_generations ADD COLUMN message_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE knowledge_index_generations ADD COLUMN completed_at_ms INTEGER;
ALTER TABLE knowledge_index_generations ADD COLUMN error_code TEXT;

CREATE UNIQUE INDEX knowledge_index_generations_one_building_idx
  ON knowledge_index_generations(status) WHERE status = 'building';

ALTER TABLE knowledge_chunks ADD COLUMN chunk_index INTEGER;
ALTER TABLE knowledge_chunks ADD COLUMN chunk_schema_version TEXT;
ALTER TABLE knowledge_chunks ADD COLUMN started_at_ms INTEGER;
ALTER TABLE knowledge_chunks ADD COLUMN ended_at_ms INTEGER;
ALTER TABLE knowledge_chunks ADD COLUMN token_count INTEGER;
ALTER TABLE knowledge_chunks ADD COLUMN message_count INTEGER;

CREATE UNIQUE INDEX knowledge_chunks_generation_conversation_index_idx
  ON knowledge_chunks(index_generation_id, conversation_id, chunk_index);
CREATE INDEX knowledge_chunks_generation_conversation_time_idx
  ON knowledge_chunks(index_generation_id, conversation_id, started_at_ms, ended_at_ms);

ALTER TABLE knowledge_message_normalizations
  ADD COLUMN direction TEXT NOT NULL DEFAULT 'other';
UPDATE knowledge_message_normalizations
SET direction = CASE
  WHEN sender_key = (
    SELECT c.account_stable_id
    FROM knowledge_message_versions v
    JOIN knowledge_messages m ON m.id = v.message_id
    JOIN knowledge_conversations c ON c.id = m.conversation_id
    WHERE v.id = knowledge_message_normalizations.message_version_id
  ) THEN 'self'
  ELSE 'other'
END;

CREATE TABLE knowledge_chunk_messages_v4 (
  chunk_id TEXT NOT NULL REFERENCES knowledge_chunks(id) ON DELETE CASCADE,
  message_id TEXT NOT NULL REFERENCES knowledge_messages(id) ON DELETE CASCADE,
  message_version_id TEXT NOT NULL,
  message_index INTEGER NOT NULL CHECK(message_index >= 0),
  PRIMARY KEY(chunk_id, message_version_id),
  UNIQUE(chunk_id, message_index),
  FOREIGN KEY(message_id, message_version_id)
    REFERENCES knowledge_message_versions(message_id, id)
);

INSERT INTO knowledge_chunk_messages_v4(
  chunk_id, message_id, message_version_id, message_index
)
SELECT old.chunk_id,
       old.message_id,
       member.message_version_id,
       ROW_NUMBER() OVER (
         PARTITION BY old.chunk_id
         ORDER BY normalization.created_at_ms,
                  normalization.source_ordinal,
                  normalization.sort_key,
                  COALESCE(msg.message_stable_id, msg.fallback_key)
       ) - 1
FROM knowledge_chunk_messages old
JOIN knowledge_chunks chunk ON chunk.id = old.chunk_id
JOIN knowledge_index_generation_imports mapping
  ON mapping.index_generation_id = chunk.index_generation_id
 AND mapping.conversation_id = chunk.conversation_id
JOIN knowledge_import_generation_members member
  ON member.import_generation_id = mapping.import_generation_id
 AND member.message_id = old.message_id
JOIN knowledge_messages msg ON msg.id = old.message_id
JOIN knowledge_message_normalizations normalization
  ON normalization.message_version_id = member.message_version_id;

DROP TABLE knowledge_chunk_messages;
ALTER TABLE knowledge_chunk_messages_v4 RENAME TO knowledge_chunk_messages;
CREATE INDEX knowledge_chunk_messages_version_idx
  ON knowledge_chunk_messages(message_version_id, chunk_id);

UPDATE knowledge_index_generations
SET token_counter_version = 'v1',
    fts_pretoken_version = 'fts-pretoken-v1',
    retrieval_token_budget = 512
WHERE id IN (SELECT DISTINCT index_generation_id FROM knowledge_chunks);
