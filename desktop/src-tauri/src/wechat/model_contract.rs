use super::types::{BindingGeneration, RequestId};
use crate::knowledge::types::RetrievedReply;

#[cfg(any(feature = "wechat-m1", feature = "wechat-m2"))]
use super::types::{GeneratedReply, SuggestionGeneration};
#[cfg(feature = "wechat-m1")]
use super::types::M1ReplyInput;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModelKnowledgeContext {
    request_id: RequestId,
    reply_text: String,
    excerpts: Vec<String>,
    no_hit: bool,
    binding_generation: BindingGeneration,
}

impl ModelKnowledgeContext {
    // This adapter is intentionally the only constructor. RetrievedReply has no public fields.
    pub(in crate::wechat) fn from_retrieved(reply: RetrievedReply, binding_generation: BindingGeneration) -> Self {
        Self {
            request_id: reply.request_id().clone(),
            reply_text: reply.query().to_owned(),
            excerpts: reply.excerpts(),
            no_hit: reply.is_no_hit(),
            binding_generation,
        }
    }

    pub(crate) fn is_no_hit(&self) -> bool { self.no_hit }

    #[cfg(test)]
    fn excerpts(&self) -> &[String] { &self.excerpts }
}

#[cfg(feature = "wechat-m1")]
pub(crate) fn generate_m1_reply(input: M1ReplyInput, suggestion: SuggestionGeneration) -> GeneratedReply {
    GeneratedReply::m1(input.request_id().clone(), suggestion, input.text().to_owned())
}

#[cfg(feature = "wechat-m2")]
pub(crate) fn generate_rag_reply(context: ModelKnowledgeContext, suggestion: SuggestionGeneration) -> GeneratedReply {
    GeneratedReply::m2(context.request_id, suggestion, context.binding_generation, context.reply_text)
}

#[cfg(all(feature = "wechat-contract-check", not(any(feature = "wechat-m1", feature = "wechat-m2"))))]
compile_error!("a WeChat release contract check requires exactly one of wechat-m1 or wechat-m2");

#[cfg(all(feature = "wechat-contract-check", feature = "wechat-m1", feature = "wechat-m2"))]
compile_error!("WeChat release contract checks cannot enable both wechat-m1 and wechat-m2");

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::types::{retrieval_fixture, RetrievalOutcome, RetrievalStatus};
    use crate::wechat::state_machine::{ReplyMode, ReplyState, StateMachine};
    use crate::wechat::types::{BindingObservationVersion, ContractError, SuggestionGeneration};
    use serde::Deserialize;

    #[derive(Default)]
    struct FakeModelTransport {
        calls: u32,
    }

    impl FakeModelTransport {
        fn generate(&mut self, _context: &ModelKnowledgeContext) {
            self.calls += 1;
        }
    }

    fn retrieving_m2(request_id: RequestId) -> StateMachine {
        let mut state = StateMachine::new(
            request_id,
            ReplyMode::M2,
            SuggestionGeneration::new(1),
            BindingGeneration::new(2),
            BindingObservationVersion::new(3),
        );
        state.advance(ReplyState::Validating, 1).unwrap();
        state.advance(ReplyState::Capturing, 2).unwrap();
        state.advance(ReplyState::Ocr, 3).unwrap();
        state.advance(ReplyState::Retrieving, 4).unwrap();
        state
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RetrievalFailedFixture {
        expected_error: ContractError,
        model_transport_calls: u32,
    }

    #[test]
    fn retrieval_failure_ends_m2_before_context_or_model_transport() {
        let fixture: RetrievalFailedFixture = serde_json::from_str(include_str!(
            "../../tests/fixtures/wechat_contract/retrieval_failed.json"
        )).unwrap();
        let request_id = RequestId::new();
        let mut state = retrieving_m2(request_id.clone());
        let retrieval = retrieval_fixture(
            request_id,
            "请回复",
            RetrievalOutcome::Failed(fixture.expected_error),
            &[],
            0,
        );
        let transport = FakeModelTransport::default();

        let error = retrieval.unwrap_err();
        state.fail_retrieval(error, 5).unwrap();

        assert_eq!(error, ContractError::KbRetrievalFailed);
        assert_eq!(state.state(), ReplyState::Failed);
        assert_eq!(transport.calls, fixture.model_transport_calls);
    }

    #[test]
    fn no_hit_is_the_only_empty_retrieval_that_reaches_generation() {
        let request_id = RequestId::new();
        let reply = retrieval_fixture(
            request_id.clone(),
            "请回复",
            RetrievalOutcome::Retrieved(RetrievalStatus::NoHit),
            &[],
            0,
        ).unwrap();
        let mut state = retrieving_m2(request_id);
        state.complete_retrieval(&reply, 5).unwrap();
        let context = ModelKnowledgeContext::from_retrieved(reply, BindingGeneration::new(2));
        let mut transport = FakeModelTransport::default();
        transport.generate(&context);

        assert_eq!(state.state(), ReplyState::Generating);
        assert!(context.is_no_hit());
        assert!(context.excerpts().is_empty());
        assert_eq!(transport.calls, 1);
    }

    #[test]
    fn model_context_receives_only_budgeted_excerpts() {
        let reply = retrieval_fixture(
            RequestId::new(),
            "请回复",
            RetrievalOutcome::Retrieved(RetrievalStatus::Success),
            &[("预算内摘要", 3), ("超预算摘要", 2)],
            4,
        ).unwrap();
        let context = ModelKnowledgeContext::from_retrieved(reply, BindingGeneration::new(2));

        assert_eq!(context.excerpts(), &["预算内摘要"]);
    }
}
