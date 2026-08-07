use crate::wechat::types::ContractError;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::path::Path;

pub(crate) const SCHEMA_HEAD: i32 = 3;
const INITIAL: &str = include_str!("migrations/knowledge/0001_initial.sql");
const SOURCE_LINEAGE_MESSAGE_GENERATIONS: &str =
    include_str!("migrations/knowledge/0002_source_lineage_message_generations.sql");
const STREAMING_MESSAGE_NORMALIZATION_MEDIA: &str =
    include_str!("migrations/knowledge/0003_streaming_message_normalization_media.sql");

pub(crate) fn open_writer(path: &Path) -> Result<Connection, ContractError> {
    let connection = Connection::open(path).map_err(|_| ContractError::KbNotReady)?;
    preflight_existing_schema(&connection)?;
    configure(&connection)?;
    migrate_and_validate(&connection)?;
    Ok(connection)
}

pub(crate) fn open_reader(path: &Path) -> Result<Connection, ContractError> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| ContractError::KbNotReady)?;
    configure_reader(&connection)?;
    connection
        .execute_batch("PRAGMA query_only=ON;")
        .map_err(|_| ContractError::KbNotReady)?;
    validate_schema(&connection, false)?;
    Ok(connection)
}

fn configure_reader(connection: &Connection) -> Result<(), ContractError> {
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| ContractError::KbNotReady)?;
    connection
        .execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|_| ContractError::KbNotReady)?;
    let foreign_keys: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(|_| ContractError::KbNotReady)?;
    let mode: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .map_err(|_| ContractError::KbNotReady)?;
    if foreign_keys != 1 || !mode.eq_ignore_ascii_case("wal") {
        return Err(ContractError::KbNotReady);
    }
    connection
        .execute_batch("CREATE VIRTUAL TABLE temp.__knowledge_fts_probe USING fts5(content); DROP TABLE temp.__knowledge_fts_probe;")
        .map_err(|_| ContractError::KbNotReady)
}

fn configure(connection: &Connection) -> Result<(), ContractError> {
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| ContractError::KbNotReady)?;
    connection
        .execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=NORMAL;")
        .map_err(|_| ContractError::KbNotReady)?;
    let foreign_keys: i64 = connection
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .map_err(|_| ContractError::KbNotReady)?;
    let mode: String = connection
        .query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))
        .map_err(|_| ContractError::KbNotReady)?;
    if foreign_keys != 1 || !mode.eq_ignore_ascii_case("wal") {
        return Err(ContractError::KbNotReady);
    }
    connection
        .execute_batch("CREATE VIRTUAL TABLE temp.__knowledge_fts_probe USING fts5(content); DROP TABLE temp.__knowledge_fts_probe;")
        .map_err(|_| ContractError::KbNotReady)
}

fn migrate_and_validate(connection: &Connection) -> Result<(), ContractError> {
    let version: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| ContractError::KbNotReady)?;
    if version > SCHEMA_HEAD || (version == 0 && has_knowledge_tables(connection)?) {
        return Err(ContractError::KbNotReady);
    }
    if version == 0 {
        connection
            .execute_batch(&format!(
                "BEGIN IMMEDIATE; {INITIAL}; PRAGMA user_version=1; COMMIT;"
            ))
            .map_err(|_| ContractError::KbNotReady)?;
    }
    let version: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| ContractError::KbNotReady)?;
    if version == 1 {
        connection
            .execute_batch(&format!(
                "BEGIN IMMEDIATE; {SOURCE_LINEAGE_MESSAGE_GENERATIONS}; PRAGMA user_version=2; COMMIT;"
            ))
            .map_err(|_| ContractError::KbNotReady)?;
    }
    let version: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| ContractError::KbNotReady)?;
    if version == 2 {
        connection
            .execute_batch(&format!(
                "BEGIN IMMEDIATE; {STREAMING_MESSAGE_NORMALIZATION_MEDIA}; PRAGMA user_version={SCHEMA_HEAD}; COMMIT;"
            ))
            .map_err(|_| ContractError::KbNotReady)?;
    }
    validate_schema(connection, true)
}

fn preflight_existing_schema(connection: &Connection) -> Result<(), ContractError> {
    let version: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| ContractError::KbNotReady)?;
    if version > SCHEMA_HEAD || (version == 0 && has_knowledge_tables(connection)?) {
        return Err(ContractError::KbNotReady);
    }
    if version == SCHEMA_HEAD {
        validate_schema(connection, true)?;
    }
    Ok(())
}

fn has_knowledge_tables(connection: &Connection) -> Result<bool, ContractError> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name LIKE 'knowledge_%' LIMIT 1",
            [],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(|_| ContractError::KbNotReady)
}

pub(crate) fn validate_schema(
    connection: &Connection,
    check_integrity: bool,
) -> Result<(), ContractError> {
    let version: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| ContractError::KbNotReady)?;
    if version != SCHEMA_HEAD {
        return Err(ContractError::KbNotReady);
    }
    for (table, column) in [
        ("knowledge_sources", "id"),
        ("knowledge_sources", "exported_at_ms"),
        ("knowledge_sources", "coverage_kind"),
        ("knowledge_message_versions", "content_hash"),
        ("knowledge_conversations", "active_import_generation_id"),
        ("knowledge_import_generations", "status"),
        ("knowledge_index_generations", "status"),
        ("knowledge_catalog_state", "active_index_generation_id"),
        ("knowledge_chunks", "index_generation_id"),
        ("knowledge_message_normalizations", "canonical_hash"),
        ("knowledge_media_refs", "exists_state"),
        ("knowledge_import_generation_input_keys", "identity_key"),
        ("knowledge_sources", "member_audit_digest"),
    ] {
        let exists: Option<i64> = connection
            .query_row(
                "SELECT 1 FROM pragma_table_info(?1) WHERE name=?2 LIMIT 1",
                [table, column],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ContractError::KbNotReady)?;
        if exists.is_none() {
            return Err(ContractError::KbNotReady);
        }
    }
    let incomplete_source: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM knowledge_sources WHERE exported_at_ms IS NULL OR coverage_kind IS NULL LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| ContractError::KbNotReady)?;
    if incomplete_source.is_some() {
        return Err(ContractError::KbNotReady);
    }
    let foreign_errors: Option<String> = connection
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()
        .map_err(|_| ContractError::KbNotReady)?;
    if foreign_errors.is_some() {
        return Err(ContractError::KbNotReady);
    }
    if check_integrity {
        let integrity: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .map_err(|_| ContractError::KbNotReady)?;
        if integrity != "ok" {
            return Err(ContractError::KbNotReady);
        }
    }
    Ok(())
}
