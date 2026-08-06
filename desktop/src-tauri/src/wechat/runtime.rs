/// Placeholder state for the future reply workflow. It intentionally holds no
/// app state, database handle, capture data, OCR text, or model client.
#[derive(Default)]
pub(crate) struct WechatReplyRuntime;

impl WechatReplyRuntime {
    /// Re-reads the foreground Windows window and returns a backend-only identity.
    /// This step never captures pixels, invokes OCR, retrieval, or a model.
    #[cfg(target_os = "windows")]
    pub(crate) fn validate_foreground_wechat(
        &self,
    ) -> Result<super::window_identity::WechatWindowIdentity, super::types::ContractError> {
        let catalog = super::profiles::CompatibilityCatalog::embedded()
            .map_err(|_| super::types::ContractError::WxProfileUnsupported)?;
        let evidence = crate::monitor::read_foreground_window_evidence()
            .map_err(|_| super::types::ContractError::WxNotForeground)?;
        super::window_identity::validate_foreground(&catalog, evidence)
    }

    /// Non-Windows targets cannot create a Windows compatibility identity.
    #[cfg(not(target_os = "windows"))]
    pub(crate) fn validate_foreground_wechat(
        &self,
    ) -> Result<super::window_identity::WechatWindowIdentity, super::types::ContractError> {
        Err(super::types::ContractError::WxWindowUnsupported)
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn revalidate_foreground_wechat(
        &self,
        previous: &super::window_identity::WechatWindowIdentity,
    ) -> Result<super::window_identity::WechatWindowIdentity, super::types::ContractError> {
        let catalog = super::profiles::CompatibilityCatalog::embedded()
            .map_err(|_| super::types::ContractError::WxProfileUnsupported)?;
        let evidence = crate::monitor::read_foreground_window_evidence()
            .map_err(|_| super::types::ContractError::WxNotForeground)?;
        super::window_identity::revalidate_foreground(&catalog, previous, evidence)
    }

    /// Non-Windows targets cannot re-read a Windows compatibility identity.
    #[cfg(not(target_os = "windows"))]
    pub(crate) fn revalidate_foreground_wechat(
        &self,
        _previous: &super::window_identity::WechatWindowIdentity,
    ) -> Result<super::window_identity::WechatWindowIdentity, super::types::ContractError> {
        Err(super::types::ContractError::WxWindowUnsupported)
    }
}

/// Coordinates only future request ownership. Step 5 never issues a capture.
#[derive(Default)]
pub(crate) struct CaptureCoordinator;

#[allow(dead_code)]
pub(crate) trait WechatCapturePort {
    fn capture_is_unsupported(&self) -> &'static str;
}

#[allow(dead_code)]
pub(crate) struct UnsupportedWechatCapture;

impl WechatCapturePort for UnsupportedWechatCapture {
    fn capture_is_unsupported(&self) -> &'static str {
        "WX_NOT_READY"
    }
}

#[allow(dead_code)]
pub(crate) trait WechatReplyModelPort {
    fn reply_is_unavailable(&self) -> &'static str;
}

#[allow(dead_code)]
pub(crate) struct UnavailableWechatReplyModel;

impl WechatReplyModelPort for UnavailableWechatReplyModel {
    fn reply_is_unavailable(&self) -> &'static str {
        "WX_TEXT_MODEL_UNAVAILABLE"
    }
}
