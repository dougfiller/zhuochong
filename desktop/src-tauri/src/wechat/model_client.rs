#[cfg(any(feature = "wechat-m1", feature = "wechat-m2"))]
use super::config::selected_verified_model;
use super::model_contract::ModelKnowledgeContext;
use super::types::ContractError;
#[cfg(feature = "wechat-m1")]
use super::types::M1ReplyInput;
#[cfg(any(feature = "wechat-m1", feature = "wechat-m2"))]
use super::types::{GeneratedReply, SuggestionGeneration};
use crate::agent::model::{
    ProviderSingleTurnTextTransport, SingleTurnTextRequest, SingleTurnTextTransport,
};
#[cfg(any(feature = "wechat-m1", feature = "wechat-m2"))]
use crate::config::{TextModelProfile, WechatConfig};
use std::sync::Arc;

const WECHAT_SYSTEM_PROMPT: &str =
    "输出一条供用户审阅的微信纯文本回复，不得调用工具或宣称已发送消息。";
const MAX_REPLY_SCALARS: usize = 8_192;
const MAX_REPLY_BYTES: usize = 32_768;

/// Private model boundary for WeChat. It accepts only domain-validated M1/M2
/// envelopes and never receives an AppState, runtime lock, or frontend input.
pub(super) struct WechatReplyModelClient {
    transport: Arc<dyn SingleTurnTextTransport>,
}

impl WechatReplyModelClient {
    pub(super) fn new() -> Self {
        Self {
            transport: Arc::new(ProviderSingleTurnTextTransport),
        }
    }

    #[cfg(test)]
    pub(super) fn with_transport(transport: Arc<dyn SingleTurnTextTransport>) -> Self {
        Self { transport }
    }

    #[cfg(feature = "wechat-m1")]
    pub(super) async fn generate_m1(
        &self,
        config: &WechatConfig,
        profiles: &[TextModelProfile],
        input: M1ReplyInput,
        suggestion: SuggestionGeneration,
    ) -> Result<GeneratedReply, ContractError> {
        let model = selected_verified_model(config, profiles)?;
        let text = self.complete(&model, input.text()).await?;
        Ok(GeneratedReply::m1(
            input.request_id().clone(),
            suggestion,
            text,
        ))
    }

    #[cfg(feature = "wechat-m2")]
    pub(super) async fn generate_m2(
        &self,
        config: &WechatConfig,
        profiles: &[TextModelProfile],
        context: ModelKnowledgeContext,
        suggestion: SuggestionGeneration,
    ) -> Result<GeneratedReply, ContractError> {
        let model = selected_verified_model(config, profiles)?;
        let prompt = m2_user_prompt(&context);
        let text = self.complete(&model, &prompt).await?;
        Ok(GeneratedReply::m2(
            context.request_id().clone(),
            suggestion,
            context.binding_generation(),
            text,
        ))
    }

    async fn complete(
        &self,
        model: &crate::config::ModelConfig,
        user_prompt: &str,
    ) -> Result<String, ContractError> {
        let text = self
            .transport
            .complete(
                model,
                SingleTurnTextRequest::new(WECHAT_SYSTEM_PROMPT, user_prompt),
            )
            .await
            .map_err(|_| ContractError::LlmFailed)?;
        validate_reply_text(text)
    }
}

fn m2_user_prompt(context: &ModelKnowledgeContext) -> String {
    let mut prompt = format!("消息：\n{}\n\n知识上下文：", context.reply_text());
    if context.is_no_hit() {
        prompt.push_str("\n未检索到匹配资料。");
    } else {
        for excerpt in context.excerpts() {
            prompt.push_str("\n- ");
            prompt.push_str(excerpt);
        }
    }
    prompt
}

fn validate_reply_text(text: String) -> Result<String, ContractError> {
    let text = text.trim().to_owned();
    if text.is_empty() || text.len() > MAX_REPLY_BYTES || text.chars().count() > MAX_REPLY_SCALARS {
        return Err(ContractError::LlmFailed);
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AiProvider, ModelConfig};
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct FakeTransport {
        calls: Mutex<Vec<SingleTurnTextRequest>>,
        response: String,
    }

    #[async_trait]
    impl SingleTurnTextTransport for FakeTransport {
        async fn complete(
            &self,
            _model: &ModelConfig,
            request: SingleTurnTextRequest,
        ) -> Result<String, crate::error::AppError> {
            self.calls.lock().unwrap().push(request);
            Ok(self.response.clone())
        }
    }

    #[cfg(feature = "wechat-m1")]
    fn m1_input(request_id: super::super::types::RequestId) -> M1ReplyInput {
        M1ReplyInput::from(
            super::super::types::OcrReadyReply::from_backend(
                super::super::types::CapturedWechat {
                    request_id,
                    capture_version: super::super::types::CaptureVersion::new(1),
                    stable_message_id: "fixture".into(),
                    is_single_chat: true,
                },
                super::super::types::OcrBackendResult::Text(
                    super::super::types::NormalizedOcrText::parse("已规范化文本").unwrap(),
                ),
            )
            .unwrap(),
        )
    }

    #[test]
    fn reply_text_rejects_empty_and_oversized_values() {
        assert_eq!(
            validate_reply_text("  ".to_string()),
            Err(ContractError::LlmFailed)
        );
        assert_eq!(
            validate_reply_text("a".repeat(MAX_REPLY_BYTES + 1)),
            Err(ContractError::LlmFailed)
        );
        assert_eq!(
            validate_reply_text("\0".repeat(MAX_REPLY_SCALARS + 1)),
            Err(ContractError::LlmFailed)
        );
    }

    #[tokio::test]
    async fn fake_transport_receives_one_fixed_system_and_one_user_prompt() {
        let fake = Arc::new(FakeTransport {
            calls: Mutex::new(Vec::new()),
            response: " 建议回复 ".into(),
        });
        let client = WechatReplyModelClient::with_transport(fake.clone());
        let text = client
            .complete(
                &ModelConfig {
                    provider: AiProvider::Ollama,
                    endpoint: "http://example.invalid".into(),
                    api_key: None,
                    model: "fixture".into(),
                },
                "已规范化文本",
            )
            .await
            .unwrap();

        assert_eq!(text, "建议回复");
        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].system_prompt(), WECHAT_SYSTEM_PROMPT);
        assert_eq!(calls[0].user_prompt(), "已规范化文本");
    }

    #[cfg(feature = "wechat-m1")]
    #[tokio::test]
    async fn m1_success_preserves_the_lease_generation_without_knowledge_context() {
        let fake = Arc::new(FakeTransport {
            calls: Mutex::new(Vec::new()),
            response: "建议回复".into(),
        });
        let client = WechatReplyModelClient::with_transport(fake.clone());
        let request_id = super::super::types::RequestId::new();
        let mut config = WechatConfig::default();
        config.text_model_profile_id = Some("selected".into());
        let profile = TextModelProfile {
            id: "selected".into(),
            name: "fixture".into(),
            test_status: "success".into(),
            last_tested_at: None,
            last_test_message: None,
            model_config: ModelConfig {
                provider: AiProvider::Ollama,
                endpoint: "http://fixture".into(),
                api_key: None,
                model: "fixture".into(),
            },
        };

        let reply = client
            .generate_m1(
                &config,
                &[profile],
                m1_input(request_id.clone()),
                SuggestionGeneration::new(7),
            )
            .await
            .unwrap();

        assert!(reply.is_current(&request_id, SuggestionGeneration::new(7), None));
        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].user_prompt(), "已规范化文本");
        assert!(!calls[0].user_prompt().contains("知识上下文"));
    }

    #[cfg(feature = "wechat-m1")]
    #[tokio::test]
    async fn m1_rejects_every_unavailable_profile_before_transport() {
        let fake = Arc::new(FakeTransport {
            calls: Mutex::new(Vec::new()),
            response: "建议回复".into(),
        });
        let client = WechatReplyModelClient::with_transport(fake.clone());
        let input = m1_input(super::super::types::RequestId::new());
        let suggestion = SuggestionGeneration::new(1);
        let profile = TextModelProfile {
            id: "selected".into(),
            name: "fixture".into(),
            test_status: "success".into(),
            last_tested_at: None,
            last_test_message: None,
            model_config: ModelConfig {
                provider: AiProvider::Ollama,
                endpoint: "http://fixture".into(),
                api_key: None,
                model: "fixture".into(),
            },
        };

        for (selected_id, candidate) in [
            (None, profile.clone()),
            (Some("unknown"), profile.clone()),
            (
                Some("selected"),
                TextModelProfile {
                    test_status: "untested".into(),
                    ..profile.clone()
                },
            ),
            (
                Some("selected"),
                TextModelProfile {
                    test_status: "error".into(),
                    ..profile.clone()
                },
            ),
            (
                Some("selected"),
                TextModelProfile {
                    model_config: ModelConfig {
                        model: " ".into(),
                        ..profile.model_config.clone()
                    },
                    ..profile.clone()
                },
            ),
        ] {
            let mut config = WechatConfig::default();
            config.text_model_profile_id = selected_id.map(str::to_owned);
            assert_eq!(
                client
                    .generate_m1(&config, &[candidate], input.clone(), suggestion)
                    .await,
                Err(ContractError::WxTextModelUnavailable),
            );
        }
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[cfg(feature = "wechat-m2")]
    #[tokio::test]
    async fn m2_allows_no_hit_and_includes_only_trusted_excerpts() {
        use crate::knowledge::types::{retrieval_fixture, RetrievalOutcome, RetrievalStatus};

        let fake = Arc::new(FakeTransport {
            calls: Mutex::new(Vec::new()),
            response: "建议回复".into(),
        });
        let client = WechatReplyModelClient::with_transport(fake.clone());
        let request_id = super::super::types::RequestId::new();
        let mut config = WechatConfig::default();
        config.text_model_profile_id = Some("selected".into());
        let profile = TextModelProfile {
            id: "selected".into(),
            name: "fixture".into(),
            test_status: "success".into(),
            last_tested_at: None,
            last_test_message: None,
            model_config: ModelConfig {
                provider: AiProvider::Ollama,
                endpoint: "http://fixture".into(),
                api_key: None,
                model: "fixture".into(),
            },
        };
        let no_hit = ModelKnowledgeContext::from_retrieved(
            retrieval_fixture(
                request_id.clone(),
                "请回复",
                RetrievalOutcome::Retrieved(RetrievalStatus::NoHit),
                &[],
                0,
            )
            .unwrap(),
            super::super::types::BindingGeneration::new(3),
        );

        let reply = client
            .generate_m2(
                &config,
                &[profile.clone()],
                no_hit,
                SuggestionGeneration::new(7),
            )
            .await
            .unwrap();
        assert!(reply.is_current(
            &request_id,
            SuggestionGeneration::new(7),
            Some(super::super::types::BindingGeneration::new(3)),
        ));
        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].user_prompt().contains("未检索到匹配资料。"));
        drop(calls);

        let excerpt_context = ModelKnowledgeContext::from_retrieved(
            retrieval_fixture(
                super::super::types::RequestId::new(),
                "请回复",
                RetrievalOutcome::Retrieved(RetrievalStatus::Success),
                &[("可信摘要", 1)],
                1,
            )
            .unwrap(),
            super::super::types::BindingGeneration::new(4),
        );
        client
            .generate_m2(
                &config,
                &[profile],
                excerpt_context,
                SuggestionGeneration::new(8),
            )
            .await
            .unwrap();
        let calls = fake.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert!(calls[1].user_prompt().contains("- 可信摘要"));
    }
}
