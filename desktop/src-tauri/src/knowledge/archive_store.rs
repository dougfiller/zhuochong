use super::archive_schema::CoverageKind;
use crate::wechat::types::ContractError;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct WechatArchiveStore {
    connection: Connection,
}

#[derive(Clone, Debug)]
pub(crate) struct ImportFingerprint {
    pub(crate) account_stable_id: String,
    pub(crate) export_id: String,
    pub(crate) schema_version: String,
    pub(crate) manifest_content_hash: String,
    pub(crate) coverage_signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemberAudit {
    pub(crate) member_path_token: String,
    pub(crate) member_kind: &'static str,
    pub(crate) size_bytes: u64,
    pub(crate) mtime_ms: i64,
    pub(crate) declared_hash: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompletenessVerdict {
    FullDeclared,
    FilteredSelected,
    Incomplete,
    Failed,
}

impl CompletenessVerdict {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::FullDeclared => "full_declared",
            Self::FilteredSelected => "filtered_selected",
            Self::Incomplete => "incomplete",
            Self::Failed => "failed",
        }
    }
}

impl WechatArchiveStore {
    pub(crate) fn open(data_dir: &Path) -> Result<Self, ContractError> {
        let root = data_dir.join("wechat_knowledge");
        fs::create_dir_all(&root).map_err(|_| ContractError::KbNotReady)?;
        let connection = Connection::open(root.join("knowledge.sqlite"))
            .map_err(|_| ContractError::KbNotReady)?;
        connection
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;
                CREATE TABLE IF NOT EXISTS archive_imports (
                  import_id TEXT PRIMARY KEY,
                  account_stable_id TEXT NOT NULL,
                  export_id TEXT NOT NULL,
                  schema_version TEXT NOT NULL,
                  manifest_content_hash TEXT NOT NULL,
                  coverage_signature TEXT NOT NULL,
                  snapshot_kind TEXT NOT NULL,
                  scope_filters_json TEXT NOT NULL,
                  declared_integrity_json TEXT NOT NULL,
                  completeness_verdict TEXT NOT NULL,
                  conversation_count INTEGER NOT NULL,
                  message_count INTEGER NOT NULL,
                  probe_at_ms INTEGER NOT NULL,
                  imported_at_ms INTEGER NOT NULL,
                  UNIQUE(account_stable_id, export_id, schema_version,
                         manifest_content_hash, coverage_signature)
                );
                CREATE TABLE IF NOT EXISTS archive_member_audits (
                  import_id TEXT NOT NULL,
                  member_path_token TEXT NOT NULL,
                  member_kind TEXT NOT NULL,
                  size_bytes INTEGER NOT NULL,
                  mtime_ms INTEGER NOT NULL,
                  declared_hash TEXT,
                  checked INTEGER NOT NULL,
                  PRIMARY KEY(import_id, member_path_token),
                  FOREIGN KEY(import_id) REFERENCES archive_imports(import_id)
                );
                CREATE TABLE IF NOT EXISTS archive_import_events (
                  import_id TEXT NOT NULL,
                  event_kind TEXT NOT NULL,
                  occurred_at_ms INTEGER NOT NULL,
                  PRIMARY KEY(import_id, event_kind, occurred_at_ms),
                  FOREIGN KEY(import_id) REFERENCES archive_imports(import_id)
                );",
            )
            .map_err(|_| ContractError::KbNotReady)?;
        Ok(Self { connection })
    }

    pub(crate) fn fast_verify(
        &self,
        fingerprint: &ImportFingerprint,
        members: &[MemberAudit],
    ) -> Result<bool, ContractError> {
        let import_id: Option<String> = self
            .connection
            .query_row(
                "SELECT import_id FROM archive_imports
                 WHERE account_stable_id = ?1 AND export_id = ?2 AND schema_version = ?3
                   AND manifest_content_hash = ?4 AND coverage_signature = ?5
                   AND completeness_verdict IN ('full_declared', 'filtered_selected')",
                params![
                    fingerprint.account_stable_id,
                    fingerprint.export_id,
                    fingerprint.schema_version,
                    fingerprint.manifest_content_hash,
                    fingerprint.coverage_signature,
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ContractError::KbNotReady)?;
        let Some(import_id) = import_id else {
            return Ok(false);
        };
        let stored_count: u64 = self
            .connection
            .query_row(
                "SELECT COUNT(*) FROM archive_member_audits WHERE import_id = ?1 AND checked = 1",
                params![import_id],
                |row| row.get(0),
            )
            .map_err(|_| ContractError::KbNotReady)?;
        if stored_count != members.len() as u64 || members.is_empty() {
            return Ok(false);
        }
        for member in members {
            let matches: Option<i64> = self
                .connection
                .query_row(
                    "SELECT 1 FROM archive_member_audits
                     WHERE import_id = ?1 AND member_path_token = ?2 AND member_kind = ?3
                       AND size_bytes = ?4 AND mtime_ms = ?5
                       AND declared_hash IS ?6 AND checked = 1",
                    params![
                        import_id,
                        member.member_path_token,
                        member.member_kind,
                        member.size_bytes as i64,
                        member.mtime_ms,
                        member.declared_hash,
                    ],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| ContractError::KbNotReady)?;
            if matches.is_none() {
                return Ok(false);
            }
        }
        self.connection
            .execute(
                "INSERT INTO archive_import_events(import_id, event_kind, occurred_at_ms) VALUES (?1, 'fast_verified', ?2)",
                params![import_id, now_ms()],
            )
            .map_err(|_| ContractError::KbNotReady)?;
        Ok(true)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_import(
        &mut self,
        import_id: &str,
        fingerprint: &ImportFingerprint,
        coverage: CoverageKind,
        verdict: CompletenessVerdict,
        scope_filters_json: &str,
        declared_integrity_json: &str,
        conversation_count: u64,
        message_count: u64,
        members: &[MemberAudit],
    ) -> Result<(), ContractError> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|_| ContractError::KbNotReady)?;
        Self::insert_import(
            &transaction,
            import_id,
            fingerprint,
            coverage,
            verdict,
            scope_filters_json,
            declared_integrity_json,
            conversation_count,
            message_count,
            members,
        )?;
        transaction.commit().map_err(|_| ContractError::KbNotReady)
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_import(
        transaction: &Transaction<'_>,
        import_id: &str,
        fingerprint: &ImportFingerprint,
        coverage: CoverageKind,
        verdict: CompletenessVerdict,
        scope_filters_json: &str,
        declared_integrity_json: &str,
        conversation_count: u64,
        message_count: u64,
        members: &[MemberAudit],
    ) -> Result<(), ContractError> {
        let now = now_ms();
        transaction
            .execute(
                "INSERT INTO archive_imports(
                    import_id, account_stable_id, export_id, schema_version,
                    manifest_content_hash, coverage_signature, snapshot_kind, scope_filters_json,
                    declared_integrity_json, completeness_verdict, conversation_count,
                    message_count, probe_at_ms, imported_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    import_id,
                    fingerprint.account_stable_id,
                    fingerprint.export_id,
                    fingerprint.schema_version,
                    fingerprint.manifest_content_hash,
                    fingerprint.coverage_signature,
                    coverage.as_str(),
                    scope_filters_json,
                    declared_integrity_json,
                    verdict.as_str(),
                    conversation_count as i64,
                    message_count as i64,
                    now,
                    now,
                ],
            )
            .map_err(|_| ContractError::KbNotReady)?;
        for member in members {
            transaction
                .execute(
                    "INSERT INTO archive_member_audits(
                       import_id, member_path_token, member_kind, size_bytes, mtime_ms, declared_hash, checked
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)",
                    params![
                        import_id, member.member_path_token, member.member_kind,
                        member.size_bytes as i64, member.mtime_ms, member.declared_hash,
                    ],
                )
                .map_err(|_| ContractError::KbNotReady)?;
        }
        Ok(())
    }
}

pub(crate) fn member_path_token(member_path: &str) -> String {
    format!("{:x}", Sha256::digest(member_path.as_bytes()))
}

pub(crate) fn coverage_signature(
    coverage: CoverageKind,
    filters_json: &str,
    stats: (u64, u64),
) -> String {
    let value = format!(
        "{}|{}|{}|{}",
        coverage.as_str(),
        filters_json,
        stats.0,
        stats.1
    );
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("wechat_archive_store_{}", now_ms()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn store_writes_only_below_product_data_dir_and_fast_verifies_exact_members() {
        let data_dir = temp_dir();
        let mut store = WechatArchiveStore::open(&data_dir).unwrap();
        let fingerprint = ImportFingerprint {
            account_stable_id: "acct_fixture_01".into(),
            export_id: "export_fixture".into(),
            schema_version: "wechat_archive_v1".into(),
            manifest_content_hash: "declared_hash".into(),
            coverage_signature: "coverage_fixture".into(),
        };
        let members = vec![MemberAudit {
            member_path_token: member_path_token("manifest.json"),
            member_kind: "manifest",
            size_bytes: 12,
            mtime_ms: 34,
            declared_hash: Some("declared_hash".into()),
        }];
        store
            .record_import(
                "import_fixture",
                &fingerprint,
                CoverageKind::Full,
                CompletenessVerdict::FullDeclared,
                "{}",
                "[]",
                1,
                2,
                &members,
            )
            .unwrap();
        assert!(data_dir.join("wechat_knowledge/knowledge.sqlite").is_file());
        assert!(store.fast_verify(&fingerprint, &members).unwrap());
        let mut changed = members.clone();
        changed[0].mtime_ms += 1;
        assert!(!store.fast_verify(&fingerprint, &changed).unwrap());
        let _ = fs::remove_dir_all(data_dir);
    }
}
