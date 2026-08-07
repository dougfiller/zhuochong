use super::state_machine::ReplyState;
use super::types::{
    BindingGeneration, BindingObservationVersion, CaptureVersion, ContractError, RequestId,
    SuggestionGeneration,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SCHEMA_VERSION: u8 = 1;
const MAX_JSONL_LINE_BYTES: usize = 16 * 1024;
const MAX_PAGE_LIMIT: u16 = 100;
const MAX_METADATA_TOKEN_BYTES: usize = 128;
const MAX_HITS: usize = 20;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RetrievalMode {
    Success,
    #[default]
    NoHit,
    FtsFallback,
}

/// Metadata only. These values deliberately cannot carry OCR, suggestion,
/// excerpt, path, window, or error-detail text.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct M2TraceMetadata {
    query_hmac: Option<String>,
    catalog_generation_seq: Option<u64>,
    active_import_snapshot_hash: Option<String>,
    index_generation_id: Option<String>,
    retrieval_mode: RetrievalMode,
    hit_ids: Vec<String>,
    hit_scores: Vec<f64>,
    model_request_id: Option<String>,
}

impl M2TraceMetadata {
    pub(crate) fn new(
        query_hmac: Option<String>,
        catalog_generation_seq: Option<u64>,
        active_import_snapshot_hash: Option<String>,
        index_generation_id: Option<String>,
        retrieval_mode: RetrievalMode,
        hit_ids: Vec<String>,
        hit_scores: Vec<f64>,
        model_request_id: Option<String>,
    ) -> Result<Self, ContractError> {
        let metadata = Self {
            query_hmac,
            catalog_generation_seq,
            active_import_snapshot_hash,
            index_generation_id,
            retrieval_mode,
            hit_ids,
            hit_scores,
            model_request_id,
        };
        validate_m2_metadata(&metadata)?;
        Ok(metadata)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReplyTraceEvent {
    schema_version: u8,
    event_id: String,
    occurred_at: DateTime<Utc>,
    request_id: String,
    stage_seq: u64,
    stage_name: String,
    status: String,
    error_code: Option<ContractError>,
    capture_version: Option<u64>,
    suggestion_generation: u64,
    binding_generation: u64,
    observation_version: u64,
    model_transport_calls: u8,
    final_state: Option<String>,
    m2: Option<M2TraceMetadata>,
}

impl ReplyTraceEvent {
    pub(crate) fn stage(
        request_id: &RequestId,
        stage_seq: u64,
        state: ReplyState,
        capture_version: Option<CaptureVersion>,
        suggestion_generation: SuggestionGeneration,
        binding_generation: BindingGeneration,
        observation_version: BindingObservationVersion,
        model_transport_calls: u8,
        error_code: Option<ContractError>,
        m2: Option<M2TraceMetadata>,
    ) -> Self {
        let terminal = matches!(
            state,
            ReplyState::ReplyReady | ReplyState::Cancelled | ReplyState::Failed
        );
        Self {
            schema_version: SCHEMA_VERSION,
            event_id: uuid::Uuid::new_v4().to_string(),
            occurred_at: Utc::now(),
            request_id: request_id.to_string(),
            stage_seq,
            stage_name: state_name(state).to_owned(),
            status: if terminal { "terminal" } else { "entered" }.to_owned(),
            error_code,
            capture_version: capture_version.map(CaptureVersion::value),
            suggestion_generation: suggestion_generation.value(),
            binding_generation: binding_generation.value(),
            observation_version: observation_version.value(),
            model_transport_calls,
            final_state: terminal.then(|| state_name(state).to_owned()),
            m2,
        }
    }

    pub(crate) fn stale_result(
        request_id: &RequestId,
        stage_seq: u64,
        state: ReplyState,
        suggestion_generation: SuggestionGeneration,
        binding_generation: BindingGeneration,
        observation_version: BindingObservationVersion,
        model_transport_calls: u8,
    ) -> Self {
        let mut event = Self::stage(
            request_id,
            stage_seq,
            state,
            None,
            suggestion_generation,
            binding_generation,
            observation_version,
            model_transport_calls,
            Some(ContractError::WxRequestStale),
            None,
        );
        event.status = "stale_result_rejected".to_owned();
        event.final_state = None;
        event
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WechatReplyTraceEntry {
    schema_version: u8,
    event_id: String,
    occurred_at: DateTime<Utc>,
    request_id: String,
    stage_seq: u64,
    stage_name: String,
    status: String,
    error_code: Option<ContractError>,
    capture_version: Option<u64>,
    suggestion_generation: u64,
    binding_generation: u64,
    observation_version: u64,
    model_transport_calls: u8,
    final_state: Option<String>,
    m2: Option<M2TraceMetadata>,
}

impl From<ReplyTraceEvent> for WechatReplyTraceEntry {
    fn from(value: ReplyTraceEvent) -> Self {
        Self {
            schema_version: value.schema_version,
            event_id: value.event_id,
            occurred_at: value.occurred_at,
            request_id: value.request_id,
            stage_seq: value.stage_seq,
            stage_name: value.stage_name,
            status: value.status,
            error_code: value.error_code,
            capture_version: value.capture_version,
            suggestion_generation: value.suggestion_generation,
            binding_generation: value.binding_generation,
            observation_version: value.observation_version,
            model_transport_calls: value.model_transport_calls,
            final_state: value.final_state,
            m2: value.m2,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TraceQuery {
    pub(crate) request_id: Option<RequestId>,
    pub(crate) occurred_after: Option<DateTime<Utc>>,
    pub(crate) occurred_before: Option<DateTime<Utc>>,
    pub(crate) cursor: Option<String>,
    pub(crate) limit: u16,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WechatReplyTracePage {
    entries: Vec<WechatReplyTraceEntry>,
    next_cursor: Option<String>,
    tail_recovered: bool,
}

impl WechatReplyTracePage {
    #[cfg(test)]
    pub(crate) fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

#[derive(Clone)]
pub(crate) struct ReplyTraceStore {
    root: PathBuf,
    writer: Arc<Mutex<()>>,
}

impl ReplyTraceStore {
    pub(crate) fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            root: data_dir.as_ref().join("wechat_reply").join("trace"),
            writer: Arc::new(Mutex::new(())),
        }
    }

    pub(crate) fn append(&self, event: ReplyTraceEvent) -> Result<(), ContractError> {
        validate_event(&event)?;
        let _writer = self
            .writer
            .lock()
            .map_err(|_| ContractError::WxTracePersistFailed)?;
        fs::create_dir_all(&self.root).map_err(|_| ContractError::WxTracePersistFailed)?;
        let path = self
            .root
            .join(format!("{}.jsonl", event.occurred_at.format("%F")));
        let mut line =
            serde_json::to_vec(&event).map_err(|_| ContractError::WxTracePersistFailed)?;
        line.push(b'\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|_| ContractError::WxTracePersistFailed)?;
        file.write_all(&line)
            .map_err(|_| ContractError::WxTracePersistFailed)?;
        file.sync_data()
            .map_err(|_| ContractError::WxTracePersistFailed)
    }

    pub(crate) fn list(&self, query: TraceQuery) -> Result<WechatReplyTracePage, ContractError> {
        if query.limit == 0
            || query.limit > MAX_PAGE_LIMIT
            || query
                .occurred_after
                .zip(query.occurred_before)
                .is_some_and(|(after, before)| after >= before)
        {
            return Err(ContractError::WxTraceInvalidQuery);
        }
        let cursor = query.cursor.as_deref().map(decode_cursor).transpose()?;
        if let Some(cursor) = &cursor {
            if cursor.request_id != query.request_id.as_ref().map(ToString::to_string)
                || cursor.occurred_after != query.occurred_after
                || cursor.occurred_before != query.occurred_before
            {
                return Err(ContractError::WxTraceInvalidQuery);
            }
        }

        let mut records = Vec::new();
        let mut tail_recovered = false;
        for path in trace_files(&self.root)? {
            let date = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .ok_or(ContractError::WxTracePersistFailed)?;
            let bytes = fs::read(&path).map_err(|_| ContractError::WxTracePersistFailed)?;
            for (line_index, raw) in bytes.split_inclusive(|byte| *byte == b'\n').enumerate() {
                let complete = raw.ends_with(b"\n");
                let line = raw.strip_suffix(b"\n").unwrap_or(raw);
                if line.is_empty() && complete {
                    return Err(ContractError::WxTracePersistFailed);
                }
                if line.len() > MAX_JSONL_LINE_BYTES {
                    return Err(ContractError::WxTracePersistFailed);
                }
                let parsed = serde_json::from_slice::<ReplyTraceEvent>(line);
                match parsed {
                    Ok(event) if complete => {
                        validate_event(&event)?;
                        if event_matches(&event, &query) {
                            records.push((
                                CursorPosition {
                                    date: date.to_owned(),
                                    line: line_index,
                                },
                                event,
                            ));
                        }
                    }
                    _ if line_index + 1 == bytes.split_inclusive(|byte| *byte == b'\n').count() => {
                        tail_recovered = true;
                        break;
                    }
                    _ => return Err(ContractError::WxTracePersistFailed),
                }
            }
        }

        let start = match cursor {
            Some(cursor) => records
                .iter()
                .position(|(position, _)| *position == cursor.position)
                .map(|index| index + 1)
                .ok_or(ContractError::WxTraceInvalidQuery)?,
            None => 0,
        };
        let end = (start + usize::from(query.limit)).min(records.len());
        let entries = records[start..end]
            .iter()
            .cloned()
            .map(|(_, event)| event.into())
            .collect();
        let next_cursor = (end < records.len()).then(|| {
            encode_cursor(&TraceCursor {
                position: records[end - 1].0.clone(),
                request_id: query.request_id.map(|id| id.to_string()),
                occurred_after: query.occurred_after,
                occurred_before: query.occurred_before,
            })
        });
        Ok(WechatReplyTracePage {
            entries,
            next_cursor,
            tail_recovered,
        })
    }
}

fn state_name(state: ReplyState) -> &'static str {
    match state {
        ReplyState::Idle => "idle",
        ReplyState::Validating => "validating",
        ReplyState::Capturing => "capturing",
        ReplyState::Ocr => "ocr",
        ReplyState::Retrieving => "retrieving",
        ReplyState::Generating => "generating",
        ReplyState::ReplyReady => "reply_ready",
        ReplyState::Copied => "copied",
        ReplyState::Dismissed => "dismissed",
        ReplyState::Cancelled => "cancelled",
        ReplyState::Failed => "failed",
    }
}

fn validate_event(event: &ReplyTraceEvent) -> Result<(), ContractError> {
    if event.schema_version != SCHEMA_VERSION
        || event.stage_seq == 0
        || RequestId::parse(&event.request_id).is_err()
        || uuid::Uuid::parse_str(&event.event_id).is_err()
    {
        return Err(ContractError::WxTracePersistFailed);
    }
    let terminal_stage = matches!(
        event.stage_name.as_str(),
        "reply_ready" | "cancelled" | "failed"
    );
    let active_stage = matches!(
        event.stage_name.as_str(),
        "validating" | "capturing" | "ocr" | "retrieving" | "generating"
    );
    match event.status.as_str() {
        "entered" if active_stage && event.final_state.is_none() && event.error_code.is_none() => {}
        "terminal" if terminal_stage && event.final_state.as_deref() == Some(&event.stage_name) => {
            if (event.stage_name == "reply_ready") != event.error_code.is_none() {
                return Err(ContractError::WxTracePersistFailed);
            }
        }
        "stale_result_rejected"
            if active_stage
                && event.final_state.is_none()
                && event.error_code == Some(ContractError::WxRequestStale) => {}
        _ => return Err(ContractError::WxTracePersistFailed),
    }
    if event.m2.is_some() && !(event.status == "entered" && event.stage_name == "generating") {
        return Err(ContractError::WxTracePersistFailed);
    }
    if let Some(metadata) = &event.m2 {
        validate_m2_metadata(metadata)?;
    }
    Ok(())
}

fn validate_m2_metadata(metadata: &M2TraceMetadata) -> Result<(), ContractError> {
    if metadata
        .catalog_generation_seq
        .is_some_and(|value| value == 0)
        || metadata.hit_ids.len() > MAX_HITS
        || metadata.hit_ids.len() != metadata.hit_scores.len()
        || !metadata
            .hit_ids
            .iter()
            .all(|value| is_metadata_token(value))
        || !metadata
            .hit_scores
            .iter()
            .all(|score| score.is_finite() && (0.0..=1.0).contains(score))
        || !metadata
            .index_generation_id
            .as_deref()
            .is_none_or(is_metadata_token)
        || !metadata
            .model_request_id
            .as_deref()
            .is_none_or(is_metadata_token)
        || !metadata.query_hmac.as_deref().is_none_or(is_sha256_hex)
        || !metadata
            .active_import_snapshot_hash
            .as_deref()
            .is_none_or(is_sha256_hex)
        || (metadata.retrieval_mode == RetrievalMode::NoHit && !metadata.hit_ids.is_empty())
    {
        return Err(ContractError::WxTracePersistFailed);
    }
    Ok(())
}

fn is_metadata_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_METADATA_TOKEN_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn event_matches(event: &ReplyTraceEvent, query: &TraceQuery) -> bool {
    query
        .request_id
        .as_ref()
        .is_none_or(|request_id| event.request_id == request_id.to_string())
        && query
            .occurred_after
            .is_none_or(|after| event.occurred_at >= after)
        && query
            .occurred_before
            .is_none_or(|before| event.occurred_at < before)
}

fn trace_files(root: &Path) -> Result<Vec<PathBuf>, ContractError> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(root).map_err(|_| ContractError::WxTracePersistFailed)? {
        let path = entry
            .map_err(|_| ContractError::WxTracePersistFailed)?
            .path();
        let valid_name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .and_then(|date| NaiveDate::parse_from_str(date, "%F").ok())
            .is_some();
        if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
            && valid_name
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct CursorPosition {
    date: String,
    line: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TraceCursor {
    position: CursorPosition,
    request_id: Option<String>,
    occurred_after: Option<DateTime<Utc>>,
    occurred_before: Option<DateTime<Utc>>,
}

fn encode_cursor(cursor: &TraceCursor) -> String {
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(cursor).expect("trace cursor is serializable"))
}

fn decode_cursor(value: &str) -> Result<TraceCursor, ContractError> {
    let decoded = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ContractError::WxTraceInvalidQuery)?;
    serde_json::from_slice(&decoded).map_err(|_| ContractError::WxTraceInvalidQuery)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(request_id: &RequestId, sequence: u64) -> ReplyTraceEvent {
        ReplyTraceEvent::stage(
            request_id,
            sequence,
            ReplyState::Validating,
            None,
            SuggestionGeneration::new(1),
            BindingGeneration::new(2),
            BindingObservationVersion::new(3),
            0,
            None,
            None,
        )
    }

    #[test]
    fn append_paginates_and_recovers_a_truncated_tail() {
        let directory = std::env::temp_dir().join(format!("wechat-trace-{}", uuid::Uuid::new_v4()));
        let store = ReplyTraceStore::new(&directory);
        let request_id = RequestId::new();
        store.append(event(&request_id, 1)).unwrap();
        store.append(event(&request_id, 2)).unwrap();
        let path = store
            .root
            .join(format!("{}.jsonl", Utc::now().format("%F")));
        OpenOptions::new()
            .append(true)
            .open(path)
            .unwrap()
            .write_all(b"{\"bad")
            .unwrap();
        let first = store
            .list(TraceQuery {
                request_id: Some(request_id.clone()),
                occurred_after: None,
                occurred_before: None,
                cursor: None,
                limit: 1,
            })
            .unwrap();
        assert_eq!(first.entries.len(), 1);
        assert!(first.tail_recovered);
        let second = store
            .list(TraceQuery {
                request_id: Some(request_id),
                occurred_after: None,
                occurred_before: None,
                cursor: first.next_cursor,
                limit: 1,
            })
            .unwrap();
        assert_eq!(second.entries.len(), 1);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn rejects_mid_file_corruption() {
        let directory = std::env::temp_dir().join(format!("wechat-trace-{}", uuid::Uuid::new_v4()));
        let store = ReplyTraceStore::new(&directory);
        fs::create_dir_all(&store.root).unwrap();
        fs::write(store.root.join("2026-08-06.jsonl"), b"{bad}\n{}\n").unwrap();
        assert!(matches!(
            store.list(TraceQuery {
                request_id: None,
                occurred_after: None,
                occurred_before: None,
                cursor: None,
                limit: 10
            }),
            Err(ContractError::WxTracePersistFailed)
        ));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn rejects_unbounded_or_invalid_metadata_and_event_shapes() {
        let request_id = RequestId::new();
        let mut invalid_event = event(&request_id, 1);
        invalid_event.event_id = "not-a-uuid".into();
        assert_eq!(
            validate_event(&invalid_event),
            Err(ContractError::WxTracePersistFailed)
        );

        assert_eq!(
            M2TraceMetadata::new(
                None,
                Some(1),
                None,
                None,
                RetrievalMode::Success,
                vec!["content with spaces is rejected".into()],
                vec![0.5],
                None,
            ),
            Err(ContractError::WxTracePersistFailed)
        );
        assert_eq!(
            M2TraceMetadata::new(
                None,
                Some(1),
                None,
                None,
                RetrievalMode::NoHit,
                vec!["hit-1".into()],
                vec![0.5],
                None,
            ),
            Err(ContractError::WxTracePersistFailed)
        );
    }
}
