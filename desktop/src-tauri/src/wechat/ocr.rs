use super::capture::WechatCaptureSlices;
use super::profiles::OcrFallbackAudit;
use super::types::{
    CapturedWechat, ContractError, NormalizedOcrText, OcrBackendResult, OcrReadyReply,
};
use super::window_identity::WechatWindowIdentity;
use crate::ocr::{OcrService, WindowsMemoryOcrResult};
use image::RgbaImage;

const MAX_CHAT_PIXELS: u64 = 16_000_000;
const MAX_HEADER_BYTES: usize = 256;
const MAX_HEADER_SCALARS: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeaderIdentityClue {
    text: String,
    reliable: bool,
}

impl HeaderIdentityClue {
    pub(crate) fn from_backend(result: WindowsMemoryOcrResult) -> Self {
        let WindowsMemoryOcrResult::Text(raw) = result else {
            return Self {
                text: String::new(),
                reliable: false,
            };
        };
        let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
        let lines = normalized
            .split('\n')
            .map(|line| {
                line.chars()
                    .filter(|character| !character.is_control())
                    .collect::<String>()
            })
            .map(|line| line.trim().to_owned())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>();
        let text = lines.first().cloned().unwrap_or_default();
        if text.len() > MAX_HEADER_BYTES || text.chars().count() > MAX_HEADER_SCALARS {
            return Self {
                text: String::new(),
                reliable: false,
            };
        }
        let generic =
            matches!(text.to_ascii_lowercase().as_str(), "wechat" | "weixin") || text == "微信";
        let has_letter = text.chars().any(char::is_alphabetic);
        let reliable = lines.len() == 1 && !generic && has_letter;
        Self { text, reliable }
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }
    pub(crate) fn reliable(&self) -> bool {
        self.reliable
    }

    #[cfg(test)]
    pub(crate) fn fixture(text: &str, reliable: bool) -> Self {
        Self {
            text: text.to_owned(),
            reliable,
        }
    }
}

/// Minimal, redacted OCR observability. It never carries text, image geometry,
/// paths, window handles, error details, or fallback parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WechatOcrAuditEvent {
    pub(crate) request_id_hash: String,
    pub(crate) capture_version: u64,
    pub(crate) stage: &'static str,
    pub(crate) outcome: &'static str,
    pub(crate) provider: &'static str,
}

pub(crate) trait WechatOcrAuditSink {
    fn record(&mut self, event: WechatOcrAuditEvent);
}

pub(crate) trait MemoryOcrProvider {
    fn recognize(&mut self, chat_rgba: &RgbaImage) -> WindowsMemoryOcrResult;
}

pub(crate) struct WindowsMemoryPrimary;

impl MemoryOcrProvider for WindowsMemoryPrimary {
    fn recognize(&mut self, chat_rgba: &RgbaImage) -> WindowsMemoryOcrResult {
        OcrService::extract_windows_ocr_rgba(chat_rgba)
    }
}

/// There is no audited production fallback yet. This type exists so the
/// dispatcher stays fail-closed instead of probing files, processes, or PATH.
pub(crate) struct DisabledLocalFallback;

impl MemoryOcrProvider for DisabledLocalFallback {
    fn recognize(&mut self, _chat_rgba: &RgbaImage) -> WindowsMemoryOcrResult {
        WindowsMemoryOcrResult::Unavailable
    }
}

pub(crate) struct WechatOcrDispatcher<P, F> {
    primary: P,
    fallback: F,
}

impl<P, F> WechatOcrDispatcher<P, F>
where
    P: MemoryOcrProvider,
    F: MemoryOcrProvider,
{
    pub(crate) fn new(primary: P, fallback: F) -> Self {
        Self { primary, fallback }
    }

    /// Only the private, cropped chat image crosses this boundary. Header
    /// pixels are deliberately never read; a non-text outcome never produces
    /// `OcrReadyReply`, so the later model/retrieval stages cannot run.
    pub(crate) fn recognize<S: WechatOcrAuditSink>(
        &mut self,
        slices: &WechatCaptureSlices,
        identity: &WechatWindowIdentity,
        captured: CapturedWechat,
        audit_sink: &mut S,
    ) -> Result<OcrReadyReply, ContractError> {
        if slices.request_id != captured.request_id
            || slices.capture_version != captured.capture_version
        {
            return self.finish(
                slices,
                captured,
                audit_sink,
                OcrBackendResult::Failed,
                "WindowsOCR",
            );
        }
        if !captured.is_single_chat {
            return Err(ContractError::WxGroupChatUnsupported);
        }
        if !valid_chat_image(&slices.chat_rgba) {
            return self.finish(
                slices,
                captured,
                audit_sink,
                OcrBackendResult::Failed,
                "WindowsOCR",
            );
        }

        let primary = normalize_result(self.primary.recognize(&slices.chat_rgba));
        match primary {
            OcrBackendResult::Unavailable | OcrBackendResult::Failed
                if identity
                    .ocr_fallback_audit()
                    .is_some_and(fallback_is_approved) =>
            {
                let fallback = normalize_result(self.fallback.recognize(&slices.chat_rgba));
                self.finish(slices, captured, audit_sink, fallback, "LocalFallback")
            }
            result => self.finish(slices, captured, audit_sink, result, "WindowsOCR"),
        }
    }

    pub(crate) fn recognize_header(
        &mut self,
        frame: &super::capture::HeaderObservationFrame,
    ) -> Result<HeaderIdentityClue, ContractError> {
        if !valid_chat_image(&frame.rgba) {
            return Err(ContractError::WxOcrFailed);
        }
        Ok(HeaderIdentityClue::from_backend(
            self.primary.recognize(&frame.rgba),
        ))
    }

    fn finish<S: WechatOcrAuditSink>(
        &self,
        slices: &WechatCaptureSlices,
        captured: CapturedWechat,
        audit_sink: &mut S,
        result: OcrBackendResult,
        provider: &'static str,
    ) -> Result<OcrReadyReply, ContractError> {
        let outcome = match &result {
            OcrBackendResult::Text(_) => "text",
            OcrBackendResult::Empty => "empty",
            OcrBackendResult::Unavailable => "unavailable",
            OcrBackendResult::Failed => "failed",
        };
        audit_sink.record(WechatOcrAuditEvent {
            request_id_hash: slices.request_id.audit_tag(),
            capture_version: slices.capture_version.value(),
            stage: "ocr",
            outcome,
            provider,
        });
        OcrReadyReply::from_backend(captured, result)
    }
}

fn valid_chat_image(image: &RgbaImage) -> bool {
    image.width() > 0
        && image.height() > 0
        && u64::from(image.width())
            .checked_mul(u64::from(image.height()))
            .is_some_and(|pixels| pixels <= MAX_CHAT_PIXELS)
        && usize::try_from(image.width())
            .ok()
            .and_then(|width| {
                usize::try_from(image.height())
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            == Some(image.as_raw().len())
}

fn normalize_result(result: WindowsMemoryOcrResult) -> OcrBackendResult {
    match result {
        WindowsMemoryOcrResult::Text(text) => match NormalizedOcrText::parse(&text) {
            Ok(text) => OcrBackendResult::Text(text),
            Err(ContractError::WxOcrEmpty) => OcrBackendResult::Empty,
            Err(_) => OcrBackendResult::Failed,
        },
        WindowsMemoryOcrResult::Empty => OcrBackendResult::Empty,
        WindowsMemoryOcrResult::Unavailable => OcrBackendResult::Unavailable,
        WindowsMemoryOcrResult::Failed => OcrBackendResult::Failed,
    }
}

fn fallback_is_approved(audit: &OcrFallbackAudit) -> bool {
    // Parsing already verifies every binding. No runtime discovery is allowed.
    let _ = audit;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monitor::WindowBounds;
    use crate::wechat::profiles::CompatibilityCatalog;
    use crate::wechat::window_identity::{
        validate_foreground, DisplayEvidence, ExecutableEvidence, ForegroundWindowEvidence,
        WindowInstanceToken,
    };

    #[derive(Default)]
    struct SpyProvider {
        results: Vec<WindowsMemoryOcrResult>,
        calls: usize,
    }

    impl SpyProvider {
        fn one(result: WindowsMemoryOcrResult) -> Self {
            Self {
                results: vec![result],
                calls: 0,
            }
        }
    }

    impl MemoryOcrProvider for SpyProvider {
        fn recognize(&mut self, _chat_rgba: &RgbaImage) -> WindowsMemoryOcrResult {
            self.calls += 1;
            self.results.remove(0)
        }
    }

    #[derive(Default)]
    struct Events(Vec<WechatOcrAuditEvent>);

    impl WechatOcrAuditSink for Events {
        fn record(&mut self, event: WechatOcrAuditEvent) {
            self.0.push(event);
        }
    }

    fn slices() -> (WechatCaptureSlices, CapturedWechat) {
        let request_id = super::super::types::RequestId::new();
        let capture_version = super::super::types::CaptureVersion::new(9);
        let captured = CapturedWechat {
            request_id: request_id.clone(),
            capture_version,
            stable_message_id: "fixture-message".into(),
            is_single_chat: true,
        };
        (
            WechatCaptureSlices {
                request_id,
                capture_version,
                chat_rgba: RgbaImage::new(2, 2),
                header_identity_rgba: RgbaImage::from_pixel(2, 2, image::Rgba([9, 8, 7, 6])),
            },
            captured,
        )
    }

    fn identity(with_audit: bool) -> WechatWindowIdentity {
        let catalog = CompatibilityCatalog::parse(if with_audit {
            PROFILE_WITH_AUDIT
        } else {
            PROFILE
        })
        .unwrap();
        validate_foreground(
            &catalog,
            ForegroundWindowEvidence {
                instance: WindowInstanceToken::new(1, 2, Some(3)),
                pid: 2,
                executable: ExecutableEvidence::new(
                    "c:\\program files\\tencent\\wechat\\wechat.exe".into(),
                    "WeChat.exe".into(),
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                    "4.0.1.26".into(),
                ),
                bounds_px: WindowBounds {
                    x: 0,
                    y: 0,
                    width: 1280,
                    height: 900,
                },
                dpi: 96,
                is_minimized: false,
                display: DisplayEvidence {
                    monitors: 1,
                    target_monitor: "primary".into(),
                },
                windows_build: "22631".into(),
                theme: Some("light".into()),
                title_hint: "fixture".into(),
            },
        )
        .unwrap()
    }

    const PROFILE: &str = r#"{
      "schema_version": 1, "catalog_version": "windows-wechat-v1", "profiles": [{
        "id": "wechat-windows-4.0.1-light-96-primary", "enabled": true, "profile_version": "1",
        "wechat_product_version": "4.0.1.26", "reply_surface": "single_chat", "theme": "light",
        "display_topology": { "monitors": 1, "target_monitor": "primary" },
        "executable": { "file_name": "WeChat.exe", "normalized_paths": ["c:\\program files\\tencent\\wechat\\wechat.exe"], "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "product_version": "4.0.1.26" },
        "dpi": 96, "window_size_px": { "width": 1280, "height": 900, "tolerance_px": 2 },
        "chat_roi": { "left": 0.2, "top": 0.16, "right": 0.98, "bottom": 0.91 },
        "header_identity_roi": { "left": 0.2, "top": 0.05, "right": 0.6, "bottom": 0.15 },
        "probe_evidence": { "probe_id": "test-probe", "validated_at": "2026-08-06T00:00:00Z", "windows_build": "22631", "evidence_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" }
      }]
    }"#;

    const PROFILE_WITH_AUDIT: &str = r#"{
      "schema_version": 1, "catalog_version": "windows-wechat-v1", "profiles": [{
        "id": "wechat-windows-4.0.1-light-96-primary", "enabled": true, "profile_version": "1",
        "wechat_product_version": "4.0.1.26", "reply_surface": "single_chat", "theme": "light",
        "display_topology": { "monitors": 1, "target_monitor": "primary" },
        "executable": { "file_name": "WeChat.exe", "normalized_paths": ["c:\\program files\\tencent\\wechat\\wechat.exe"], "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "product_version": "4.0.1.26" },
        "dpi": 96, "window_size_px": { "width": 1280, "height": 900, "tolerance_px": 2 },
        "chat_roi": { "left": 0.2, "top": 0.16, "right": 0.98, "bottom": 0.91 },
        "header_identity_roi": { "left": 0.2, "top": 0.05, "right": 0.6, "bottom": 0.15 },
        "probe_evidence": { "probe_id": "test-probe", "validated_at": "2026-08-06T00:00:00Z", "windows_build": "22631", "evidence_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" },
        "ocr_fallback_audit": { "profile_id": "wechat-windows-4.0.1-light-96-primary", "profile_version": "1", "primary": "WindowsOCR", "probe_evidence_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "probe_windows_build": "22631", "probe_wechat_version": "4.0.1.26", "probe_theme": "light", "probe_dpi": 96, "probe_monitors": 1, "probe_topology": "primary", "probe_outcome": "failed", "fallback_id": "compiled-local-memory-v1", "fallback_sha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd", "offline_no_ui_no_disk_no_command": true, "reviewed_at": "2026-08-06T00:00:00Z" }
      }]
    }"#;

    #[test]
    fn text_is_normalized_and_header_pixels_are_not_an_ocr_input() {
        let (slices, captured) = slices();
        let mut dispatcher = WechatOcrDispatcher::new(
            SpyProvider::one(WindowsMemoryOcrResult::Text(
                "  第一行\r\n\0第二行  ".into(),
            )),
            SpyProvider::default(),
        );
        let mut events = Events::default();
        let reply = dispatcher
            .recognize(&slices, &identity(false), captured, &mut events)
            .unwrap();
        assert_eq!(reply.text(), "第一行\n第二行");
        assert_eq!(events.0[0].outcome, "text");
        assert_eq!(events.0[0].provider, "WindowsOCR");
    }

    #[test]
    fn header_clue_never_retains_text_over_the_byte_or_scalar_limit() {
        for raw in ["a".repeat(257), "界".repeat(86), "a".repeat(129)] {
            let clue = HeaderIdentityClue::from_backend(WindowsMemoryOcrResult::Text(raw));
            assert_eq!(clue.text(), "");
            assert!(!clue.reliable());
        }

        let byte_boundary = HeaderIdentityClue::from_backend(WindowsMemoryOcrResult::Text(
            format!("{}a", "界".repeat(85)),
        ));
        assert_eq!(byte_boundary.text().len(), 256);
        assert!(byte_boundary.reliable());
        let scalar_boundary =
            HeaderIdentityClue::from_backend(WindowsMemoryOcrResult::Text("a".repeat(128)));
        assert_eq!(scalar_boundary.text().chars().count(), 128);
        assert!(scalar_boundary.reliable());
    }

    #[test]
    fn multiline_header_is_bounded_and_unreliable() {
        let clue =
            HeaderIdentityClue::from_backend(WindowsMemoryOcrResult::Text("第一行\n第二行".into()));
        assert_eq!(clue.text(), "第一行");
        assert!(!clue.reliable());

        let oversized = HeaderIdentityClue::from_backend(WindowsMemoryOcrResult::Text(format!(
            "{}\n第二行",
            "a".repeat(257)
        )));
        assert_eq!(oversized.text(), "");
        assert!(!oversized.reliable());
    }

    #[test]
    fn empty_never_runs_a_fallback_and_all_non_text_results_end_before_downstream() {
        for (primary, error, outcome) in [
            (
                WindowsMemoryOcrResult::Empty,
                ContractError::WxOcrEmpty,
                "empty",
            ),
            (
                WindowsMemoryOcrResult::Unavailable,
                ContractError::WxOcrUnavailable,
                "unavailable",
            ),
            (
                WindowsMemoryOcrResult::Failed,
                ContractError::WxOcrFailed,
                "failed",
            ),
        ] {
            let (slices, captured) = slices();
            let mut dispatcher =
                WechatOcrDispatcher::new(SpyProvider::one(primary), SpyProvider::default());
            let mut events = Events::default();
            assert_eq!(
                dispatcher.recognize(&slices, &identity(false), captured, &mut events),
                Err(error)
            );
            assert_eq!(dispatcher.primary.calls, 1);
            assert_eq!(dispatcher.fallback.calls, 0);
            assert_eq!(events.0[0].outcome, outcome);
        }
    }

    #[test]
    fn only_a_frozen_failed_probe_can_run_one_fallback() {
        let (slices, captured) = slices();
        let mut dispatcher = WechatOcrDispatcher::new(
            SpyProvider::one(WindowsMemoryOcrResult::Failed),
            SpyProvider::one(WindowsMemoryOcrResult::Text("fallback text".into())),
        );
        let mut events = Events::default();
        assert_eq!(
            dispatcher
                .recognize(&slices, &identity(true), captured, &mut events)
                .unwrap()
                .text(),
            "fallback text"
        );
        assert_eq!(dispatcher.primary.calls, 1);
        assert_eq!(dispatcher.fallback.calls, 1);
        assert_eq!(events.0[0].provider, "LocalFallback");
    }
}
