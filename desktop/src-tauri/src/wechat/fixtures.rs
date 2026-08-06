#[cfg(test)]
mod tests {
    use super::super::types::{CapturedWechat, CaptureVersion, ContractError, NormalizedOcrText, OcrBackendResult, OcrReadyReply, RequestId};
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct OcrFixture {
        request_id: RequestId,
        capture_version: CaptureVersion,
        stable_message_id: String,
        is_single_chat: bool,
        ocr_text: String,
    }

    fn fixture(name: &str) -> &'static str {
        match name {
            "normal_m1" => include_str!("../../tests/fixtures/wechat_contract/normal_m1.json"),
            "empty_ocr" => include_str!("../../tests/fixtures/wechat_contract/empty_ocr.json"),
            "group_chat" => include_str!("../../tests/fixtures/wechat_contract/group_chat.json"),
            "duplicate_message" => include_str!("../../tests/fixtures/wechat_contract/duplicate_message.json"),
            "unsupported_schema" => include_str!("../../tests/fixtures/wechat_contract/unsupported_schema.json"),
            "ambiguous_conversations" => include_str!("../../tests/fixtures/wechat_contract/ambiguous_conversations.json"),
            "no_hit" => include_str!("../../tests/fixtures/wechat_contract/no_hit.json"),
            "retrieval_failed" => include_str!("../../tests/fixtures/wechat_contract/retrieval_failed.json"),
            _ => unreachable!(),
        }
    }

    #[test]
    fn every_desensitized_contract_fixture_is_valid_json() {
        for name in ["normal_m1", "empty_ocr", "group_chat", "duplicate_message", "unsupported_schema", "ambiguous_conversations", "no_hit", "retrieval_failed"] {
            let value: serde_json::Value = serde_json::from_str(fixture(name)).unwrap();
            assert_eq!(value["fixture"], name);
            assert!(!fixture(name).contains("微信聊天记录知识库"));
        }
    }

    #[test]
    fn ocr_fixtures_preserve_the_input_boundary() {
        let normal: OcrFixture = serde_json::from_str(fixture("normal_m1")).unwrap();
        let captured = CapturedWechat {
            request_id: normal.request_id,
            capture_version: normal.capture_version,
            stable_message_id: normal.stable_message_id,
            is_single_chat: normal.is_single_chat,
        };
        assert!(OcrReadyReply::from_backend(captured, OcrBackendResult::Text(NormalizedOcrText::parse(&normal.ocr_text).unwrap())).is_ok());

        let empty: OcrFixture = serde_json::from_str(fixture("empty_ocr")).unwrap();
        assert_eq!(NormalizedOcrText::parse(&empty.ocr_text), Err(ContractError::WxOcrEmpty));

        let group: OcrFixture = serde_json::from_str(fixture("group_chat")).unwrap();
        let captured = CapturedWechat {
            request_id: group.request_id,
            capture_version: group.capture_version,
            stable_message_id: group.stable_message_id,
            is_single_chat: group.is_single_chat,
        };
        assert_eq!(
            OcrReadyReply::from_backend(captured, OcrBackendResult::Text(NormalizedOcrText::parse(&group.ocr_text).unwrap())),
            Err(ContractError::WxGroupChatUnsupported),
        );
    }
}
