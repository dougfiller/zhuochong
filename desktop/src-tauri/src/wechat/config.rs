use crate::config::{TextModelProfile, WechatConfig};
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
    let Some(id) = config.text_model_profile_id.as_deref() else {
        return false;
    };
    profiles.iter().any(|profile| {
        profile.id == id
            && profile.test_status.eq_ignore_ascii_case("success")
            && !profile.model_config.model.trim().is_empty()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unprobed_embedded_catalog_exposes_no_selectable_profile() {
        assert!(profile_options().is_empty());
        assert!(!profile_is_trusted(Some("wechat-windows-v1")));
    }
}
