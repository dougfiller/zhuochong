use crate::config::{TextModelProfile, WechatConfig};
use serde::Serialize;

#[derive(Clone, Copy)]
struct CompatibilityProfile {
    id: &'static str,
    label: &'static str,
    version: &'static str,
    signature_material: &'static str,
}

const COMPATIBILITY_CATALOG: &[CompatibilityProfile] = &[CompatibilityProfile {
    id: "wechat-windows-v1",
    label: "WeChat for Windows (prepared)",
    version: "1",
    // The catalog, not config.json, owns the verification material and window constraints.
    signature_material: "catalog-only-v1",
}];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompatibilityProfileOption {
    pub id: String,
    pub label: String,
    pub version: String,
}

pub(crate) fn profile_options() -> Vec<CompatibilityProfileOption> {
    COMPATIBILITY_CATALOG
        .iter()
        .map(|profile| CompatibilityProfileOption {
            id: profile.id.to_string(),
            label: profile.label.to_string(),
            version: profile.version.to_string(),
        })
        .collect()
}

pub(crate) fn profile_is_trusted(id: Option<&str>) -> bool {
    id.is_some_and(|id| {
        COMPATIBILITY_CATALOG.iter().any(|profile| {
            profile.id == id && !profile.signature_material.trim().is_empty()
        })
    })
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
