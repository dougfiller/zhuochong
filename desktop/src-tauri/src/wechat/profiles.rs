use serde::Deserialize;
use std::collections::HashSet;

const EMBEDDED_CATALOG: &str = include_str!("profiles/windows-wechat-v1.json");
const SUPPORTED_CATALOG_VERSION: &str = "windows-wechat-v1";
const SUPPORTED_PROFILE_VERSION: &str = "1";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompatibilityProfile {
    pub(crate) id: String,
    enabled: bool,
    pub(crate) profile_version: String,
    pub(crate) wechat_product_version: String,
    pub(crate) windows_build: String,
    pub(crate) theme: String,
    pub(crate) display_topology: DisplayTopology,
    pub(crate) executable: ExecutableProfile,
    pub(crate) dpi: u32,
    pub(crate) window_size_px: WindowSize,
    pub(crate) chat_roi: NormalizedRoi,
    pub(crate) header_identity_roi: NormalizedRoi,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplayTopology {
    pub(crate) monitors: u32,
    pub(crate) target_monitor: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExecutableProfile {
    pub(crate) file_name: String,
    pub(crate) normalized_paths: Vec<String>,
    pub(crate) sha256: String,
    pub(crate) product_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WindowSize {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) tolerance_px: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NormalizedRoi {
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) right: f64,
    pub(crate) bottom: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CompatibilityCatalog {
    catalog_version: String,
    profiles: Vec<CompatibilityProfile>,
}

impl CompatibilityCatalog {
    pub(crate) fn embedded() -> Result<Self, CatalogError> {
        Self::parse(EMBEDDED_CATALOG)
    }

    pub(crate) fn parse(source: &str) -> Result<Self, CatalogError> {
        let raw: RawCatalog = serde_json::from_str(source).map_err(|_| CatalogError::InvalidCatalog)?;
        if raw.schema_version != 1 || raw.catalog_version != SUPPORTED_CATALOG_VERSION {
            return Err(CatalogError::InvalidCatalog);
        }

        let mut ids = HashSet::new();
        let mut profiles = Vec::with_capacity(raw.profiles.len());
        for raw_profile in raw.profiles {
            let profile = CompatibilityProfile::try_from(raw_profile)?;
            if !ids.insert(profile.id.clone()) {
                return Err(CatalogError::InvalidCatalog);
            }
            profiles.push(profile);
        }

        Ok(Self { catalog_version: raw.catalog_version, profiles })
    }

    pub(crate) fn enabled_profiles(&self) -> impl Iterator<Item = &CompatibilityProfile> {
        self.profiles.iter().filter(|profile| profile.enabled)
    }

    pub(crate) fn catalog_version(&self) -> &str {
        &self.catalog_version
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CatalogError {
    InvalidCatalog,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCatalog {
    schema_version: u32,
    catalog_version: String,
    profiles: Vec<RawProfile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    id: String,
    enabled: bool,
    profile_version: String,
    wechat_product_version: String,
    theme: String,
    display_topology: RawDisplayTopology,
    executable: RawExecutableProfile,
    dpi: u32,
    window_size_px: RawWindowSize,
    chat_roi: RawNormalizedRoi,
    header_identity_roi: RawNormalizedRoi,
    probe_evidence: RawProbeEvidence,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDisplayTopology {
    monitors: u32,
    target_monitor: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExecutableProfile {
    file_name: String,
    normalized_paths: Vec<String>,
    sha256: String,
    product_version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWindowSize {
    width: u32,
    height: u32,
    tolerance_px: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNormalizedRoi {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProbeEvidence {
    probe_id: String,
    validated_at: String,
    windows_build: String,
    evidence_sha256: String,
}

impl TryFrom<RawProfile> for CompatibilityProfile {
    type Error = CatalogError;

    fn try_from(raw: RawProfile) -> Result<Self, Self::Error> {
        if blank(&raw.id)
            || raw.profile_version != SUPPORTED_PROFILE_VERSION
            || blank(&raw.wechat_product_version)
            || !matches!(raw.theme.as_str(), "light" | "dark")
            || raw.display_topology.monitors == 0
            || blank(&raw.display_topology.target_monitor)
            || blank(&raw.executable.file_name)
            || raw.executable.normalized_paths.is_empty()
            || raw.executable.normalized_paths.iter().any(|path| blank(path))
            || !is_sha256(&raw.executable.sha256)
            || blank(&raw.executable.product_version)
            || raw.dpi == 0
            || raw.window_size_px.width == 0
            || raw.window_size_px.height == 0
            || !raw.chat_roi.is_valid()
            || !raw.header_identity_roi.is_valid()
            || blank(&raw.probe_evidence.probe_id)
            || blank(&raw.probe_evidence.validated_at)
            || blank(&raw.probe_evidence.windows_build)
            || !is_sha256(&raw.probe_evidence.evidence_sha256)
        {
            return Err(CatalogError::InvalidCatalog);
        }

        Ok(Self {
            id: raw.id,
            enabled: raw.enabled,
            profile_version: raw.profile_version,
            wechat_product_version: raw.wechat_product_version,
            windows_build: raw.probe_evidence.windows_build,
            theme: raw.theme,
            display_topology: DisplayTopology {
                monitors: raw.display_topology.monitors,
                target_monitor: raw.display_topology.target_monitor,
            },
            executable: ExecutableProfile {
                file_name: raw.executable.file_name,
                normalized_paths: raw.executable.normalized_paths,
                sha256: raw.executable.sha256,
                product_version: raw.executable.product_version,
            },
            dpi: raw.dpi,
            window_size_px: WindowSize {
                width: raw.window_size_px.width,
                height: raw.window_size_px.height,
                tolerance_px: raw.window_size_px.tolerance_px,
            },
            chat_roi: NormalizedRoi {
                left: raw.chat_roi.left,
                top: raw.chat_roi.top,
                right: raw.chat_roi.right,
                bottom: raw.chat_roi.bottom,
            },
            header_identity_roi: NormalizedRoi {
                left: raw.header_identity_roi.left,
                top: raw.header_identity_roi.top,
                right: raw.header_identity_roi.right,
                bottom: raw.header_identity_roi.bottom,
            },
        })
    }
}

impl RawNormalizedRoi {
    fn is_valid(&self) -> bool {
        [self.left, self.top, self.right, self.bottom]
            .into_iter()
            .all(f64::is_finite)
            && (0.0..=1.0).contains(&self.left)
            && (0.0..=1.0).contains(&self.top)
            && (0.0..=1.0).contains(&self.right)
            && (0.0..=1.0).contains(&self.bottom)
            && self.left < self.right
            && self.top < self.bottom
    }
}

fn blank(value: &str) -> bool {
    value.trim().is_empty()
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_PROFILE: &str = r#"{
      "schema_version": 1,
        "catalog_version": "windows-wechat-v1",
      "profiles": [{
        "id": "wechat-windows-4.0.1-light-96-primary",
        "enabled": true,
        "profile_version": "1",
        "wechat_product_version": "4.0.1.26",
        "theme": "light",
        "display_topology": { "monitors": 1, "target_monitor": "primary" },
        "executable": {
          "file_name": "WeChat.exe",
          "normalized_paths": ["c:\\program files\\tencent\\wechat\\wechat.exe"],
          "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
          "product_version": "4.0.1.26"
        },
        "dpi": 96,
        "window_size_px": { "width": 1280, "height": 900, "tolerance_px": 2 },
        "chat_roi": { "left": 0.2, "top": 0.16, "right": 0.98, "bottom": 0.91 },
        "header_identity_roi": { "left": 0.2, "top": 0.05, "right": 0.6, "bottom": 0.15 },
        "probe_evidence": {
          "probe_id": "test-probe",
          "validated_at": "2026-08-06T00:00:00Z",
          "windows_build": "22631",
          "evidence_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }
      }]
    }"#;

    #[test]
    fn embedded_catalog_has_no_enabled_profile_before_windows_probe() {
        let catalog = CompatibilityCatalog::embedded().unwrap();
        assert_eq!(catalog.catalog_version(), "windows-wechat-v1");
        assert_eq!(catalog.enabled_profiles().count(), 0);
    }

    #[test]
    fn valid_frozen_profile_is_loaded() {
        let catalog = CompatibilityCatalog::parse(VALID_PROFILE).unwrap();
        assert_eq!(catalog.enabled_profiles().count(), 1);
    }

    #[test]
    fn disabled_profile_is_preserved_but_not_enabled() {
        let catalog = CompatibilityCatalog::parse(&VALID_PROFILE.replace("\"enabled\": true", "\"enabled\": false")).unwrap();
        assert_eq!(catalog.enabled_profiles().count(), 0);
    }

    #[test]
    fn unknown_catalog_or_profile_version_is_rejected() {
        assert_eq!(CompatibilityCatalog::parse(&VALID_PROFILE.replace("windows-wechat-v1", "windows-wechat-v2")), Err(CatalogError::InvalidCatalog));
        assert_eq!(CompatibilityCatalog::parse(&VALID_PROFILE.replace("\"profile_version\": \"1\"", "\"profile_version\": \"2\"")), Err(CatalogError::InvalidCatalog));
    }

    #[test]
    fn mixed_enabled_and_disabled_profiles_only_exposes_enabled_entries() {
        let disabled = VALID_PROFILE
            .replace("\"id\": \"wechat-windows-4.0.1-light-96-primary\"", "\"id\": \"wechat-windows-4.0.1-dark-96-primary\"")
            .replace("\"enabled\": true", "\"enabled\": false")
            .replace("\"theme\": \"light\"", "\"theme\": \"dark\"");
        let profiles = serde_json::from_str::<serde_json::Value>(VALID_PROFILE).unwrap()["profiles"].clone();
        let disabled_profiles = serde_json::from_str::<serde_json::Value>(&disabled).unwrap()["profiles"].clone();
        let mixed = serde_json::json!({
            "schema_version": 1,
            "catalog_version": "windows-wechat-v1",
            "profiles": [profiles[0].clone(), disabled_profiles[0].clone()]
        });
        let catalog = CompatibilityCatalog::parse(&mixed.to_string()).unwrap();
        assert_eq!(catalog.enabled_profiles().count(), 1);
    }

    #[test]
    fn invalid_roi_profile_is_rejected() {
        assert_eq!(CompatibilityCatalog::parse(&VALID_PROFILE.replace("\"right\": 0.98", "\"right\": 0.2")), Err(CatalogError::InvalidCatalog));
    }
}
