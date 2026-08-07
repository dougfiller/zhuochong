use super::archive_importer::{ArchiveImportSummary, WechatJsonArchiveImporter};
use super::archive_schema::CoverageKind;
use super::archive_store::{
    CompletenessVerdict, ImportFingerprint, MemberAudit, SourceAuditDigest,
};
use super::chunk::{
    draft_from_messages, fts_match_query, BuildMessage, ChunkDraft, Direction,
    CHUNK_SCHEMA_VERSION, FTS_PRETOKEN_VERSION, TOKEN_COUNTER_VERSION,
};
use super::config::validate_local_embedding;
use super::migrations;
use crate::config::LocalEmbeddingConfig;
use crate::wechat::types::ContractError;
use rusqlite::{params, params_from_iter, types::Value, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const MAX_READERS: usize = 4;
const READER_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

#[cfg(test)]
thread_local! {
    static FAIL_ON_APPEND_BATCH: Cell<Option<usize>> = const { Cell::new(None) };
    static FAIL_CHUNK_WRITE_STEP: Cell<Option<u8>> = const { Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn fail_on_append_batch(batch: usize) {
    FAIL_ON_APPEND_BATCH.with(|remaining| remaining.set(Some(batch)));
}

#[cfg(test)]
pub(crate) fn clear_append_batch_failure() {
    FAIL_ON_APPEND_BATCH.with(|remaining| remaining.set(None));
}

#[cfg(test)]
fn should_fail_append_batch() -> bool {
    FAIL_ON_APPEND_BATCH.with(|remaining| match remaining.get() {
        Some(1) => {
            remaining.set(None);
            true
        }
        Some(batch) => {
            remaining.set(Some(batch - 1));
            false
        }
        None => false,
    })
}

#[cfg(test)]
fn fail_chunk_write_at(step: u8) {
    FAIL_CHUNK_WRITE_STEP.with(|value| value.set(Some(step)));
}

#[cfg(test)]
fn clear_chunk_write_failure() {
    FAIL_CHUNK_WRITE_STEP.with(|value| value.set(None));
}

#[cfg(test)]
fn should_fail_chunk_write(step: u8) -> bool {
    FAIL_CHUNK_WRITE_STEP.with(|value| value.get() == Some(step))
}

#[cfg(not(test))]
fn should_fail_chunk_write(_step: u8) -> bool {
    false
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoreAvailability {
    Ready,
    Unavailable,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnowledgeSourceStatus {
    pub(crate) source_id: String,
    pub(crate) coverage_kind: String,
    pub(crate) source_state: String,
    pub(crate) import_status: String,
    pub(crate) lineage_count: i64,
    pub(crate) message_count: i64,
    pub(crate) conversation_count: i64,
    pub(crate) checked_at_ms: i64,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MaintenanceStatus {
    pub(crate) operation_id: Option<String>,
    pub(crate) operation: String,
    pub(crate) state: String,
    pub(crate) maintenance: String,
    pub(crate) completed: u64,
    pub(crate) total: u64,
    pub(crate) error_code: Option<&'static str>,
}

impl MaintenanceStatus {
    fn idle() -> Self {
        Self {
            operation_id: None,
            operation: "idle".into(),
            state: "idle".into(),
            maintenance: "open".into(),
            completed: 0,
            total: 0,
            error_code: None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NewSource {
    pub(crate) account_stable_id: String,
    pub(crate) conversation_stable_id: String,
    pub(crate) export_id: String,
    pub(crate) schema_version: String,
    pub(crate) manifest_hash: String,
    pub(crate) coverage_hash: String,
    pub(crate) exported_at_ms: i64,
    pub(crate) coverage_kind: CoverageKind,
}

#[derive(Clone, Debug)]
pub(crate) struct StagingImport {
    id: String,
    conversation_id: String,
    source_id: String,
}

impl StagingImport {
    pub(crate) fn source_id(&self) -> &str {
        &self.source_id
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CandidateImport(StagingImport);

#[derive(Clone, Debug)]
pub(crate) struct IncomingMessage {
    pub(crate) stable_id: Option<String>,
    pub(crate) fallback_key: Option<String>,
    pub(crate) content: String,
    pub(crate) normalized_content: String,
    pub(crate) content_hash: String,
    pub(crate) source_member_token: String,
    pub(crate) created_at_ms: i64,
    pub(crate) source_ordinal: u64,
    pub(crate) sort_key: String,
    pub(crate) message_kind: String,
    pub(crate) render_kind: String,
    pub(crate) sender_key: String,
    pub(crate) text_hash: String,
    pub(crate) reference_json: Option<String>,
    pub(crate) extra_json: Option<String>,
    pub(crate) media_refs: Vec<IncomingMediaRef>,
}

#[derive(Clone, Debug)]
pub(crate) struct IncomingMediaRef {
    pub(crate) ordinal: u64,
    pub(crate) kind: String,
    pub(crate) relative_path: Option<String>,
    pub(crate) metadata_json: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct CandidateChecks {
    pub(crate) expected_message_count: u64,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct ReadyIndex {
    pub(crate) id: String,
    pub(crate) snapshot_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActiveSnapshot {
    pub(crate) catalog_generation: u64,
    pub(crate) active_index_id: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ActiveReadRequest;

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct FrozenEmbeddingIdentity {
    pub(crate) provider: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) fingerprint: String,
    pub(crate) dimension: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrozenIndexBuildSpec {
    pub(crate) chunk_schema_version: String,
    pub(crate) token_counter_version: String,
    pub(crate) fts_pretoken_version: String,
    pub(crate) retrieval_token_budget: u32,
    pub(crate) embedding: FrozenEmbeddingIdentity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrozenIndexBuild {
    pub(crate) index_generation_id: String,
    pub(crate) import_snapshot_hash: String,
    pub(crate) message_count: u64,
    pub(crate) spec: FrozenIndexBuildSpec,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct StableConversationKey {
    pub(crate) account_stable_id: String,
    pub(crate) conversation_stable_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildMessageCursor {
    pub(crate) created_at_ms: i64,
    pub(crate) source_ordinal: u64,
    pub(crate) sort_key: String,
    pub(crate) stable_message_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildMessagePage {
    pub(crate) messages: Vec<BuildMessage>,
    pub(crate) next_cursor: Option<BuildMessageCursor>,
    pub(crate) has_more: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ActiveFtsRequest {
    pub(crate) scope: Vec<StableConversationKey>,
    pub(crate) query: String,
    pub(crate) from_ms: Option<i64>,
    pub(crate) to_ms: Option<i64>,
    pub(crate) top_k: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FtsHit {
    pub(crate) chunk_key: String,
    pub(crate) content: String,
    pub(crate) token_count: u32,
    pub(crate) started_at_ms: i64,
    pub(crate) ended_at_ms: i64,
    pub(crate) rank: f64,
}

#[derive(Clone, Debug)]
pub(crate) struct DeletionRequest {
    pub(crate) source_id: Option<String>,
    pub(crate) conversation_id: Option<String>,
    pub(crate) message_id: Option<String>,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SourceFact {
    account_stable_id: String,
    exported_at_ms: i64,
    coverage_rank: u8,
    manifest_hash: String,
    coverage_hash: String,
    export_id: String,
}

/// The only owner of knowledge.sqlite connections and SQL. An unavailable
/// store is intentional: callers receive KB_NOT_READY and must not fall back.
#[derive(Default)]
struct MaintenanceRegistry {
    active_operation_id: Option<String>,
    statuses: HashMap<String, MaintenanceStatus>,
}

struct KnowledgeStoreInner {
    availability: StoreAvailability,
    path: Option<PathBuf>,
    writer: Mutex<Option<Connection>>,
    active_readers: Mutex<usize>,
    maintenance: Mutex<MaintenanceRegistry>,
}

#[derive(Clone)]
pub(crate) struct KnowledgeStore {
    inner: Arc<KnowledgeStoreInner>,
}

impl Default for KnowledgeStore {
    fn default() -> Self {
        Self::unavailable()
    }
}

impl KnowledgeStore {
    pub(crate) fn open_or_unavailable(data_dir: &Path) -> Self {
        Self::open(data_dir).unwrap_or_else(|_| Self::unavailable())
    }

    pub(crate) fn open(data_dir: &Path) -> Result<Self, ContractError> {
        let root = data_dir.join("wechat_knowledge");
        fs::create_dir_all(&root).map_err(|_| ContractError::KbNotReady)?;
        let path = root.join("knowledge.sqlite");
        Self::open_database(path)
    }

    fn open_database(path: PathBuf) -> Result<Self, ContractError> {
        let writer = migrations::open_writer(&path)?;
        Ok(Self {
            inner: Arc::new(KnowledgeStoreInner {
                availability: StoreAvailability::Ready,
                path: Some(path),
                writer: Mutex::new(Some(writer)),
                active_readers: Mutex::new(0),
                maintenance: Mutex::new(MaintenanceRegistry::default()),
            }),
        })
    }

    fn unavailable() -> Self {
        Self {
            inner: Arc::new(KnowledgeStoreInner {
                availability: StoreAvailability::Unavailable,
                path: None,
                writer: Mutex::new(None),
                active_readers: Mutex::new(0),
                maintenance: Mutex::new(MaintenanceRegistry::default()),
            }),
        }
    }

    pub(crate) fn availability(&self) -> StoreAvailability {
        self.inner.availability
    }

    pub(crate) fn import_wechat_json_archive(
        &self,
        source_root: &Path,
    ) -> Result<ArchiveImportSummary, ContractError> {
        WechatJsonArchiveImporter::open(source_root, self)?.import()
    }

    pub(crate) fn list_sources(&self) -> Result<Vec<KnowledgeSourceStatus>, ContractError> {
        if self.inner.availability != StoreAvailability::Ready {
            return Ok(Vec::new());
        }
        self.with_reader(|connection| {
            let mut statement = connection.prepare(
                "SELECT s.id,s.coverage_kind,CASE WHEN EXISTS(SELECT 1 FROM knowledge_denials d WHERE d.source_id=s.id) THEN 'denied' ELSE s.source_state END,s.import_status,(SELECT COUNT(*) FROM knowledge_source_lineage l WHERE l.predecessor_source_id=s.id OR l.successor_source_id=s.id),(SELECT COUNT(DISTINCT v.message_id) FROM knowledge_message_sources p JOIN knowledge_message_versions v ON v.id=p.message_version_id WHERE p.source_id=s.id),(SELECT COUNT(DISTINCT m.conversation_id) FROM knowledge_message_sources p JOIN knowledge_message_versions v ON v.id=p.message_version_id JOIN knowledge_messages m ON m.id=v.message_id WHERE p.source_id=s.id),s.checked_at_ms FROM knowledge_sources s ORDER BY s.checked_at_ms DESC"
            ).map_err(|_| ContractError::KbNotReady)?;
            let rows = statement.query_map([], |row| Ok(KnowledgeSourceStatus {
                source_id: row.get(0)?, coverage_kind: row.get(1)?, source_state: row.get(2)?, import_status: row.get(3)?, lineage_count: row.get(4)?, message_count: row.get(5)?, conversation_count: row.get(6)?, checked_at_ms: row.get(7)?,
            })).map_err(|_| ContractError::KbNotReady)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|_| ContractError::KbNotReady)
        })
    }

    pub(crate) fn maintenance_status(&self) -> Result<MaintenanceStatus, ContractError> {
        let registry = self
            .inner
            .maintenance
            .lock()
            .map_err(|_| ContractError::KbNotReady)?;
        Ok(registry
            .active_operation_id
            .as_ref()
            .and_then(|operation_id| registry.statuses.get(operation_id))
            .cloned()
            .unwrap_or_else(MaintenanceStatus::idle))
    }

    pub(crate) fn maintenance_status_for(
        &self,
        operation_id: Option<&str>,
    ) -> Result<MaintenanceStatus, ContractError> {
        let Some(operation_id) = operation_id else {
            return self.maintenance_status();
        };
        self.inner
            .maintenance
            .lock()
            .map_err(|_| ContractError::KbNotReady)?
            .statuses
            .get(operation_id)
            .cloned()
            .ok_or(ContractError::KbNotReady)
    }

    pub(crate) fn start_source_import(
        &self,
        source_root: PathBuf,
    ) -> Result<String, ContractError> {
        let operation_id = self.begin_maintenance("importing", 1)?;
        let store = self.clone();
        let worker_operation_id = operation_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = store.import_wechat_json_archive(&source_root);
            store.finish_maintenance(&worker_operation_id, result.is_ok());
        });
        Ok(operation_id)
    }

    pub(crate) fn retire_source(&self, source_id: &str) -> Result<(), ContractError> {
        let operation_id = self.begin_maintenance("retiring", 1)?;
        let result = self.with_writer(|connection| {
            let changed = connection.execute("UPDATE knowledge_sources SET source_state='retired',checked_at_ms=?1 WHERE id=?2 AND source_state='active' AND NOT EXISTS(SELECT 1 FROM knowledge_denials WHERE source_id=?2) AND NOT EXISTS(SELECT 1 FROM knowledge_index_generations WHERE status='building')", params![now_ms(), source_id]).map_err(|_| ContractError::KbNotReady)?;
            if changed == 1 { Ok(()) } else { Err(ContractError::KbNotReady) }
        });
        self.finish_maintenance(&operation_id, result.is_ok());
        result
    }

    pub(crate) fn deny_source(&self, source_id: &str) -> Result<(), ContractError> {
        let operation_id = self.begin_maintenance("denying", 1)?;
        let exists = self.with_reader(|connection| {
            connection
                .query_row(
                    "SELECT 1 FROM knowledge_sources WHERE id=?1",
                    [source_id],
                    |_| Ok(()),
                )
                .map_err(|_| ContractError::KbNotReady)
        });
        let result = exists.and_then(|_| {
            self.deny_or_delete(DeletionRequest {
                source_id: Some(source_id.to_owned()),
                conversation_id: None,
                message_id: None,
                reason: "user_denied_source".into(),
            })
        });
        self.finish_maintenance(&operation_id, result.is_ok());
        result
    }

    pub(crate) fn start_rebuild(&self, roots: Vec<PathBuf>) -> Result<String, ContractError> {
        if roots.is_empty() || roots.iter().any(|root| root.as_os_str().is_empty()) {
            return Err(ContractError::KbNotReady);
        }
        let operation_id = self.begin_maintenance("rebuilding", roots.len() as u64)?;
        let store = self.clone();
        let worker_operation_id = operation_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let result = store.rebuild_from_selected_roots(&roots);
            store.finish_maintenance(&worker_operation_id, result.is_ok());
        });
        Ok(operation_id)
    }

    fn rebuild_from_selected_roots(&self, roots: &[PathBuf]) -> Result<(), ContractError> {
        let primary = self.inner.path.clone().ok_or(ContractError::KbNotReady)?;
        let root = primary.parent().ok_or(ContractError::KbNotReady)?;
        let candidate_dir = root.join(format!(".candidate-{}", Uuid::new_v4().simple()));
        fs::create_dir(&candidate_dir).map_err(|_| ContractError::KbNotReady)?;
        let candidate_path = candidate_dir.join("knowledge.sqlite");
        let candidate_manifest = candidate_dir.join("knowledge.sqlite.manifest.json");
        let candidate_result = (|| {
            let sources = {
                let candidate = Self::open_database(candidate_path.clone())?;
                for selected_root in roots {
                    candidate.import_wechat_json_archive(selected_root)?;
                }
                let sources = candidate.list_sources()?;
                if sources.is_empty()
                    || sources.iter().any(|source| source.source_state != "active")
                {
                    return Err(ContractError::KbNotReady);
                }
                sources
            };
            write_redacted_manifest(&candidate_manifest, &candidate_path, &sources)?;
            validate_database_pair(&candidate_path, &candidate_manifest)
        })();
        if candidate_result.is_err() {
            let _ = fs::remove_dir_all(&candidate_dir);
        }
        candidate_result?;
        self.wait_for_reader_drain(READER_DRAIN_TIMEOUT)?;
        let mut writer = self
            .inner
            .writer
            .lock()
            .map_err(|_| ContractError::KbNotReady)?;
        drop(writer.take().ok_or(ContractError::KbNotReady)?);
        let primary_manifest = root.join("knowledge.sqlite.manifest.json");
        let result = (|| {
            let primary_sources = self.list_sources()?;
            write_redacted_manifest(&primary_manifest, &primary, &primary_sources)?;
            validate_database_pair(&primary, &primary_manifest)?;
            publish_candidate_pair(
                &primary,
                &primary_manifest,
                &candidate_path,
                &candidate_manifest,
                root,
            )
        })();
        match migrations::open_writer(&primary) {
            Ok(reopened) => {
                *writer = Some(reopened);
                result
            }
            Err(_) => {
                *writer = None;
                Err(ContractError::KbNotReady)
            }
        }
    }

    fn begin_maintenance(&self, operation: &str, total: u64) -> Result<String, ContractError> {
        let mut registry = self
            .inner
            .maintenance
            .lock()
            .map_err(|_| ContractError::KbNotReady)?;
        if registry.active_operation_id.is_some() {
            return Err(ContractError::KbNotReady);
        }
        let operation_id = opaque_id("operation");
        registry.statuses.insert(
            operation_id.clone(),
            MaintenanceStatus {
                operation_id: Some(operation_id.clone()),
                operation: operation.into(),
                state: "running".into(),
                maintenance: "closed".into(),
                completed: 0,
                total,
                error_code: None,
            },
        );
        registry.active_operation_id = Some(operation_id.clone());
        Ok(operation_id)
    }

    fn finish_maintenance(&self, operation_id: &str, success: bool) {
        if let Ok(mut registry) = self.inner.maintenance.lock() {
            if registry.active_operation_id.as_deref() == Some(operation_id) {
                let Some(status) = registry.statuses.get_mut(operation_id) else {
                    return;
                };
                status.completed = status.total;
                status.state = if success { "succeeded" } else { "failed" }.into();
                status.maintenance = "open".into();
                status.error_code = (!success).then_some("KB_NOT_READY");
                registry.active_operation_id = None;
            }
        }
    }

    fn wait_for_reader_drain(&self, timeout: std::time::Duration) -> Result<(), ContractError> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if *self
                .inner
                .active_readers
                .lock()
                .map_err(|_| ContractError::KbNotReady)?
                == 0
            {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err(ContractError::KbNotReady);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    pub(crate) fn begin_staging_source(
        &self,
        input: NewSource,
    ) -> Result<StagingImport, ContractError> {
        self.with_writer(|connection| {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| ContractError::KbNotReady)?;
            let building: Option<i64> = transaction
                .query_row(
                    "SELECT 1 FROM knowledge_index_generations WHERE status='building'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| ContractError::KbNotReady)?;
            if building.is_some() {
                return Err(ContractError::KbNotReady);
            }
            let source_id: Option<String> = transaction
                .query_row(
                    "SELECT id FROM knowledge_sources WHERE account_stable_id=?1 AND export_id=?2 AND schema_version=?3 AND manifest_hash=?4 AND coverage_hash=?5",
                    params![input.account_stable_id, input.export_id, input.schema_version, input.manifest_hash, input.coverage_hash],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| ContractError::KbNotReady)?;
            let source_id = match source_id {
                Some(id) => id,
                None => {
                    let id = opaque_id("source");
                    transaction.execute(
                        "INSERT INTO knowledge_sources(id,account_stable_id,export_id,schema_version,manifest_hash,coverage_hash,snapshot_kind,scope_filters_json,integrity_json,source_state,import_status,checked_at_ms,exported_at_ms,coverage_kind) VALUES(?1,?2,?3,?4,?5,?6,?7,'{}','{}','active','staging',?8,?9,?10)",
                        params![id, input.account_stable_id, input.export_id, input.schema_version, input.manifest_hash, input.coverage_hash, input.coverage_kind.as_str(), now_ms(), input.exported_at_ms, input.coverage_kind.as_str()],
                    ).map_err(|_| ContractError::KbNotReady)?;
                    id
                }
            };
            let predecessor_ids = {
                let mut statement = transaction
                    .prepare("SELECT id FROM knowledge_sources WHERE account_stable_id=?1 AND id<>?2 ORDER BY exported_at_ms,manifest_hash,coverage_hash,export_id")
                    .map_err(|_| ContractError::KbNotReady)?;
                let ids = statement
                    .query_map(params![input.account_stable_id, source_id], |row| row.get::<_, String>(0))
                    .map_err(|_| ContractError::KbNotReady)?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|_| ContractError::KbNotReady)?;
                ids
            };
            for predecessor_id in predecessor_ids {
                transaction.execute(
                    "INSERT OR IGNORE INTO knowledge_source_lineage(predecessor_source_id,successor_source_id,relation_kind,verified_at_ms,evidence_hash) VALUES(?1,?2,'overlaps',?3,?4)",
                    params![predecessor_id, source_id, now_ms(), hex_hash(&format!("{}|{}|{}", input.coverage_hash, input.manifest_hash, input.export_id))],
                ).map_err(|_| ContractError::KbNotReady)?;
            }
            let conversation_id: Option<String> = transaction
                .query_row(
                    "SELECT id FROM knowledge_conversations WHERE account_stable_id=?1 AND conversation_stable_id=?2",
                    params![input.account_stable_id, input.conversation_stable_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| ContractError::KbNotReady)?;
            let conversation_id = match conversation_id {
                Some(id) => id,
                None => {
                    let id = opaque_id("conversation");
                    transaction.execute(
                        "INSERT INTO knowledge_conversations(id,account_stable_id,conversation_stable_id) VALUES(?1,?2,?3)",
                        params![id, input.account_stable_id, input.conversation_stable_id],
                    ).map_err(|_| ContractError::KbNotReady)?;
                    id
                }
            };
            let parent_generation_id: Option<String> = transaction
                .query_row(
                    "SELECT active_import_generation_id FROM knowledge_conversations WHERE id=?1",
                    [&conversation_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| ContractError::KbNotReady)?
                .flatten();
            let current_source = source_fact(&transaction, &source_id)?;
            let mut source_set = parent_generation_id
                .as_deref()
                .map(|parent| source_facts_for_generation(&transaction, parent))
                .transpose()?
                .unwrap_or_default();
            source_set.push(current_source.clone());
            source_set.sort();
            source_set.dedup();
            let generation_id = opaque_id("generation");
            let source_set_hash = source_set_hash(&source_set);
            transaction.execute(
                "INSERT INTO knowledge_import_generations(id,trigger_source_id,conversation_id,parent_generation_id,source_set_hash,merge_mode,status,created_at_ms) VALUES(?1,?2,?3,?4,?5,'replace','staging',?6)",
                params![generation_id, source_id, conversation_id, parent_generation_id, source_set_hash, now_ms()],
            ).map_err(|_| ContractError::KbNotReady)?;
            for (precedence, fact) in source_set.iter().enumerate() {
                let associated_source_id: String = transaction.query_row(
                    "SELECT id FROM knowledge_sources WHERE account_stable_id=?1 AND exported_at_ms=?2 AND coverage_kind=?3 AND manifest_hash=?4 AND coverage_hash=?5 AND export_id=?6",
                    params![fact.account_stable_id, fact.exported_at_ms, coverage_kind_from_rank(fact.coverage_rank), fact.manifest_hash, fact.coverage_hash, fact.export_id],
                    |row| row.get(0),
                ).map_err(|_| ContractError::KbNotReady)?;
                transaction.execute("INSERT INTO knowledge_import_generation_sources(import_generation_id,source_id,precedence,coverage_role) VALUES(?1,?2,?3,?4)", params![generation_id, associated_source_id, precedence as i64, if associated_source_id == source_id { "primary" } else { "merged" }]).map_err(|_| ContractError::KbNotReady)?;
            }
            let retain_parent = input.coverage_kind != CoverageKind::Full
                || source_set.iter().any(|fact| fact != &current_source && fact > &current_source);
            if retain_parent {
                if let Some(parent_generation_id) = parent_generation_id.as_deref() {
                    transaction.execute(
                        "INSERT INTO knowledge_import_generation_members(import_generation_id,message_id,message_version_id,selection_reason) SELECT ?1,message_id,message_version_id,'retained_outside_coverage' FROM knowledge_import_generation_members WHERE import_generation_id=?2",
                        params![generation_id, parent_generation_id],
                    ).map_err(|_| ContractError::KbNotReady)?;
                }
            }
            transaction.commit().map_err(|_| ContractError::KbNotReady)?;
            Ok(StagingImport { id: generation_id, conversation_id, source_id })
        })
    }

    pub(crate) fn append_staging_messages(
        &self,
        staging: &StagingImport,
        batch: &[IncomingMessage],
    ) -> Result<(), ContractError> {
        #[cfg(test)]
        if should_fail_append_batch() {
            return Err(ContractError::KbNotReady);
        }
        self.with_writer(|connection| {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| ContractError::KbNotReady)?;
            let staging_exists: Option<i64> = transaction.query_row(
                "SELECT 1 FROM knowledge_import_generations WHERE id=?1 AND conversation_id=?2 AND status='staging'",
                params![staging.id, staging.conversation_id], |row| row.get(0),
            ).optional().map_err(|_| ContractError::KbNotReady)?;
            if staging_exists.is_none() { return Err(ContractError::KbNotReady); }
            for incoming in batch {
                if (incoming.stable_id.is_some()) == (incoming.fallback_key.is_some()) {
                    return Err(ContractError::KbNotReady);
                }
                let identity_key = incoming
                    .stable_id
                    .as_ref()
                    .map(|id| format!("stable:{id}"))
                    .or_else(|| incoming.fallback_key.as_ref().map(|key| format!("fallback:{key}")))
                    .ok_or(ContractError::KbNotReady)?;
                if transaction.execute(
                    "INSERT INTO knowledge_import_generation_input_keys(import_generation_id,identity_key) VALUES(?1,?2)",
                    params![staging.id, identity_key],
                ).map_err(|_| ContractError::KbNotReady)? != 1 {
                    return Err(ContractError::KbNotReady);
                }
                let message_id: Option<String> = if let Some(stable_id) = &incoming.stable_id {
                    transaction.query_row(
                        "SELECT id FROM knowledge_messages WHERE conversation_id=?1 AND message_stable_id=?2",
                        params![staging.conversation_id, stable_id], |row| row.get(0),
                    ).optional().map_err(|_| ContractError::KbNotReady)?
                } else {
                    transaction.query_row(
                        "SELECT id FROM knowledge_messages WHERE conversation_id=?1 AND fallback_key=?2",
                        params![staging.conversation_id, incoming.fallback_key], |row| row.get(0),
                    ).optional().map_err(|_| ContractError::KbNotReady)?
                };
                let message_id = match message_id {
                    Some(id) => id,
                    None => {
                        let id = opaque_id("message");
                        transaction.execute(
                            "INSERT INTO knowledge_messages(id,conversation_id,message_stable_id,fallback_key,low_confidence) VALUES(?1,?2,?3,?4,?5)",
                            params![id, staging.conversation_id, incoming.stable_id, incoming.fallback_key, i64::from(incoming.fallback_key.is_some())],
                        ).map_err(|_| ContractError::KbNotReady)?;
                        id
                    }
                };
                let version_id: Option<String> = transaction.query_row(
                    "SELECT id FROM knowledge_message_versions WHERE message_id=?1 AND content_hash=?2",
                    params![message_id, incoming.content_hash], |row| row.get(0),
                ).optional().map_err(|_| ContractError::KbNotReady)?;
                let (version_id, selection_reason) = match version_id {
                    Some(id) => (id, "unchanged"),
                    None => {
                        let id = opaque_id("version");
                        transaction.execute(
                            "INSERT INTO knowledge_message_versions(id,message_id,import_generation_id,content,normalized_content,content_hash) VALUES(?1,?2,?3,?4,?5,?6)",
                            params![id, message_id, staging.id, incoming.content, incoming.normalized_content, incoming.content_hash],
                        ).map_err(|_| ContractError::KbNotReady)?;
                        (id, "new")
                    }
                };
                transaction.execute(
                    "INSERT OR IGNORE INTO knowledge_message_sources(message_version_id,source_id,source_relative_path) VALUES(?1,?2,?3)",
                    params![version_id, staging.source_id, incoming.source_member_token],
                ).map_err(|_| ContractError::KbNotReady)?;
                transaction.execute(
                    "INSERT OR IGNORE INTO knowledge_message_normalizations(message_version_id,created_at_ms,source_ordinal,sort_key,message_kind,render_kind,sender_key,text_hash,reference_json,extra_json,canonical_hash,direction) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,CASE WHEN ?7=(SELECT account_stable_id FROM knowledge_conversations WHERE id=?12) THEN 'self' ELSE 'other' END)",
                    params![version_id, incoming.created_at_ms, incoming.source_ordinal as i64, incoming.sort_key, incoming.message_kind, incoming.render_kind, incoming.sender_key, incoming.text_hash, incoming.reference_json, incoming.extra_json, incoming.content_hash, staging.conversation_id],
                ).map_err(|_| ContractError::KbNotReady)?;
                for media in &incoming.media_refs {
                    transaction.execute(
                        "INSERT OR IGNORE INTO knowledge_media_refs(id,message_version_id,source_id,source_relative_path,ordinal,media_kind,exists_state,metadata_json) VALUES(?1,?2,?3,?4,?5,?6,'unknown',?7)",
                        params![opaque_id("media"), version_id, staging.source_id, media.relative_path.as_deref().unwrap_or(""), media.ordinal as i64, media.kind, media.metadata_json],
                    ).map_err(|_| ContractError::KbNotReady)?;
                }
                if incoming_source_wins(&transaction, staging, &message_id)? {
                    transaction.execute(
                        "INSERT INTO knowledge_import_generation_members(import_generation_id,message_id,message_version_id,selection_reason) VALUES(?1,?2,?3,?4) ON CONFLICT(import_generation_id,message_id) DO UPDATE SET message_version_id=excluded.message_version_id,selection_reason='newer_source'",
                        params![staging.id, message_id, version_id, selection_reason],
                    ).map_err(|_| ContractError::KbNotReady)?;
                }
            }
            transaction.execute(
                "UPDATE knowledge_import_generations SET message_count=(SELECT COUNT(*) FROM knowledge_import_generation_members WHERE import_generation_id=?1) WHERE id=?1 AND status='staging'",
                [&staging.id],
            ).map_err(|_| ContractError::KbNotReady)?;
            transaction.commit().map_err(|_| ContractError::KbNotReady)
        })
    }

    pub(crate) fn mark_ready_candidate(
        &self,
        staging: StagingImport,
        checks: CandidateChecks,
    ) -> Result<CandidateImport, ContractError> {
        self.with_writer(|connection| {
            let changed = connection.execute(
                "UPDATE knowledge_import_generations SET status='ready_candidate' WHERE id=?1 AND conversation_id=?2 AND status='staging' AND (SELECT COUNT(DISTINCT v.message_id) FROM knowledge_message_sources p JOIN knowledge_message_versions v ON v.id=p.message_version_id JOIN knowledge_messages m ON m.id=v.message_id WHERE p.source_id=?3 AND m.conversation_id=?2)=?4",
                params![staging.id, staging.conversation_id, staging.source_id, checks.expected_message_count as i64],
            ).map_err(|_| ContractError::KbNotReady)?;
            if changed == 1 { Ok(CandidateImport(staging)) } else { Err(ContractError::KbNotReady) }
        })
    }

    pub(crate) fn record_source_audits(
        &self,
        staging: &StagingImport,
        verdict: CompletenessVerdict,
        members: &[MemberAudit],
    ) -> Result<(), ContractError> {
        self.with_writer(|connection| {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| ContractError::KbNotReady)?;
            for member in members {
                transaction.execute(
                    "INSERT OR IGNORE INTO knowledge_source_members(source_id,member_path_token,member_kind,size_bytes,mtime_ms,declared_hash,checked) VALUES(?1,?2,?3,?4,?5,?6,1)",
                    params![staging.source_id, member.member_path_token, member.member_kind, member.size_bytes as i64, member.mtime_ms, member.declared_hash],
                ).map_err(|_| ContractError::KbNotReady)?;
            }
            transaction.execute(
                "UPDATE knowledge_sources SET import_status=?1,checked_at_ms=?2 WHERE id=?3",
                params![verdict.as_str(), now_ms(), staging.source_id],
            ).map_err(|_| ContractError::KbNotReady)?;
            transaction.commit().map_err(|_| ContractError::KbNotReady)
        })
    }

    pub(crate) fn set_source_verdict(
        &self,
        source_id: &str,
        verdict: CompletenessVerdict,
    ) -> Result<(), ContractError> {
        self.with_writer(|connection| {
            connection
                .execute(
                    "UPDATE knowledge_sources SET import_status=?1,checked_at_ms=?2 WHERE id=?3",
                    params![verdict.as_str(), now_ms(), source_id],
                )
                .map_err(|_| ContractError::KbNotReady)
                .and_then(|changed| {
                    (changed == 1)
                        .then_some(())
                        .ok_or(ContractError::KbNotReady)
                })
        })
    }

    pub(crate) fn discard_stagings(&self, stagings: &[StagingImport]) -> Result<(), ContractError> {
        if stagings.is_empty() {
            return Ok(());
        }
        self.with_writer(|connection| {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| ContractError::KbNotReady)?;
            for staging in stagings {
                transaction.execute("DELETE FROM knowledge_index_generation_imports WHERE import_generation_id=?1", [&staging.id]).map_err(|_| ContractError::KbNotReady)?;
                transaction.execute("DELETE FROM knowledge_import_generation_members WHERE import_generation_id=?1", [&staging.id]).map_err(|_| ContractError::KbNotReady)?;
                transaction.execute("DELETE FROM knowledge_message_versions WHERE import_generation_id=?1", [&staging.id]).map_err(|_| ContractError::KbNotReady)?;
                transaction.execute("DELETE FROM knowledge_import_generations WHERE id=?1 AND status IN ('staging','ready_candidate')", [&staging.id]).map_err(|_| ContractError::KbNotReady)?;
            }
            transaction.execute("DELETE FROM knowledge_index_generations WHERE id<>COALESCE((SELECT active_index_generation_id FROM knowledge_catalog_state WHERE singleton_id=1),'') AND NOT EXISTS(SELECT 1 FROM knowledge_index_generation_imports m WHERE m.index_generation_id=knowledge_index_generations.id)", []).map_err(|_| ContractError::KbNotReady)?;
            for staging in stagings {
                transaction.execute("DELETE FROM knowledge_source_lineage WHERE predecessor_source_id=?1 OR successor_source_id=?1", [&staging.source_id]).map_err(|_| ContractError::KbNotReady)?;
                transaction.execute("DELETE FROM knowledge_message_sources WHERE source_id=?1 AND NOT EXISTS(SELECT 1 FROM knowledge_import_generations WHERE trigger_source_id=?1)", [&staging.source_id]).map_err(|_| ContractError::KbNotReady)?;
                transaction.execute("DELETE FROM knowledge_sources WHERE id=?1 AND NOT EXISTS(SELECT 1 FROM knowledge_import_generations WHERE trigger_source_id=?1)", [&staging.source_id]).map_err(|_| ContractError::KbNotReady)?;
            }
            transaction.commit().map_err(|_| ContractError::KbNotReady)
        })
    }

    pub(crate) fn discard_source_candidates(&self, source_id: &str) -> Result<(), ContractError> {
        self.with_writer(|connection| {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| ContractError::KbNotReady)?;
            transaction
                .execute(
                    "DELETE FROM knowledge_index_generation_imports WHERE import_generation_id IN (SELECT id FROM knowledge_import_generations WHERE trigger_source_id=?1 AND status IN ('staging','ready_candidate'))",
                    [source_id],
                )
                .map_err(|_| ContractError::KbNotReady)?;
            transaction
                .execute(
                    "DELETE FROM knowledge_import_generation_members WHERE import_generation_id IN (SELECT id FROM knowledge_import_generations WHERE trigger_source_id=?1 AND status IN ('staging','ready_candidate'))",
                    [source_id],
                )
                .map_err(|_| ContractError::KbNotReady)?;
            transaction
                .execute(
                    "DELETE FROM knowledge_message_versions WHERE import_generation_id IN (SELECT id FROM knowledge_import_generations WHERE trigger_source_id=?1 AND status IN ('staging','ready_candidate'))",
                    [source_id],
                )
                .map_err(|_| ContractError::KbNotReady)?;
            transaction
                .execute(
                    "DELETE FROM knowledge_import_generations WHERE trigger_source_id=?1 AND status IN ('staging','ready_candidate')",
                    [source_id],
                )
                .map_err(|_| ContractError::KbNotReady)?;
            transaction
                .execute(
                    "DELETE FROM knowledge_source_lineage WHERE predecessor_source_id=?1 OR successor_source_id=?1",
                    [source_id],
                )
                .map_err(|_| ContractError::KbNotReady)?;
            transaction
                .execute(
                    "DELETE FROM knowledge_message_sources WHERE source_id=?1 AND NOT EXISTS(SELECT 1 FROM knowledge_import_generations WHERE trigger_source_id=?1)",
                    [source_id],
                )
                .map_err(|_| ContractError::KbNotReady)?;
            transaction
                .execute(
                    "DELETE FROM knowledge_sources WHERE id=?1 AND NOT EXISTS(SELECT 1 FROM knowledge_import_generations WHERE trigger_source_id=?1)",
                    [source_id],
                )
                .map_err(|_| ContractError::KbNotReady)?;
            transaction.commit().map_err(|_| ContractError::KbNotReady)
        })
    }

    pub(crate) fn begin_or_resume_index_build(
        &self,
        spec: FrozenIndexBuildSpec,
    ) -> Result<FrozenIndexBuild, ContractError> {
        validate_build_spec(&spec)?;
        self.with_writer(|connection| {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| ContractError::KbNotReady)?;
            let existing: Option<String> = transaction
                .query_row(
                    "SELECT id FROM knowledge_index_generations WHERE status='building'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| ContractError::KbNotReady)?;
            if let Some(index_generation_id) = existing {
                let frozen = load_frozen_build(&transaction, &index_generation_id)?;
                if frozen.spec != spec || !mapping_is_publishable(&transaction, &index_generation_id)? {
                    return Err(ContractError::KbNotReady);
                }
                transaction.commit().map_err(|_| ContractError::KbNotReady)?;
                return Ok(frozen);
            }

            let selections = select_index_imports(&transaction)?;
            if selections.is_empty() {
                return Err(ContractError::KbNotReady);
            }
            let import_snapshot_hash = snapshot_hash(&selections);
            let message_count = selections.iter().map(|selection| selection.message_count).sum();
            let index_generation_id = opaque_id("index");
            let embedding_metadata_json = serde_json::to_string(&spec.embedding)
                .map_err(|_| ContractError::KbNotReady)?;
            transaction.execute(
                "INSERT INTO knowledge_index_generations(id,schema_version,embedding_metadata_json,snapshot_hash,status,chunk_count,created_at_ms,token_counter_version,fts_pretoken_version,retrieval_token_budget,message_count) VALUES(?1,?2,?3,?4,'building',0,?5,?6,?7,?8,?9)",
                params![index_generation_id, spec.chunk_schema_version, embedding_metadata_json, import_snapshot_hash, now_ms(), spec.token_counter_version, spec.fts_pretoken_version, spec.retrieval_token_budget as i64, message_count as i64],
            ).map_err(|_| ContractError::KbNotReady)?;
            for selection in &selections {
                transaction.execute(
                    "INSERT INTO knowledge_index_generation_imports(index_generation_id,conversation_id,import_generation_id) VALUES(?1,?2,?3)",
                    params![index_generation_id, selection.conversation_id, selection.import_generation_id],
                ).map_err(|_| ContractError::KbNotReady)?;
            }
            let mapping_count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM knowledge_index_generation_imports WHERE index_generation_id=?1",
                [&index_generation_id],
                |row| row.get(0),
            ).map_err(|_| ContractError::KbNotReady)?;
            if mapping_count != selections.len() as i64 {
                return Err(ContractError::KbNotReady);
            }
            transaction.commit().map_err(|_| ContractError::KbNotReady)?;
            Ok(FrozenIndexBuild {
                index_generation_id,
                import_snapshot_hash,
                message_count,
                spec,
            })
        })
    }

    pub(crate) fn list_build_conversations(
        &self,
        index_generation_id: &str,
    ) -> Result<Vec<StableConversationKey>, ContractError> {
        self.with_reader(|connection| {
            let mut statement = connection.prepare(
                "SELECT c.account_stable_id,c.conversation_stable_id FROM knowledge_index_generations i JOIN knowledge_index_generation_imports mapping ON mapping.index_generation_id=i.id JOIN knowledge_conversations c ON c.id=mapping.conversation_id WHERE i.id=?1 AND i.status='building' ORDER BY c.account_stable_id,c.conversation_stable_id",
            ).map_err(|_| ContractError::KbNotReady)?;
            let rows = statement.query_map([index_generation_id], |row| Ok(StableConversationKey {
                account_stable_id: row.get(0)?,
                conversation_stable_id: row.get(1)?,
            })).map_err(|_| ContractError::KbNotReady)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| ContractError::KbNotReady)?;
            Ok(rows)
        })
    }

    pub(crate) fn read_build_message_page(
        &self,
        index_generation_id: &str,
        conversation: &StableConversationKey,
        after: Option<&BuildMessageCursor>,
        limit: u16,
    ) -> Result<BuildMessagePage, ContractError> {
        if limit == 0 || limit > 256 {
            return Err(ContractError::KbNotReady);
        }
        self.with_reader(|connection| {
            let mut statement = connection.prepare(
                "SELECT c.account_stable_id,c.conversation_stable_id,m.id,v.id,COALESCE(m.message_stable_id,m.fallback_key),n.created_at_ms,n.source_ordinal,n.sort_key,n.sender_key,n.direction,n.message_kind,v.normalized_content,v.content_hash FROM knowledge_index_generations i JOIN knowledge_index_generation_imports mapping ON mapping.index_generation_id=i.id JOIN knowledge_conversations c ON c.id=mapping.conversation_id JOIN knowledge_import_generation_members member ON member.import_generation_id=mapping.import_generation_id JOIN knowledge_messages m ON m.id=member.message_id JOIN knowledge_message_versions v ON v.id=member.message_version_id AND v.message_id=m.id LEFT JOIN knowledge_message_normalizations n ON n.message_version_id=v.id WHERE i.id=?1 AND i.status='building' AND c.account_stable_id=?2 AND c.conversation_stable_id=?3 AND (?4 IS NULL OR n.created_at_ms>?4 OR (n.created_at_ms=?4 AND n.source_ordinal>?5) OR (n.created_at_ms=?4 AND n.source_ordinal=?5 AND n.sort_key>?6) OR (n.created_at_ms=?4 AND n.source_ordinal=?5 AND n.sort_key=?6 AND COALESCE(m.message_stable_id,m.fallback_key)>?7)) ORDER BY n.created_at_ms,n.source_ordinal,n.sort_key,COALESCE(m.message_stable_id,m.fallback_key) LIMIT ?8",
            ).map_err(|_| ContractError::KbNotReady)?;
            let rows = statement.query_map(
                params![
                    index_generation_id,
                    conversation.account_stable_id,
                    conversation.conversation_stable_id,
                    after.map(|cursor| cursor.created_at_ms),
                    after.map(|cursor| cursor.source_ordinal as i64),
                    after.map(|cursor| cursor.sort_key.as_str()),
                    after.map(|cursor| cursor.stable_message_key.as_str()),
                    limit as i64,
                ],
                |row| Ok((
                    row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<i64>>(6)?, row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?, row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?, row.get::<_, String>(11)?, row.get::<_, String>(12)?,
                )),
            ).map_err(|_| ContractError::KbNotReady)?;
            let raw = rows.collect::<Result<Vec<_>, _>>().map_err(|_| ContractError::KbNotReady)?;
            let mut messages = Vec::new();
            let mut next_cursor = None;
            for (account, stable_conversation, message_id, version_id, stable_key, created_at, source_ordinal, sort_key, sender_key, direction, message_kind, content, content_hash) in &raw {
                let created_at_ms = created_at.ok_or(ContractError::KbNotReady)?;
                let source_ordinal = source_ordinal.and_then(|value| u64::try_from(value).ok()).ok_or(ContractError::KbNotReady)?;
                let sort_key = sort_key.as_ref().ok_or(ContractError::KbNotReady)?;
                next_cursor = Some(BuildMessageCursor { created_at_ms, source_ordinal, sort_key: sort_key.clone(), stable_message_key: stable_key.clone() });
                let kind = message_kind.as_ref().ok_or(ContractError::KbNotReady)?;
                if matches!(kind.as_str(), "image" | "video" | "voice" | "file" | "recall") {
                    continue;
                }
                if !matches!(kind.as_str(), "text" | "quote" | "reply" | "link" | "location" | "emoji" | "system") {
                    return Err(ContractError::KbNotReady);
                }
                messages.push(BuildMessage {
                    account_stable_id: account.clone(), conversation_stable_id: stable_conversation.clone(),
                    message_id: message_id.clone(), message_version_id: version_id.clone(), stable_message_key: stable_key.clone(),
                    created_at_ms, source_ordinal, sort_key: sort_key.clone(),
                    sender_key: sender_key.as_ref().ok_or(ContractError::KbNotReady)?.clone(),
                    direction: Direction::parse(direction.as_deref().ok_or(ContractError::KbNotReady)?)?,
                    message_kind: kind.clone(), content: content.clone(), content_hash: content_hash.clone(),
                });
            }
            Ok(BuildMessagePage { messages, next_cursor, has_more: raw.len() == limit as usize })
        })
    }

    pub(crate) fn reset_build_chunks(
        &self,
        index_generation_id: &str,
    ) -> Result<(), ContractError> {
        self.with_writer(|connection| {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| ContractError::KbNotReady)?;
            let building: Option<i64> = transaction
                .query_row(
                    "SELECT 1 FROM knowledge_index_generations WHERE id=?1 AND status='building'",
                    [index_generation_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| ContractError::KbNotReady)?;
            if building.is_none() {
                return Err(ContractError::KbNotReady);
            }
            transaction
                .execute(
                    "DELETE FROM knowledge_chunks_fts WHERE index_generation_id=?1",
                    [index_generation_id],
                )
                .map_err(|_| ContractError::KbNotReady)?;
            transaction
                .execute(
                    "DELETE FROM knowledge_chunks WHERE index_generation_id=?1",
                    [index_generation_id],
                )
                .map_err(|_| ContractError::KbNotReady)?;
            transaction
                .execute(
                    "UPDATE knowledge_index_generations SET chunk_count=0 WHERE id=?1",
                    [index_generation_id],
                )
                .map_err(|_| ContractError::KbNotReady)?;
            transaction.commit().map_err(|_| ContractError::KbNotReady)
        })
    }

    pub(crate) fn write_chunk_batch(
        &self,
        index_generation_id: &str,
        drafts: &[ChunkDraft],
    ) -> Result<(), ContractError> {
        if drafts.is_empty() {
            return Err(ContractError::KbNotReady);
        }
        self.with_writer(|connection| {
            let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|_| ContractError::KbNotReady)?;
            let frozen = load_frozen_build(&transaction, index_generation_id)?;
            for draft in drafts {
                let conversation_id: String = transaction.query_row(
                    "SELECT c.id FROM knowledge_index_generations i JOIN knowledge_index_generation_imports mapping ON mapping.index_generation_id=i.id JOIN knowledge_conversations c ON c.id=mapping.conversation_id WHERE i.id=?1 AND i.status='building' AND c.account_stable_id=?2 AND c.conversation_stable_id=?3",
                    params![index_generation_id, draft.account_stable_id, draft.conversation_stable_id], |row| row.get(0),
                ).map_err(|_| ContractError::KbNotReady)?;
                validate_chunk_draft(&transaction, &frozen, &conversation_id, draft)?;
                let chunk_id = opaque_id("chunk");
                transaction.execute(
                    "INSERT INTO knowledge_chunks(id,index_generation_id,chunk_key,conversation_id,first_message_version_id,last_message_version_id,content,content_hash,chunk_index,chunk_schema_version,started_at_ms,ended_at_ms,token_count,message_count) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
                    params![chunk_id,index_generation_id,draft.chunk_key,conversation_id,draft.first_message_version_id,draft.last_message_version_id,draft.content,draft.content_hash,draft.chunk_index as i64,CHUNK_SCHEMA_VERSION,draft.started_at_ms,draft.ended_at_ms,draft.token_count as i64,draft.members.len() as i64],
                ).map_err(|_| ContractError::KbNotReady)?;
                if should_fail_chunk_write(1) { return Err(ContractError::KbNotReady); }
                for member in &draft.members {
                    transaction.execute(
                        "INSERT INTO knowledge_chunk_messages(chunk_id,message_id,message_version_id,message_index) VALUES(?1,?2,?3,?4)",
                        params![chunk_id,member.message_id,member.message_version_id,member.message_index as i64],
                    ).map_err(|_| ContractError::KbNotReady)?;
                }
                if should_fail_chunk_write(2) { return Err(ContractError::KbNotReady); }
                transaction.execute(
                    "INSERT INTO knowledge_chunks_fts(content,chunk_id,index_generation_id) VALUES(?1,?2,?3)",
                    params![draft.fts_terms,chunk_id,index_generation_id],
                ).map_err(|_| ContractError::KbNotReady)?;
                if should_fail_chunk_write(3) { return Err(ContractError::KbNotReady); }
            }
            let (chunks, fts): (i64, i64) = transaction.query_row(
                "SELECT (SELECT COUNT(*) FROM knowledge_chunks WHERE index_generation_id=?1),(SELECT COUNT(*) FROM knowledge_chunks_fts WHERE index_generation_id=?1)",
                [index_generation_id], |row| Ok((row.get(0)?,row.get(1)?)),
            ).map_err(|_| ContractError::KbNotReady)?;
            if chunks != fts { return Err(ContractError::KbNotReady); }
            let changed = transaction.execute("UPDATE knowledge_index_generations SET chunk_count=?1 WHERE id=?2 AND status='building'", params![chunks,index_generation_id]).map_err(|_| ContractError::KbNotReady)?;
            if changed != 1 { return Err(ContractError::KbNotReady); }
            transaction.commit().map_err(|_| ContractError::KbNotReady)
        })
    }

    pub(crate) fn search_active_fts(
        &self,
        request: ActiveFtsRequest,
    ) -> Result<Vec<FtsHit>, ContractError> {
        if request.scope.is_empty()
            || request.scope.len() > 32
            || !(1..=12).contains(&request.top_k)
        {
            return Err(ContractError::KbScopeUnresolved);
        }
        if request
            .from_ms
            .zip(request.to_ms)
            .is_some_and(|(from, to)| from >= to)
        {
            return Err(ContractError::KbRetrievalFailed);
        }
        let unique = request.scope.iter().cloned().collect::<BTreeSet<_>>();
        if unique.len() != request.scope.len() {
            return Err(ContractError::KbScopeUnresolved);
        }
        self.with_reader(|connection| {
            let active_pretoken_version: Option<String> = connection.query_row(
                "SELECT generation.fts_pretoken_version FROM knowledge_catalog_state catalog JOIN knowledge_index_generations generation ON generation.id=catalog.active_index_generation_id AND generation.status='ready' WHERE catalog.singleton_id=1",
                [], |row| row.get(0),
            ).optional().map_err(|_| ContractError::KbRetrievalFailed)?;
            let active_pretoken_version = active_pretoken_version.ok_or(ContractError::KbNotReady)?;
            let values = std::iter::repeat("(?,?)").take(request.scope.len()).collect::<Vec<_>>().join(",");
            let scope_sql = format!(
                "WITH requested(account_stable_id,conversation_stable_id) AS (VALUES {values}) SELECT COUNT(*) FROM requested JOIN knowledge_conversations conversation ON conversation.account_stable_id=requested.account_stable_id AND conversation.conversation_stable_id=requested.conversation_stable_id JOIN knowledge_catalog_state catalog ON catalog.singleton_id=1 JOIN knowledge_index_generations generation ON generation.id=catalog.active_index_generation_id AND generation.status='ready' JOIN knowledge_index_generation_imports mapping ON mapping.index_generation_id=generation.id AND mapping.conversation_id=conversation.id"
            );
            let mut scope_parameters = Vec::with_capacity(request.scope.len() * 2);
            for scope in &request.scope {
                scope_parameters.push(Value::Text(scope.account_stable_id.clone()));
                scope_parameters.push(Value::Text(scope.conversation_stable_id.clone()));
            }
            let resolved: i64 = connection.query_row(
                &scope_sql,
                params_from_iter(scope_parameters),
                |row| row.get(0),
            ).map_err(|_| ContractError::KbRetrievalFailed)?;
            if resolved != request.scope.len() as i64 {
                return Err(ContractError::KbScopeUnresolved);
            }
            let denial: Option<i64> = connection.query_row("SELECT 1 FROM knowledge_denials LIMIT 1", [], |row| row.get(0)).optional().map_err(|_| ContractError::KbRetrievalFailed)?;
            if denial.is_some() { return Err(ContractError::KbNotReady); }
            let match_query = fts_match_query(&active_pretoken_version, &request.query)?;
            let Some(match_query) = match_query else { return Ok(Vec::new()); };
            let sql = format!(
                "WITH requested(account_stable_id,conversation_stable_id) AS (VALUES {values}) SELECT chunk.chunk_key,chunk.content,chunk.token_count,chunk.started_at_ms,chunk.ended_at_ms,knowledge_chunks_fts.rank FROM knowledge_catalog_state catalog JOIN knowledge_index_generations generation ON generation.id=catalog.active_index_generation_id AND generation.status='ready' JOIN knowledge_chunks_fts ON knowledge_chunks_fts.index_generation_id=generation.id JOIN knowledge_chunks chunk ON chunk.id=knowledge_chunks_fts.chunk_id AND chunk.index_generation_id=knowledge_chunks_fts.index_generation_id JOIN knowledge_conversations conversation ON conversation.id=chunk.conversation_id JOIN requested scope ON scope.account_stable_id=conversation.account_stable_id AND scope.conversation_stable_id=conversation.conversation_stable_id WHERE knowledge_chunks_fts MATCH ? AND (? IS NULL OR chunk.ended_at_ms>=?) AND (? IS NULL OR chunk.started_at_ms<?) AND NOT EXISTS(SELECT 1 FROM knowledge_denials denial WHERE denial.conversation_id=chunk.conversation_id OR denial.message_id IN (SELECT message_id FROM knowledge_chunk_messages WHERE chunk_id=chunk.id)) ORDER BY knowledge_chunks_fts.rank,chunk.chunk_key LIMIT ?"
            );
            let mut parameters = Vec::new();
            for scope in &request.scope {
                parameters.push(Value::Text(scope.account_stable_id.clone()));
                parameters.push(Value::Text(scope.conversation_stable_id.clone()));
            }
            parameters.push(Value::Text(match_query));
            parameters.push(request.from_ms.map(Value::Integer).unwrap_or(Value::Null));
            parameters.push(request.from_ms.map(Value::Integer).unwrap_or(Value::Null));
            parameters.push(request.to_ms.map(Value::Integer).unwrap_or(Value::Null));
            parameters.push(request.to_ms.map(Value::Integer).unwrap_or(Value::Null));
            parameters.push(Value::Integer(request.top_k as i64));
            let mut statement = connection.prepare(&sql).map_err(|_| ContractError::KbRetrievalFailed)?;
            let hits = statement.query_map(params_from_iter(parameters), |row| Ok(FtsHit {
                chunk_key: row.get(0)?, content: row.get(1)?, token_count: row.get::<_,i64>(2)? as u32,
                started_at_ms: row.get(3)?, ended_at_ms: row.get(4)?, rank: row.get(5)?,
            })).map_err(|_| ContractError::KbRetrievalFailed)?
                .collect::<Result<Vec<_>, _>>().map_err(|_| ContractError::KbRetrievalFailed)?;
            Ok(hits)
        })
    }

    #[cfg(test)]
    pub(crate) fn register_ready_index(
        &self,
        candidate: &CandidateImport,
        snapshot_hash: String,
    ) -> Result<ReadyIndex, ContractError> {
        self.with_writer(|connection| {
            let id = opaque_id("index");
            connection.execute(
                "INSERT INTO knowledge_index_generations(id,schema_version,embedding_metadata_json,snapshot_hash,status,created_at_ms) VALUES(?1,'v1','{}',?2,'ready',?3)",
                params![id, snapshot_hash, now_ms()],
            ).map_err(|_| ContractError::KbNotReady)?;
            connection.execute(
                "INSERT INTO knowledge_index_generation_imports(index_generation_id,conversation_id,import_generation_id) VALUES(?1,?2,?3)",
                params![id, candidate.0.conversation_id, candidate.0.id],
            ).map_err(|_| ContractError::KbNotReady)?;
            Ok(ReadyIndex { id, snapshot_hash })
        })
    }

    #[cfg(test)]
    pub(crate) fn register_ready_index_set(
        &self,
        candidates: &[CandidateImport],
    ) -> Result<ReadyIndex, ContractError> {
        if candidates.is_empty() {
            return Err(ContractError::KbNotReady);
        }
        self.with_writer(|connection| {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| ContractError::KbNotReady)?;
            let mut hashes = Vec::with_capacity(candidates.len());
            for candidate in candidates {
                let hash: String = transaction.query_row(
                    "SELECT source_set_hash FROM knowledge_import_generations WHERE id=?1 AND conversation_id=?2 AND status='ready_candidate'",
                    params![candidate.0.id, candidate.0.conversation_id], |row| row.get(0),
                ).map_err(|_| ContractError::KbNotReady)?;
                hashes.push(hash);
            }
            hashes.sort();
            let snapshot_hash = hex_hash(&hashes.join("|"));
            let id = opaque_id("index");
            transaction.execute(
                "INSERT INTO knowledge_index_generations(id,schema_version,embedding_metadata_json,snapshot_hash,status,created_at_ms) VALUES(?1,'v1','{}',?2,'ready',?3)",
                params![id, snapshot_hash, now_ms()],
            ).map_err(|_| ContractError::KbNotReady)?;
            for candidate in candidates {
                transaction.execute(
                    "INSERT INTO knowledge_index_generation_imports(index_generation_id,conversation_id,import_generation_id) VALUES(?1,?2,?3)",
                    params![id, candidate.0.conversation_id, candidate.0.id],
                ).map_err(|_| ContractError::KbNotReady)?;
            }
            transaction.commit().map_err(|_| ContractError::KbNotReady)?;
            Ok(ReadyIndex { id, snapshot_hash })
        })
    }

    #[cfg(test)]
    pub(crate) fn activate_candidate(
        &self,
        candidate: CandidateImport,
        ready_index: ReadyIndex,
    ) -> Result<u64, ContractError> {
        self.with_writer(|connection| {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| ContractError::KbNotReady)?;
            let ready: Option<i64> = transaction.query_row(
                "SELECT 1 FROM knowledge_index_generations i JOIN knowledge_index_generation_imports m ON m.index_generation_id=i.id WHERE i.id=?1 AND i.status='ready' AND m.conversation_id=?2 AND m.import_generation_id=?3",
                params![ready_index.id, candidate.0.conversation_id, candidate.0.id], |row| row.get(0),
            ).optional().map_err(|_| ContractError::KbNotReady)?;
            if ready.is_none() { return Err(ContractError::KbNotReady); }
            let changed = transaction.execute("UPDATE knowledge_import_generations SET status='active' WHERE id=?1 AND conversation_id=?2 AND status='ready_candidate'", params![candidate.0.id, candidate.0.conversation_id]).map_err(|_| ContractError::KbNotReady)?;
            if changed != 1 { return Err(ContractError::KbNotReady); }
            transaction.execute("UPDATE knowledge_import_generations SET status='superseded' WHERE conversation_id=?1 AND id<>?2 AND status='active'", params![candidate.0.conversation_id, candidate.0.id]).map_err(|_| ContractError::KbNotReady)?;
            transaction.execute("UPDATE knowledge_conversations SET active_import_generation_id=?1 WHERE id=?2", params![candidate.0.id, candidate.0.conversation_id]).map_err(|_| ContractError::KbNotReady)?;
            let next: u64 = transaction.query_row("SELECT catalog_generation_seq+1 FROM knowledge_catalog_state WHERE singleton_id=1", [], |row| row.get(0)).map_err(|_| ContractError::KbNotReady)?;
            transaction.execute("UPDATE knowledge_catalog_state SET catalog_generation_seq=?1,active_snapshot_hash=?2,active_index_generation_id=?3,activated_at_ms=?4 WHERE singleton_id=1", params![next as i64, ready_index.snapshot_hash, ready_index.id, now_ms()]).map_err(|_| ContractError::KbNotReady)?;
            transaction.commit().map_err(|_| ContractError::KbNotReady)?;
            Ok(next)
        })
    }

    #[cfg(test)]
    pub(crate) fn activate_candidates(
        &self,
        candidates: Vec<CandidateImport>,
        ready_index: ReadyIndex,
    ) -> Result<u64, ContractError> {
        if candidates.is_empty() {
            return Err(ContractError::KbNotReady);
        }
        self.with_writer(|connection| {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| ContractError::KbNotReady)?;
            for candidate in &candidates {
                let ready: Option<i64> = transaction.query_row(
                    "SELECT 1 FROM knowledge_index_generations i JOIN knowledge_index_generation_imports m ON m.index_generation_id=i.id WHERE i.id=?1 AND i.status='ready' AND m.conversation_id=?2 AND m.import_generation_id=?3",
                    params![ready_index.id, candidate.0.conversation_id, candidate.0.id], |row| row.get(0),
                ).optional().map_err(|_| ContractError::KbNotReady)?;
                if ready.is_none() { return Err(ContractError::KbNotReady); }
                let parent_matches: Option<i64> = transaction.query_row(
                    "SELECT 1 FROM knowledge_import_generations g JOIN knowledge_conversations c ON c.id=g.conversation_id WHERE g.id=?1 AND g.status='ready_candidate' AND (g.parent_generation_id IS c.active_import_generation_id OR (g.parent_generation_id IS NULL AND c.active_import_generation_id IS NULL))",
                    [candidate.0.id.as_str()], |row| row.get(0),
                ).optional().map_err(|_| ContractError::KbNotReady)?;
                if parent_matches.is_none() { return Err(ContractError::KbNotReady); }
            }
            for candidate in &candidates {
                let changed = transaction.execute(
                    "UPDATE knowledge_import_generations SET status='active' WHERE id=?1 AND conversation_id=?2 AND status='ready_candidate'",
                    params![candidate.0.id, candidate.0.conversation_id],
                ).map_err(|_| ContractError::KbNotReady)?;
                if changed != 1 { return Err(ContractError::KbNotReady); }
                transaction.execute(
                    "UPDATE knowledge_import_generations SET status='superseded' WHERE conversation_id=?1 AND id<>?2 AND status='active'",
                    params![candidate.0.conversation_id, candidate.0.id],
                ).map_err(|_| ContractError::KbNotReady)?;
                transaction.execute(
                    "UPDATE knowledge_conversations SET active_import_generation_id=?1 WHERE id=?2",
                    params![candidate.0.id, candidate.0.conversation_id],
                ).map_err(|_| ContractError::KbNotReady)?;
            }
            let next: u64 = transaction.query_row("SELECT catalog_generation_seq+1 FROM knowledge_catalog_state WHERE singleton_id=1", [], |row| row.get(0)).map_err(|_| ContractError::KbNotReady)?;
            transaction.execute("UPDATE knowledge_catalog_state SET catalog_generation_seq=?1,active_snapshot_hash=?2,active_index_generation_id=?3,activated_at_ms=?4 WHERE singleton_id=1", params![next as i64, ready_index.snapshot_hash, ready_index.id, now_ms()]).map_err(|_| ContractError::KbNotReady)?;
            transaction.commit().map_err(|_| ContractError::KbNotReady)?;
            Ok(next)
        })
    }

    pub(crate) fn read_active_snapshot(
        &self,
        _request: ActiveReadRequest,
    ) -> Result<ActiveSnapshot, ContractError> {
        if self.maintenance_status()?.maintenance == "closed" {
            return Err(ContractError::KbNotReady);
        }
        self.with_reader(|connection| {
            let denial: Option<i64> = connection.query_row("SELECT 1 FROM knowledge_denials LIMIT 1", [], |row| row.get(0)).optional().map_err(|_| ContractError::KbNotReady)?;
            if denial.is_some() { return Err(ContractError::KbNotReady); }
            connection.query_row(
                "SELECT c.catalog_generation_seq,c.active_index_generation_id FROM knowledge_catalog_state c JOIN knowledge_index_generations i ON i.id=c.active_index_generation_id WHERE c.singleton_id=1 AND i.status='ready'",
                [], |row| Ok(ActiveSnapshot { catalog_generation: row.get(0)?, active_index_id: row.get(1)? }),
            ).map_err(|_| ContractError::KbNotReady)
        })
    }

    pub(crate) fn deny_or_delete(&self, request: DeletionRequest) -> Result<(), ContractError> {
        if request.source_id.is_none()
            && request.conversation_id.is_none()
            && request.message_id.is_none()
        {
            return Err(ContractError::KbNotReady);
        }
        self.with_writer(|connection| {
            let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|_| ContractError::KbNotReady)?;
            let building: Option<i64> = transaction.query_row("SELECT 1 FROM knowledge_index_generations WHERE status='building'", [], |row| row.get(0)).optional().map_err(|_| ContractError::KbNotReady)?;
            if building.is_some() { return Err(ContractError::KbNotReady); }
            transaction.execute("INSERT INTO knowledge_denials(id,source_id,conversation_id,message_id,reason,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6)", params![opaque_id("denial"), request.source_id, request.conversation_id, request.message_id, request.reason, now_ms()]).map_err(|_| ContractError::KbNotReady)?;
            transaction.execute("UPDATE knowledge_catalog_state SET active_index_generation_id=NULL,active_snapshot_hash=NULL,activated_at_ms=NULL WHERE singleton_id=1", []).map_err(|_| ContractError::KbNotReady)?;
            transaction.commit().map_err(|_| ContractError::KbNotReady)
        })
    }

    pub(crate) fn fast_verify_archive(
        &self,
        fingerprint: &ImportFingerprint,
        audit: &SourceAuditDigest,
    ) -> Result<bool, ContractError> {
        self.with_reader(|connection| {
            let source_id: Option<String> = connection.query_row("SELECT id FROM knowledge_sources WHERE account_stable_id=?1 AND export_id=?2 AND schema_version=?3 AND manifest_hash=?4 AND coverage_hash=?5 AND import_status IN ('full_declared','filtered_selected')", params![fingerprint.account_stable_id, fingerprint.export_id, fingerprint.schema_version, fingerprint.manifest_content_hash, fingerprint.coverage_signature], |row| row.get(0)).optional().map_err(|_| ContractError::KbNotReady)?;
            let Some(source_id) = source_id else { return Ok(false); };
            let matching: Option<i64> = connection.query_row(
                "SELECT 1 FROM knowledge_sources WHERE id=?1 AND member_audit_count=?2 AND member_audit_digest=?3",
                params![source_id, audit.count as i64, audit.digest],
                |row| row.get(0),
            ).optional().map_err(|_| ContractError::KbNotReady)?;
            Ok(matching.is_some())
        })
    }

    pub(crate) fn record_source_audit(
        &self,
        source_id: &str,
        member: &MemberAudit,
    ) -> Result<(), ContractError> {
        self.with_writer(|connection| {
            connection.execute(
                "INSERT OR IGNORE INTO knowledge_source_members(source_id,member_path_token,member_kind,size_bytes,mtime_ms,declared_hash,checked) VALUES(?1,?2,?3,?4,?5,?6,1)",
                params![source_id, member.member_path_token, member.member_kind, member.size_bytes as i64, member.mtime_ms, member.declared_hash],
            ).map_err(|_| ContractError::KbNotReady)?;
            Ok(())
        })
    }

    pub(crate) fn finalize_source_candidates(
        &self,
        source_id: &str,
        verdict: CompletenessVerdict,
        audit: &SourceAuditDigest,
    ) -> Result<(), ContractError> {
        self.with_writer(|connection| {
            let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|_| ContractError::KbNotReady)?;
            let member_count: i64 = transaction.query_row("SELECT COUNT(*) FROM knowledge_source_members WHERE source_id=?1 AND checked=1", [source_id], |row| row.get(0)).map_err(|_| ContractError::KbNotReady)?;
            if member_count != audit.count as i64 { return Err(ContractError::KbNotReady); }
            transaction.execute(
                "UPDATE knowledge_sources SET import_status=?1,checked_at_ms=?2,member_audit_count=?3,member_audit_digest=?4 WHERE id=?5",
                params![verdict.as_str(), now_ms(), audit.count as i64, audit.digest, source_id],
            ).map_err(|_| ContractError::KbNotReady)?;
            transaction.execute(
                "UPDATE knowledge_import_generations SET status='ready_candidate' WHERE trigger_source_id=?1 AND status='staging' AND (SELECT COUNT(DISTINCT v.message_id) FROM knowledge_message_sources p JOIN knowledge_message_versions v ON v.id=p.message_version_id JOIN knowledge_messages m ON m.id=v.message_id WHERE p.source_id=knowledge_import_generations.trigger_source_id AND m.conversation_id=knowledge_import_generations.conversation_id)=(SELECT COUNT(*) FROM knowledge_import_generation_input_keys WHERE import_generation_id=knowledge_import_generations.id)",
                [source_id],
            ).map_err(|_| ContractError::KbNotReady)?;
            transaction.commit().map_err(|_| ContractError::KbNotReady)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_archive_import(
        &self,
        fingerprint: &ImportFingerprint,
        coverage: CoverageKind,
        verdict: CompletenessVerdict,
        scope_filters_json: &str,
        integrity_json: &str,
        members: &[MemberAudit],
    ) -> Result<String, ContractError> {
        self.with_writer(|connection| {
            let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|_| ContractError::KbNotReady)?;
            let source_id = opaque_id("source");
            transaction.execute("INSERT INTO knowledge_sources(id,account_stable_id,export_id,schema_version,manifest_hash,coverage_hash,snapshot_kind,scope_filters_json,integrity_json,source_state,import_status,checked_at_ms,exported_at_ms,coverage_kind) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'active',?10,?11,0,?7)", params![source_id, fingerprint.account_stable_id, fingerprint.export_id, fingerprint.schema_version, fingerprint.manifest_content_hash, fingerprint.coverage_signature, coverage.as_str(), scope_filters_json, integrity_json, verdict.as_str(), now_ms()]).map_err(|_| ContractError::KbNotReady)?;
            for member in members {
                transaction.execute("INSERT INTO knowledge_source_members(source_id,member_path_token,member_kind,size_bytes,mtime_ms,declared_hash,checked) VALUES(?1,?2,?3,?4,?5,?6,1)", params![source_id, member.member_path_token, member.member_kind, member.size_bytes as i64, member.mtime_ms, member.declared_hash]).map_err(|_| ContractError::KbNotReady)?;
            }
            transaction.commit().map_err(|_| ContractError::KbNotReady)?;
            Ok(source_id)
        })
    }

    fn with_writer<T>(
        &self,
        operation: impl FnOnce(&mut Connection) -> Result<T, ContractError>,
    ) -> Result<T, ContractError> {
        if self.inner.availability != StoreAvailability::Ready {
            return Err(ContractError::KbNotReady);
        }
        let mut writer = self
            .inner
            .writer
            .lock()
            .map_err(|_| ContractError::KbNotReady)?;
        operation(writer.as_mut().ok_or(ContractError::KbNotReady)?)
    }

    fn with_reader<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, ContractError>,
    ) -> Result<T, ContractError> {
        if self.inner.availability != StoreAvailability::Ready {
            return Err(ContractError::KbNotReady);
        }
        {
            let mut readers = self
                .inner
                .active_readers
                .lock()
                .map_err(|_| ContractError::KbNotReady)?;
            if *readers >= MAX_READERS {
                return Err(ContractError::KbNotReady);
            }
            *readers += 1;
        }
        let result = self
            .inner
            .path
            .as_ref()
            .ok_or(ContractError::KbNotReady)
            .and_then(|path| migrations::open_reader(path))
            .and_then(|connection| operation(&connection));
        if let Ok(mut readers) = self.inner.active_readers.lock() {
            *readers = readers.saturating_sub(1);
        }
        result
    }
}

#[derive(Clone, Debug)]
struct IndexImportSelection {
    account_stable_id: String,
    conversation_stable_id: String,
    conversation_id: String,
    import_generation_id: String,
    source_set_hash: String,
    membership_digest: String,
    message_count: u64,
}

fn validate_build_spec(spec: &FrozenIndexBuildSpec) -> Result<(), ContractError> {
    if spec.chunk_schema_version != CHUNK_SCHEMA_VERSION
        || spec.token_counter_version != TOKEN_COUNTER_VERSION
        || spec.fts_pretoken_version != FTS_PRETOKEN_VERSION
        || !(256..=4096).contains(&spec.retrieval_token_budget)
        || spec.embedding.fingerprint.trim().is_empty()
        || !(1..=65536).contains(&spec.embedding.dimension)
    {
        return Err(ContractError::KbNotReady);
    }
    validate_local_embedding(&LocalEmbeddingConfig {
        provider: spec.embedding.provider.clone(),
        endpoint: spec.embedding.endpoint.clone(),
        model: spec.embedding.model.clone(),
    })
    .map_err(|_| ContractError::KbNotReady)
}

fn load_frozen_build(
    connection: &Connection,
    index_generation_id: &str,
) -> Result<FrozenIndexBuild, ContractError> {
    connection
        .query_row(
            "SELECT schema_version,token_counter_version,fts_pretoken_version,retrieval_token_budget,embedding_metadata_json,snapshot_hash,message_count FROM knowledge_index_generations WHERE id=?1 AND status='building'",
            [index_generation_id],
            |row| Ok((
                row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
            )),
        )
        .map_err(|_| ContractError::KbNotReady)
        .and_then(|(chunk_schema_version, token_counter_version, fts_pretoken_version, budget, embedding_json, snapshot_hash, message_count)| {
            let retrieval_token_budget = u32::try_from(budget).map_err(|_| ContractError::KbNotReady)?;
            let message_count = u64::try_from(message_count).map_err(|_| ContractError::KbNotReady)?;
            let embedding = serde_json::from_str(&embedding_json).map_err(|_| ContractError::KbNotReady)?;
            let spec = FrozenIndexBuildSpec { chunk_schema_version, token_counter_version, fts_pretoken_version, retrieval_token_budget, embedding };
            validate_build_spec(&spec)?;
            Ok(FrozenIndexBuild { index_generation_id: index_generation_id.into(), import_snapshot_hash: snapshot_hash, message_count, spec })
        })
}

fn validate_chunk_draft(
    connection: &Connection,
    frozen: &FrozenIndexBuild,
    conversation_id: &str,
    draft: &ChunkDraft,
) -> Result<(), ContractError> {
    if draft.members.is_empty()
        || draft.members.len() > 40
        || draft
            .members
            .iter()
            .enumerate()
            .any(|(index, member)| member.message_index != index as u32)
    {
        return Err(ContractError::KbNotReady);
    }
    let next_chunk_index: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(chunk_index)+1,0) FROM knowledge_chunks WHERE index_generation_id=?1 AND conversation_id=?2",
            params![frozen.index_generation_id, conversation_id],
            |row| row.get(0),
        )
        .map_err(|_| ContractError::KbNotReady)?;
    if i64::from(draft.chunk_index) != next_chunk_index {
        return Err(ContractError::KbNotReady);
    }
    let mut messages = Vec::with_capacity(draft.members.len());
    for member in &draft.members {
        let message = connection.query_row(
            "SELECT conversation.account_stable_id,conversation.conversation_stable_id,message.id,version.id,COALESCE(message.message_stable_id,message.fallback_key),normalization.created_at_ms,normalization.source_ordinal,normalization.sort_key,normalization.sender_key,normalization.direction,normalization.message_kind,version.normalized_content,version.content_hash FROM knowledge_index_generation_imports mapping JOIN knowledge_import_generation_members source_member ON source_member.import_generation_id=mapping.import_generation_id JOIN knowledge_messages message ON message.id=source_member.message_id JOIN knowledge_message_versions version ON version.id=source_member.message_version_id AND version.message_id=message.id JOIN knowledge_message_normalizations normalization ON normalization.message_version_id=version.id JOIN knowledge_conversations conversation ON conversation.id=mapping.conversation_id WHERE mapping.index_generation_id=?1 AND mapping.conversation_id=?2 AND source_member.message_id=?3 AND source_member.message_version_id=?4",
            params![frozen.index_generation_id, conversation_id, member.message_id, member.message_version_id],
            |row| Ok((
                row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
                row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?, row.get::<_, String>(7)?, row.get::<_, String>(8)?,
                row.get::<_, String>(9)?, row.get::<_, String>(10)?, row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
            )),
        ).map_err(|_| ContractError::KbNotReady)?;
        messages.push(BuildMessage {
            account_stable_id: message.0,
            conversation_stable_id: message.1,
            message_id: message.2,
            message_version_id: message.3,
            stable_message_key: message.4,
            created_at_ms: message.5,
            source_ordinal: u64::try_from(message.6).map_err(|_| ContractError::KbNotReady)?,
            sort_key: message.7,
            sender_key: message.8,
            direction: Direction::parse(&message.9)?,
            message_kind: message.10,
            content: message.11,
            content_hash: message.12,
        });
    }
    if messages.windows(2).any(|pair| {
        (
            pair[0].created_at_ms,
            pair[0].source_ordinal,
            pair[0].sort_key.as_str(),
            pair[0].stable_message_key.as_str(),
        ) >= (
            pair[1].created_at_ms,
            pair[1].source_ordinal,
            pair[1].sort_key.as_str(),
            pair[1].stable_message_key.as_str(),
        )
    }) {
        return Err(ContractError::KbNotReady);
    }
    let expected = draft_from_messages(
        &frozen.import_snapshot_hash,
        draft.chunk_index,
        &frozen.spec.fts_pretoken_version,
        &messages,
    )?;
    let cap = 512_usize.min(frozen.spec.retrieval_token_budget as usize);
    if &expected != draft || expected.token_count as usize > cap {
        return Err(ContractError::KbNotReady);
    }
    Ok(())
}

fn mapping_is_publishable(
    connection: &Connection,
    index_generation_id: &str,
) -> Result<bool, ContractError> {
    let invalid: Option<i64> = connection.query_row(
        "SELECT 1 FROM knowledge_index_generation_imports mapping JOIN knowledge_import_generations generation ON generation.id=mapping.import_generation_id JOIN knowledge_conversations conversation ON conversation.id=mapping.conversation_id WHERE mapping.index_generation_id=?1 AND (generation.conversation_id<>mapping.conversation_id OR generation.status NOT IN ('ready_candidate','active') OR (generation.status='ready_candidate' AND NOT (generation.parent_generation_id IS conversation.active_import_generation_id OR (generation.parent_generation_id IS NULL AND conversation.active_import_generation_id IS NULL))) OR (conversation.active_import_generation_id IS NULL AND (SELECT COUNT(*) FROM knowledge_import_generations active WHERE active.conversation_id=conversation.id AND active.status='active')<>0) OR (conversation.active_import_generation_id IS NOT NULL AND (SELECT COUNT(*) FROM knowledge_import_generations active WHERE active.conversation_id=conversation.id AND active.status='active')<>1) OR NOT EXISTS(SELECT 1 FROM knowledge_import_generation_sources source_mapping WHERE source_mapping.import_generation_id=generation.id) OR EXISTS(SELECT 1 FROM knowledge_import_generation_sources source_mapping JOIN knowledge_sources source ON source.id=source_mapping.source_id WHERE source_mapping.import_generation_id=generation.id AND (source.source_state<>'active' OR EXISTS(SELECT 1 FROM knowledge_denials denial WHERE denial.source_id=source.id))) OR EXISTS(SELECT 1 FROM knowledge_denials denial WHERE denial.conversation_id=mapping.conversation_id OR denial.message_id IN (SELECT member.message_id FROM knowledge_import_generation_members member WHERE member.import_generation_id=generation.id))) LIMIT 1",
        [index_generation_id], |row| row.get(0),
    ).optional().map_err(|_| ContractError::KbNotReady)?;
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_index_generation_imports WHERE index_generation_id=?1",
            [index_generation_id],
            |row| row.get(0),
        )
        .map_err(|_| ContractError::KbNotReady)?;
    Ok(invalid.is_none() && count > 0)
}

fn select_index_imports(
    connection: &Connection,
) -> Result<Vec<IndexImportSelection>, ContractError> {
    let staging: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM knowledge_import_generations WHERE status='staging' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| ContractError::KbNotReady)?;
    if staging.is_some() {
        return Err(ContractError::KbNotReady);
    }
    let mut statement = connection.prepare(
        "SELECT id,account_stable_id,conversation_stable_id,active_import_generation_id FROM knowledge_conversations ORDER BY account_stable_id,conversation_stable_id",
    ).map_err(|_| ContractError::KbNotReady)?;
    let conversations = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|_| ContractError::KbNotReady)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ContractError::KbNotReady)?;
    let mut selections = Vec::with_capacity(conversations.len());
    for (conversation_id, account_stable_id, conversation_stable_id, active_pointer) in
        conversations
    {
        let active_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_import_generations WHERE conversation_id=?1 AND status='active'",
                [&conversation_id],
                |row| row.get(0),
            )
            .map_err(|_| ContractError::KbNotReady)?;
        if (active_pointer.is_none() && active_count != 0)
            || (active_pointer.is_some() && active_count != 1)
        {
            return Err(ContractError::KbNotReady);
        }
        let candidates = {
            let mut candidate_statement = connection.prepare(
                "SELECT id,parent_generation_id FROM knowledge_import_generations WHERE conversation_id=?1 AND status='ready_candidate' ORDER BY id",
            ).map_err(|_| ContractError::KbNotReady)?;
            let rows = candidate_statement
                .query_map([&conversation_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })
                .map_err(|_| ContractError::KbNotReady)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| ContractError::KbNotReady)?;
            rows
        };
        let import_generation_id = match candidates.as_slice() {
            [(candidate_id, parent)] if parent == &active_pointer => candidate_id.clone(),
            [] => {
                let pointer = active_pointer.as_deref().ok_or(ContractError::KbNotReady)?;
                connection.query_row(
                    "SELECT id FROM knowledge_import_generations WHERE id=?1 AND conversation_id=?2 AND status='active'",
                    params![pointer,conversation_id], |row| row.get(0),
                ).map_err(|_| ContractError::KbNotReady)?
            }
            _ => return Err(ContractError::KbNotReady),
        };
        let invalid_source: Option<i64> = connection.query_row(
            "SELECT 1 FROM knowledge_import_generation_sources mapping JOIN knowledge_sources source ON source.id=mapping.source_id WHERE mapping.import_generation_id=?1 AND (source.source_state<>'active' OR EXISTS(SELECT 1 FROM knowledge_denials denial WHERE denial.source_id=source.id)) LIMIT 1",
            [&import_generation_id], |row| row.get(0),
        ).optional().map_err(|_| ContractError::KbNotReady)?;
        let source_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM knowledge_import_generation_sources WHERE import_generation_id=?1",
                [&import_generation_id],
                |row| row.get(0),
            )
            .map_err(|_| ContractError::KbNotReady)?;
        let denied_conversation: Option<i64> = connection
            .query_row(
                "SELECT 1 FROM knowledge_denials denial WHERE denial.conversation_id=?1 OR denial.message_id IN (SELECT member.message_id FROM knowledge_import_generation_members member WHERE member.import_generation_id=?2) LIMIT 1",
                params![conversation_id, import_generation_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ContractError::KbNotReady)?;
        if source_count == 0 || invalid_source.is_some() || denied_conversation.is_some() {
            return Err(ContractError::KbNotReady);
        }
        let source_set_hash: String = connection
            .query_row(
                "SELECT source_set_hash FROM knowledge_import_generations WHERE id=?1",
                [&import_generation_id],
                |row| row.get(0),
            )
            .map_err(|_| ContractError::KbNotReady)?;
        let members = {
            let mut member_statement = connection.prepare(
                "SELECT COALESCE(message.message_stable_id,message.fallback_key),version.content_hash FROM knowledge_import_generation_members member JOIN knowledge_messages message ON message.id=member.message_id JOIN knowledge_message_versions version ON version.id=member.message_version_id AND version.message_id=message.id WHERE member.import_generation_id=?1 ORDER BY COALESCE(message.message_stable_id,message.fallback_key),version.content_hash",
            ).map_err(|_| ContractError::KbNotReady)?;
            let rows = member_statement
                .query_map([&import_generation_id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })
                .map_err(|_| ContractError::KbNotReady)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| ContractError::KbNotReady)?;
            rows
        };
        if members.is_empty() {
            return Err(ContractError::KbNotReady);
        }
        let mut digest = Sha256::new();
        digest.update(b"membership-v1");
        for (stable_key, content_hash) in &members {
            digest.update((stable_key.len() as u64).to_be_bytes());
            digest.update(stable_key.as_bytes());
            digest.update((content_hash.len() as u64).to_be_bytes());
            digest.update(content_hash.as_bytes());
        }
        selections.push(IndexImportSelection {
            account_stable_id,
            conversation_stable_id,
            conversation_id,
            import_generation_id,
            source_set_hash,
            membership_digest: hex::encode(digest.finalize()),
            message_count: members.len() as u64,
        });
    }
    Ok(selections)
}

fn snapshot_hash(selections: &[IndexImportSelection]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"import-snapshot-v1");
    for selection in selections {
        for part in [
            selection.account_stable_id.as_str(),
            selection.conversation_stable_id.as_str(),
            selection.source_set_hash.as_str(),
            selection.membership_digest.as_str(),
        ] {
            hash.update((part.len() as u64).to_be_bytes());
            hash.update(part.as_bytes());
        }
    }
    hex::encode(hash.finalize())
}

fn opaque_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as i64)
        .unwrap_or_default()
}

fn hex_hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn source_fact(
    transaction: &rusqlite::Transaction<'_>,
    source_id: &str,
) -> Result<SourceFact, ContractError> {
    transaction
        .query_row(
            "SELECT account_stable_id,exported_at_ms,coverage_kind,manifest_hash,coverage_hash,export_id FROM knowledge_sources WHERE id=?1",
            [source_id],
            |row| Ok(SourceFact {
                account_stable_id: row.get(0)?,
                exported_at_ms: row.get(1)?,
                coverage_rank: coverage_rank(&row.get::<_, String>(2)?),
                manifest_hash: row.get(3)?,
                coverage_hash: row.get(4)?,
                export_id: row.get(5)?,
            }),
        )
        .map_err(|_| ContractError::KbNotReady)
}

fn source_facts_for_generation(
    transaction: &rusqlite::Transaction<'_>,
    generation_id: &str,
) -> Result<Vec<SourceFact>, ContractError> {
    let mut statement = transaction
        .prepare("SELECT s.account_stable_id,s.exported_at_ms,s.coverage_kind,s.manifest_hash,s.coverage_hash,s.export_id FROM knowledge_import_generation_sources g JOIN knowledge_sources s ON s.id=g.source_id WHERE g.import_generation_id=?1")
        .map_err(|_| ContractError::KbNotReady)?;
    let facts = statement
        .query_map([generation_id], |row| {
            Ok(SourceFact {
                account_stable_id: row.get(0)?,
                exported_at_ms: row.get(1)?,
                coverage_rank: coverage_rank(&row.get::<_, String>(2)?),
                manifest_hash: row.get(3)?,
                coverage_hash: row.get(4)?,
                export_id: row.get(5)?,
            })
        })
        .map_err(|_| ContractError::KbNotReady)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ContractError::KbNotReady)?;
    Ok(facts)
}

fn incoming_source_wins(
    transaction: &rusqlite::Transaction<'_>,
    staging: &StagingImport,
    message_id: &str,
) -> Result<bool, ContractError> {
    let incoming = source_fact(transaction, &staging.source_id)?;
    let mut statement = transaction
        .prepare("SELECT s.account_stable_id,s.exported_at_ms,s.coverage_kind,s.manifest_hash,s.coverage_hash,s.export_id FROM knowledge_import_generation_members m JOIN knowledge_message_sources p ON p.message_version_id=m.message_version_id JOIN knowledge_sources s ON s.id=p.source_id WHERE m.import_generation_id=?1 AND m.message_id=?2")
        .map_err(|_| ContractError::KbNotReady)?;
    let incumbent = statement
        .query_map(params![staging.id, message_id], |row| {
            Ok(SourceFact {
                account_stable_id: row.get(0)?,
                exported_at_ms: row.get(1)?,
                coverage_rank: coverage_rank(&row.get::<_, String>(2)?),
                manifest_hash: row.get(3)?,
                coverage_hash: row.get(4)?,
                export_id: row.get(5)?,
            })
        })
        .map_err(|_| ContractError::KbNotReady)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ContractError::KbNotReady)?
        .into_iter()
        .max();
    Ok(incumbent.is_none_or(|fact| incoming >= fact))
}

fn coverage_rank(coverage: &str) -> u8 {
    match coverage {
        "full" => 2,
        "filtered" => 1,
        "selected" => 0,
        _ => 0,
    }
}

fn coverage_kind_from_rank(rank: u8) -> &'static str {
    match rank {
        2 => "full",
        1 => "filtered",
        _ => "selected",
    }
}

fn source_set_hash(sources: &[SourceFact]) -> String {
    hex_hash(
        &sources
            .iter()
            .map(|source| {
                format!(
                    "{}|{}|{}|{}|{}|{}",
                    source.account_stable_id,
                    source.exported_at_ms,
                    source.coverage_rank,
                    source.manifest_hash,
                    source.coverage_hash,
                    source.export_id,
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn write_redacted_manifest(
    path: &Path,
    database: &Path,
    sources: &[KnowledgeSourceStatus],
) -> Result<(), ContractError> {
    fs::write(path, redacted_manifest_bytes(database, sources)?)
        .map_err(|_| ContractError::KbNotReady)
}

fn redacted_manifest_bytes(
    database: &Path,
    sources: &[KnowledgeSourceStatus],
) -> Result<Vec<u8>, ContractError> {
    validate_database(database)?;
    let connection = Connection::open(database).map_err(|_| ContractError::KbNotReady)?;
    let catalog_generation: i64 = connection
        .query_row(
            "SELECT catalog_generation_seq FROM knowledge_catalog_state WHERE singleton_id=1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| ContractError::KbNotReady)?;
    let sources = sources
        .iter()
        .map(|source| {
            serde_json::json!({
                "sourceId": source.source_id, "coverageKind": source.coverage_kind,
                "sourceState": source.source_state, "messageCount": source.message_count,
                "conversationCount": source.conversation_count,
            })
        })
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&serde_json::json!({
        "format": 1, "schemaHead": migrations::SCHEMA_HEAD,
        "catalogGeneration": catalog_generation,
        "sourceSetDigest": hex_hash(&serde_json::to_string(&sources).map_err(|_| ContractError::KbNotReady)?),
        "sources": sources,
    }))
    .map_err(|_| ContractError::KbNotReady)?;
    Ok(bytes)
}

fn validate_database(database: &Path) -> Result<(), ContractError> {
    let connection = Connection::open(database).map_err(|_| ContractError::KbNotReady)?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|_| ContractError::KbNotReady)?;
    if !integrity.eq_ignore_ascii_case("ok") {
        return Err(ContractError::KbNotReady);
    }
    let foreign_key_error: Option<i64> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()
        .map_err(|_| ContractError::KbNotReady)?;
    if foreign_key_error.is_some() {
        return Err(ContractError::KbNotReady);
    }
    let schema_head: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| ContractError::KbNotReady)?;
    (schema_head == migrations::SCHEMA_HEAD)
        .then_some(())
        .ok_or(ContractError::KbNotReady)
}

fn sources_for_database(database: &Path) -> Result<Vec<KnowledgeSourceStatus>, ContractError> {
    let store = KnowledgeStore::open_database(database.to_path_buf())?;
    let sources = store.list_sources()?;
    drop(store);
    Ok(sources)
}

fn validate_database_pair(database: &Path, manifest: &Path) -> Result<(), ContractError> {
    let sources = sources_for_database(database)?;
    let expected = redacted_manifest_bytes(database, &sources)?;
    let actual = fs::read(manifest).map_err(|_| ContractError::KbNotReady)?;
    (actual == expected)
        .then_some(())
        .ok_or(ContractError::KbNotReady)
}

fn publish_candidate_pair(
    primary: &Path,
    primary_manifest: &Path,
    candidate: &Path,
    candidate_manifest: &Path,
    root: &Path,
) -> Result<(), ContractError> {
    let generation = Uuid::new_v4().simple().to_string();
    let backup = root.join(format!("knowledge.sqlite.backup-{generation}"));
    let backup_manifest = root.join(format!(
        "knowledge.sqlite.manifest.json.backup-{generation}"
    ));
    fs::rename(primary, &backup).map_err(|_| ContractError::KbNotReady)?;
    if fs::rename(primary_manifest, &backup_manifest).is_err() {
        let _ = fs::rename(&backup, primary);
        return Err(ContractError::KbNotReady);
    }
    if fs::rename(candidate, primary).is_err() {
        let _ = fs::rename(&backup_manifest, primary_manifest);
        let _ = fs::rename(&backup, primary);
        return Err(ContractError::KbNotReady);
    }
    if fs::rename(candidate_manifest, primary_manifest).is_err()
        || validate_database_pair(primary, primary_manifest).is_err()
    {
        let _ = fs::rename(primary, candidate);
        let _ = fs::rename(primary_manifest, candidate_manifest);
        let _ = fs::rename(&backup, primary);
        let _ = fs::rename(&backup_manifest, primary_manifest);
        return Err(ContractError::KbNotReady);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("knowledge_store_{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn new_store_is_versioned_and_candidate_is_invisible_until_activation() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        assert_eq!(store.availability(), StoreAvailability::Ready);
        assert_eq!(
            store.read_active_snapshot(ActiveReadRequest),
            Err(ContractError::KbNotReady)
        );
        let staging = store
            .begin_staging_source(NewSource {
                account_stable_id: "acct".into(),
                conversation_stable_id: "conv".into(),
                export_id: "export".into(),
                schema_version: "v1".into(),
                manifest_hash: "manifest".into(),
                coverage_hash: "coverage".into(),
                exported_at_ms: 1,
                coverage_kind: CoverageKind::Full,
            })
            .unwrap();
        store
            .append_staging_messages(&staging, &[message("one", "body")])
            .unwrap();
        let candidate = store
            .mark_ready_candidate(
                staging,
                CandidateChecks {
                    expected_message_count: 1,
                },
            )
            .unwrap();
        assert_eq!(
            store.read_active_snapshot(ActiveReadRequest),
            Err(ContractError::KbNotReady)
        );
        let index = store
            .register_ready_index(&candidate, "snapshot".into())
            .unwrap();
        assert_eq!(store.activate_candidate(candidate, index).unwrap(), 1);
        assert_eq!(
            store
                .read_active_snapshot(ActiveReadRequest)
                .unwrap()
                .catalog_generation,
            1
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn future_or_invalid_schema_is_unavailable_without_replacement() {
        let data_dir = temp_dir();
        let database = data_dir.join("wechat_knowledge/knowledge.sqlite");
        fs::create_dir_all(database.parent().unwrap()).unwrap();
        let connection = Connection::open(&database).unwrap();
        connection.execute_batch("PRAGMA user_version=99;").unwrap();
        drop(connection);
        assert_eq!(
            KnowledgeStore::open_or_unavailable(&data_dir).availability(),
            StoreAvailability::Unavailable
        );
        let version: i32 = Connection::open(&database)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 99);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn reopening_keeps_the_single_catalog_row_and_schema_head() {
        let data_dir = temp_dir();
        {
            let store = KnowledgeStore::open(&data_dir).unwrap();
            assert_eq!(store.availability(), StoreAvailability::Ready);
        }
        let store = KnowledgeStore::open(&data_dir).unwrap();
        assert_eq!(store.availability(), StoreAvailability::Ready);
        let database =
            Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
        let version: i32 = database
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let catalog_rows: u64 = database
            .query_row("SELECT COUNT(*) FROM knowledge_catalog_state", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(version, migrations::SCHEMA_HEAD);
        assert_eq!(catalog_rows, 1);
        let _ = fs::remove_dir_all(data_dir);
    }

    fn source(export_id: &str) -> NewSource {
        NewSource {
            account_stable_id: "acct".into(),
            conversation_stable_id: "conv".into(),
            export_id: export_id.into(),
            schema_version: "v1".into(),
            manifest_hash: format!("manifest-{export_id}"),
            coverage_hash: format!("coverage-{export_id}"),
            exported_at_ms: 1,
            coverage_kind: CoverageKind::Full,
        }
    }

    fn source_at(export_id: &str, exported_at_ms: i64, coverage_kind: CoverageKind) -> NewSource {
        NewSource {
            export_id: export_id.into(),
            manifest_hash: format!("manifest-{export_id}"),
            coverage_hash: format!("coverage-{export_id}"),
            exported_at_ms,
            coverage_kind,
            ..source(export_id)
        }
    }

    fn ready_candidate(store: &KnowledgeStore, source: NewSource) -> CandidateImport {
        let staging = store.begin_staging_source(source).unwrap();
        store
            .append_staging_messages(&staging, &[message("one", "body")])
            .unwrap();
        store
            .mark_ready_candidate(
                staging,
                CandidateChecks {
                    expected_message_count: 1,
                },
            )
            .unwrap()
    }

    fn message(stable_id: &str, content: &str) -> IncomingMessage {
        IncomingMessage {
            stable_id: Some(stable_id.into()),
            fallback_key: None,
            content: content.into(),
            normalized_content: content.into(),
            content_hash: hex_hash(content),
            source_member_token: "member".into(),
            created_at_ms: 0,
            source_ordinal: 0,
            sort_key: "00000000000000000000|00000000000000000000|fixture".into(),
            message_kind: "text".into(),
            render_kind: "text".into(),
            sender_key: "fixture".into(),
            text_hash: hex_hash(content),
            reference_json: None,
            extra_json: None,
            media_refs: Vec::new(),
        }
    }

    fn build_spec() -> FrozenIndexBuildSpec {
        FrozenIndexBuildSpec {
            chunk_schema_version: CHUNK_SCHEMA_VERSION.into(),
            token_counter_version: TOKEN_COUNTER_VERSION.into(),
            fts_pretoken_version: FTS_PRETOKEN_VERSION.into(),
            retrieval_token_budget: 512,
            embedding: FrozenEmbeddingIdentity {
                provider: "ollama_loopback".into(),
                endpoint: "http://127.0.0.1:11434".into(),
                model: "fixture-model".into(),
                fingerprint: "fixture-fingerprint".into(),
                dimension: 8,
            },
        }
    }

    #[test]
    fn frozen_build_chunks_and_active_fts_are_generation_and_scope_isolated() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let staging = store.begin_staging_source(source("fts-build")).unwrap();
        store
            .append_staging_messages(
                &staging,
                &[
                    message("one", "周五下午同步知识库迁移进度"),
                    message("two", "ProjectA 发布窗口调整到 v2"),
                ],
            )
            .unwrap();
        store
            .mark_ready_candidate(
                staging,
                CandidateChecks {
                    expected_message_count: 2,
                },
            )
            .unwrap();

        let frozen = store.begin_or_resume_index_build(build_spec()).unwrap();
        assert_eq!(frozen.message_count, 2);
        assert_eq!(
            store.begin_or_resume_index_build(build_spec()).unwrap(),
            frozen
        );
        let mut changed = build_spec();
        changed.embedding.fingerprint = "different".into();
        assert_eq!(
            store.begin_or_resume_index_build(changed),
            Err(ContractError::KbNotReady)
        );
        let mut changed_pretoken = build_spec();
        changed_pretoken.fts_pretoken_version = "fts-pretoken-v2".into();
        assert_eq!(
            store.begin_or_resume_index_build(changed_pretoken),
            Err(ContractError::KbNotReady)
        );

        let conversations = store
            .list_build_conversations(&frozen.index_generation_id)
            .unwrap();
        assert_eq!(conversations.len(), 1);
        let page = store
            .read_build_message_page(&frozen.index_generation_id, &conversations[0], None, 256)
            .unwrap();
        assert_eq!(page.messages.len(), 2);
        assert_eq!(page.messages[0].direction, Direction::Other);
        let drafts = super::super::chunk::chunk_messages(
            &frozen.import_snapshot_hash,
            frozen.spec.retrieval_token_budget,
            &page.messages,
        )
        .unwrap();
        store
            .write_chunk_batch(&frozen.index_generation_id, &drafts)
            .unwrap();
        store
            .reset_build_chunks(&frozen.index_generation_id)
            .unwrap();
        store
            .write_chunk_batch(&frozen.index_generation_id, &drafts)
            .unwrap();
        assert!(matches!(
            store.begin_staging_source(source("blocked-during-build")),
            Err(ContractError::KbNotReady)
        ));

        let request = ActiveFtsRequest {
            scope: conversations.clone(),
            query: "迁移".into(),
            from_ms: None,
            to_ms: None,
            top_k: 12,
        };
        assert_eq!(
            store.search_active_fts(request.clone()),
            Err(ContractError::KbNotReady)
        );
        store
            .with_writer(|connection| {
                connection
                    .execute(
                        "UPDATE knowledge_index_generations SET status='ready' WHERE id=?1",
                        [&frozen.index_generation_id],
                    )
                    .unwrap();
                connection
                    .execute(
                        "UPDATE knowledge_catalog_state SET active_index_generation_id=?1,active_snapshot_hash=?2 WHERE singleton_id=1",
                        params![frozen.index_generation_id, frozen.import_snapshot_hash],
                    )
                    .unwrap();
                Ok(())
            })
            .unwrap();
        let hits = store.search_active_fts(request).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].content.contains("知识库迁移"));
        for scope in [
            vec![StableConversationKey {
                account_stable_id: "unknown".into(),
                conversation_stable_id: "conv".into(),
            }],
            vec![
                conversations[0].clone(),
                StableConversationKey {
                    account_stable_id: "acct".into(),
                    conversation_stable_id: "unknown".into(),
                },
            ],
            vec![conversations[0].clone(), conversations[0].clone()],
        ] {
            assert_eq!(
                store.search_active_fts(ActiveFtsRequest {
                    scope,
                    query: "迁移".into(),
                    from_ms: None,
                    to_ms: None,
                    top_k: 12,
                }),
                Err(ContractError::KbScopeUnresolved)
            );
        }
        assert_eq!(
            store.search_active_fts(ActiveFtsRequest {
                scope: (0..33)
                    .map(|index| StableConversationKey {
                        account_stable_id: "acct".into(),
                        conversation_stable_id: format!("scope-{index}"),
                    })
                    .collect(),
                query: "迁移".into(),
                from_ms: None,
                to_ms: None,
                top_k: 12,
            }),
            Err(ContractError::KbScopeUnresolved)
        );
        let mut scope_32 = vec![conversations[0].clone()];
        store
            .with_writer(|connection| {
                let source_id: String = connection
                    .query_row("SELECT id FROM knowledge_sources LIMIT 1", [], |row| row.get(0))
                    .unwrap();
                for index in 1..32 {
                    let conversation_id = format!("scope-conversation-{index}");
                    let import_id = format!("scope-import-{index}");
                    let stable_id = format!("scope-{index}");
                    connection.execute(
                        "INSERT INTO knowledge_conversations(id,account_stable_id,conversation_stable_id) VALUES(?1,'acct',?2)",
                        params![conversation_id, stable_id],
                    ).unwrap();
                    connection.execute(
                        "INSERT INTO knowledge_import_generations(id,trigger_source_id,conversation_id,source_set_hash,merge_mode,status,message_count,created_at_ms) VALUES(?1,?2,?3,?4,'replace','active',0,1)",
                        params![import_id,source_id,conversation_id,format!("scope-hash-{index}")],
                    ).unwrap();
                    connection.execute(
                        "INSERT INTO knowledge_index_generation_imports(index_generation_id,conversation_id,import_generation_id) VALUES(?1,?2,?3)",
                        params![frozen.index_generation_id,conversation_id,import_id],
                    ).unwrap();
                    scope_32.push(StableConversationKey {
                        account_stable_id: "acct".into(),
                        conversation_stable_id: stable_id,
                    });
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(scope_32.len(), 32);
        assert!(store
            .search_active_fts(ActiveFtsRequest {
                scope: scope_32,
                query: String::new(),
                from_ms: None,
                to_ms: None,
                top_k: 12,
            })
            .unwrap()
            .is_empty());
        assert!(store
            .search_active_fts(ActiveFtsRequest {
                scope: conversations.clone(),
                query: "迁移 OR 不存在".into(),
                from_ms: None,
                to_ms: None,
                top_k: 12,
            })
            .unwrap()
            .is_empty());
        store
            .with_writer(|connection| {
                connection
                    .execute(
                        "UPDATE knowledge_index_generations SET fts_pretoken_version='unknown' WHERE id=?1",
                        [&frozen.index_generation_id],
                    )
                    .unwrap();
                Ok(())
            })
            .unwrap();
        assert_eq!(
            store.search_active_fts(ActiveFtsRequest {
                scope: conversations,
                query: "迁移".into(),
                from_ms: None,
                to_ms: None,
                top_k: 12,
            }),
            Err(ContractError::KbNotReady)
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn store_pagination_streams_more_than_256_messages_without_index_collisions() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let staging = store.begin_staging_source(source("many-pages")).unwrap();
        let messages = (0..260)
            .map(|index| {
                let mut incoming =
                    message(&format!("message-{index:03}"), &format!("body-{index:03}"));
                incoming.created_at_ms = if index < 180 {
                    index as i64
                } else {
                    30 * 60 * 1000 + index as i64 + 1
                };
                incoming.source_ordinal = index as u64;
                incoming.sort_key = format!("sort-{index:03}");
                incoming.source_member_token = format!("member-{index:03}");
                incoming
            })
            .collect::<Vec<_>>();
        store.append_staging_messages(&staging, &messages).unwrap();
        store
            .mark_ready_candidate(
                staging,
                CandidateChecks {
                    expected_message_count: messages.len() as u64,
                },
            )
            .unwrap();
        let frozen = store.begin_or_resume_index_build(build_spec()).unwrap();
        let conversation = store
            .list_build_conversations(&frozen.index_generation_id)
            .unwrap()
            .remove(0);
        let mut cursor = None;
        let mut chunker = super::super::chunk::ChunkerState::new(
            &frozen.import_snapshot_hash,
            frozen.spec.retrieval_token_budget,
            &frozen.spec.fts_pretoken_version,
        )
        .unwrap();
        let mut page_count = 0;
        loop {
            let page = store
                .read_build_message_page(
                    &frozen.index_generation_id,
                    &conversation,
                    cursor.as_ref(),
                    128,
                )
                .unwrap();
            page_count += 1;
            let drafts = chunker.push_page(&page.messages).unwrap();
            if !drafts.is_empty() {
                store
                    .write_chunk_batch(&frozen.index_generation_id, &drafts)
                    .unwrap();
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
        }
        let final_drafts = chunker.finish().unwrap();
        store
            .write_chunk_batch(&frozen.index_generation_id, &final_drafts)
            .unwrap();
        assert_eq!(page_count, 3);
        store
            .with_reader(|connection| {
                let (count, distinct_indexes, min_index, max_index): (i64, i64, i64, i64) =
                    connection
                        .query_row(
                            "SELECT COUNT(*),COUNT(DISTINCT chunk_index),MIN(chunk_index),MAX(chunk_index) FROM knowledge_chunks WHERE index_generation_id=?1",
                            [&frozen.index_generation_id],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                        )
                        .unwrap();
                assert_eq!(count, distinct_indexes);
                assert_eq!(min_index, 0);
                assert_eq!(max_index, count - 1);
                Ok(())
            })
            .unwrap();
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn chunk_batch_rejects_tampering_and_rolls_back_each_write_step() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let staging = store
            .begin_staging_source(source("chunk-validation"))
            .unwrap();
        store
            .append_staging_messages(
                &staging,
                &[message("one", "first"), message("two", "second")],
            )
            .unwrap();
        store
            .mark_ready_candidate(
                staging,
                CandidateChecks {
                    expected_message_count: 2,
                },
            )
            .unwrap();
        let frozen = store.begin_or_resume_index_build(build_spec()).unwrap();
        let conversation = store
            .list_build_conversations(&frozen.index_generation_id)
            .unwrap()
            .remove(0);
        let page = store
            .read_build_message_page(&frozen.index_generation_id, &conversation, None, 256)
            .unwrap();
        let draft = super::super::chunk::chunk_messages(
            &frozen.import_snapshot_hash,
            frozen.spec.retrieval_token_budget,
            &page.messages,
        )
        .unwrap()
        .remove(0);
        assert_eq!(
            store.write_chunk_batch(&frozen.index_generation_id, &[]),
            Err(ContractError::KbNotReady)
        );
        assert_eq!(
            store.write_chunk_batch("missing-generation", std::slice::from_ref(&draft)),
            Err(ContractError::KbNotReady)
        );
        for step in 1..=3 {
            fail_chunk_write_at(step);
            assert_eq!(
                store.write_chunk_batch(&frozen.index_generation_id, std::slice::from_ref(&draft)),
                Err(ContractError::KbNotReady)
            );
            clear_chunk_write_failure();
            store
                .with_reader(|connection| {
                    let counts: (i64, i64, i64) = connection
                        .query_row(
                            "SELECT (SELECT COUNT(*) FROM knowledge_chunks WHERE index_generation_id=?1),(SELECT COUNT(*) FROM knowledge_chunk_messages),(SELECT COUNT(*) FROM knowledge_chunks_fts WHERE index_generation_id=?1)",
                            [&frozen.index_generation_id],
                            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                        )
                        .unwrap();
                    assert_eq!(counts, (0, 0, 0));
                    Ok(())
                })
                .unwrap();
        }
        let mut tampered = Vec::new();
        let mut changed = draft.clone();
        changed.content_hash = "tampered".into();
        tampered.push(changed);
        let mut changed = draft.clone();
        changed.fts_terms = "tampered".into();
        tampered.push(changed);
        let mut changed = draft.clone();
        changed.first_message_version_id = "tampered".into();
        tampered.push(changed);
        let mut changed = draft.clone();
        changed.last_message_version_id = "tampered".into();
        tampered.push(changed);
        let mut changed = draft.clone();
        changed.chunk_key = "tampered".into();
        tampered.push(changed);
        let mut changed = draft.clone();
        changed.content.push_str("tampered");
        tampered.push(changed);
        let mut changed = draft.clone();
        changed.started_at_ms += 1;
        tampered.push(changed);
        let mut changed = draft.clone();
        changed.ended_at_ms += 1;
        tampered.push(changed);
        let mut changed = draft.clone();
        changed.chunk_index = 1;
        tampered.push(changed);
        let mut changed = draft.clone();
        changed.members[0].message_index = 1;
        tampered.push(changed);
        for changed in tampered {
            assert_eq!(
                store.write_chunk_batch(&frozen.index_generation_id, &[changed]),
                Err(ContractError::KbNotReady)
            );
        }
        store
            .write_chunk_batch(&frozen.index_generation_id, &[draft])
            .unwrap();
        store
            .with_writer(|connection| {
                connection
                    .execute("UPDATE knowledge_chunks_fts SET content='tampered'", [])
                    .unwrap();
                Ok(())
            })
            .unwrap();
        drop(store);
        assert_eq!(
            KnowledgeStore::open_or_unavailable(&data_dir).availability(),
            StoreAvailability::Unavailable
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn multiple_ready_candidates_fail_closed_before_snapshot_creation() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        ready_candidate(&store, source("candidate-a"));
        ready_candidate(&store, source("candidate-b"));
        assert_eq!(
            store.begin_or_resume_index_build(build_spec()),
            Err(ContractError::KbNotReady)
        );
        let database =
            Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
        let count: i64 = database
            .query_row(
                "SELECT COUNT(*) FROM knowledge_index_generations WHERE status='building'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn same_conversation_reimport_supersedes_only_after_candidate_activates() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let first = ready_candidate(&store, source("export-a"));
        let first_id = first.0.id.clone();
        let first_conversation = first.0.conversation_id.clone();
        let first_index = store
            .register_ready_index(&first, "snapshot-a".into())
            .unwrap();
        assert_eq!(store.activate_candidate(first, first_index).unwrap(), 1);

        let second = ready_candidate(&store, source("export-b"));
        assert_eq!(second.0.conversation_id, first_conversation);
        assert_eq!(
            store.activate_candidate(
                second.clone(),
                ReadyIndex {
                    id: "missing-index".into(),
                    snapshot_hash: "missing".into(),
                },
            ),
            Err(ContractError::KbNotReady)
        );
        assert_eq!(
            store
                .read_active_snapshot(ActiveReadRequest)
                .unwrap()
                .catalog_generation,
            1
        );

        let second_id = second.0.id.clone();
        let second_index = store
            .register_ready_index(&second, "snapshot-b".into())
            .unwrap();
        assert_eq!(store.activate_candidate(second, second_index).unwrap(), 2);
        let database =
            Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
        let statuses: Vec<(String, String)> = database
            .prepare("SELECT id,status FROM knowledge_import_generations ORDER BY created_at_ms,id")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            statuses,
            vec![
                (first_id, "superseded".into()),
                (second_id, "active".into())
            ]
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn immutable_versions_reuse_same_content_and_preserve_changed_content() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let first = store.begin_staging_source(source("versions-a")).unwrap();
        store
            .append_staging_messages(&first, &[message("message-1", "first body")])
            .unwrap();
        let first = store
            .mark_ready_candidate(
                first,
                CandidateChecks {
                    expected_message_count: 1,
                },
            )
            .unwrap();
        let first_index = store.register_ready_index(&first, "first".into()).unwrap();
        store.activate_candidate(first, first_index).unwrap();

        let second = store.begin_staging_source(source("versions-b")).unwrap();
        store
            .append_staging_messages(&second, &[message("message-1", "second body")])
            .unwrap();
        let second = store
            .mark_ready_candidate(
                second,
                CandidateChecks {
                    expected_message_count: 1,
                },
            )
            .unwrap();
        let second_index = store
            .register_ready_index(&second, "second".into())
            .unwrap();
        store.activate_candidate(second, second_index).unwrap();

        let database =
            Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
        let version_count: i64 = database
            .query_row(
                "SELECT COUNT(*) FROM knowledge_message_versions",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let source_count: i64 = database
            .query_row(
                "SELECT COUNT(*) FROM knowledge_message_sources",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version_count, 2);
        assert_eq!(source_count, 2);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn media_references_are_ordinal_metadata_rows_with_unknown_existence() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let staging = store.begin_staging_source(source("media")).unwrap();
        let mut incoming = message("message-1", "body");
        incoming.media_refs = vec![
            IncomingMediaRef {
                ordinal: 0,
                kind: "image".into(),
                relative_path: Some("media/a.jpg".into()),
                metadata_json: Some(r#"{"label":"a"}"#.into()),
            },
            IncomingMediaRef {
                ordinal: 1,
                kind: "voice".into(),
                relative_path: None,
                metadata_json: None,
            },
        ];
        store
            .append_staging_messages(&staging, &[incoming])
            .unwrap();
        let database =
            Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
        let rows: Vec<(i64, String, String)> = database
            .prepare(
                "SELECT ordinal,media_kind,exists_state FROM knowledge_media_refs ORDER BY ordinal",
            )
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                (0, "image".into(), "unknown".into()),
                (1, "voice".into(), "unknown".into())
            ]
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn selected_candidate_keeps_parent_members_and_finalizes_from_its_own_input_count() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let full = store
            .begin_staging_source(source_at("full", 1, CoverageKind::Full))
            .unwrap();
        store
            .append_staging_messages(&full, &[message("one", "old"), message("two", "kept")])
            .unwrap();
        let full = store
            .mark_ready_candidate(
                full,
                CandidateChecks {
                    expected_message_count: 2,
                },
            )
            .unwrap();
        let index = store.register_ready_index(&full, "full".into()).unwrap();
        store.activate_candidate(full, index).unwrap();

        let selected = store
            .begin_staging_source(source_at("selected", 2, CoverageKind::Selected))
            .unwrap();
        store
            .append_staging_messages(&selected, &[message("one", "new")])
            .unwrap();
        let selected_id = selected.id.clone();
        let selected_source_id = selected.source_id().to_owned();
        store
            .finalize_source_candidates(
                &selected_source_id,
                CompletenessVerdict::FilteredSelected,
                &SourceAuditDigest {
                    count: 0,
                    digest: String::new(),
                },
            )
            .unwrap();
        let database =
            Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
        let members: i64 = database
            .query_row(
                "SELECT COUNT(*) FROM knowledge_import_generation_members WHERE import_generation_id=?1",
                [&selected_id],
                |row| row.get(0),
            )
            .unwrap();
        let retained: i64 = database
            .query_row(
                "SELECT COUNT(*) FROM knowledge_import_generation_members WHERE import_generation_id=?1 AND selection_reason='retained_outside_coverage'",
                [&selected_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((members, retained), (2, 1));
        let status: String = database
            .query_row(
                "SELECT status FROM knowledge_import_generations WHERE id=?1",
                [&selected_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "ready_candidate");
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn source_priority_and_source_set_hash_do_not_depend_on_arrival_order() {
        fn import_pair(
            first: NewSource,
            first_body: &str,
            second: NewSource,
            second_body: &str,
        ) -> (String, String) {
            let data_dir = temp_dir();
            let store = KnowledgeStore::open(&data_dir).unwrap();
            for (source, body) in [(first, first_body), (second, second_body)] {
                let staging = store.begin_staging_source(source).unwrap();
                store
                    .append_staging_messages(&staging, &[message("one", body)])
                    .unwrap();
                let candidate = store
                    .mark_ready_candidate(
                        staging,
                        CandidateChecks {
                            expected_message_count: 1,
                        },
                    )
                    .unwrap();
                let index = store
                    .register_ready_index(&candidate, "snapshot".into())
                    .unwrap();
                store.activate_candidate(candidate, index).unwrap();
            }
            let database =
                Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
            let result = database
                .query_row(
                    "SELECT g.source_set_hash,v.content FROM knowledge_import_generations g JOIN knowledge_import_generation_members m ON m.import_generation_id=g.id JOIN knowledge_message_versions v ON v.id=m.message_version_id WHERE g.status='active'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            let _ = fs::remove_dir_all(data_dir);
            result
        }

        let older = source_at("older", 1, CoverageKind::Full);
        let newer = source_at("newer", 2, CoverageKind::Full);
        let forward = import_pair(older.clone(), "old", newer.clone(), "new");
        let reverse = import_pair(newer, "new", older, "old");
        assert_eq!(forward, reverse);
        assert_eq!(forward.1, "new");
    }

    #[test]
    fn discarding_a_failed_multi_conversation_import_removes_staging_and_source_audit() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let first = store
            .begin_staging_source(source_at("first", 1, CoverageKind::Full))
            .unwrap();
        store
            .append_staging_messages(&first, &[message("one", "first")])
            .unwrap();
        let second = store
            .begin_staging_source(NewSource {
                conversation_stable_id: "other".into(),
                ..source_at("second", 2, CoverageKind::Full)
            })
            .unwrap();
        store
            .append_staging_messages(&second, &[message("one", "second")])
            .unwrap();
        store.discard_stagings(&[first, second]).unwrap();

        let database =
            Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
        for table in [
            "knowledge_import_generations",
            "knowledge_message_versions",
            "knowledge_sources",
            "knowledge_source_lineage",
        ] {
            let count: i64 = database
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table}");
        }
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn writer_busy_lock_fails_closed_within_timeout() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let lock = Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
        lock.execute_batch("BEGIN IMMEDIATE;").unwrap();
        let started = std::time::Instant::now();
        assert!(matches!(
            store.begin_staging_source(source("busy")),
            Err(ContractError::KbNotReady)
        ));
        assert!(started.elapsed() < std::time::Duration::from_secs(7));
        lock.execute_batch("ROLLBACK;").unwrap();
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn rebuild_waits_for_readers_then_fails_closed_at_the_deadline() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let reader_store = store.clone();
        let reader = std::thread::spawn(move || {
            reader_store.with_reader(|_| {
                std::thread::sleep(std::time::Duration::from_millis(80));
                Ok(())
            })
        });
        for _ in 0..20 {
            if *store.inner.active_readers.lock().unwrap() == 1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(*store.inner.active_readers.lock().unwrap(), 1);
        assert_eq!(
            store.wait_for_reader_drain(std::time::Duration::from_millis(20)),
            Err(ContractError::KbNotReady)
        );
        reader.join().unwrap().unwrap();
        assert_eq!(
            store.wait_for_reader_drain(std::time::Duration::from_millis(20)),
            Ok(())
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn invalid_existing_databases_are_not_replaced() {
        let data_dir = temp_dir();
        let database = data_dir.join("wechat_knowledge/knowledge.sqlite");
        fs::create_dir_all(database.parent().unwrap()).unwrap();
        for contents in [
            b"not sqlite".as_slice(),
            b"corrupted sqlite header".as_slice(),
        ] {
            fs::write(&database, contents).unwrap();
            assert_eq!(
                KnowledgeStore::open_or_unavailable(&data_dir).availability(),
                StoreAvailability::Unavailable
            );
            assert_eq!(fs::read(&database).unwrap(), contents);
        }
        fs::remove_file(&database).unwrap();
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch("CREATE TABLE knowledge_unrecognized(id INTEGER);")
            .unwrap();
        drop(connection);
        let before = fs::read(&database).unwrap();
        assert_eq!(
            KnowledgeStore::open_or_unavailable(&data_dir).availability(),
            StoreAvailability::Unavailable
        );
        assert_eq!(fs::read(&database).unwrap(), before);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn integrity_failed_database_is_not_replaced() {
        let data_dir = temp_dir();
        let database = data_dir.join("wechat_knowledge/knowledge.sqlite");
        KnowledgeStore::open(&data_dir).unwrap();
        let connection = Connection::open(&database).unwrap();
        connection
            .execute_batch(
                "PRAGMA writable_schema=ON; UPDATE sqlite_master SET rootpage=999 WHERE name='knowledge_sources'; PRAGMA writable_schema=OFF;",
            )
            .unwrap();
        drop(connection);
        let before = fs::read(&database).unwrap();
        assert_eq!(
            KnowledgeStore::open_or_unavailable(&data_dir).availability(),
            StoreAvailability::Unavailable
        );
        assert_eq!(fs::read(&database).unwrap(), before);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn foreign_keys_and_denial_keep_deleted_content_unreadable() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let candidate = ready_candidate(&store, source("active"));
        let conversation_id = candidate.0.conversation_id.clone();
        let index = store
            .register_ready_index(&candidate, "snapshot".into())
            .unwrap();
        store.activate_candidate(candidate, index).unwrap();
        store
            .deny_or_delete(DeletionRequest {
                source_id: None,
                conversation_id: Some(conversation_id),
                message_id: None,
                reason: "test denial".into(),
            })
            .unwrap();
        assert_eq!(
            store.read_active_snapshot(ActiveReadRequest),
            Err(ContractError::KbNotReady)
        );

        let database =
            Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
        database.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        let referenced_source: String = database
            .query_row(
                "SELECT trigger_source_id FROM knowledge_import_generations LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(database
            .execute(
                "DELETE FROM knowledge_sources WHERE id=?1",
                [&referenced_source]
            )
            .is_err());
        let source_id = store
            .record_archive_import(
                &ImportFingerprint {
                    account_stable_id: "other".into(),
                    export_id: "unreferenced".into(),
                    schema_version: "v1".into(),
                    manifest_content_hash: "manifest".into(),
                    coverage_signature: "coverage".into(),
                },
                CoverageKind::Full,
                CompletenessVerdict::FullDeclared,
                "{}",
                "{}",
                &[MemberAudit {
                    member_path_token: "member".into(),
                    member_kind: "json".into(),
                    size_bytes: 1,
                    mtime_ms: 1,
                    declared_hash: None,
                }],
            )
            .unwrap();
        database
            .execute("DELETE FROM knowledge_sources WHERE id=?1", [&source_id])
            .unwrap();
        let members: u64 = database
            .query_row(
                "SELECT COUNT(*) FROM knowledge_source_members WHERE source_id=?1",
                [&source_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(members, 0);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn maintenance_receipt_is_immediate_and_keeps_a_terminal_result() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let started = std::time::Instant::now();
        let operation_id = store
            .start_source_import(data_dir.join("missing-export"))
            .unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(
            store.start_source_import(data_dir.join("another-export")),
            Err(ContractError::KbNotReady)
        );
        let terminal = (0..100)
            .find_map(|_| {
                let status = store.maintenance_status_for(Some(&operation_id)).unwrap();
                if status.state != "running" {
                    Some(status)
                } else {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    None
                }
            })
            .expect("background import should complete");
        assert_eq!(
            terminal.operation_id.as_deref(),
            Some(operation_id.as_str())
        );
        assert_eq!(terminal.state, "failed");
        assert_eq!(terminal.maintenance, "open");
        assert_eq!(terminal.error_code, Some("KB_NOT_READY"));
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn invalid_candidate_manifest_rolls_back_the_old_database_and_manifest_pair() {
        let data_dir = temp_dir();
        let primary = data_dir.join("wechat_knowledge/knowledge.sqlite");
        let primary_manifest = data_dir.join("wechat_knowledge/knowledge.sqlite.manifest.json");
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let primary_sources = store.list_sources().unwrap();
        drop(store);
        write_redacted_manifest(&primary_manifest, &primary, &primary_sources).unwrap();

        let candidate_dir = data_dir.join("wechat_knowledge/.candidate-test");
        fs::create_dir(&candidate_dir).unwrap();
        let candidate = candidate_dir.join("knowledge.sqlite");
        let candidate_manifest = candidate_dir.join("knowledge.sqlite.manifest.json");
        let candidate_store = KnowledgeStore::open_database(candidate.clone()).unwrap();
        let candidate_sources = candidate_store.list_sources().unwrap();
        drop(candidate_store);
        write_redacted_manifest(&candidate_manifest, &candidate, &candidate_sources).unwrap();

        let old_database = fs::read(&primary).unwrap();
        let old_manifest = fs::read(&primary_manifest).unwrap();
        fs::write(&candidate_manifest, b"not a knowledge manifest").unwrap();
        assert_eq!(
            publish_candidate_pair(
                &primary,
                &primary_manifest,
                &candidate,
                &candidate_manifest,
                primary.parent().unwrap(),
            ),
            Err(ContractError::KbNotReady)
        );
        assert_eq!(fs::read(&primary).unwrap(), old_database);
        assert_eq!(fs::read(&primary_manifest).unwrap(), old_manifest);
        validate_database_pair(&primary, &primary_manifest).unwrap();
        let _ = fs::remove_dir_all(data_dir);
    }
}
