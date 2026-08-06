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

    #[test]
    fn unknown_profile_fails_closed() {
        let mut config = WechatConfig::default();
        config.compatibility_profile_id = Some("tampered-profile".into());
        let status = status_for(&config, &[], "idle");
        assert!(!status.selected_profile_valid);
        assert_eq!(status.not_ready_reason, Some("WX_PROFILE_UNSUPPORTED"));
        assert!(!status.auto_trigger);
    }
}
