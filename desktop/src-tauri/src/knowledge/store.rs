use super::archive_importer::{ArchiveImportSummary, WechatJsonArchiveImporter};
use super::archive_schema::CoverageKind;
use super::archive_store::{CompletenessVerdict, ImportFingerprint, MemberAudit};
use super::migrations;
use crate::wechat::types::ContractError;
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const MAX_READERS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StoreAvailability {
    Ready,
    Unavailable,
}

#[derive(Clone, Debug)]
pub(crate) struct NewSource {
    pub(crate) account_stable_id: String,
    pub(crate) conversation_stable_id: String,
    pub(crate) export_id: String,
    pub(crate) schema_version: String,
    pub(crate) manifest_hash: String,
    pub(crate) coverage_hash: String,
}

#[derive(Clone, Debug)]
pub(crate) struct StagingImport {
    id: String,
    conversation_id: String,
}

#[derive(Clone, Debug)]
pub(crate) struct CandidateImport(StagingImport);

#[derive(Clone, Debug)]
pub(crate) struct MessageBatch {
    pub(crate) message_count: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct CandidateChecks {
    pub(crate) expected_message_count: u64,
}

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

#[derive(Clone, Debug)]
pub(crate) struct DeletionRequest {
    pub(crate) source_id: Option<String>,
    pub(crate) conversation_id: Option<String>,
    pub(crate) message_id: Option<String>,
    pub(crate) reason: String,
}

/// The only owner of knowledge.sqlite connections and SQL. An unavailable
/// store is intentional: callers receive KB_NOT_READY and must not fall back.
pub(crate) struct KnowledgeStore {
    availability: StoreAvailability,
    path: Option<PathBuf>,
    writer: Mutex<Option<Connection>>,
    active_readers: Mutex<usize>,
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
        let writer = migrations::open_writer(&path)?;
        Ok(Self {
            availability: StoreAvailability::Ready,
            path: Some(path),
            writer: Mutex::new(Some(writer)),
            active_readers: Mutex::new(0),
        })
    }

    fn unavailable() -> Self {
        Self {
            availability: StoreAvailability::Unavailable,
            path: None,
            writer: Mutex::new(None),
            active_readers: Mutex::new(0),
        }
    }

    pub(crate) fn availability(&self) -> StoreAvailability {
        self.availability
    }

    pub(crate) fn import_wechat_json_archive(
        &self,
        source_root: &Path,
    ) -> Result<ArchiveImportSummary, ContractError> {
        WechatJsonArchiveImporter::open(source_root, self)?.import()
    }

    pub(crate) fn begin_staging_source(
        &self,
        input: NewSource,
    ) -> Result<StagingImport, ContractError> {
        self.with_writer(|connection| {
            let transaction = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(|_| ContractError::KbNotReady)?;
            let source_id = opaque_id("source");
            let conversation_id: Option<String> = transaction
                .query_row(
                    "SELECT id FROM knowledge_conversations WHERE account_stable_id=?1 AND conversation_stable_id=?2",
                    params![input.account_stable_id, input.conversation_stable_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| ContractError::KbNotReady)?;
            let generation_id = opaque_id("generation");
            transaction.execute(
                "INSERT INTO knowledge_sources(id,account_stable_id,export_id,schema_version,manifest_hash,coverage_hash,snapshot_kind,scope_filters_json,integrity_json,source_state,import_status,checked_at_ms) VALUES(?1,?2,?3,?4,?5,?6,'staging','{}','{}','active','staging',?7)",
                params![source_id, input.account_stable_id, input.export_id, input.schema_version, input.manifest_hash, input.coverage_hash, now_ms()],
            ).map_err(|_| ContractError::KbNotReady)?;
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
            transaction.execute(
                "INSERT INTO knowledge_import_generations(id,trigger_source_id,conversation_id,source_set_hash,merge_mode,status,created_at_ms) VALUES(?1,?2,?3,?4,'replace','staging',?5)",
                params![generation_id, source_id, conversation_id, input.coverage_hash, now_ms()],
            ).map_err(|_| ContractError::KbNotReady)?;
            transaction.execute("INSERT INTO knowledge_import_generation_sources(import_generation_id,source_id,precedence,coverage_role) VALUES(?1,?2,0,'primary')", params![generation_id, source_id]).map_err(|_| ContractError::KbNotReady)?;
            transaction.commit().map_err(|_| ContractError::KbNotReady)?;
            Ok(StagingImport { id: generation_id, conversation_id })
        })
    }

    pub(crate) fn append_staging_batch(
        &self,
        staging: &StagingImport,
        batch: MessageBatch,
    ) -> Result<(), ContractError> {
        self.with_writer(|connection| {
            let changed = connection.execute(
                "UPDATE knowledge_import_generations SET message_count=message_count+?1 WHERE id=?2 AND conversation_id=?3 AND status='staging'",
                params![batch.message_count as i64, staging.id, staging.conversation_id],
            ).map_err(|_| ContractError::KbNotReady)?;
            if changed == 1 { Ok(()) } else { Err(ContractError::KbNotReady) }
        })
    }

    pub(crate) fn mark_ready_candidate(
        &self,
        staging: StagingImport,
        checks: CandidateChecks,
    ) -> Result<CandidateImport, ContractError> {
        self.with_writer(|connection| {
            let changed = connection.execute(
                "UPDATE knowledge_import_generations SET status='ready_candidate' WHERE id=?1 AND conversation_id=?2 AND status='staging' AND message_count=?3",
                params![staging.id, staging.conversation_id, checks.expected_message_count as i64],
            ).map_err(|_| ContractError::KbNotReady)?;
            if changed == 1 { Ok(CandidateImport(staging)) } else { Err(ContractError::KbNotReady) }
        })
    }

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

    pub(crate) fn read_active_snapshot(
        &self,
        _request: ActiveReadRequest,
    ) -> Result<ActiveSnapshot, ContractError> {
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
            transaction.execute("INSERT INTO knowledge_denials(id,source_id,conversation_id,message_id,reason,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6)", params![opaque_id("denial"), request.source_id, request.conversation_id, request.message_id, request.reason, now_ms()]).map_err(|_| ContractError::KbNotReady)?;
            transaction.execute("UPDATE knowledge_catalog_state SET active_index_generation_id=NULL,active_snapshot_hash=NULL,activated_at_ms=NULL WHERE singleton_id=1", []).map_err(|_| ContractError::KbNotReady)?;
            transaction.commit().map_err(|_| ContractError::KbNotReady)
        })
    }

    pub(crate) fn fast_verify_archive(
        &self,
        fingerprint: &ImportFingerprint,
        members: &[MemberAudit],
    ) -> Result<bool, ContractError> {
        self.with_reader(|connection| {
            let source_id: Option<String> = connection.query_row("SELECT id FROM knowledge_sources WHERE account_stable_id=?1 AND export_id=?2 AND schema_version=?3 AND manifest_hash=?4 AND coverage_hash=?5 AND import_status IN ('full_declared','filtered_selected')", params![fingerprint.account_stable_id, fingerprint.export_id, fingerprint.schema_version, fingerprint.manifest_content_hash, fingerprint.coverage_signature], |row| row.get(0)).optional().map_err(|_| ContractError::KbNotReady)?;
            let Some(source_id) = source_id else { return Ok(false); };
            let count: u64 = connection.query_row("SELECT COUNT(*) FROM knowledge_source_members WHERE source_id=?1 AND checked=1", [&source_id], |row| row.get(0)).map_err(|_| ContractError::KbNotReady)?;
            if members.is_empty() || count != members.len() as u64 { return Ok(false); }
            for member in members {
                let found: Option<i64> = connection.query_row("SELECT 1 FROM knowledge_source_members WHERE source_id=?1 AND member_path_token=?2 AND member_kind=?3 AND size_bytes=?4 AND mtime_ms=?5 AND declared_hash IS ?6 AND checked=1", params![source_id, member.member_path_token, member.member_kind, member.size_bytes as i64, member.mtime_ms, member.declared_hash], |row| row.get(0)).optional().map_err(|_| ContractError::KbNotReady)?;
                if found.is_none() { return Ok(false); }
            }
            Ok(true)
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
            transaction.execute("INSERT INTO knowledge_sources(id,account_stable_id,export_id,schema_version,manifest_hash,coverage_hash,snapshot_kind,scope_filters_json,integrity_json,source_state,import_status,checked_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'active',?10,?11)", params![source_id, fingerprint.account_stable_id, fingerprint.export_id, fingerprint.schema_version, fingerprint.manifest_content_hash, fingerprint.coverage_signature, coverage.as_str(), scope_filters_json, integrity_json, verdict.as_str(), now_ms()]).map_err(|_| ContractError::KbNotReady)?;
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
        if self.availability != StoreAvailability::Ready {
            return Err(ContractError::KbNotReady);
        }
        let mut writer = self.writer.lock().map_err(|_| ContractError::KbNotReady)?;
        operation(writer.as_mut().ok_or(ContractError::KbNotReady)?)
    }

    fn with_reader<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, ContractError>,
    ) -> Result<T, ContractError> {
        if self.availability != StoreAvailability::Ready {
            return Err(ContractError::KbNotReady);
        }
        {
            let mut readers = self
                .active_readers
                .lock()
                .map_err(|_| ContractError::KbNotReady)?;
            if *readers >= MAX_READERS {
                return Err(ContractError::KbNotReady);
            }
            *readers += 1;
        }
        let result = self
            .path
            .as_ref()
            .ok_or(ContractError::KbNotReady)
            .and_then(|path| migrations::open_reader(path))
            .and_then(|connection| operation(&connection));
        if let Ok(mut readers) = self.active_readers.lock() {
            *readers = readers.saturating_sub(1);
        }
        result
    }
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
            })
            .unwrap();
        store
            .append_staging_batch(&staging, MessageBatch { message_count: 1 })
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
        }
    }

    fn ready_candidate(store: &KnowledgeStore, source: NewSource) -> CandidateImport {
        let staging = store.begin_staging_source(source).unwrap();
        store
            .append_staging_batch(&staging, MessageBatch { message_count: 1 })
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
}
