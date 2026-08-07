use super::types::{BindingGeneration, ContractError, RequestId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SCHEMA_VERSION: u8 = 1;
const MAX_JSONL_LINE_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Hash, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum M2AuditKind {
    RetrievalCompleted,
    ModelTransportStarted,
    EmbeddingTransport,
    OcrLocalProcessStarted,
    UploadQueueObserved,
    CapabilityObserved,
    TerminalObserved,
}

#[derive(Clone, Copy, Debug, Deserialize, Hash, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CapabilityKind {
    ReplyModelNonLoopback,
    KnowledgeEmbeddingLoopback,
    OcrBackendLocalProcess,
    Mcp,
    Bot,
    LocalhostApi,
    RemoteUpload,
    Search,
    Action,
    CloudEmbedding,
    PetResourceExternalProcess,
    PetResourceNetwork,
    PetResourceModuleLoad,
    PetResourceSyntheticInput,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AuditOrigin {
    WechatReplyOrchestrator,
    WechatReplyModelTransport,
    KnowledgeEmbedding,
    WechatOcrOrchestrator,
    UploadQueueSpy,
    PetResourceObserver,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum AuditOutcomeCode {
    RetrievalSuccess,
    RetrievalNoHit,
    RetrievalFtsFallback,
    TransportStarted,
    MetadataOk,
    MetadataUnavailable,
    QueryOk,
    QueryUnavailable,
    OcrFallbackStarted,
    ObservedZero,
    ObservedCount,
    TerminalSuccess,
    TerminalFailed,
    TerminalCancelled,
}

/// Fixed metadata-only M2 event. It has no generic string payload or map, so
/// chat/model text, credentials, endpoints, paths and internal source IDs have
/// no representable field.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct M2AuditEvent {
    schema_version: u8,
    event_id: String,
    occurred_at: DateTime<Utc>,
    request_id: String,
    kind: M2AuditKind,
    stage_seq: Option<u64>,
    attempt: Option<u8>,
    capability: Option<CapabilityKind>,
    origin: AuditOrigin,
    outcome_code: AuditOutcomeCode,
    count: u8,
    elapsed_ms: Option<u64>,
    binding_generation: Option<u64>,
    context_hash: Option<String>,
    model_request_id: Option<String>,
    request_bytes_sha256: Option<String>,
    selected_hit_count: Option<u8>,
    error_code: Option<String>,
}

impl M2AuditEvent {
    #[allow(clippy::too_many_arguments)]
    fn create(
        request_id: &RequestId,
        kind: M2AuditKind,
        stage_seq: Option<u64>,
        attempt: Option<u8>,
        capability: Option<CapabilityKind>,
        origin: AuditOrigin,
        outcome_code: AuditOutcomeCode,
        count: u8,
        elapsed_ms: Option<u64>,
        binding_generation: Option<BindingGeneration>,
        context_hash: Option<String>,
        model_request_id: Option<String>,
        request_bytes_sha256: Option<String>,
        selected_hit_count: Option<u8>,
        error_code: Option<String>,
    ) -> Result<Self, ContractError> {
        let event = Self {
            schema_version: SCHEMA_VERSION,
            event_id: uuid::Uuid::new_v4().to_string(),
            occurred_at: Utc::now(),
            request_id: request_id.to_string(),
            kind,
            stage_seq,
            attempt,
            capability,
            origin,
            outcome_code,
            count,
            elapsed_ms,
            binding_generation: binding_generation.map(BindingGeneration::value),
            context_hash,
            model_request_id,
            request_bytes_sha256,
            selected_hit_count,
            error_code,
        };
        validate_event(&event)?;
        Ok(event)
    }

    pub(crate) fn retrieval_completed(
        request_id: &RequestId,
        binding_generation: BindingGeneration,
        context_hash: String,
        model_request_id: String,
        selected_hit_count: u8,
        outcome_code: AuditOutcomeCode,
    ) -> Result<Self, ContractError> {
        if !matches!(
            outcome_code,
            AuditOutcomeCode::RetrievalSuccess
                | AuditOutcomeCode::RetrievalNoHit
                | AuditOutcomeCode::RetrievalFtsFallback
        ) {
            return Err(ContractError::WxAuditPersistFailed);
        }
        Self::create(
            request_id,
            M2AuditKind::RetrievalCompleted,
            Some(5),
            None,
            None,
            AuditOrigin::WechatReplyOrchestrator,
            outcome_code,
            1,
            None,
            Some(binding_generation),
            Some(context_hash),
            Some(model_request_id),
            None,
            Some(selected_hit_count),
            None,
        )
    }

    pub(crate) fn model_transport_started(
        request_id: &RequestId,
        binding_generation: BindingGeneration,
        stage_seq: u64,
        attempt: u8,
        context_hash: String,
        model_request_id: String,
        request_bytes_sha256: String,
    ) -> Result<Self, ContractError> {
        Self::create(
            request_id,
            M2AuditKind::ModelTransportStarted,
            Some(stage_seq),
            Some(attempt),
            Some(CapabilityKind::ReplyModelNonLoopback),
            AuditOrigin::WechatReplyModelTransport,
            AuditOutcomeCode::TransportStarted,
            1,
            None,
            Some(binding_generation),
            Some(context_hash),
            Some(model_request_id),
            Some(request_bytes_sha256),
            None,
            None,
        )
    }

    pub(crate) fn embedding_transport(
        request_id: &RequestId,
        outcome_code: AuditOutcomeCode,
        elapsed_ms: u64,
    ) -> Result<Self, ContractError> {
        if !matches!(
            outcome_code,
            AuditOutcomeCode::MetadataOk
                | AuditOutcomeCode::MetadataUnavailable
                | AuditOutcomeCode::QueryOk
                | AuditOutcomeCode::QueryUnavailable
        ) {
            return Err(ContractError::WxAuditPersistFailed);
        }
        Self::create(
            request_id,
            M2AuditKind::EmbeddingTransport,
            Some(4),
            None,
            Some(CapabilityKind::KnowledgeEmbeddingLoopback),
            AuditOrigin::KnowledgeEmbedding,
            outcome_code,
            1,
            Some(elapsed_ms),
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    pub(crate) fn capability_observed(
        request_id: &RequestId,
        capability: CapabilityKind,
        count: u8,
    ) -> Result<Self, ContractError> {
        Self::create(
            request_id,
            M2AuditKind::CapabilityObserved,
            None,
            None,
            Some(capability),
            match capability {
                CapabilityKind::RemoteUpload => AuditOrigin::UploadQueueSpy,
                CapabilityKind::PetResourceExternalProcess
                | CapabilityKind::PetResourceNetwork
                | CapabilityKind::PetResourceModuleLoad
                | CapabilityKind::PetResourceSyntheticInput => AuditOrigin::PetResourceObserver,
                _ => AuditOrigin::WechatReplyOrchestrator,
            },
            if count == 0 {
                AuditOutcomeCode::ObservedZero
            } else {
                AuditOutcomeCode::ObservedCount
            },
            count,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    pub(crate) fn ocr_local_process_started(request_id: &RequestId) -> Result<Self, ContractError> {
        Self::create(
            request_id,
            M2AuditKind::OcrLocalProcessStarted,
            Some(3),
            None,
            Some(CapabilityKind::OcrBackendLocalProcess),
            AuditOrigin::WechatOcrOrchestrator,
            AuditOutcomeCode::OcrFallbackStarted,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    pub(crate) fn upload_queue_observed_zero(
        request_id: &RequestId,
    ) -> Result<Self, ContractError> {
        Self::create(
            request_id,
            M2AuditKind::UploadQueueObserved,
            None,
            None,
            Some(CapabilityKind::RemoteUpload),
            AuditOrigin::UploadQueueSpy,
            AuditOutcomeCode::ObservedZero,
            0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    pub(crate) fn terminal_observed(
        request_id: &RequestId,
        stage_seq: u64,
        outcome_code: AuditOutcomeCode,
        error: Option<ContractError>,
    ) -> Result<Self, ContractError> {
        if !matches!(
            outcome_code,
            AuditOutcomeCode::TerminalSuccess
                | AuditOutcomeCode::TerminalFailed
                | AuditOutcomeCode::TerminalCancelled
        ) || (outcome_code == AuditOutcomeCode::TerminalSuccess) != error.is_none()
        {
            return Err(ContractError::WxAuditPersistFailed);
        }
        let error_code = error
            .map(|error| {
                serde_json::to_value(error).map_err(|_| ContractError::WxAuditPersistFailed)
            })
            .transpose()?
            .map(|value| value.as_str().unwrap_or_default().to_owned());
        Self::create(
            request_id,
            M2AuditKind::TerminalObserved,
            Some(stage_seq),
            None,
            None,
            AuditOrigin::WechatReplyOrchestrator,
            outcome_code,
            1,
            None,
            None,
            None,
            None,
            None,
            None,
            error_code,
        )
    }

    #[cfg(test)]
    pub(crate) fn request_id(&self) -> &str {
        &self.request_id
    }

    #[cfg(test)]
    pub(crate) fn kind(&self) -> M2AuditKind {
        self.kind
    }

    #[cfg(test)]
    pub(crate) fn attempt(&self) -> Option<u8> {
        self.attempt
    }

    #[cfg(test)]
    pub(crate) fn request_bytes_sha256(&self) -> Option<&str> {
        self.request_bytes_sha256.as_deref()
    }
}

pub(crate) trait M2AuditSink: Send + Sync {
    fn record(&self, event: M2AuditEvent) -> Result<(), ContractError>;
}

pub(crate) struct DiscardM2AuditSink;

impl M2AuditSink for DiscardM2AuditSink {
    fn record(&self, _event: M2AuditEvent) -> Result<(), ContractError> {
        Ok(())
    }
}

pub(crate) struct M2AuditStore {
    root: PathBuf,
    writer: Mutex<()>,
}

impl M2AuditStore {
    pub(crate) fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            root: data_dir.as_ref().join("wechat_reply").join("audit"),
            writer: Mutex::new(()),
        }
    }

    fn append(&self, event: M2AuditEvent) -> Result<(), ContractError> {
        validate_event(&event)?;
        let mut line =
            serde_json::to_vec(&event).map_err(|_| ContractError::WxAuditPersistFailed)?;
        line.push(b'\n');
        if line.len() > MAX_JSONL_LINE_BYTES {
            return Err(ContractError::WxAuditPersistFailed);
        }
        let _writer = self
            .writer
            .lock()
            .map_err(|_| ContractError::WxAuditPersistFailed)?;
        fs::create_dir_all(&self.root).map_err(|_| ContractError::WxAuditPersistFailed)?;
        let path = self
            .root
            .join(format!("{}.jsonl", event.occurred_at.format("%F")));
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|_| ContractError::WxAuditPersistFailed)?;
        file.write_all(&line)
            .and_then(|_| file.sync_data())
            .map_err(|_| ContractError::WxAuditPersistFailed)
    }
}

impl M2AuditSink for M2AuditStore {
    fn record(&self, event: M2AuditEvent) -> Result<(), ContractError> {
        self.append(event)
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct SpyM2AuditSink {
    events: Mutex<Vec<M2AuditEvent>>,
}

#[cfg(test)]
impl SpyM2AuditSink {
    pub(crate) fn snapshot(&self) -> Vec<M2AuditEvent> {
        self.events.lock().unwrap().clone()
    }
}

#[cfg(test)]
impl M2AuditSink for SpyM2AuditSink {
    fn record(&self, event: M2AuditEvent) -> Result<(), ContractError> {
        validate_event(&event)?;
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CapabilityCounts {
    reply_model_non_loopback: u8,
    knowledge_embedding_loopback: u8,
    ocr_backend_local_process: u8,
    mcp: u8,
    bot: u8,
    localhost_api: u8,
    remote_upload: u8,
    search: u8,
    action: u8,
    cloud_embedding: u8,
    pet_resource_external_process: u8,
    pet_resource_network: u8,
    pet_resource_module_load: u8,
    pet_resource_synthetic_input: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CapabilitySnapshot {
    request_id: String,
    counts: CapabilityCounts,
}

impl CapabilitySnapshot {
    pub(crate) fn from_events(
        request_id: &RequestId,
        events: &[M2AuditEvent],
    ) -> Result<Self, ContractError> {
        let expected = request_id.to_string();
        let mut counts = CapabilityCounts::default();
        let mut observed = HashSet::new();
        for event in events {
            if event.request_id != expected {
                return Err(ContractError::WxAuditPersistFailed);
            }
            let Some(capability) = event.capability else {
                continue;
            };
            observed.insert(capability);
            let target = match capability {
                CapabilityKind::ReplyModelNonLoopback => &mut counts.reply_model_non_loopback,
                CapabilityKind::KnowledgeEmbeddingLoopback => {
                    &mut counts.knowledge_embedding_loopback
                }
                CapabilityKind::OcrBackendLocalProcess => &mut counts.ocr_backend_local_process,
                CapabilityKind::Mcp => &mut counts.mcp,
                CapabilityKind::Bot => &mut counts.bot,
                CapabilityKind::LocalhostApi => &mut counts.localhost_api,
                CapabilityKind::RemoteUpload => &mut counts.remote_upload,
                CapabilityKind::Search => &mut counts.search,
                CapabilityKind::Action => &mut counts.action,
                CapabilityKind::CloudEmbedding => &mut counts.cloud_embedding,
                CapabilityKind::PetResourceExternalProcess => {
                    &mut counts.pet_resource_external_process
                }
                CapabilityKind::PetResourceNetwork => &mut counts.pet_resource_network,
                CapabilityKind::PetResourceModuleLoad => &mut counts.pet_resource_module_load,
                CapabilityKind::PetResourceSyntheticInput => {
                    &mut counts.pet_resource_synthetic_input
                }
            };
            *target = target
                .checked_add(event.count)
                .ok_or(ContractError::WxAuditPersistFailed)?;
        }
        let required = HashSet::from([
            CapabilityKind::ReplyModelNonLoopback,
            CapabilityKind::KnowledgeEmbeddingLoopback,
            CapabilityKind::OcrBackendLocalProcess,
            CapabilityKind::Mcp,
            CapabilityKind::Bot,
            CapabilityKind::LocalhostApi,
            CapabilityKind::RemoteUpload,
            CapabilityKind::Search,
            CapabilityKind::Action,
            CapabilityKind::CloudEmbedding,
            CapabilityKind::PetResourceExternalProcess,
            CapabilityKind::PetResourceNetwork,
            CapabilityKind::PetResourceModuleLoad,
            CapabilityKind::PetResourceSyntheticInput,
        ]);
        if observed != required {
            return Err(ContractError::WxAuditPersistFailed);
        }
        Ok(Self {
            request_id: expected,
            counts,
        })
    }

    pub(crate) fn forbidden_are_zero(&self) -> bool {
        self.counts.mcp == 0
            && self.counts.bot == 0
            && self.counts.localhost_api == 0
            && self.counts.remote_upload == 0
            && self.counts.search == 0
            && self.counts.action == 0
            && self.counts.cloud_embedding == 0
            && self.counts.pet_resource_external_process == 0
            && self.counts.pet_resource_network == 0
            && self.counts.pet_resource_module_load == 0
            && self.counts.pet_resource_synthetic_input == 0
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct M2CollectedRequestEvidence {
    request_id: String,
    events: Vec<M2AuditEvent>,
    capability_snapshot: CapabilitySnapshot,
}

pub(crate) struct M2EvidenceCollector;

impl M2EvidenceCollector {
    pub(crate) fn collect_request(
        request_id: &RequestId,
        events: &[M2AuditEvent],
    ) -> Result<M2CollectedRequestEvidence, ContractError> {
        let expected = request_id.to_string();
        if events.is_empty()
            || events.iter().any(|event| event.request_id != expected)
            || events
                .iter()
                .filter(|event| event.kind == M2AuditKind::TerminalObserved)
                .count()
                != 1
            || !events
                .iter()
                .any(|event| event.kind == M2AuditKind::UploadQueueObserved)
        {
            return Err(ContractError::WxAuditPersistFailed);
        }
        let capability_snapshot = CapabilitySnapshot::from_events(request_id, events)?;
        Ok(M2CollectedRequestEvidence {
            request_id: expected,
            events: events.to_vec(),
            capability_snapshot,
        })
    }
}

fn validate_event(event: &M2AuditEvent) -> Result<(), ContractError> {
    let valid_request = uuid::Uuid::parse_str(&event.request_id).is_ok();
    let valid_event = uuid::Uuid::parse_str(&event.event_id).is_ok();
    let valid_attempt = event.attempt.is_none_or(|attempt| matches!(attempt, 1 | 2));
    let valid_sha =
        |value: &str| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit());
    let valid_model_request = event.model_request_id.as_deref().is_none_or(|value| {
        value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    });
    let valid_error_code = event.error_code.as_ref().is_none_or(|code| {
        serde_json::from_value::<ContractError>(serde_json::Value::String(code.clone())).is_ok()
    });
    let valid_terminal = event.kind != M2AuditKind::TerminalObserved
        || (event
            .stage_seq
            .is_some_and(|stage| (1..=6).contains(&stage))
            && event.count == 1
            && match event.outcome_code {
                AuditOutcomeCode::TerminalSuccess => event.error_code.is_none(),
                AuditOutcomeCode::TerminalFailed | AuditOutcomeCode::TerminalCancelled => {
                    event.error_code.is_some()
                }
                _ => false,
            });
    let valid_observation = match event.outcome_code {
        AuditOutcomeCode::ObservedZero => event.count == 0,
        AuditOutcomeCode::ObservedCount => event.count > 0,
        _ => true,
    };
    if event.schema_version != SCHEMA_VERSION
        || !valid_request
        || !valid_event
        || !valid_attempt
        || event.count > 2
        || !event.context_hash.as_deref().is_none_or(valid_sha)
        || !event.request_bytes_sha256.as_deref().is_none_or(valid_sha)
        || !valid_model_request
        || !valid_error_code
        || !valid_terminal
        || !valid_observation
        || (event.kind != M2AuditKind::TerminalObserved && event.error_code.is_some())
        || (event.kind == M2AuditKind::ModelTransportStarted
            && (event.stage_seq != Some(5)
                || event.attempt.is_none()
                || event.binding_generation.is_none()
                || event.context_hash.is_none()
                || event.model_request_id.is_none()
                || event.request_bytes_sha256.is_none()
                || event.capability != Some(CapabilityKind::ReplyModelNonLoopback)
                || event.count != 1))
    {
        return Err(ContractError::WxAuditPersistFailed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_rejects_unknown_or_sensitive_fields() {
        let request_id = RequestId::new();
        let event = M2AuditEvent::capability_observed(&request_id, CapabilityKind::Mcp, 0).unwrap();
        let mut value = serde_json::to_value(event).unwrap();
        value.as_object_mut().unwrap().insert(
            "prompt".into(),
            serde_json::Value::String("forbidden fixture".into()),
        );
        assert!(serde_json::from_value::<M2AuditEvent>(value).is_err());
    }

    #[test]
    fn store_writes_bounded_metadata_only_jsonl() {
        let directory = std::env::temp_dir().join(format!("m2-audit-{}", uuid::Uuid::new_v4()));
        let store = M2AuditStore::new(&directory);
        let request_id = RequestId::new();
        store
            .record(
                M2AuditEvent::model_transport_started(
                    &request_id,
                    BindingGeneration::new(3),
                    5,
                    1,
                    "a".repeat(64),
                    "b".repeat(32),
                    "c".repeat(64),
                )
                .unwrap(),
            )
            .unwrap();
        let path = store
            .root
            .join(format!("{}.jsonl", Utc::now().format("%F")));
        let bytes = fs::read(path).unwrap();
        assert!(bytes.len() < MAX_JSONL_LINE_BYTES);
        let parsed: M2AuditEvent = serde_json::from_slice(&bytes[..bytes.len() - 1]).unwrap();
        assert_eq!(parsed.request_id, request_id.to_string());
        let serialized = String::from_utf8(bytes).unwrap();
        for forbidden in ["prompt", "body", "apiKey", "authorization", "sourcePath"] {
            assert!(!serialized.contains(forbidden));
        }
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn capability_snapshot_requires_one_request_and_explicit_zeroes() {
        let request_id = RequestId::new();
        let capabilities = [
            CapabilityKind::ReplyModelNonLoopback,
            CapabilityKind::KnowledgeEmbeddingLoopback,
            CapabilityKind::OcrBackendLocalProcess,
            CapabilityKind::Mcp,
            CapabilityKind::Bot,
            CapabilityKind::LocalhostApi,
            CapabilityKind::RemoteUpload,
            CapabilityKind::Search,
            CapabilityKind::Action,
            CapabilityKind::CloudEmbedding,
            CapabilityKind::PetResourceExternalProcess,
            CapabilityKind::PetResourceNetwork,
            CapabilityKind::PetResourceModuleLoad,
            CapabilityKind::PetResourceSyntheticInput,
        ];
        let mut events = capabilities
            .into_iter()
            .map(|capability| {
                M2AuditEvent::capability_observed(&request_id, capability, 0).unwrap()
            })
            .collect::<Vec<_>>();
        events.push(
            M2AuditEvent::model_transport_started(
                &request_id,
                BindingGeneration::new(3),
                5,
                1,
                "a".repeat(64),
                "b".repeat(32),
                "c".repeat(64),
            )
            .unwrap(),
        );
        events.push(M2AuditEvent::upload_queue_observed_zero(&request_id).unwrap());
        events.push(
            M2AuditEvent::terminal_observed(
                &request_id,
                6,
                AuditOutcomeCode::TerminalSuccess,
                None,
            )
            .unwrap(),
        );
        let snapshot = CapabilitySnapshot::from_events(&request_id, &events).unwrap();
        assert_eq!(snapshot.counts.reply_model_non_loopback, 1);
        assert!(snapshot.forbidden_are_zero());
        assert!(M2EvidenceCollector::collect_request(&request_id, &events).is_ok());

        assert!(CapabilitySnapshot::from_events(&request_id, &events[..2]).is_err());
        assert!(
            M2EvidenceCollector::collect_request(&request_id, &events[..events.len() - 1]).is_err()
        );
        let other = RequestId::new();
        let mixed = [
            events[0].clone(),
            M2AuditEvent::capability_observed(&other, CapabilityKind::Mcp, 0).unwrap(),
        ];
        assert!(CapabilitySnapshot::from_events(&request_id, &mixed).is_err());
    }

    #[test]
    fn terminal_and_observer_events_are_production_schema_events() {
        let request_id = RequestId::new();
        let terminal = M2AuditEvent::terminal_observed(
            &request_id,
            6,
            AuditOutcomeCode::TerminalFailed,
            Some(ContractError::LlmFailed),
        )
        .unwrap();
        assert_eq!(terminal.kind, M2AuditKind::TerminalObserved);
        assert_eq!(terminal.error_code.as_deref(), Some("LLM_FAILED"));
        assert!(M2AuditEvent::upload_queue_observed_zero(&request_id).is_ok());
        assert!(M2AuditEvent::ocr_local_process_started(&request_id).is_ok());
    }
}
