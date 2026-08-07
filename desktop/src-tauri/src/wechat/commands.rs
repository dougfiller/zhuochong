use super::config::{
    model_profile_is_available, profile_is_trusted, profile_options, CompatibilityProfileOption,
};
use super::content::{ContentDeleteResult, WechatContentStore};
use super::trace::{ReplyTraceStore, TraceQuery, WechatReplyTracePage};
use super::WechatReplyRuntime;
use crate::avatar_engine::{self, AvatarBubblePayload};
use crate::config::WechatConfig;
use crate::error::AppError;
use crate::AppState;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BindingCasInput {
    session_nonce: String,
    expected_binding_generation: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RequestedKnowledgeScopeInput {
    kind: String,
    #[serde(default)]
    keys: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConfirmKnowledgeScopeBindingInput {
    session_nonce: String,
    expected_binding_generation: u64,
    expected_observation_version: u64,
    expected_catalog_generation: u64,
    scope: RequestedKnowledgeScopeInput,
    header_confirmed: bool,
    global_confirmed: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ConfirmScopeForRequestInput {
    session_nonce: String,
    expected_binding_generation: u64,
    expected_observation_version: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OneShotConfirmationReceipt {
    one_shot_epoch: u64,
}

#[tauri::command]
pub(crate) async fn get_knowledge_scope_binding_status(
    binding: State<'_, super::binding::KnowledgeScopeBinding>,
) -> Result<super::binding::KnowledgeScopeBindingStatus, AppError> {
    binding.status().map_err(contract_error)
}

#[tauri::command]
pub(crate) async fn begin_knowledge_scope_observation(
    input: BindingCasInput,
    app: AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
    runtime: State<'_, WechatReplyRuntime>,
    coordinator: State<'_, super::CaptureCoordinator>,
    binding: State<'_, super::binding::KnowledgeScopeBinding>,
) -> Result<super::binding::HeaderObservationResponse, AppError> {
    let status = binding.status().map_err(contract_error)?;
    if status.session_nonce != input.session_nonce
        || status.binding_generation != input.expected_binding_generation
    {
        return Err(contract_error(super::types::ContractError::WxRequestStale));
    }
    let screenshot_service = state
        .lock()
        .map_err(|error| AppError::Unknown(error.to_string()))?
        .screenshot_service
        .clone();
    let identity = runtime
        .validate_foreground_wechat()
        .map_err(contract_error)?;
    let (header, current) = runtime
        .capture_header_identity_observation(
            app.clone(),
            &coordinator,
            screenshot_service,
            identity,
            std::time::Duration::from_secs(5),
        )
        .await
        .map_err(contract_error)?;
    let (response, mutation) = binding
        .record_observation(
            &input.session_nonce,
            input.expected_binding_generation,
            super::binding::BindingWindowIdentity::from(&current),
            header,
        )
        .map_err(contract_error)?;
    if let Some(mutation) = mutation {
        apply_binding_mutation(&app, &runtime, &coordinator, mutation).map_err(contract_error)?;
    }
    Ok(response)
}

#[allow(dead_code)]
pub(crate) fn begin_m2_binding_request(
    app: &AppHandle,
    store: &crate::knowledge::KnowledgeStore,
    runtime: &WechatReplyRuntime,
    coordinator: &super::CaptureCoordinator,
    binding: &super::binding::KnowledgeScopeBinding,
) -> Result<super::binding::BindingRequestSnapshot, super::types::ContractError> {
    binding
        .begin_m2_request(store)
        .map_err(|failure| apply_binding_failure(app, runtime, coordinator, failure))
}

#[allow(dead_code)]
pub(crate) async fn revalidate_knowledge_binding_for_stage(
    app: &AppHandle,
    screenshot_service: crate::screenshot::ScreenshotService,
    runtime: &WechatReplyRuntime,
    coordinator: &super::CaptureCoordinator,
    binding: &super::binding::KnowledgeScopeBinding,
    request: &mut super::binding::BindingRequestSnapshot,
    stage: super::binding::BindingStage,
) -> Result<super::types::BindingObservationVersion, super::types::ContractError> {
    let identity = runtime.validate_foreground_wechat()?;
    let (header, current) = runtime
        .capture_header_identity_observation(
            app.clone(),
            coordinator,
            screenshot_service,
            identity,
            std::time::Duration::from_secs(5),
        )
        .await?;
    let observation = binding
        .record_stage_observation(
            request,
            stage,
            super::binding::BindingWindowIdentity::from(&current),
            header,
        )
        .map_err(|failure| apply_binding_failure(app, runtime, coordinator, failure))?;
    runtime.update_m2_observation(request.binding_generation, observation)?;
    Ok(observation)
}

#[tauri::command]
pub(crate) async fn confirm_knowledge_scope_binding(
    input: ConfirmKnowledgeScopeBindingInput,
    app: AppHandle,
    store: State<'_, crate::knowledge::KnowledgeStore>,
    runtime: State<'_, WechatReplyRuntime>,
    coordinator: State<'_, super::CaptureCoordinator>,
    binding: State<'_, super::binding::KnowledgeScopeBinding>,
) -> Result<super::binding::KnowledgeScopeBindingStatus, AppError> {
    let requested_scope = match input.scope.kind.as_str() {
        "conversation" if input.scope.keys.len() == 1 => {
            crate::knowledge::store::RequestedScopeKeys::Conversation(input.scope.keys[0].clone())
        }
        "selected_conversations" => {
            crate::knowledge::store::RequestedScopeKeys::Selected(input.scope.keys)
        }
        "global_user_selected" if input.scope.keys.is_empty() => {
            crate::knowledge::store::RequestedScopeKeys::GlobalUserSelected
        }
        _ => {
            return Err(contract_error(
                super::types::ContractError::KbScopeUnresolved,
            ))
        }
    };
    let (status, mutation) = binding
        .confirm_binding(
            &store,
            super::binding::ConfirmBindingInput {
                session_nonce: input.session_nonce,
                expected_binding_generation: input.expected_binding_generation,
                expected_observation_version: input.expected_observation_version,
                expected_catalog_generation: input.expected_catalog_generation,
                requested_scope,
                header_confirmed: input.header_confirmed,
                global_confirmed: input.global_confirmed,
            },
        )
        .map_err(contract_error)?;
    apply_binding_mutation(&app, &runtime, &coordinator, mutation).map_err(contract_error)?;
    Ok(status)
}

#[tauri::command]
pub(crate) async fn confirm_knowledge_scope_for_next_request(
    input: ConfirmScopeForRequestInput,
    binding: State<'_, super::binding::KnowledgeScopeBinding>,
) -> Result<OneShotConfirmationReceipt, AppError> {
    binding
        .confirm_scope_for_next_request(
            &input.session_nonce,
            input.expected_binding_generation,
            input.expected_observation_version,
        )
        .map(|one_shot_epoch| OneShotConfirmationReceipt { one_shot_epoch })
        .map_err(contract_error)
}

#[tauri::command]
pub(crate) async fn clear_knowledge_scope_binding(
    input: BindingCasInput,
    app: AppHandle,
    runtime: State<'_, WechatReplyRuntime>,
    coordinator: State<'_, super::CaptureCoordinator>,
    binding: State<'_, super::binding::KnowledgeScopeBinding>,
) -> Result<super::binding::KnowledgeScopeBindingStatus, AppError> {
    let (status, mutation) = binding
        .clear(&input.session_nonce, input.expected_binding_generation)
        .map_err(contract_error)?;
    apply_binding_mutation(&app, &runtime, &coordinator, mutation).map_err(contract_error)?;
    Ok(status)
}

fn apply_binding_mutation(
    app: &AppHandle,
    runtime: &WechatReplyRuntime,
    coordinator: &super::CaptureCoordinator,
    mutation: super::binding::BindingMutation,
) -> Result<(), super::types::ContractError> {
    let payload = compensate_binding_mutation(runtime, coordinator, mutation)?;
    if let Some(payload) = payload {
        avatar_engine::emit_avatar_bubble(app, &payload);
    }
    Ok(())
}

fn compensate_binding_mutation(
    runtime: &WechatReplyRuntime,
    coordinator: &super::CaptureCoordinator,
    mutation: super::binding::BindingMutation,
) -> Result<Option<AvatarBubblePayload>, super::types::ContractError> {
    coordinator.invalidate_current_capture()?;
    let payload = runtime.invalidate_m2_binding(mutation.old_generation)?;
    log::info!(
        "knowledge_binding_mutation reason={:?} old_generation={} new_generation={} clear_checked=true",
        mutation.reason,
        mutation.old_generation.value(),
        mutation.new_generation.value()
    );
    Ok(payload)
}

fn apply_binding_failure(
    app: &AppHandle,
    runtime: &WechatReplyRuntime,
    coordinator: &super::CaptureCoordinator,
    failure: super::binding::BindingFailure,
) -> super::types::ContractError {
    if let Some(mutation) = failure.mutation {
        if let Err(error) = apply_binding_mutation(app, runtime, coordinator, mutation) {
            return error;
        }
    }
    failure.error
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TraceQueryInput {
    request_id: Option<String>,
    occurred_after: Option<String>,
    occurred_before: Option<String>,
    cursor: Option<String>,
    limit: Option<u16>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct WechatSuggestionActionInput {
    request_id: String,
    suggestion_generation: u64,
    binding_generation: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WechatSuggestionCopyResult {
    text: String,
}

impl WechatSuggestionActionInput {
    fn versions(
        &self,
    ) -> Result<
        (
            super::types::RequestId,
            super::types::SuggestionGeneration,
            Option<super::types::BindingGeneration>,
        ),
        super::types::ContractError,
    > {
        if self.suggestion_generation == 0 || self.binding_generation == Some(0) {
            return Err(super::types::ContractError::WxTraceInvalidQuery);
        }
        Ok((
            super::types::RequestId::parse(&self.request_id)?,
            super::types::SuggestionGeneration::new(self.suggestion_generation),
            self.binding_generation
                .map(super::types::BindingGeneration::new),
        ))
    }

    fn clear_payload(&self) -> AvatarBubblePayload {
        AvatarBubblePayload::clear_wechat_suggestion(
            self.request_id.clone(),
            self.suggestion_generation,
            self.binding_generation,
        )
    }
}

#[tauri::command]
pub(crate) async fn request_wechat_suggestion_copy(
    input: WechatSuggestionActionInput,
    runtime: State<'_, WechatReplyRuntime>,
) -> Result<WechatSuggestionCopyResult, AppError> {
    let (request_id, suggestion_generation, binding_generation) =
        input.versions().map_err(contract_error)?;
    runtime
        .request_suggestion_copy(&request_id, suggestion_generation, binding_generation)
        .map(|text| WechatSuggestionCopyResult { text })
        .map_err(contract_error)
}

#[tauri::command]
pub(crate) async fn confirm_wechat_suggestion_copy(
    input: WechatSuggestionActionInput,
    runtime: State<'_, WechatReplyRuntime>,
    app: AppHandle,
) -> Result<(), AppError> {
    let (request_id, suggestion_generation, binding_generation) =
        input.versions().map_err(contract_error)?;
    runtime
        .confirm_suggestion_copy(&request_id, suggestion_generation, binding_generation)
        .map_err(contract_error)?;
    avatar_engine::emit_avatar_bubble(&app, &input.clear_payload());
    Ok(())
}

#[tauri::command]
pub(crate) async fn dismiss_wechat_suggestion(
    input: WechatSuggestionActionInput,
    runtime: State<'_, WechatReplyRuntime>,
    app: AppHandle,
) -> Result<(), AppError> {
    let (request_id, suggestion_generation, binding_generation) =
        input.versions().map_err(contract_error)?;
    runtime
        .dismiss_suggestion(&request_id, suggestion_generation, binding_generation)
        .map_err(contract_error)?;
    avatar_engine::emit_avatar_bubble(&app, &input.clear_payload());
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WechatSettingsStatus {
    catalog_options: Vec<CompatibilityProfileOption>,
    selected_profile_valid: bool,
    selected_model_valid: bool,
    auto_trigger: bool,
    request_phase: &'static str,
    content_retention_enabled: bool,
    content_retention_days: u16,
    not_ready_reason: Option<&'static str>,
}

#[tauri::command]
pub(crate) async fn get_wechat_settings_status(
    runtime: State<'_, WechatReplyRuntime>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<WechatSettingsStatus, AppError> {
    let (config, profiles) = {
        let state = state
            .lock()
            .map_err(|error| AppError::Unknown(error.to_string()))?;
        (
            state.config.wechat.clone(),
            state.config.text_model_profiles.clone(),
        )
    };
    Ok(status_for(&config, &profiles, runtime.request_phase()))
}

/// The sole user-initiated M1 entry point. It deliberately accepts no frontend
/// data, so profile, window identity, capture scope, and model choice remain
/// in trusted backend state.
#[tauri::command]
pub(crate) async fn generate_wechat_reply(
    app: AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
    runtime: State<'_, WechatReplyRuntime>,
    coordinator: State<'_, super::CaptureCoordinator>,
) -> Result<(), AppError> {
    #[cfg(feature = "wechat-m1")]
    {
        super::reply_flow::generate_wechat_reply(app, state.inner().clone(), &runtime, &coordinator)
            .await
            .map_err(contract_error)
    }
    #[cfg(not(feature = "wechat-m1"))]
    {
        let _ = (app, state, runtime, coordinator);
        Err(contract_error(
            super::types::ContractError::WxWindowUnsupported,
        ))
    }
}

/// Read-only metadata query. The input and output intentionally have no content
/// body, data-directory path, error detail, or filesystem handle fields.
#[tauri::command]
pub(crate) async fn list_wechat_reply_traces(
    input: TraceQueryInput,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<WechatReplyTracePage, AppError> {
    let data_dir = state
        .lock()
        .map_err(|error| AppError::Unknown(error.to_string()))?
        .data_dir
        .clone();
    let request_id = input
        .request_id
        .as_deref()
        .map(super::types::RequestId::parse)
        .transpose()
        .map_err(contract_error)?;
    let occurred_after =
        parse_timestamp(input.occurred_after.as_deref()).map_err(contract_error)?;
    let occurred_before =
        parse_timestamp(input.occurred_before.as_deref()).map_err(contract_error)?;
    ReplyTraceStore::new(data_dir)
        .list(TraceQuery {
            request_id,
            occurred_after,
            occurred_before,
            cursor: input.cursor,
            limit: input.limit.unwrap_or(50),
        })
        .map_err(contract_error)
}

#[tauri::command]
pub(crate) async fn delete_wechat_reply_content(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<ContentDeleteResult, AppError> {
    let data_dir = state
        .lock()
        .map_err(|error| AppError::Unknown(error.to_string()))?
        .data_dir
        .clone();
    WechatContentStore::new(data_dir)
        .delete_all()
        .map_err(contract_error)
}

fn parse_timestamp(
    value: Option<&str>,
) -> Result<Option<DateTime<Utc>>, super::types::ContractError> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|time| time.with_timezone(&Utc))
                .map_err(|_| super::types::ContractError::WxTraceInvalidQuery)
        })
        .transpose()
}

fn contract_error(error: super::types::ContractError) -> AppError {
    AppError::Unknown(
        serde_json::to_string(&error).unwrap_or_else(|_| "\"WX_CONTRACT_VIOLATION\"".into()),
    )
}

fn status_for(
    config: &WechatConfig,
    profiles: &[crate::config::TextModelProfile],
    request_phase: &'static str,
) -> WechatSettingsStatus {
    let selected_profile_valid = profile_is_trusted(config.compatibility_profile_id.as_deref());
    let selected_model_valid = model_profile_is_available(config, profiles);
    let not_ready_reason = if !selected_profile_valid {
        Some("WX_PROFILE_UNSUPPORTED")
    } else if !selected_model_valid {
        Some("WX_TEXT_MODEL_UNAVAILABLE")
    } else {
        Some("WX_NOT_READY")
    };
    WechatSettingsStatus {
        catalog_options: profile_options(),
        selected_profile_valid,
        selected_model_valid,
        auto_trigger: false,
        request_phase,
        content_retention_enabled: config.content_retention_enabled,
        content_retention_days: config.content_retention_days,
        not_ready_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wechat::runtime::BeginReplySnapshot;
    use crate::wechat::state_machine::ReplyMode;
    use crate::wechat::types::{BindingGeneration, BindingObservationVersion};
    use std::time::Duration;

    #[test]
    fn unknown_profile_fails_closed() {
        let mut config = WechatConfig::default();
        config.compatibility_profile_id = Some("tampered-profile".into());
        let status = status_for(&config, &[], "idle");
        assert!(!status.selected_profile_valid);
        assert_eq!(status.not_ready_reason, Some("WX_PROFILE_UNSUPPORTED"));
        assert!(!status.auto_trigger);
    }

    #[test]
    fn binding_compensation_invalidates_capture_m2_lease_and_exact_suggestion_only() {
        let coordinator = super::super::CaptureCoordinator::default();
        let capture = coordinator.next_capture_version();
        let binding_generation = BindingGeneration::new(9);
        let runtime = WechatReplyRuntime::default();
        let trace_dir = std::env::temp_dir().join(format!(
            "wechat-binding-compensation-{}",
            uuid::Uuid::new_v4()
        ));
        runtime
            .begin_reply(
                BeginReplySnapshot {
                    mode: ReplyMode::M2,
                    binding_generation,
                    observation_version: BindingObservationVersion::new(3),
                    capture_version: Some(capture),
                    timeout: Duration::from_secs(5),
                },
                ReplyTraceStore::new(&trace_dir),
            )
            .unwrap();
        let (request_id, suggestion_generation) =
            runtime.install_presented_suggestion_fixture(Some(binding_generation));
        let payload = compensate_binding_mutation(
            &runtime,
            &coordinator,
            super::super::binding::BindingMutation {
                old_generation: binding_generation,
                new_generation: BindingGeneration::new(10),
                reason: super::super::binding::BindingMutationReason::HeaderChanged,
            },
        )
        .unwrap()
        .unwrap();

        assert!(!coordinator.is_current_capture(capture));
        assert_eq!(runtime.request_phase(), "idle");
        let request_id_text = request_id.to_string();
        assert_eq!(
            payload.request_id.as_deref(),
            Some(request_id_text.as_str())
        );
        assert_eq!(
            payload.suggestion_generation,
            Some(suggestion_generation.value())
        );
        assert_eq!(payload.binding_generation, Some(binding_generation.value()));
        assert!(payload.clear);
        assert_eq!(
            runtime.request_suggestion_copy(
                &request_id,
                suggestion_generation,
                Some(binding_generation)
            ),
            Err(super::super::types::ContractError::WxRequestStale)
        );

        let m1_runtime = WechatReplyRuntime::default();
        let (m1_request, m1_suggestion) = m1_runtime.install_presented_suggestion_fixture(None);
        assert!(compensate_binding_mutation(
            &m1_runtime,
            &super::super::CaptureCoordinator::default(),
            super::super::binding::BindingMutation {
                old_generation: binding_generation,
                new_generation: BindingGeneration::new(10),
                reason: super::super::binding::BindingMutationReason::ActiveCatalogChanged,
            },
        )
        .unwrap()
        .is_none());
        assert_eq!(
            m1_runtime
                .request_suggestion_copy(&m1_request, m1_suggestion, None)
                .unwrap(),
            "虚构建议"
        );
        let _ = std::fs::remove_dir_all(trace_dir);
    }
}
