use super::model_client::WechatReplyModelClient;
#[cfg(target_os = "windows")]
use super::ocr::{
    WechatOcrAuditEvent, WechatOcrAuditSink, WechatOcrDispatcher, WindowsMemoryPrimary,
};
use super::runtime::{BeginReplySnapshot, CaptureCoordinator, ReplyLease, WechatReplyRuntime};
use super::state_machine::ReplyMode;
use super::state_machine::ReplyState;
#[cfg(any(target_os = "windows", test))]
use super::trace::ReplyTraceStore;
#[cfg(target_os = "windows")]
use super::types::CapturedWechat;
use super::types::{
    BindingGeneration, BindingObservationVersion, CaptureVersion, ContractError, GeneratedReply,
    M1ReplyInput, OcrReadyReply,
};
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

/// Finishes the platform-independent tail of the M1 facade after guarded
/// capture has supplied its lease-bound version and OCR result. Keeping this
/// seam private lets the exact Windows-only front half remain small while the
/// stateful hand-off is exercised with a controlled transport on every host.
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
fn publish_m1_reply_and_finish(
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
#[cfg(target_os = "windows")]
pub(crate) async fn generate_wechat_reply(
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
    publish_m1_reply_and_finish(runtime, lease, &reply, |payload| {
        avatar_engine::emit_avatar_bubble(&app, payload);
    })
}

#[cfg(not(target_os = "windows"))]
pub(crate) async fn generate_wechat_reply(
    _app: tauri::AppHandle,
    _state: Arc<Mutex<AppState>>,
    _runtime: &WechatReplyRuntime,
    _coordinator: &CaptureCoordinator,
) -> Result<(), ContractError> {
    Err(ContractError::WxWindowUnsupported)
}

#[cfg(test)]
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
        publish_m1_reply_and_finish(&runtime, lease, &reply, move |payload| {
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
            publish_m1_reply_and_finish(&runtime, lease, &stale_reply, move |_| {
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
