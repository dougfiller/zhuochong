use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::OnceLock;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct RequestId(Uuid);

impl RequestId {
    pub(crate) fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub(crate) fn audit_tag(&self) -> String {
        static PROCESS_AUDIT_SALT: OnceLock<Uuid> = OnceLock::new();
        let salt = PROCESS_AUDIT_SALT.get_or_init(Uuid::new_v4);
        let mut hash = Sha256::new();
        hash.update(salt.as_bytes());
        hash.update(self.0.as_bytes());
        hex::encode(&hash.finalize()[..12])
    }

    pub(crate) fn parse(value: &str) -> Result<Self, ContractError> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| ContractError::WxTraceInvalidQuery)
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

macro_rules! version_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub(crate) struct $name(u64);

        impl $name {
            pub(crate) fn new(value: u64) -> Self {
                Self(value)
            }

            pub(crate) fn value(self) -> u64 {
                self.0
            }
        }
    };
}

version_type!(CaptureVersion);
version_type!(SuggestionGeneration);
version_type!(BindingGeneration);
version_type!(BindingObservationVersion);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapturedWechat {
    pub(crate) request_id: RequestId,
    pub(crate) capture_version: CaptureVersion,
    pub(crate) stable_message_id: String,
    pub(crate) is_single_chat: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OcrBackendResult {
    Text(NormalizedOcrText),
    Empty,
    Unavailable,
    Failed,
}

const MAX_OCR_TEXT_SCALARS: usize = 8_192;
const MAX_OCR_TEXT_BYTES: usize = 32_768;

/// A private token proving text has passed the WeChat OCR normalization gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NormalizedOcrText(String);

impl NormalizedOcrText {
    pub(crate) fn parse(raw: &str) -> Result<Self, ContractError> {
        let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
        let mut lines = Vec::new();
        let mut previous_blank = false;
        for line in normalized.split('\n') {
            let cleaned: String = line
                .chars()
                .filter(|character| *character == '\n' || !character.is_control())
                .collect();
            let cleaned = cleaned.trim();
            if cleaned.is_empty() {
                if !previous_blank {
                    lines.push(String::new());
                }
                previous_blank = true;
            } else {
                lines.push(cleaned.to_owned());
                previous_blank = false;
            }
        }
        let text = lines.join("\n").trim().to_owned();
        if text.is_empty() {
            return Err(ContractError::WxOcrEmpty);
        }
        if text.len() > MAX_OCR_TEXT_BYTES || text.chars().count() > MAX_OCR_TEXT_SCALARS {
            return Err(ContractError::WxOcrFailed);
        }
        Ok(Self(text))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OcrReadyReply {
    captured: CapturedWechat,
    normalized_text: String,
}

impl OcrReadyReply {
    pub(crate) fn from_backend(
        captured: CapturedWechat,
        result: OcrBackendResult,
    ) -> Result<Self, ContractError> {
        if !captured.is_single_chat {
            return Err(ContractError::WxGroupChatUnsupported);
        }

        match result {
            OcrBackendResult::Text(text) => Ok(Self {
                captured,
                normalized_text: text.0,
            }),
            OcrBackendResult::Empty => Err(ContractError::WxOcrEmpty),
            OcrBackendResult::Unavailable => Err(ContractError::WxOcrUnavailable),
            OcrBackendResult::Failed => Err(ContractError::WxOcrFailed),
        }
    }

    pub(crate) fn request_id(&self) -> &RequestId {
        &self.captured.request_id
    }

    pub(crate) fn text(&self) -> &str {
        &self.normalized_text
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct M1ReplyInput {
    request_id: RequestId,
    text: String,
}

impl From<OcrReadyReply> for M1ReplyInput {
    fn from(value: OcrReadyReply) -> Self {
        Self {
            request_id: value.captured.request_id,
            text: value.normalized_text,
        }
    }
}

impl M1ReplyInput {
    pub(crate) fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GeneratedReply {
    request_id: RequestId,
    suggestion_generation: SuggestionGeneration,
    binding_generation: Option<BindingGeneration>,
    text: String,
}

impl GeneratedReply {
    pub(crate) fn m1(
        request_id: RequestId,
        suggestion_generation: SuggestionGeneration,
        text: String,
    ) -> Self {
        Self {
            request_id,
            suggestion_generation,
            binding_generation: None,
            text,
        }
    }

    pub(crate) fn m2(
        request_id: RequestId,
        suggestion_generation: SuggestionGeneration,
        binding_generation: BindingGeneration,
        text: String,
    ) -> Self {
        Self {
            request_id,
            suggestion_generation,
            binding_generation: Some(binding_generation),
            text,
        }
    }

    pub(crate) fn is_current(
        &self,
        request_id: &RequestId,
        suggestion_generation: SuggestionGeneration,
        binding_generation: Option<BindingGeneration>,
    ) -> bool {
        self.request_id == *request_id
            && self.suggestion_generation == suggestion_generation
            && self.binding_generation == binding_generation
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ContractError {
    #[serde(rename = "WX_BUSY")]
    WxBusy,
    #[serde(rename = "WX_NOT_FOREGROUND")]
    WxNotForeground,
    #[serde(rename = "WX_PROFILE_UNSUPPORTED")]
    WxProfileUnsupported,
    #[serde(rename = "WX_WINDOW_UNSUPPORTED")]
    WxWindowUnsupported,
    #[serde(rename = "WX_CAPTURE_FAILED")]
    WxCaptureFailed,
    #[serde(rename = "WX_CAPTURE_TIMEOUT")]
    WxCaptureTimeout,
    #[serde(rename = "WX_OCR_EMPTY")]
    WxOcrEmpty,
    #[serde(rename = "WX_OCR_UNAVAILABLE")]
    WxOcrUnavailable,
    #[serde(rename = "WX_OCR_FAILED")]
    WxOcrFailed,
    #[serde(rename = "WX_GROUP_CHAT_UNSUPPORTED")]
    WxGroupChatUnsupported,
    #[serde(rename = "WX_TEXT_MODEL_UNAVAILABLE")]
    WxTextModelUnavailable,
    #[serde(rename = "WX_REQUEST_CANCELLED")]
    WxRequestCancelled,
    #[serde(rename = "WX_REQUEST_STALE")]
    WxRequestStale,
    #[serde(rename = "KB_NOT_READY")]
    KbNotReady,
    #[serde(rename = "KB_REBUILD_SELECTION_REQUIRED")]
    KbRebuildSelectionRequired,
    #[serde(rename = "KB_SOURCE_UNSUPPORTED")]
    KbSourceUnsupported,
    #[serde(rename = "KB_SCOPE_UNRESOLVED")]
    KbScopeUnresolved,
    #[serde(rename = "KB_RETRIEVAL_FAILED")]
    KbRetrievalFailed,
    #[serde(rename = "LLM_FAILED")]
    LlmFailed,
    #[serde(rename = "WX_CONTRACT_VIOLATION")]
    WxContractViolation,
    #[serde(rename = "WX_TRACE_PERSIST_FAILED")]
    WxTracePersistFailed,
    #[serde(rename = "WX_TRACE_INVALID_QUERY")]
    WxTraceInvalidQuery,
    #[serde(rename = "WX_AUDIT_PERSIST_FAILED")]
    WxAuditPersistFailed,
    #[serde(rename = "WX_CONTENT_PERSIST_FAILED")]
    WxContentPersistFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture() -> CapturedWechat {
        CapturedWechat {
            request_id: RequestId::new(),
            capture_version: CaptureVersion::new(7),
            stable_message_id: "fixture-message-1".into(),
            is_single_chat: true,
        }
    }

    #[test]
    fn ocr_text_creates_only_the_m1_input_chain() {
        let ocr = OcrReadyReply::from_backend(
            capture(),
            OcrBackendResult::Text(NormalizedOcrText::parse("  请确认  ").unwrap()),
        )
        .unwrap();
        assert_eq!(ocr.text(), "请确认");
        let input = M1ReplyInput::from(ocr);
        assert_eq!(input.text(), "请确认");
    }

    #[test]
    fn non_text_ocr_cannot_create_a_reply_input() {
        for (result, error) in [
            (OcrBackendResult::Empty, ContractError::WxOcrEmpty),
            (
                OcrBackendResult::Unavailable,
                ContractError::WxOcrUnavailable,
            ),
            (OcrBackendResult::Failed, ContractError::WxOcrFailed),
        ] {
            assert_eq!(OcrReadyReply::from_backend(capture(), result), Err(error));
        }
    }

    #[test]
    fn group_chat_cannot_create_an_m1_reply_input() {
        let mut group_capture = capture();
        group_capture.is_single_chat = false;

        assert_eq!(
            OcrReadyReply::from_backend(
                group_capture,
                OcrBackendResult::Text(NormalizedOcrText::parse("请确认").unwrap())
            ),
            Err(ContractError::WxGroupChatUnsupported),
        );
    }

    #[test]
    fn error_codes_are_stable_wire_values() {
        assert_eq!(
            serde_json::to_string(&ContractError::KbRetrievalFailed).unwrap(),
            "\"KB_RETRIEVAL_FAILED\""
        );
        assert_eq!(
            serde_json::to_string(&ContractError::WxGroupChatUnsupported).unwrap(),
            "\"WX_GROUP_CHAT_UNSUPPORTED\""
        );
        assert_eq!(
            serde_json::from_str::<ContractError>("\"WX_OCR_EMPTY\"").unwrap(),
            ContractError::WxOcrEmpty
        );
    }

    #[test]
    fn normalization_removes_controls_and_rejects_empty_or_over_limit_text() {
        let text =
            NormalizedOcrText::parse("  第一行\r\n\0第二\u{0007}行\n\n\n  第三行  ").unwrap();
        assert_eq!(text.as_str(), "第一行\n第二行\n\n第三行");
        assert_eq!(
            NormalizedOcrText::parse("\0\r\n\t"),
            Err(ContractError::WxOcrEmpty)
        );
        assert_eq!(
            NormalizedOcrText::parse(&"a".repeat(MAX_OCR_TEXT_BYTES + 1)),
            Err(ContractError::WxOcrFailed)
        );
    }
}
