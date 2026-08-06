use super::types::{
    BindingGeneration, BindingObservationVersion, ContractError, RequestId, SuggestionGeneration,
};
use crate::knowledge::types::RetrievedReply;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplyMode { M1, M2 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplyState {
    Idle, Validating, Capturing, Ocr, Retrieving, Generating, ReplyReady, Copied, Dismissed, Cancelled, Failed,
}

#[derive(Clone, Debug)]
pub(crate) struct StateMachine {
    request_id: RequestId,
    mode: ReplyMode,
    state: ReplyState,
    stage_seq: u64,
    suggestion_generation: SuggestionGeneration,
    binding_generation: BindingGeneration,
    observation_version: BindingObservationVersion,
}

impl StateMachine {
    pub(crate) fn new(
        request_id: RequestId,
        mode: ReplyMode,
        suggestion_generation: SuggestionGeneration,
        binding_generation: BindingGeneration,
        observation_version: BindingObservationVersion,
    ) -> Self {
        Self {
            request_id,
            mode,
            state: ReplyState::Idle,
            stage_seq: 0,
            suggestion_generation,
            binding_generation,
            observation_version,
        }
    }

    pub(crate) fn advance(&mut self, next: ReplyState, stage_seq: u64) -> Result<(), ContractError> {
        if stage_seq <= self.stage_seq || !self.is_legal(next) {
            return Err(ContractError::WxContractViolation);
        }
        self.state = next;
        self.stage_seq = stage_seq;
        Ok(())
    }

    pub(crate) fn complete_retrieval(
        &mut self,
        reply: &RetrievedReply,
        stage_seq: u64,
    ) -> Result<(), ContractError> {
        if self.mode != ReplyMode::M2
            || self.state != ReplyState::Retrieving
            || self.request_id != *reply.request_id()
            || stage_seq <= self.stage_seq
        {
            return Err(ContractError::WxContractViolation);
        }

        self.state = ReplyState::Generating;
        self.stage_seq = stage_seq;
        Ok(())
    }

    pub(crate) fn fail_retrieval(
        &mut self,
        error: ContractError,
        stage_seq: u64,
    ) -> Result<(), ContractError> {
        if self.mode != ReplyMode::M2
            || self.state != ReplyState::Retrieving
            || stage_seq <= self.stage_seq
            || !matches!(
                error,
                ContractError::KbNotReady
                    | ContractError::KbScopeUnresolved
                    | ContractError::KbRetrievalFailed
            )
        {
            return Err(ContractError::WxContractViolation);
        }

        self.state = ReplyState::Failed;
        self.stage_seq = stage_seq;
        Ok(())
    }

    pub(crate) fn fail_ocr(
        &mut self,
        error: ContractError,
        stage_seq: u64,
    ) -> Result<(), ContractError> {
        if self.state != ReplyState::Ocr
            || stage_seq <= self.stage_seq
            || !matches!(
                error,
                ContractError::WxOcrEmpty | ContractError::WxOcrUnavailable | ContractError::WxOcrFailed
            )
        {
            return Err(ContractError::WxContractViolation);
        }
        self.state = ReplyState::Failed;
        self.stage_seq = stage_seq;
        Ok(())
    }

    pub(crate) fn accepts_reply(
        &self,
        request_id: &RequestId,
        suggestion: SuggestionGeneration,
        binding: Option<BindingGeneration>,
        observation: Option<BindingObservationVersion>,
    ) -> bool {
        self.state == ReplyState::Generating
            && self.request_id == *request_id
            && self.suggestion_generation == suggestion
            && (self.mode == ReplyMode::M1
                || (binding == Some(self.binding_generation)
                    && observation == Some(self.observation_version)))
    }

    pub(crate) fn state(&self) -> ReplyState { self.state }

    pub(crate) fn request_id(&self) -> &RequestId { &self.request_id }

    pub(crate) fn mode(&self) -> ReplyMode { self.mode }

    pub(crate) fn stage_seq(&self) -> u64 { self.stage_seq }

    pub(crate) fn suggestion_generation(&self) -> SuggestionGeneration { self.suggestion_generation }

    pub(crate) fn binding_generation(&self) -> BindingGeneration { self.binding_generation }

    pub(crate) fn observation_version(&self) -> BindingObservationVersion { self.observation_version }

    fn is_legal(&self, next: ReplyState) -> bool {
        use ReplyState::*;
        match (self.state, next) {
            (Idle, Validating)
            | (Validating, Capturing)
            | (Capturing, Ocr) => true,
            (Ocr, Generating) if self.mode == ReplyMode::M1 => true,
            (Ocr, Retrieving) if self.mode == ReplyMode::M2 => true,
            (Generating, ReplyReady)
            | (ReplyReady, Copied)
            | (ReplyReady, Dismissed)
            | (ReplyReady, Cancelled)
            | (Copied, Idle)
            | (Dismissed, Idle)
            | (Cancelled, Idle)
            | (Failed, Idle) => true,
            (Validating | Capturing | Ocr | Retrieving | Generating, Cancelled | Failed) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::types::{retrieval_fixture, RetrievalOutcome, RetrievalStatus};

    fn machine(mode: ReplyMode) -> StateMachine {
        StateMachine::new(
            RequestId::new(),
            mode,
            SuggestionGeneration::new(3),
            BindingGeneration::new(4),
            BindingObservationVersion::new(5),
        )
    }

    #[test]
    fn m1_cannot_retrieve_and_m2_must_retrieve() {
        let mut m1 = machine(ReplyMode::M1);
        m1.advance(ReplyState::Validating, 1).unwrap(); m1.advance(ReplyState::Capturing, 2).unwrap(); m1.advance(ReplyState::Ocr, 3).unwrap();
        assert_eq!(m1.advance(ReplyState::Retrieving, 4), Err(ContractError::WxContractViolation));
        m1.advance(ReplyState::Generating, 4).unwrap();

        let mut m2 = machine(ReplyMode::M2);
        m2.advance(ReplyState::Validating, 1).unwrap(); m2.advance(ReplyState::Capturing, 2).unwrap(); m2.advance(ReplyState::Ocr, 3).unwrap();
        assert_eq!(m2.advance(ReplyState::Generating, 4), Err(ContractError::WxContractViolation));
        m2.advance(ReplyState::Retrieving, 4).unwrap();
        assert_eq!(m2.advance(ReplyState::Generating, 5), Err(ContractError::WxContractViolation));
    }

    #[test]
    fn stage_sequence_is_strict_and_stale_m2_result_is_rejected() {
        let mut state = machine(ReplyMode::M2);
        state.advance(ReplyState::Validating, 1).unwrap();
        assert_eq!(state.advance(ReplyState::Capturing, 1), Err(ContractError::WxContractViolation));
        state.advance(ReplyState::Capturing, 2).unwrap(); state.advance(ReplyState::Ocr, 3).unwrap(); state.advance(ReplyState::Retrieving, 4).unwrap();
        let reply = retrieval_fixture(
            state.request_id.clone(),
            "请回复",
            RetrievalOutcome::Retrieved(RetrievalStatus::Success),
            &[],
            0,
        ).unwrap();
        state.complete_retrieval(&reply, 5).unwrap();
        assert!(!state.accepts_reply(
            &RequestId::new(),
            SuggestionGeneration::new(3),
            Some(BindingGeneration::new(4)),
            Some(BindingObservationVersion::new(5)),
        ));
        assert!(!state.accepts_reply(
            &state.request_id,
            SuggestionGeneration::new(3),
            Some(BindingGeneration::new(5)),
            Some(BindingObservationVersion::new(5)),
        ));
        assert!(!state.accepts_reply(
            &state.request_id,
            SuggestionGeneration::new(3),
            Some(BindingGeneration::new(4)),
            Some(BindingObservationVersion::new(6)),
        ));
        assert!(state.accepts_reply(
            &state.request_id,
            SuggestionGeneration::new(3),
            Some(BindingGeneration::new(4)),
            Some(BindingObservationVersion::new(5)),
        ));
    }

    #[test]
    fn ocr_failures_terminate_before_model_or_retrieval_stages() {
        for error in [
            ContractError::WxOcrEmpty,
            ContractError::WxOcrUnavailable,
            ContractError::WxOcrFailed,
        ] {
            let mut state = machine(ReplyMode::M2);
            state.advance(ReplyState::Validating, 1).unwrap();
            state.advance(ReplyState::Capturing, 2).unwrap();
            state.advance(ReplyState::Ocr, 3).unwrap();
            state.fail_ocr(error, 4).unwrap();
            assert_eq!(state.state(), ReplyState::Failed);
            assert_eq!(state.advance(ReplyState::Retrieving, 5), Err(ContractError::WxContractViolation));
            assert_eq!(state.advance(ReplyState::Generating, 5), Err(ContractError::WxContractViolation));
        }
    }
}
