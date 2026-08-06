use super::config::{model_profile_is_available, profile_is_trusted, profile_options, CompatibilityProfileOption};
use super::WechatReplyRuntime;
use crate::config::WechatConfig;
use crate::error::AppError;
use crate::AppState;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WechatSettingsStatus {
    catalog_options: Vec<CompatibilityProfileOption>,
    selected_profile_valid: bool,
    selected_model_valid: bool,
    auto_trigger: bool,
    content_retention_enabled: bool,
    content_retention_days: u16,
    not_ready_reason: Option<&'static str>,
}

#[tauri::command]
pub(crate) async fn get_wechat_settings_status(
    _runtime: State<'_, WechatReplyRuntime>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<WechatSettingsStatus, AppError> {
    let (config, profiles) = {
        let state = state.lock().map_err(|error| AppError::Unknown(error.to_string()))?;
        (state.config.wechat.clone(), state.config.text_model_profiles.clone())
    };
    Ok(status_for(&config, &profiles))
}

fn status_for(config: &WechatConfig, profiles: &[crate::config::TextModelProfile]) -> WechatSettingsStatus {
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
        let status = status_for(&config, &[]);
        assert!(!status.selected_profile_valid);
        assert_eq!(status.not_ready_reason, Some("WX_PROFILE_UNSUPPORTED"));
        assert!(!status.auto_trigger);
    }
}
