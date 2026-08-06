use super::profiles::{CompatibilityCatalog, CompatibilityProfile, NormalizedRoi, OcrFallbackAudit};
use super::types::ContractError;
use crate::monitor::WindowBounds;

/// Process-local token. It intentionally has no serialization or display implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WindowInstanceToken {
    hwnd: usize,
    pid: u32,
    process_started_at: Option<u64>,
}

impl WindowInstanceToken {
    pub(crate) fn new(hwnd: usize, pid: u32, process_started_at: Option<u64>) -> Self {
        Self { hwnd, pid, process_started_at }
    }
}

/// Evidence read directly from the foreground Windows window. It stays backend-private.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExecutableEvidence {
    normalized_path: String,
    file_name: String,
    sha256: String,
    product_version: String,
}

impl ExecutableEvidence {
    pub(crate) fn new(
        normalized_path: String,
        file_name: String,
        sha256: String,
        product_version: String,
    ) -> Self {
        Self { normalized_path, file_name, sha256, product_version }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplayEvidence {
    pub(crate) monitors: u32,
    pub(crate) target_monitor: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ForegroundWindowEvidence {
    pub(crate) instance: WindowInstanceToken,
    pub(crate) pid: u32,
    pub(crate) executable: ExecutableEvidence,
    pub(crate) bounds_px: WindowBounds,
    pub(crate) dpi: u32,
    pub(crate) is_minimized: bool,
    pub(crate) display: DisplayEvidence,
    pub(crate) windows_build: String,
    /// Theme must be directly identified by a future Windows probe; unknown is rejected.
    pub(crate) theme: Option<String>,
    pub(crate) title_hint: String,
}

/// Backend-only identity passed to the future capture stage. No Serialize/Deserialize derives.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WechatWindowIdentity {
    instance: WindowInstanceToken,
    pid: u32,
    bounds_px: WindowBounds,
    dpi: u32,
    profile_id: String,
    profile_version: String,
    chat_roi: NormalizedRoi,
    header_identity_roi: NormalizedRoi,
    title_hint: String,
    ocr_fallback_audit: Option<OcrFallbackAudit>,
}

impl WechatWindowIdentity {
    pub(crate) fn is_current(&self, other: &Self) -> bool {
        self.instance == other.instance
            && self.pid == other.pid
            && self.bounds_px == other.bounds_px
            && self.dpi == other.dpi
            && self.profile_id == other.profile_id
            && self.profile_version == other.profile_version
    }

    pub(crate) fn bounds_px(&self) -> WindowBounds {
        self.bounds_px
    }

    pub(crate) fn dpi(&self) -> u32 {
        self.dpi
    }

    pub(crate) fn chat_roi(&self) -> NormalizedRoi {
        self.chat_roi
    }

    pub(crate) fn header_identity_roi(&self) -> NormalizedRoi {
        self.header_identity_roi
    }

    pub(crate) fn capture_target_window(&self) -> crate::monitor::ActiveWindow {
        crate::monitor::ActiveWindow {
            app_name: "WeChat".to_string(),
            window_title: String::new(),
            browser_url: None,
            executable_path: None,
            window_bounds: Some(self.bounds_px),
            is_minimized: false,
        }
    }

    pub(crate) fn ocr_fallback_audit(&self) -> Option<&OcrFallbackAudit> {
        self.ocr_fallback_audit.as_ref()
    }
}

pub(crate) fn validate_foreground(
    catalog: &CompatibilityCatalog,
    evidence: ForegroundWindowEvidence,
) -> Result<WechatWindowIdentity, ContractError> {
    if evidence.pid == 0 || evidence.is_minimized {
        return Err(ContractError::WxNotForeground);
    }
    if evidence.bounds_px.width == 0 || evidence.bounds_px.height == 0 || evidence.dpi == 0 {
        return Err(ContractError::WxWindowUnsupported);
    }

    let matches: Vec<_> = catalog
        .enabled_profiles()
        .filter(|profile| profile_matches(profile, &evidence))
        .collect();
    if matches.len() != 1 {
        return Err(ContractError::WxProfileUnsupported);
    }

    let profile = matches[0];
    Ok(WechatWindowIdentity {
        instance: evidence.instance,
        pid: evidence.pid,
        bounds_px: evidence.bounds_px,
        dpi: evidence.dpi,
        profile_id: profile.id.clone(),
        profile_version: profile.profile_version.clone(),
        chat_roi: profile.chat_roi,
        header_identity_roi: profile.header_identity_roi,
        title_hint: sanitize_title_hint(&evidence.title_hint),
        ocr_fallback_audit: profile.ocr_fallback_audit.clone(),
    })
}

pub(crate) fn revalidate_foreground(
    catalog: &CompatibilityCatalog,
    previous: &WechatWindowIdentity,
    evidence: ForegroundWindowEvidence,
) -> Result<WechatWindowIdentity, ContractError> {
    if evidence.pid == 0 || evidence.is_minimized {
        return Err(ContractError::WxNotForeground);
    }
    if evidence.bounds_px.width == 0 || evidence.bounds_px.height == 0 || evidence.dpi == 0 {
        return Err(ContractError::WxWindowUnsupported);
    }
    if previous.instance != evidence.instance
        || previous.pid != evidence.pid
        || previous.bounds_px != evidence.bounds_px
        || previous.dpi != evidence.dpi
    {
        return Err(ContractError::WxRequestStale);
    }
    let current = validate_foreground(catalog, evidence)?;
    if previous.is_current(&current) {
        Ok(current)
    } else {
        Err(ContractError::WxRequestStale)
    }
}

fn profile_matches(profile: &CompatibilityProfile, evidence: &ForegroundWindowEvidence) -> bool {
    let Some(theme) = evidence.theme.as_deref() else {
        return false;
    };
    profile.wechat_product_version == profile.executable.product_version
        && profile.windows_build == evidence.windows_build
        && profile.theme == theme
        && profile.display_topology.monitors == evidence.display.monitors
        && profile.display_topology.target_monitor == evidence.display.target_monitor
        && profile.dpi == evidence.dpi
        && profile.executable.file_name.eq_ignore_ascii_case(&evidence.executable.file_name)
        && profile
            .executable
            .normalized_paths
            .iter()
            .any(|path| normalized_path_eq(path, &evidence.executable.normalized_path))
        && profile.executable.sha256.eq_ignore_ascii_case(&evidence.executable.sha256)
        && profile.executable.product_version == evidence.executable.product_version
        && within_tolerance(profile.window_size_px.width, evidence.bounds_px.width, profile.window_size_px.tolerance_px)
        && within_tolerance(profile.window_size_px.height, evidence.bounds_px.height, profile.window_size_px.tolerance_px)
}

fn normalized_path_eq(expected: &str, actual: &str) -> bool {
    expected.trim().replace('/', "\\").eq_ignore_ascii_case(&actual.trim().replace('/', "\\"))
}

fn within_tolerance(expected: u32, actual: u32, tolerance: u32) -> bool {
    expected.abs_diff(actual) <= tolerance
}

fn sanitize_title_hint(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(160)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = r#"{
      "schema_version": 1,
      "catalog_version": "windows-wechat-v1",
      "profiles": [{
        "id": "wechat-windows-4.0.1-light-96-primary",
        "enabled": true,
        "profile_version": "1",
        "wechat_product_version": "4.0.1.26",
        "theme": "light",
        "display_topology": { "monitors": 1, "target_monitor": "primary" },
        "executable": { "file_name": "WeChat.exe", "normalized_paths": ["c:\\program files\\tencent\\wechat\\wechat.exe"], "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "product_version": "4.0.1.26" },
        "dpi": 96,
        "window_size_px": { "width": 1280, "height": 900, "tolerance_px": 2 },
        "chat_roi": { "left": 0.2, "top": 0.16, "right": 0.98, "bottom": 0.91 },
        "header_identity_roi": { "left": 0.2, "top": 0.05, "right": 0.6, "bottom": 0.15 },
        "probe_evidence": { "probe_id": "test-probe", "validated_at": "2026-08-06T00:00:00Z", "windows_build": "22631", "evidence_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" }
      }]
    }"#;

    fn evidence() -> ForegroundWindowEvidence {
        ForegroundWindowEvidence {
            instance: WindowInstanceToken::new(42, 7, Some(99)),
            pid: 7,
            executable: ExecutableEvidence::new(
                "C:/Program Files/Tencent/WeChat/WeChat.exe".into(),
                "WeChat.exe".into(),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                "4.0.1.26".into(),
            ),
            bounds_px: WindowBounds { x: -100, y: 0, width: 1281, height: 899 },
            dpi: 96,
            is_minimized: false,
            display: DisplayEvidence { monitors: 1, target_monitor: "primary".into() },
            windows_build: "22631".into(),
            theme: Some("light".into()),
            title_hint: "测试\u{0000}聊天".into(),
        }
    }

    #[test]
    fn matching_evidence_creates_a_private_identity() {
        let catalog = CompatibilityCatalog::parse(PROFILE).unwrap();
        let identity = validate_foreground(&catalog, evidence()).unwrap();
        assert_eq!(identity.title_hint, "测试聊天");
        assert!(identity.chat_roi.left < identity.chat_roi.right);
        assert!(identity.header_identity_roi.top < identity.header_identity_roi.bottom);
    }

    #[test]
    fn title_cannot_rescue_untrusted_executable_or_layout() {
        let catalog = CompatibilityCatalog::parse(PROFILE).unwrap();
        let mut wrong_executable = evidence();
        wrong_executable.executable.sha256 = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".into();
        assert_eq!(validate_foreground(&catalog, wrong_executable), Err(ContractError::WxProfileUnsupported));

        let mut wrong_dpi = evidence();
        wrong_dpi.dpi = 144;
        assert_eq!(validate_foreground(&catalog, wrong_dpi), Err(ContractError::WxProfileUnsupported));
    }

    #[test]
    fn reread_rejects_a_changed_window_instance_or_bounds() {
        let catalog = CompatibilityCatalog::parse(PROFILE).unwrap();
        let identity = validate_foreground(&catalog, evidence()).unwrap();
        let mut changed = evidence();
        changed.instance = WindowInstanceToken::new(43, 7, Some(99));
        assert_eq!(revalidate_foreground(&catalog, &identity, changed), Err(ContractError::WxRequestStale));

        let mut resized = evidence();
        resized.bounds_px.width = 1283;
        assert_eq!(revalidate_foreground(&catalog, &identity, resized), Err(ContractError::WxRequestStale));
    }

    #[test]
    fn minimized_or_missing_geometry_is_rejected_before_profile_matching() {
        let catalog = CompatibilityCatalog::parse(PROFILE).unwrap();
        let mut minimized = evidence();
        minimized.is_minimized = true;
        assert_eq!(validate_foreground(&catalog, minimized), Err(ContractError::WxNotForeground));

        let mut invalid_bounds = evidence();
        invalid_bounds.bounds_px.width = 0;
        assert_eq!(validate_foreground(&catalog, invalid_bounds), Err(ContractError::WxWindowUnsupported));
    }
}
