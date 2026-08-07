ALTER TABLE knowledge_index_generations ADD COLUMN error_summary TEXT;

CREATE INDEX knowledge_index_generations_status_created_idx
  ON knowledge_index_generations(status, created_at_ms, id);

UPDATE knowledge_index_generations
SET completed_at_ms = COALESCE(
  (SELECT catalog.activated_at_ms
   FROM knowledge_catalog_state catalog
   WHERE catalog.active_index_generation_id = knowledge_index_generations.id),
  created_at_ms
)
WHERE status = 'ready' AND completed_at_ms IS NULL;
