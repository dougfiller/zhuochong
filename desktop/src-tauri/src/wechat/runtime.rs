/// Placeholder state for the future reply workflow. It intentionally holds no
/// app state, database handle, capture data, OCR text, or model client.
#[derive(Default)]
pub(crate) struct WechatReplyRuntime;

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
