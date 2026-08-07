use super::chunk::{
    chunk_content_hash_v1, draft_from_messages, fts_pretoken, BuildMessage, Direction,
    FTS_PRETOKEN_VERSION,
};
use crate::wechat::types::ContractError;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Transaction};
use std::path::Path;

pub(crate) const SCHEMA_HEAD: i32 = 4;
const INITIAL: &str = include_str!("migrations/knowledge/0001_initial.sql");
const SOURCE_LINEAGE_MESSAGE_GENERATIONS: &str =
    include_str!("migrations/knowledge/0002_source_lineage_message_generations.sql");
const STREAMING_MESSAGE_NORMALIZATION_MEDIA: &str =
    include_str!("migrations/knowledge/0003_streaming_message_normalization_media.sql");
const CANDIDATE_INDEX_CHUNKS_FTS: &str =
    include_str!("migrations/knowledge/0004_candidate_index_chunks_fts.sql");

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
                "BEGIN IMMEDIATE; {STREAMING_MESSAGE_NORMALIZATION_MEDIA}; PRAGMA user_version=3; COMMIT;"
            ))
            .map_err(|_| ContractError::KbNotReady)?;
    }
    let version: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| ContractError::KbNotReady)?;
    if version == 3 {
        migrate_v3_to_v4(connection)?;
    }
    validate_schema(connection, true)
}

fn migrate_v3_to_v4(connection: &Connection) -> Result<(), ContractError> {
    preflight_v3_chunk_members(connection)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| ContractError::KbNotReady)?;
    transaction
        .execute_batch(CANDIDATE_INDEX_CHUNKS_FTS)
        .map_err(|_| ContractError::KbNotReady)?;
    backfill_v4_chunks(&transaction)?;
    transaction
        .pragma_update(None, "user_version", SCHEMA_HEAD)
        .map_err(|_| ContractError::KbNotReady)?;
    validate_schema(&transaction, true)?;
    transaction.commit().map_err(|_| ContractError::KbNotReady)
}

fn preflight_v3_chunk_members(connection: &Connection) -> Result<(), ContractError> {
    let unrecoverable: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM knowledge_index_generations WHERE status='building' UNION ALL SELECT 1 FROM knowledge_chunks chunk WHERE NOT EXISTS(SELECT 1 FROM knowledge_chunk_messages old WHERE old.chunk_id=chunk.id) OR (SELECT COUNT(*) FROM knowledge_chunks_fts fts WHERE fts.chunk_id=chunk.id AND fts.index_generation_id=chunk.index_generation_id)<>1 UNION ALL SELECT 1 FROM knowledge_chunk_messages old JOIN knowledge_chunks chunk ON chunk.id=old.chunk_id LEFT JOIN knowledge_index_generation_imports mapping ON mapping.index_generation_id=chunk.index_generation_id AND mapping.conversation_id=chunk.conversation_id LEFT JOIN knowledge_import_generation_members member ON member.import_generation_id=mapping.import_generation_id AND member.message_id=old.message_id LEFT JOIN knowledge_message_normalizations normalization ON normalization.message_version_id=member.message_version_id GROUP BY old.chunk_id,old.message_id HAVING COUNT(member.message_version_id)<>1 OR COUNT(normalization.message_version_id)<>1 LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| ContractError::KbNotReady)?;
    if unrecoverable.is_some() {
        return Err(ContractError::KbNotReady);
    }
    Ok(())
}

fn backfill_v4_chunks(transaction: &Transaction<'_>) -> Result<(), ContractError> {
    let chunks = {
        let mut statement = transaction.prepare(
            "SELECT chunk.id,generation.id,generation.snapshot_hash,conversation.account_stable_id,conversation.conversation_stable_id FROM knowledge_chunks chunk JOIN knowledge_index_generations generation ON generation.id=chunk.index_generation_id JOIN knowledge_conversations conversation ON conversation.id=chunk.conversation_id ORDER BY generation.id,conversation.account_stable_id,conversation.conversation_stable_id,(SELECT MIN(normalization.created_at_ms) FROM knowledge_chunk_messages member JOIN knowledge_message_normalizations normalization ON normalization.message_version_id=member.message_version_id WHERE member.chunk_id=chunk.id),(SELECT MIN(normalization.source_ordinal) FROM knowledge_chunk_messages member JOIN knowledge_message_normalizations normalization ON normalization.message_version_id=member.message_version_id WHERE member.chunk_id=chunk.id),chunk.chunk_key,chunk.id",
        ).map_err(|_| ContractError::KbNotReady)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|_| ContractError::KbNotReady)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ContractError::KbNotReady)?;
        rows
    };
    let mut previous_generation = String::new();
    let mut previous_conversation = String::new();
    let mut chunk_index = 0_u32;
    for (chunk_id, generation_id, snapshot_hash, account, conversation) in chunks {
        if generation_id != previous_generation || conversation != previous_conversation {
            chunk_index = 0;
            previous_generation.clone_from(&generation_id);
            previous_conversation.clone_from(&conversation);
        }
        let messages = load_chunk_messages(transaction, &chunk_id, &account, &conversation)?;
        let draft =
            draft_from_messages(&snapshot_hash, chunk_index, FTS_PRETOKEN_VERSION, &messages)?;
        transaction.execute(
            "UPDATE knowledge_chunks SET chunk_key=?1,first_message_version_id=?2,last_message_version_id=?3,content=?4,content_hash=?5,chunk_index=?6,chunk_schema_version=?7,started_at_ms=?8,ended_at_ms=?9,token_count=?10,message_count=?11 WHERE id=?12",
            params![draft.chunk_key,draft.first_message_version_id,draft.last_message_version_id,draft.content,draft.content_hash,draft.chunk_index as i64,"chunk-v1",draft.started_at_ms,draft.ended_at_ms,draft.token_count as i64,draft.members.len() as i64,chunk_id],
        ).map_err(|_| ContractError::KbNotReady)?;
        transaction
            .execute(
                "DELETE FROM knowledge_chunks_fts WHERE chunk_id=?1",
                [&chunk_id],
            )
            .map_err(|_| ContractError::KbNotReady)?;
        transaction.execute(
            "INSERT INTO knowledge_chunks_fts(content,chunk_id,index_generation_id) VALUES(?1,?2,?3)",
            params![draft.fts_terms,chunk_id,generation_id],
        ).map_err(|_| ContractError::KbNotReady)?;
        chunk_index = chunk_index
            .checked_add(1)
            .ok_or(ContractError::KbNotReady)?;
    }
    transaction.execute(
        "UPDATE knowledge_index_generations SET schema_version='chunk-v1',token_counter_version='v1',fts_pretoken_version=?1,retrieval_token_budget=COALESCE(retrieval_token_budget,512) WHERE id IN (SELECT DISTINCT index_generation_id FROM knowledge_chunks)",
        [FTS_PRETOKEN_VERSION],
    ).map_err(|_| ContractError::KbNotReady)?;
    Ok(())
}

fn load_chunk_messages(
    connection: &Connection,
    chunk_id: &str,
    account: &str,
    conversation: &str,
) -> Result<Vec<BuildMessage>, ContractError> {
    let mut statement = connection.prepare(
        "SELECT message.id,version.id,COALESCE(message.message_stable_id,message.fallback_key),normalization.created_at_ms,normalization.source_ordinal,normalization.sort_key,normalization.sender_key,normalization.direction,normalization.message_kind,version.normalized_content,version.content_hash FROM knowledge_chunk_messages member JOIN knowledge_messages message ON message.id=member.message_id JOIN knowledge_message_versions version ON version.id=member.message_version_id AND version.message_id=message.id JOIN knowledge_message_normalizations normalization ON normalization.message_version_id=version.id WHERE member.chunk_id=?1 ORDER BY member.message_index",
    ).map_err(|_| ContractError::KbNotReady)?;
    let rows = statement
        .query_map([chunk_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
            ))
        })
        .map_err(|_| ContractError::KbNotReady)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ContractError::KbNotReady)?;
    rows.into_iter()
        .map(
            |(
                message_id,
                message_version_id,
                stable_message_key,
                created_at_ms,
                source_ordinal,
                sort_key,
                sender_key,
                direction,
                message_kind,
                content,
                content_hash,
            )| {
                Ok(BuildMessage {
                    account_stable_id: account.into(),
                    conversation_stable_id: conversation.into(),
                    message_id,
                    message_version_id,
                    stable_message_key,
                    created_at_ms,
                    source_ordinal: u64::try_from(source_ordinal)
                        .map_err(|_| ContractError::KbNotReady)?,
                    sort_key,
                    sender_key,
                    direction: Direction::parse(&direction)?,
                    message_kind,
                    content,
                    content_hash,
                })
            },
        )
        .collect()
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
        ("knowledge_index_generations", "token_counter_version"),
        ("knowledge_index_generations", "fts_pretoken_version"),
        ("knowledge_index_generations", "retrieval_token_budget"),
        ("knowledge_index_generations", "message_count"),
        ("knowledge_catalog_state", "active_index_generation_id"),
        ("knowledge_chunks", "index_generation_id"),
        ("knowledge_chunks", "chunk_index"),
        ("knowledge_chunks", "chunk_schema_version"),
        ("knowledge_chunks", "started_at_ms"),
        ("knowledge_chunks", "ended_at_ms"),
        ("knowledge_chunks", "token_count"),
        ("knowledge_chunks", "message_count"),
        ("knowledge_message_normalizations", "canonical_hash"),
        ("knowledge_message_normalizations", "direction"),
        ("knowledge_chunk_messages", "message_version_id"),
        ("knowledge_chunk_messages", "message_index"),
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
    for index in [
        "knowledge_index_generations_one_building_idx",
        "knowledge_chunks_generation_conversation_index_idx",
        "knowledge_chunks_generation_conversation_time_idx",
        "knowledge_chunk_messages_version_idx",
    ] {
        let exists: Option<i64> = connection
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1",
                [index],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| ContractError::KbNotReady)?;
        if exists.is_none() {
            return Err(ContractError::KbNotReady);
        }
    }
    let invalid_build: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM knowledge_index_generations WHERE status='building' AND (schema_version<>'chunk-v1' OR token_counter_version<>'v1' OR fts_pretoken_version<>'fts-pretoken-v1' OR retrieval_token_budget NOT BETWEEN 256 AND 4096 OR COALESCE(trim(json_extract(embedding_metadata_json,'$.provider')),'')='' OR COALESCE(trim(json_extract(embedding_metadata_json,'$.endpoint')),'')='' OR COALESCE(trim(json_extract(embedding_metadata_json,'$.model')),'')='' OR COALESCE(trim(json_extract(embedding_metadata_json,'$.fingerprint')),'')='' OR json_extract(embedding_metadata_json,'$.dimension') NOT BETWEEN 1 AND 65536) LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| ContractError::KbNotReady)?;
    let invalid_chunk: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM knowledge_chunks chunk JOIN knowledge_index_generations generation ON generation.id=chunk.index_generation_id WHERE generation.fts_pretoken_version<>'fts-pretoken-v1' OR chunk.chunk_index IS NULL OR chunk.chunk_schema_version<>'chunk-v1' OR chunk.started_at_ms IS NULL OR chunk.ended_at_ms IS NULL OR chunk.started_at_ms>chunk.ended_at_ms OR chunk.token_count IS NULL OR chunk.token_count<0 OR chunk.message_count IS NULL OR chunk.message_count<>(SELECT COUNT(*) FROM knowledge_chunk_messages member WHERE member.chunk_id=chunk.id) OR (SELECT COUNT(DISTINCT member.message_index) FROM knowledge_chunk_messages member WHERE member.chunk_id=chunk.id)<>chunk.message_count OR COALESCE((SELECT MIN(member.message_index) FROM knowledge_chunk_messages member WHERE member.chunk_id=chunk.id),-1)<>0 OR COALESCE((SELECT MAX(member.message_index) FROM knowledge_chunk_messages member WHERE member.chunk_id=chunk.id),-1)<>chunk.message_count-1 OR chunk.first_message_version_id<>(SELECT member.message_version_id FROM knowledge_chunk_messages member WHERE member.chunk_id=chunk.id AND member.message_index=0) OR chunk.last_message_version_id<>(SELECT member.message_version_id FROM knowledge_chunk_messages member WHERE member.chunk_id=chunk.id AND member.message_index=chunk.message_count-1) LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| ContractError::KbNotReady)?;
    let mismatched_fts: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM knowledge_chunks chunk LEFT JOIN knowledge_chunks_fts fts ON fts.chunk_id=chunk.id AND fts.index_generation_id=chunk.index_generation_id WHERE fts.chunk_id IS NULL UNION ALL SELECT 1 FROM knowledge_chunks_fts fts LEFT JOIN knowledge_chunks chunk ON chunk.id=fts.chunk_id AND chunk.index_generation_id=fts.index_generation_id WHERE chunk.id IS NULL LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| ContractError::KbNotReady)?;
    let invalid_member: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM knowledge_chunk_messages member JOIN knowledge_chunks chunk ON chunk.id=member.chunk_id LEFT JOIN knowledge_index_generation_imports mapping ON mapping.index_generation_id=chunk.index_generation_id AND mapping.conversation_id=chunk.conversation_id LEFT JOIN knowledge_import_generation_members generation_member ON generation_member.import_generation_id=mapping.import_generation_id AND generation_member.message_id=member.message_id AND generation_member.message_version_id=member.message_version_id WHERE mapping.import_generation_id IS NULL OR generation_member.message_id IS NULL UNION ALL SELECT 1 FROM knowledge_chunks_fts GROUP BY chunk_id,index_generation_id HAVING COUNT(*)<>1 LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| ContractError::KbNotReady)?;
    if invalid_build.is_some()
        || invalid_chunk.is_some()
        || mismatched_fts.is_some()
        || invalid_member.is_some()
    {
        return Err(ContractError::KbNotReady);
    }
    validate_chunk_derivations(connection)?;
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

fn validate_chunk_derivations(connection: &Connection) -> Result<(), ContractError> {
    let chunks = {
        let mut statement = connection.prepare(
            "SELECT chunk.id,generation.snapshot_hash,conversation.account_stable_id,conversation.conversation_stable_id,chunk.chunk_index,generation.fts_pretoken_version,chunk.chunk_key,chunk.first_message_version_id,chunk.last_message_version_id,chunk.started_at_ms,chunk.ended_at_ms,chunk.token_count,chunk.content,chunk.content_hash,fts.content FROM knowledge_chunks chunk JOIN knowledge_index_generations generation ON generation.id=chunk.index_generation_id JOIN knowledge_conversations conversation ON conversation.id=chunk.conversation_id JOIN knowledge_chunks_fts fts ON fts.chunk_id=chunk.id AND fts.index_generation_id=chunk.index_generation_id ORDER BY chunk.id",
        ).map_err(|_| ContractError::KbNotReady)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, String>(14)?,
                ))
            })
            .map_err(|_| ContractError::KbNotReady)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ContractError::KbNotReady)?;
        rows
    };
    for (
        chunk_id,
        snapshot_hash,
        account,
        conversation,
        chunk_index,
        pretoken_version,
        chunk_key,
        first_version,
        last_version,
        started_at,
        ended_at,
        token_count,
        content,
        content_hash,
        fts_terms,
    ) in chunks
    {
        let chunk_index = u32::try_from(chunk_index).map_err(|_| ContractError::KbNotReady)?;
        let messages = load_chunk_messages(connection, &chunk_id, &account, &conversation)?;
        let expected =
            draft_from_messages(&snapshot_hash, chunk_index, &pretoken_version, &messages)?;
        if expected.chunk_key != chunk_key
            || expected.first_message_version_id != first_version
            || expected.last_message_version_id != last_version
            || expected.started_at_ms != started_at
            || expected.ended_at_ms != ended_at
            || i64::from(expected.token_count) != token_count
            || expected.content != content
            || expected.content_hash != content_hash
            || expected.fts_terms != fts_terms
            || chunk_content_hash_v1(&content) != content_hash
            || fts_pretoken(&pretoken_version, &content)? != fts_terms
        {
            return Err(ContractError::KbNotReady);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_database() -> PathBuf {
        std::env::temp_dir().join(format!(
            "knowledge-migration-{}.sqlite",
            uuid::Uuid::new_v4()
        ))
    }

    fn create_v3(path: &Path, with_chunk: bool) -> Connection {
        let connection = Connection::open(path).unwrap();
        configure(&connection).unwrap();
        connection
            .execute_batch(&format!(
                "BEGIN IMMEDIATE; {INITIAL}; PRAGMA user_version=1; {SOURCE_LINEAGE_MESSAGE_GENERATIONS}; PRAGMA user_version=2; {STREAMING_MESSAGE_NORMALIZATION_MEDIA}; PRAGMA user_version=3; COMMIT;"
            ))
            .unwrap();
        if with_chunk {
            connection.execute_batch(
                "BEGIN IMMEDIATE;
                 INSERT INTO knowledge_sources(id,account_stable_id,export_id,schema_version,manifest_hash,coverage_hash,snapshot_kind,scope_filters_json,integrity_json,source_state,import_status,checked_at_ms,exported_at_ms,coverage_kind) VALUES('source','account','export','v1','manifest','coverage','full','{}','{}','active','full_declared',1,1,'full');
                 INSERT INTO knowledge_conversations(id,account_stable_id,conversation_stable_id) VALUES('conversation','account','stable-conversation');
                 INSERT INTO knowledge_import_generations(id,trigger_source_id,conversation_id,source_set_hash,merge_mode,status,message_count,created_at_ms) VALUES('import','source','conversation','source-set','replace','active',1,1);
                 INSERT INTO knowledge_messages(id,conversation_id,message_stable_id) VALUES('message','conversation','stable-message');
                 INSERT INTO knowledge_message_versions(id,message_id,import_generation_id,content,normalized_content,content_hash) VALUES('version','message','import','历史正文','历史正文','message-hash');
                 INSERT INTO knowledge_import_generation_members(import_generation_id,message_id,message_version_id,selection_reason) VALUES('import','message','version','fixture');
                 INSERT INTO knowledge_message_normalizations(message_version_id,created_at_ms,source_ordinal,sort_key,message_kind,render_kind,sender_key,text_hash,canonical_hash) VALUES('version',100,0,'sort','text','text','account','text-hash','canonical-hash');
                 INSERT INTO knowledge_index_generations(id,schema_version,embedding_metadata_json,snapshot_hash,status,chunk_count,created_at_ms) VALUES('index','legacy-v3','{}','snapshot','ready',1,1);
                 INSERT INTO knowledge_index_generation_imports(index_generation_id,conversation_id,import_generation_id) VALUES('index','conversation','import');
                 INSERT INTO knowledge_chunks(id,index_generation_id,chunk_key,conversation_id,first_message_version_id,last_message_version_id,content,content_hash) VALUES('chunk','index','legacy-key','conversation','version','version','legacy content','legacy-hash');
                 INSERT INTO knowledge_chunk_messages(chunk_id,message_id) VALUES('chunk','message');
                 INSERT INTO knowledge_chunks_fts(content,chunk_id,index_generation_id) VALUES('legacy content','chunk','index');
                 UPDATE knowledge_catalog_state SET active_snapshot_hash='snapshot',active_index_generation_id='index',activated_at_ms=1 WHERE singleton_id=1;
                 COMMIT;",
            ).unwrap();
        }
        connection
    }

    #[test]
    fn v3_empty_database_migrates_and_reopens_at_v4() {
        let path = temp_database();
        drop(create_v3(&path, false));
        drop(open_writer(&path).unwrap());
        drop(open_writer(&path).unwrap());
        let connection = Connection::open(&path).unwrap();
        let version: i32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_HEAD);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn recoverable_v3_chunk_is_rebuilt_inside_the_v4_transaction() {
        let path = temp_database();
        drop(create_v3(&path, true));
        drop(open_writer(&path).unwrap());
        drop(open_writer(&path).unwrap());
        let connection = Connection::open(&path).unwrap();
        let migrated: (i64, String, i64, String, String, String) = connection
            .query_row(
                "SELECT chunk.chunk_index,chunk.chunk_schema_version,chunk.message_count,generation.fts_pretoken_version,catalog.active_index_generation_id,fts.content FROM knowledge_chunks chunk JOIN knowledge_index_generations generation ON generation.id=chunk.index_generation_id JOIN knowledge_catalog_state catalog ON catalog.singleton_id=1 JOIN knowledge_chunks_fts fts ON fts.chunk_id=chunk.id",
                [],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)),
            )
            .unwrap();
        assert_eq!(migrated.0, 0);
        assert_eq!(migrated.1, "chunk-v1");
        assert_eq!(migrated.2, 1);
        assert_eq!(migrated.3, FTS_PRETOKEN_VERSION);
        assert_eq!(migrated.4, "index");
        assert_eq!(
            migrated.5,
            fts_pretoken(FTS_PRETOKEN_VERSION, "[100][self][account] 历史正文").unwrap()
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn unrecoverable_v3_member_rolls_back_without_changing_version_or_data() {
        let path = temp_database();
        let connection = create_v3(&path, true);
        connection
            .execute("DELETE FROM knowledge_import_generation_members", [])
            .unwrap();
        drop(connection);
        assert!(matches!(open_writer(&path), Err(ContractError::KbNotReady)));
        let connection = Connection::open(&path).unwrap();
        let version: i32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let original: (String, String) = connection
            .query_row(
                "SELECT chunk_key,content FROM knowledge_chunks WHERE id='chunk'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(version, 3);
        assert_eq!(original, ("legacy-key".into(), "legacy content".into()));
        let _ = fs::remove_file(path);
    }
}
