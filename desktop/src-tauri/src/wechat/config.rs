use crate::config::{ModelConfig, TextModelProfile, WechatConfig};
use super::types::ContractError;
use super::profiles::CompatibilityCatalog;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompatibilityProfileOption {
    pub id: String,
    pub label: String,
    pub version: String,
}

pub(crate) fn profile_options() -> Vec<CompatibilityProfileOption> {
    let Ok(catalog) = CompatibilityCatalog::embedded() else {
        return Vec::new();
    };
    catalog
        .enabled_profiles()
        .map(|profile| CompatibilityProfileOption {
            id: profile.id.to_string(),
            label: format!("WeChat for Windows ({})", profile.id),
            version: profile.profile_version.to_string(),
        })
        .collect()
}

pub(crate) fn profile_is_trusted(id: Option<&str>) -> bool {
    let Ok(catalog) = CompatibilityCatalog::embedded() else {
        return false;
    };
    id.is_some_and(|id| catalog.enabled_profiles().any(|profile| profile.id == id))
}

pub(crate) fn model_profile_is_available(config: &WechatConfig, profiles: &[TextModelProfile]) -> bool {
    selected_verified_model(config, profiles).is_ok()
}

pub(crate) fn selected_verified_model(
    config: &WechatConfig,
    profiles: &[TextModelProfile],
) -> Result<ModelConfig, ContractError> {
    let Some(id) = config.text_model_profile_id.as_deref() else {
        return Err(ContractError::WxTextModelUnavailable);
    };
    profiles.iter()
        .find(|profile| profile.id == id
            && profile.test_status.eq_ignore_ascii_case("success")
            && !profile.model_config.model.trim().is_empty())
        .map(|profile| profile.model_config.clone())
        .ok_or(ContractError::WxTextModelUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AiProvider, ModelConfig};

    #[test]
    fn unprobed_embedded_catalog_exposes_no_selectable_profile() {
        assert!(profile_options().is_empty());
        assert!(!profile_is_trusted(Some("wechat-windows-v1")));
    }

    #[test]
    fn selected_model_requires_the_exact_successful_nonempty_profile() {
        let mut config = WechatConfig::default();
        config.text_model_profile_id = Some("selected".into());
        let profile = TextModelProfile {
            id: "selected".into(), name: "fixture".into(), test_status: "success".into(),
            last_tested_at: None, last_test_message: None,
            model_config: ModelConfig { provider: AiProvider::Ollama, endpoint: "http://fixture".into(), api_key: None, model: "fixture".into() },
        };
        assert_eq!(selected_verified_model(&config, &[profile.clone()]).unwrap().model, "fixture");

        let mut untested = profile.clone();
        untested.test_status = "untested".into();
        assert!(matches!(selected_verified_model(&config, &[untested]), Err(ContractError::WxTextModelUnavailable)));
        config.text_model_profile_id = Some("other".into());
        assert!(matches!(selected_verified_model(&config, &[profile]), Err(ContractError::WxTextModelUnavailable)));
    }
}
