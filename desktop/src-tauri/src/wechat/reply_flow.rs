#[cfg(any(feature = "wechat-m1", feature = "wechat-m2"))]
use super::model_client::WechatReplyModelClient;
#[cfg(feature = "wechat-m2")]
use super::model_contract::build_model_context;
#[cfg(feature = "wechat-m2")]
use super::observability::{AuditOutcomeCode, M2AuditEvent, M2AuditSink};
#[cfg(any(target_os = "windows", all(test, feature = "wechat-m2")))]
use super::ocr::{WechatOcrAuditEvent, WechatOcrAuditSink};
#[cfg(target_os = "windows")]
use super::ocr::{WechatOcrDispatcher, WindowsMemoryPrimary};
use super::runtime::{BeginReplySnapshot, CaptureCoordinator, ReplyLease, WechatReplyRuntime};
use super::state_machine::ReplyMode;
use super::state_machine::ReplyState;
#[cfg(any(target_os = "windows", all(test, feature = "wechat-m1")))]
use super::trace::ReplyTraceStore;
#[cfg(target_os = "windows")]
use super::types::CapturedWechat;
#[cfg(feature = "wechat-m1")]
use super::types::M1ReplyInput;
#[cfg(feature = "wechat-m1")]
use super::types::{BindingGeneration, BindingObservationVersion, CaptureVersion, OcrReadyReply};
use super::types::{ContractError, GeneratedReply};
#[cfg(target_os = "windows")]
use crate::avatar_engine;
use crate::AppState;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const REPLY_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(target_os = "windows")]
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

#[cfg(target_os = "windows")]
struct DiscardOcrAudit;

#[cfg(target_os = "windows")]
impl WechatOcrAuditSink for DiscardOcrAudit {
    fn record(&mut self, _event: WechatOcrAuditEvent) {}
}

#[cfg(all(feature = "wechat-m2", any(target_os = "windows", test)))]
struct M2OcrAudit<'a> {
    request_id: &'a super::types::RequestId,
    sink: &'a dyn M2AuditSink,
    error: Option<ContractError>,
}

#[cfg(all(feature = "wechat-m2", any(target_os = "windows", test)))]
impl WechatOcrAuditSink for M2OcrAudit<'_> {
    fn record(&mut self, event: WechatOcrAuditEvent) {
        let observed = if event.provider == "LocalFallback" {
            M2AuditEvent::ocr_local_process_started(self.request_id)
        } else {
            M2AuditEvent::capability_observed(
                self.request_id,
                super::observability::CapabilityKind::OcrBackendLocalProcess,
                0,
            )
        }
        .and_then(|event| self.sink.record(event));
        if let Err(error) = observed {
            self.error = Some(error);
        }
    }
}

#[cfg(feature = "wechat-m1")]
fn m1_snapshot() -> BeginReplySnapshot {
    BeginReplySnapshot {
        mode: ReplyMode::M1,
        binding_generation: BindingGeneration::new(0),
        observation_version: BindingObservationVersion::new(0),
        capture_version: None,
        timeout: REPLY_TIMEOUT,
    }
}

fn finish_failed(
    runtime: &WechatReplyRuntime,
    lease: ReplyLease,
    state: ReplyState,
    error: ContractError,
) {
    let _ = runtime.transition(&lease, state, ReplyState::Failed, Some(error), None);
    let _ = runtime.finish_reply(lease);
}

fn finish_terminal(runtime: &WechatReplyRuntime, lease: ReplyLease) {
    let _ = runtime.finish_reply(lease);
}

#[cfg(feature = "wechat-m2")]
fn complete_m2_revalidation(
    runtime: &WechatReplyRuntime,
    lease: &ReplyLease,
    state: ReplyState,
    result: Result<(), ContractError>,
) -> Result<(), ContractError> {
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            finish_failed(runtime, lease.clone(), state, error);
            Err(error)
        }
    }
}

/// Finishes the platform-independent tail of the M1 facade after guarded
/// capture has supplied its lease-bound version and OCR result. Keeping this
/// seam private lets the exact Windows-only front half remain small while the
/// stateful hand-off is exercised with a controlled transport on every host.
#[cfg(feature = "wechat-m1")]
async fn finish_captured_m1_reply(
    runtime: &WechatReplyRuntime,
    coordinator: &CaptureCoordinator,
    lease: &ReplyLease,
    capture_version: CaptureVersion,
    ocr: Result<OcrReadyReply, ContractError>,
    config: crate::config::WechatConfig,
    profiles: Vec<crate::config::TextModelProfile>,
    client: &WechatReplyModelClient,
) -> Result<GeneratedReply, ContractError> {
    if let Err(error) = runtime.enter_ocr_after_capture(lease, capture_version) {
        finish_terminal(runtime, lease.clone());
        return Err(error);
    }
    let ocr = match ocr {
        Ok(ocr) => ocr,
        Err(error) => {
            finish_failed(runtime, lease.clone(), ReplyState::Ocr, error);
            return Err(error);
        }
    };
    if !coordinator.is_current_capture(capture_version) {
        finish_failed(
            runtime,
            lease.clone(),
            ReplyState::Ocr,
            ContractError::WxRequestStale,
        );
        return Err(ContractError::WxRequestStale);
    }
    if let Err(error) =
        runtime.transition(lease, ReplyState::Ocr, ReplyState::Generating, None, None)
    {
        finish_terminal(runtime, lease.clone());
        return Err(error);
    }
    runtime
        .generate_m1_reply_with_client(client, config, profiles, M1ReplyInput::from(ocr), lease)
        .await
}

/// Publishes an already accepted M1 reply, emits its constrained avatar payload,
/// and releases the private lease. The caller supplies the existing event sink
/// so this terminal tail can be verified without Windows capture dependencies.
fn publish_reply_and_finish(
    runtime: &WechatReplyRuntime,
    lease: ReplyLease,
    reply: &GeneratedReply,
    emit: impl FnOnce(&crate::avatar_engine::AvatarBubblePayload),
) -> Result<(), ContractError> {
    let payload = match runtime.publish_generated_suggestion(reply) {
        Ok(payload) => payload,
        Err(error) => {
            let _ = runtime.cancel_reply(&lease);
            let _ = runtime.finish_reply(lease);
            return Err(error);
        }
    };
    emit(&payload);
    runtime.finish_reply(lease)
}

/// Private M1-only orchestration. The lease never leaves this module: a
/// generated reply is published to the existing avatar event and then released
/// before the command returns.
#[cfg(all(feature = "wechat-m1", target_os = "windows"))]
pub(crate) async fn generate_m1_wechat_reply(
    app: tauri::AppHandle,
    state: Arc<Mutex<AppState>>,
    runtime: &WechatReplyRuntime,
    coordinator: &CaptureCoordinator,
) -> Result<(), ContractError> {
    let (config, profiles, screenshot_service, data_dir) = {
        let state = state
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        (
            state.config.wechat.clone(),
            state.config.text_model_profiles.clone(),
            state.screenshot_service.clone(),
            state.data_dir.clone(),
        )
    };
    let lease = runtime.begin_reply(m1_snapshot(), ReplyTraceStore::new(data_dir))?;
    let cancel = match runtime.cancellation_receiver(&lease) {
        Ok(cancel) => cancel,
        Err(error) => return Err(error),
    };

    let identity = match runtime.validate_foreground_wechat() {
        Ok(identity) => identity,
        Err(error) => {
            finish_failed(runtime, lease, ReplyState::Validating, error);
            return Err(error);
        }
    };
    if let Err(error) = runtime.transition(
        &lease,
        ReplyState::Validating,
        ReplyState::Capturing,
        None,
        None,
    ) {
        finish_terminal(runtime, lease);
        return Err(error);
    }
    let (slices, identity) = match runtime
        .capture_foreground_wechat(
            app,
            coordinator,
            screenshot_service,
            identity,
            lease.request_id().clone(),
            CAPTURE_TIMEOUT,
            Some(cancel),
        )
        .await
    {
        Ok(slices) => slices,
        Err(error) => {
            finish_failed(runtime, lease, ReplyState::Capturing, error);
            return Err(error);
        }
    };
    let captured = CapturedWechat {
        request_id: lease.request_id().clone(),
        capture_version: slices.capture_version,
        stable_message_id: "m1-capture".into(),
        is_single_chat: true,
    };
    let mut dispatcher =
        WechatOcrDispatcher::new(WindowsMemoryPrimary, super::ocr::DisabledLocalFallback);
    let mut audit = DiscardOcrAudit;
    let ocr = dispatcher.recognize(&slices, &identity, captured, &mut audit);
    let reply = finish_captured_m1_reply(
        runtime,
        coordinator,
        &lease,
        slices.capture_version,
        ocr,
        config,
        profiles,
        &WechatReplyModelClient::new(),
    )
    .await?;
    publish_reply_and_finish(runtime, lease, &reply, |payload| {
        avatar_engine::emit_avatar_bubble(&app, payload);
    })
}

#[cfg(all(feature = "wechat-m1", not(target_os = "windows")))]
pub(crate) async fn generate_m1_wechat_reply(
    _app: tauri::AppHandle,
    _state: Arc<Mutex<AppState>>,
    _runtime: &WechatReplyRuntime,
    _coordinator: &CaptureCoordinator,
) -> Result<(), ContractError> {
    Err(ContractError::WxWindowUnsupported)
}

#[cfg(feature = "wechat-m2")]
fn m2_snapshot(request: &super::binding::BindingRequestSnapshot) -> BeginReplySnapshot {
    BeginReplySnapshot {
        mode: ReplyMode::M2,
        binding_generation: request.binding_generation,
        observation_version: request.observation_version,
        capture_version: None,
        timeout: REPLY_TIMEOUT,
    }
}

#[cfg(feature = "wechat-m2")]
fn retrieval_error(error: crate::knowledge::KnowledgeError) -> ContractError {
    match error {
        crate::knowledge::KnowledgeError::NotReady => ContractError::KbNotReady,
        crate::knowledge::KnowledgeError::ScopeUnresolved => ContractError::KbScopeUnresolved,
        crate::knowledge::KnowledgeError::RetrievalFailed => ContractError::KbRetrievalFailed,
        crate::knowledge::KnowledgeError::AuditPersistFailed => ContractError::WxAuditPersistFailed,
    }
}

#[cfg(feature = "wechat-m2")]
#[async_trait::async_trait]
trait M2ReplyTailPort {
    async fn revalidate(
        &mut self,
        stage: super::binding::BindingStage,
    ) -> Result<(), ContractError>;

    fn retrieval_scope(
        &self,
    ) -> (
        super::types::BindingGeneration,
        Option<String>,
        crate::knowledge::types::KnowledgeScope,
    );

    async fn retrieve(
        &mut self,
        request: crate::knowledge::KnowledgeRetrieveRequest,
        audit: &dyn M2AuditSink,
    ) -> Result<crate::knowledge::types::RetrievedReply, crate::knowledge::KnowledgeError>;
}

#[cfg(feature = "wechat-m2")]
async fn finish_captured_m2_reply(
    runtime: &WechatReplyRuntime,
    lease: &ReplyLease,
    query: String,
    knowledge: crate::config::KnowledgeConfig,
    config: crate::config::WechatConfig,
    profiles: Vec<crate::config::TextModelProfile>,
    port: &mut dyn M2ReplyTailPort,
    client: &WechatReplyModelClient,
    terminal_stage: &mut u64,
) -> Result<GeneratedReply, ContractError> {
    complete_m2_revalidation(
        runtime,
        lease,
        ReplyState::Retrieving,
        port.revalidate(super::binding::BindingStage::BeforeRetrieval)
            .await,
    )?;
    let (binding_generation, bound_conversation_id, scope) = port.retrieval_scope();
    let retrieval = port
        .retrieve(
            crate::knowledge::KnowledgeRetrieveRequest {
                request_id: lease.request_id().clone(),
                query_text: query,
                binding_generation,
                bound_conversation_id,
                scope,
                top_k: knowledge.top_k,
                token_budget: knowledge.token_budget,
                token_counter_version: "v1".into(),
                same_conversation_boost: knowledge.same_conversation_boost,
            },
            client.m2_audit_sink(),
        )
        .await;
    let retrieval = match retrieval {
        Ok(reply) => reply,
        Err(error) => {
            let error = retrieval_error(error);
            finish_failed(runtime, lease.clone(), ReplyState::Retrieving, error);
            return Err(error);
        }
    };
    let built = match build_model_context(retrieval) {
        Ok(built) => built,
        Err(error) => {
            let error = ContractError::from(error);
            finish_failed(runtime, lease.clone(), ReplyState::Retrieving, error);
            return Err(error);
        }
    };
    let model_request_id = uuid::Uuid::new_v4().simple().to_string();
    let trace_metadata =
        super::trace::M2TraceMetadata::from_audit(&built.audit, model_request_id.clone())?;
    runtime.complete_retrieval(lease, &built.context, trace_metadata)?;
    *terminal_stage = 6;
    let retrieval_outcome = if built.audit.status == crate::knowledge::types::RetrievalStatus::NoHit
    {
        AuditOutcomeCode::RetrievalNoHit
    } else if built.audit.retrieval_mode == crate::knowledge::types::RetrievalMode::FtsFallback {
        AuditOutcomeCode::RetrievalFtsFallback
    } else {
        AuditOutcomeCode::RetrievalSuccess
    };
    if let Err(error) = client
        .m2_audit_sink()
        .record(M2AuditEvent::retrieval_completed(
            lease.request_id(),
            built.audit.binding_generation,
            built.audit.context_hash.as_str().to_owned(),
            model_request_id.clone(),
            built.audit.selected_hits.len() as u8,
            retrieval_outcome,
        )?)
    {
        finish_failed(runtime, lease.clone(), ReplyState::Generating, error);
        return Err(error);
    }
    complete_m2_revalidation(
        runtime,
        lease,
        ReplyState::Generating,
        port.revalidate(super::binding::BindingStage::BeforeModelTransport)
            .await,
    )?;
    let permit = runtime.authorize_model_call(lease, &built.context, model_request_id)?;
    runtime
        .generate_m2_reply_with_client(client, config, profiles, built.context, permit, lease)
        .await
}

#[cfg(all(feature = "wechat-m2", target_os = "windows"))]
struct WindowsM2ReplyTailPort<'a> {
    app: &'a tauri::AppHandle,
    screenshot_service: crate::screenshot::ScreenshotService,
    store: &'a crate::knowledge::KnowledgeStore,
    runtime: &'a WechatReplyRuntime,
    coordinator: &'a CaptureCoordinator,
    binding: &'a super::binding::KnowledgeScopeBinding,
    binding_request: &'a mut super::binding::BindingRequestSnapshot,
}

#[cfg(all(feature = "wechat-m2", target_os = "windows"))]
#[async_trait::async_trait]
impl M2ReplyTailPort for WindowsM2ReplyTailPort<'_> {
    async fn revalidate(
        &mut self,
        stage: super::binding::BindingStage,
    ) -> Result<(), ContractError> {
        super::commands::revalidate_knowledge_binding_for_stage(
            self.app,
            self.screenshot_service.clone(),
            self.runtime,
            self.coordinator,
            self.binding,
            self.binding_request,
            stage,
        )
        .await
    }

    fn retrieval_scope(
        &self,
    ) -> (
        super::types::BindingGeneration,
        Option<String>,
        crate::knowledge::KnowledgeScope,
    ) {
        (
            self.binding_request.binding_generation,
            self.binding_request
                .resolved_scope
                .bound_conversation_key
                .clone(),
            self.binding_request.resolved_scope.knowledge_scope.clone(),
        )
    }

    async fn retrieve(
        &mut self,
        request: crate::knowledge::KnowledgeRetrieveRequest,
        audit: &dyn M2AuditSink,
    ) -> Result<crate::knowledge::types::RetrievedReply, crate::knowledge::KnowledgeError> {
        self.store
            .knowledge_retrieve_with_audit(request, Some(audit))
            .await
    }
}

#[cfg(all(feature = "wechat-m2", target_os = "windows"))]
pub(crate) async fn generate_m2_wechat_reply(
    app: tauri::AppHandle,
    state: Arc<Mutex<AppState>>,
    store: &crate::knowledge::KnowledgeStore,
    runtime: &WechatReplyRuntime,
    coordinator: &CaptureCoordinator,
    binding: &super::binding::KnowledgeScopeBinding,
) -> Result<(), ContractError> {
    let (config, profiles, knowledge, screenshot_service, data_dir) = {
        let state = state
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        (
            state.config.wechat.clone(),
            state.config.text_model_profiles.clone(),
            state.config.knowledge.clone(),
            state.screenshot_service.clone(),
            state.data_dir.clone(),
        )
    };
    let mut binding_request =
        super::commands::begin_m2_binding_request(&app, store, runtime, coordinator, binding)?;
    let lease = runtime.begin_reply(
        m2_snapshot(&binding_request),
        ReplyTraceStore::new(&data_dir),
    )?;
    let client = WechatReplyModelClient::new_with_m2_audit(&data_dir);
    let request_id = lease.request_id().clone();
    let mut terminal_stage = 2;
    let result = async {
        let cancel = runtime.cancellation_receiver(&lease)?;
        let identity = match runtime.validate_foreground_wechat() {
            Ok(identity) => identity,
            Err(error) => {
                finish_failed(runtime, lease.clone(), ReplyState::Validating, error);
                return Err(error);
            }
        };
        runtime.transition(
            &lease,
            ReplyState::Validating,
            ReplyState::Capturing,
            None,
            None,
        )?;
        terminal_stage = 3;
        let (slices, identity) = match runtime
            .capture_foreground_wechat(
                app.clone(),
                coordinator,
                screenshot_service.clone(),
                identity,
                request_id.clone(),
                CAPTURE_TIMEOUT,
                Some(cancel),
            )
            .await
        {
            Ok(result) => result,
            Err(error) => {
                finish_failed(runtime, lease.clone(), ReplyState::Capturing, error);
                return Err(error);
            }
        };
        let captured = CapturedWechat {
            request_id: request_id.clone(),
            capture_version: slices.capture_version,
            stable_message_id: "m2-capture".into(),
            is_single_chat: true,
        };
        let mut dispatcher =
            WechatOcrDispatcher::new(WindowsMemoryPrimary, super::ocr::DisabledLocalFallback);
        let mut audit_sink = M2OcrAudit {
            request_id: &request_id,
            sink: client.m2_audit_sink(),
            error: None,
        };
        let ocr = dispatcher.recognize(&slices, &identity, captured, &mut audit_sink);
        if let Some(error) = audit_sink.error {
            finish_failed(runtime, lease.clone(), ReplyState::Capturing, error);
            return Err(error);
        }
        runtime.enter_ocr_after_capture(&lease, slices.capture_version)?;
        terminal_stage = 4;
        let ocr = match ocr {
            Ok(ocr) => ocr,
            Err(error) => {
                finish_failed(runtime, lease.clone(), ReplyState::Ocr, error);
                return Err(error);
            }
        };
        if !coordinator.is_current_capture(slices.capture_version) {
            finish_failed(
                runtime,
                lease.clone(),
                ReplyState::Ocr,
                ContractError::WxRequestStale,
            );
            return Err(ContractError::WxRequestStale);
        }
        runtime.transition(&lease, ReplyState::Ocr, ReplyState::Retrieving, None, None)?;
        terminal_stage = 5;
        let mut port = WindowsM2ReplyTailPort {
            app: &app,
            screenshot_service,
            store,
            runtime,
            coordinator,
            binding,
            binding_request: &mut binding_request,
        };
        let reply = finish_captured_m2_reply(
            runtime,
            &lease,
            ocr.text().to_owned(),
            knowledge,
            config,
            profiles,
            &mut port,
            &client,
            &mut terminal_stage,
        )
        .await?;
        publish_reply_and_finish(runtime, lease.clone(), &reply, |payload| {
            avatar_engine::emit_avatar_bubble(&app, payload);
        })
    }
    .await;
    client
        .m2_audit_sink()
        .record(M2AuditEvent::upload_queue_observed_zero(&request_id)?)?;
    let (outcome, error) = match result {
        Ok(()) => (AuditOutcomeCode::TerminalSuccess, None),
        Err(ContractError::WxRequestCancelled) => (
            AuditOutcomeCode::TerminalCancelled,
            Some(ContractError::WxRequestCancelled),
        ),
        Err(error) => (AuditOutcomeCode::TerminalFailed, Some(error)),
    };
    client
        .m2_audit_sink()
        .record(M2AuditEvent::terminal_observed(
            &request_id,
            terminal_stage,
            outcome,
            error,
        )?)?;
    result
}

#[cfg(all(feature = "wechat-m2", not(target_os = "windows")))]
pub(crate) async fn generate_m2_wechat_reply(
    _app: tauri::AppHandle,
    _state: Arc<Mutex<AppState>>,
    _store: &crate::knowledge::KnowledgeStore,
    _runtime: &WechatReplyRuntime,
    _coordinator: &CaptureCoordinator,
    _binding: &super::binding::KnowledgeScopeBinding,
) -> Result<(), ContractError> {
    Err(ContractError::WxWindowUnsupported)
}

#[cfg(all(test, feature = "wechat-m1"))]
mod tests {
    use super::*;
    use crate::agent::model::{SingleTurnTextRequest, SingleTurnTextTransport};
    use crate::config::{AiProvider, ModelConfig, TextModelProfile, WechatConfig};
    use crate::error::AppError;
    use crate::wechat::types::{CapturedWechat, NormalizedOcrText, OcrBackendResult};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeTransport {
        calls: Arc<AtomicUsize>,
        reply: Option<String>,
    }

    #[async_trait]
    impl SingleTurnTextTransport for FakeTransport {
        async fn complete(
            &self,
            _model: &ModelConfig,
            _request: SingleTurnTextRequest,
        ) -> Result<String, AppError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.reply
                .clone()
                .ok_or_else(|| AppError::Unknown("fixture transport failure".into()))
        }
    }

    fn selected_model_snapshot() -> (WechatConfig, Vec<TextModelProfile>) {
        let mut config = WechatConfig::default();
        config.text_model_profile_id = Some("selected".into());
        (
            config,
            vec![TextModelProfile {
                id: "selected".into(),
                name: "fixture".into(),
                test_status: "success".into(),
                last_tested_at: None,
                last_test_message: None,
                model_config: ModelConfig {
                    provider: AiProvider::Ollama,
                    endpoint: "http://fixture".into(),
                    api_key: None,
                    model: "fixture".into(),
                },
            }],
        )
    }

    fn begin_capturing(
        runtime: &WechatReplyRuntime,
        directory: &std::path::Path,
    ) -> (ReplyLease, CaptureCoordinator) {
        let lease = runtime
            .begin_reply(m1_snapshot(), ReplyTraceStore::new(directory))
            .unwrap();
        runtime
            .transition(
                &lease,
                ReplyState::Validating,
                ReplyState::Capturing,
                None,
                None,
            )
            .unwrap();
        let coordinator = CaptureCoordinator::default();
        coordinator.next_capture_version();
        (lease, coordinator)
    }

    fn ocr_for(lease: &ReplyLease, version: CaptureVersion) -> OcrReadyReply {
        OcrReadyReply::from_backend(
            CapturedWechat {
                request_id: lease.request_id().clone(),
                capture_version: version,
                stable_message_id: "fixture".into(),
                is_single_chat: true,
            },
            OcrBackendResult::Text(NormalizedOcrText::parse("请确认").unwrap()),
        )
        .unwrap()
    }

    #[test]
    fn m1_snapshot_starts_without_capture_or_m2_binding() {
        let snapshot = m1_snapshot();
        assert_eq!(snapshot.mode, ReplyMode::M1);
        assert!(snapshot.capture_version.is_none());
        assert_eq!(snapshot.binding_generation.value(), 0);
        assert_eq!(snapshot.observation_version.value(), 0);
    }

    #[tokio::test]
    async fn captured_m1_facade_records_the_full_order_and_one_transport() {
        let directory = std::env::temp_dir().join(format!("wechat-flow-{}", uuid::Uuid::new_v4()));
        let runtime = WechatReplyRuntime::default();
        let (lease, coordinator) = begin_capturing(&runtime, &directory);
        let version = CaptureVersion::new(1);
        let calls = Arc::new(AtomicUsize::new(0));
        let client = WechatReplyModelClient::with_transport(Arc::new(FakeTransport {
            calls: calls.clone(),
            reply: Some("建议回复".into()),
        }));
        let (config, profiles) = selected_model_snapshot();

        let reply = finish_captured_m1_reply(
            &runtime,
            &coordinator,
            &lease,
            version,
            Ok(ocr_for(&lease, version)),
            config,
            profiles,
            &client,
        )
        .await
        .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(reply.is_current(lease.request_id(), lease.suggestion_generation(), None));
        let trace_file = directory
            .join("wechat_reply")
            .join("trace")
            .join(format!("{}.jsonl", chrono::Utc::now().format("%F")));
        let trace = std::fs::read_to_string(trace_file).unwrap();
        let stages: Vec<_> = trace
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .map(|entry| entry["stageName"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            stages,
            [
                "validating",
                "capturing",
                "ocr",
                "generating",
                "reply_ready"
            ]
        );
        assert!(trace.contains(&lease.request_id().to_string()));
        assert!(trace.contains("\"captureVersion\":1"));

        let request_id = lease.request_id().clone();
        let suggestion_generation = lease.suggestion_generation();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observer = observed.clone();
        let observed_runtime = runtime.clone();
        publish_reply_and_finish(&runtime, lease, &reply, move |payload| {
            assert_eq!(
                observed_runtime.request_suggestion_copy(&request_id, suggestion_generation, None,),
                Ok("建议回复".into())
            );
            observer.lock().unwrap().push(payload.kind.clone());
        })
        .unwrap();
        assert_eq!(
            observed.lock().unwrap().as_slice(),
            &[crate::avatar_engine::AvatarBubbleKind::WechatSuggestion]
        );
        assert!(runtime
            .begin_reply(m1_snapshot(), ReplyTraceStore::new(&directory))
            .is_ok());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[tokio::test]
    async fn captured_m1_facade_stops_ocr_stale_and_model_failures_without_extra_transport() {
        for (ocr, make_config, response, expected_calls, expected_error) in [
            (
                Err(ContractError::WxOcrEmpty),
                false,
                Some("unused".to_owned()),
                0,
                ContractError::WxOcrEmpty,
            ),
            (
                Err(ContractError::WxGroupChatUnsupported),
                false,
                Some("unused".to_owned()),
                0,
                ContractError::WxGroupChatUnsupported,
            ),
            (
                Ok(()),
                true,
                Some("unused".to_owned()),
                0,
                ContractError::WxTextModelUnavailable,
            ),
            (Ok(()), false, None, 1, ContractError::LlmFailed),
            (
                Ok(()),
                false,
                Some(" ".to_owned()),
                1,
                ContractError::LlmFailed,
            ),
            (
                Ok(()),
                false,
                Some("x".repeat(32_769)),
                1,
                ContractError::LlmFailed,
            ),
        ] {
            let directory =
                std::env::temp_dir().join(format!("wechat-flow-{}", uuid::Uuid::new_v4()));
            let runtime = WechatReplyRuntime::default();
            let (lease, coordinator) = begin_capturing(&runtime, &directory);
            let version = CaptureVersion::new(1);
            let calls = Arc::new(AtomicUsize::new(0));
            let client = WechatReplyModelClient::with_transport(Arc::new(FakeTransport {
                calls: calls.clone(),
                reply: response,
            }));
            let (mut config, profiles) = selected_model_snapshot();
            if make_config {
                config.text_model_profile_id = Some("missing".into());
            }
            let ocr = ocr.map(|()| ocr_for(&lease, version));

            assert_eq!(
                finish_captured_m1_reply(
                    &runtime,
                    &coordinator,
                    &lease,
                    version,
                    ocr,
                    config,
                    profiles,
                    &client,
                )
                .await,
                Err(expected_error),
            );
            assert_eq!(calls.load(Ordering::SeqCst), expected_calls);
            assert!(runtime
                .begin_reply(m1_snapshot(), ReplyTraceStore::new(&directory))
                .is_ok());
            let _ = std::fs::remove_dir_all(directory);
        }
    }

    #[tokio::test]
    async fn captured_m1_facade_rejects_an_expired_capture_before_model() {
        let directory = std::env::temp_dir().join(format!("wechat-flow-{}", uuid::Uuid::new_v4()));
        let runtime = WechatReplyRuntime::default();
        let (lease, coordinator) = begin_capturing(&runtime, &directory);
        let version = CaptureVersion::new(1);
        coordinator.next_capture_version();
        let calls = Arc::new(AtomicUsize::new(0));
        let client = WechatReplyModelClient::with_transport(Arc::new(FakeTransport {
            calls: calls.clone(),
            reply: Some("unused".into()),
        }));
        let (config, profiles) = selected_model_snapshot();

        assert_eq!(
            finish_captured_m1_reply(
                &runtime,
                &coordinator,
                &lease,
                version,
                Ok(ocr_for(&lease, version)),
                config,
                profiles,
                &client,
            )
            .await,
            Err(ContractError::WxRequestStale),
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(runtime
            .begin_reply(m1_snapshot(), ReplyTraceStore::new(&directory))
            .is_ok());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[tokio::test]
    async fn publish_tail_does_not_emit_when_publish_fails_and_releases_the_slot() {
        let directory = std::env::temp_dir().join(format!("wechat-flow-{}", uuid::Uuid::new_v4()));
        let runtime = WechatReplyRuntime::default();
        let (lease, coordinator) = begin_capturing(&runtime, &directory);
        let version = CaptureVersion::new(1);
        let client = WechatReplyModelClient::with_transport(Arc::new(FakeTransport {
            calls: Arc::new(AtomicUsize::new(0)),
            reply: Some("建议回复".into()),
        }));
        let (config, profiles) = selected_model_snapshot();
        let _reply = finish_captured_m1_reply(
            &runtime,
            &coordinator,
            &lease,
            version,
            Ok(ocr_for(&lease, version)),
            config,
            profiles,
            &client,
        )
        .await
        .unwrap();
        let stale_reply = GeneratedReply::m1(
            lease.request_id().clone(),
            crate::wechat::types::SuggestionGeneration::new(999),
            "stale".into(),
        );

        let emitted = Arc::new(AtomicUsize::new(0));
        let observer = emitted.clone();
        assert_eq!(
            publish_reply_and_finish(&runtime, lease, &stale_reply, move |_| {
                observer.fetch_add(1, Ordering::SeqCst);
            }),
            Err(ContractError::WxRequestStale)
        );
        assert_eq!(emitted.load(Ordering::SeqCst), 0);
        assert!(runtime
            .begin_reply(m1_snapshot(), ReplyTraceStore::new(&directory))
            .is_ok());
        let _ = std::fs::remove_dir_all(directory);
    }
}

#[cfg(all(test, feature = "wechat-m2"))]
mod m2_tests {
    use super::*;
    use crate::agent::model::{SingleTurnTextRequest, SingleTurnTextTransport};
    use crate::config::{AiProvider, ModelConfig, TextModelProfile, WechatConfig};
    use crate::error::AppError;
    use crate::knowledge::types::{retrieval_fixture, RetrievalOutcome, RetrievalStatus};
    use crate::wechat::trace::ReplyTraceStore;
    use crate::wechat::types::{BindingGeneration, BindingObservationVersion};
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeTransport {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl SingleTurnTextTransport for FakeTransport {
        async fn complete(
            &self,
            _model: &ModelConfig,
            _request: SingleTurnTextRequest,
        ) -> Result<String, AppError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok("建议回复".into())
        }
    }

    struct FakeTailPort {
        revalidations: VecDeque<Result<(), ContractError>>,
        retrieval: Option<
            Result<crate::knowledge::types::RetrievedReply, crate::knowledge::KnowledgeError>,
        >,
        retrieval_calls: usize,
    }

    #[async_trait]
    impl M2ReplyTailPort for FakeTailPort {
        async fn revalidate(
            &mut self,
            _stage: crate::wechat::binding::BindingStage,
        ) -> Result<(), ContractError> {
            self.revalidations.pop_front().unwrap()
        }

        fn retrieval_scope(
            &self,
        ) -> (
            BindingGeneration,
            Option<String>,
            crate::knowledge::types::KnowledgeScope,
        ) {
            (
                BindingGeneration::new(1),
                None,
                crate::knowledge::types::KnowledgeScope::GlobalUserSelected,
            )
        }

        async fn retrieve(
            &mut self,
            _request: crate::knowledge::KnowledgeRetrieveRequest,
            _audit: &dyn M2AuditSink,
        ) -> Result<crate::knowledge::types::RetrievedReply, crate::knowledge::KnowledgeError>
        {
            self.retrieval_calls += 1;
            self.retrieval.take().unwrap()
        }
    }

    fn snapshot() -> BeginReplySnapshot {
        BeginReplySnapshot {
            mode: ReplyMode::M2,
            binding_generation: BindingGeneration::new(1),
            observation_version: BindingObservationVersion::new(1),
            capture_version: None,
            timeout: Duration::from_secs(5),
        }
    }

    fn begin_retrieving(runtime: &WechatReplyRuntime, trace: ReplyTraceStore) -> ReplyLease {
        let lease = runtime.begin_reply(snapshot(), trace).unwrap();
        runtime
            .transition(
                &lease,
                ReplyState::Validating,
                ReplyState::Capturing,
                None,
                None,
            )
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Capturing, ReplyState::Ocr, None, None)
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Ocr, ReplyState::Retrieving, None, None)
            .unwrap();
        lease
    }

    fn selected_model_snapshot() -> (WechatConfig, Vec<TextModelProfile>) {
        let mut config = WechatConfig::default();
        config.text_model_profile_id = Some("selected".into());
        (
            config,
            vec![TextModelProfile {
                id: "selected".into(),
                name: "fixture".into(),
                test_status: "success".into(),
                last_tested_at: None,
                last_test_message: None,
                model_config: ModelConfig {
                    provider: AiProvider::Ollama,
                    endpoint: "http://fixture".into(),
                    api_key: None,
                    model: "fixture".into(),
                },
            }],
        )
    }

    #[test]
    fn m2_ocr_adapter_records_explicit_zero_or_local_process_start() {
        use crate::wechat::observability::{M2AuditKind, SpyM2AuditSink};

        let request_id = crate::wechat::types::RequestId::new();
        let sink = SpyM2AuditSink::default();
        let mut adapter = M2OcrAudit {
            request_id: &request_id,
            sink: &sink,
            error: None,
        };
        adapter.record(WechatOcrAuditEvent {
            request_id_hash: request_id.audit_tag(),
            capture_version: 1,
            stage: "ocr",
            outcome: "text",
            provider: "WindowsOCR",
        });
        adapter.record(WechatOcrAuditEvent {
            request_id_hash: request_id.audit_tag(),
            capture_version: 1,
            stage: "ocr",
            outcome: "text",
            provider: "LocalFallback",
        });
        assert!(adapter.error.is_none());
        let events = sink.snapshot();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].kind(), M2AuditKind::OcrLocalProcessStarted);
    }

    #[tokio::test]
    async fn m2_tail_revalidation_failures_stop_transport_and_release_the_lease() {
        for (revalidations, expected_retrieval_calls, error) in [
            (
                VecDeque::from([Err(ContractError::WxNotForeground)]),
                0,
                ContractError::WxNotForeground,
            ),
            (
                VecDeque::from([Ok(()), Err(ContractError::WxCaptureFailed)]),
                1,
                ContractError::WxCaptureFailed,
            ),
        ] {
            let directory =
                std::env::temp_dir().join(format!("wechat-m2-flow-{}", uuid::Uuid::new_v4()));
            let trace = ReplyTraceStore::new(&directory);
            let runtime = WechatReplyRuntime::default();
            let lease = begin_retrieving(&runtime, trace.clone());
            let calls = Arc::new(AtomicUsize::new(0));
            let client = WechatReplyModelClient::with_transport(Arc::new(FakeTransport {
                calls: calls.clone(),
            }));
            let (config, profiles) = selected_model_snapshot();
            let mut port = FakeTailPort {
                revalidations,
                retrieval: Some(Ok(retrieval_fixture(
                    lease.request_id().clone(),
                    "请确认",
                    RetrievalOutcome::Retrieved(RetrievalStatus::NoHit),
                    &[],
                    512,
                )
                .unwrap())),
                retrieval_calls: 0,
            };
            let mut terminal_stage = 5;

            assert_eq!(
                finish_captured_m2_reply(
                    &runtime,
                    &lease,
                    "请确认".into(),
                    crate::config::KnowledgeConfig::default(),
                    config,
                    profiles,
                    &mut port,
                    &client,
                    &mut terminal_stage,
                )
                .await,
                Err(error)
            );
            assert_eq!(port.retrieval_calls, expected_retrieval_calls);
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            let trace_file = directory
                .join("wechat_reply")
                .join("trace")
                .join(format!("{}.jsonl", chrono::Utc::now().format("%F")));
            let entries = std::fs::read_to_string(trace_file).unwrap();
            let terminal = entries
                .lines()
                .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
                .filter(|entry| entry["status"] == "terminal")
                .collect::<Vec<_>>();
            assert_eq!(terminal.len(), 1);
            assert_eq!(terminal[0]["finalState"], "failed");
            assert_eq!(terminal[0]["errorCode"], serde_json::json!(error));
            assert_eq!(terminal[0]["logicalModelRequests"], 0);
            assert!(runtime.begin_reply(snapshot(), trace).is_ok());
            let _ = std::fs::remove_dir_all(directory);
        }
    }

    #[tokio::test]
    async fn m2_tail_runs_both_gates_one_retrieval_and_one_logical_model_request() {
        let directory =
            std::env::temp_dir().join(format!("wechat-m2-flow-{}", uuid::Uuid::new_v4()));
        let trace = ReplyTraceStore::new(&directory);
        let runtime = WechatReplyRuntime::default();
        let lease = begin_retrieving(&runtime, trace);
        let calls = Arc::new(AtomicUsize::new(0));
        let client = WechatReplyModelClient::with_transport(Arc::new(FakeTransport {
            calls: calls.clone(),
        }));
        let (config, profiles) = selected_model_snapshot();
        let mut port = FakeTailPort {
            revalidations: VecDeque::from([Ok(()), Ok(())]),
            retrieval: Some(Ok(retrieval_fixture(
                lease.request_id().clone(),
                "请确认",
                RetrievalOutcome::Retrieved(RetrievalStatus::NoHit),
                &[],
                512,
            )
            .unwrap())),
            retrieval_calls: 0,
        };
        let mut terminal_stage = 5;

        let reply = finish_captured_m2_reply(
            &runtime,
            &lease,
            "请确认".into(),
            crate::config::KnowledgeConfig::default(),
            config,
            profiles,
            &mut port,
            &client,
            &mut terminal_stage,
        )
        .await
        .unwrap();
        assert_eq!(port.retrieval_calls, 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(reply.is_current(
            lease.request_id(),
            lease.suggestion_generation(),
            Some(BindingGeneration::new(1))
        ));
        runtime.finish_reply(lease).unwrap();
        let _ = std::fs::remove_dir_all(directory);
    }
}
