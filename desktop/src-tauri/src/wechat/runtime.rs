#[cfg(any(feature = "wechat-m1", feature = "wechat-m2"))]
use super::config::selected_verified_model;
use super::content::{WechatContentKind, WechatContentStore};
#[cfg(any(feature = "wechat-m1", feature = "wechat-m2"))]
use super::model_client::WechatReplyModelClient;
#[cfg(feature = "wechat-m2")]
use super::model_contract::ModelKnowledgeContext;
use super::state_machine::{ReplyMode, ReplyState, StateMachine};
use super::trace::{M2TraceMetadata, ReplyTraceEvent, ReplyTraceStore};
#[cfg(feature = "wechat-m1")]
use super::types::M1ReplyInput;
use super::types::{
    BindingGeneration, BindingObservationVersion, CaptureVersion, ContractError, GeneratedReply,
    OcrReadyReply, RequestId, SuggestionGeneration,
};
#[cfg(any(feature = "wechat-m1", feature = "wechat-m2"))]
use crate::config::{TextModelProfile, WechatConfig};
use crate::knowledge::types::RetrievedReply;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Metadata-only authority for one in-process reply. It intentionally holds no
/// application state, capture pixels, OCR text, excerpts, suggestions, database,
/// or model client. The trace handle is the sole durable side effect.
#[derive(Clone)]
pub(crate) struct WechatReplyRuntime {
    inner: Arc<Mutex<RuntimeInner>>,
}

struct RuntimeInner {
    next_suggestion_generation: u64,
    active: Option<ActiveReply>,
}

struct ActiveReply {
    state: StateMachine,
    capture_version: Option<CaptureVersion>,
    deadline: Instant,
    cancel: tokio::sync::watch::Sender<bool>,
    trace: ReplyTraceStore,
    model_transport_calls: u8,
}

#[derive(Clone, Debug)]
pub(crate) struct BeginReplySnapshot {
    pub(crate) mode: ReplyMode,
    pub(crate) binding_generation: BindingGeneration,
    pub(crate) observation_version: BindingObservationVersion,
    pub(crate) capture_version: Option<CaptureVersion>,
    pub(crate) timeout: Duration,
}

/// A private, unforgeable capability. It contains no body data and no path.
#[derive(Clone, Debug)]
pub(crate) struct ReplyLease {
    request_id: RequestId,
    suggestion_generation: SuggestionGeneration,
}

impl ReplyLease {
    pub(crate) fn request_id(&self) -> &RequestId {
        &self.request_id
    }
    pub(crate) fn suggestion_generation(&self) -> SuggestionGeneration {
        self.suggestion_generation
    }
}

impl Default for WechatReplyRuntime {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RuntimeInner {
                next_suggestion_generation: 0,
                active: None,
            })),
        }
    }
}

impl WechatReplyRuntime {
    /// Claims the authoritative slot and makes `validating` durable before
    /// exposing a lease. A busy request is never created or traced.
    pub(crate) fn begin_reply(
        &self,
        snapshot: BeginReplySnapshot,
        trace: ReplyTraceStore,
    ) -> Result<ReplyLease, ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        if inner.active.is_some() {
            return Err(ContractError::WxBusy);
        }
        let suggestion_generation =
            SuggestionGeneration::new(inner.next_suggestion_generation.saturating_add(1));
        let request_id = RequestId::new();
        let mut state = StateMachine::new(
            request_id.clone(),
            snapshot.mode,
            suggestion_generation,
            snapshot.binding_generation,
            snapshot.observation_version,
        );
        let (cancel, _) = tokio::sync::watch::channel(false);
        let active = ActiveReply {
            state: state.clone(),
            capture_version: snapshot.capture_version,
            deadline: Instant::now() + snapshot.timeout,
            cancel,
            trace: trace.clone(),
            model_transport_calls: 0,
        };
        let event = event_for(&active, ReplyState::Validating, None, None)?;
        trace.append(event)?;
        state.advance(ReplyState::Validating, 1)?;
        inner.next_suggestion_generation = suggestion_generation.value();
        inner.active = Some(ActiveReply { state, ..active });
        Ok(ReplyLease {
            request_id,
            suggestion_generation,
        })
    }

    /// Stages are checked, traced, and committed while the same lock is held.
    /// Any trace failure clears the slot and prevents downstream continuation.
    pub(crate) fn transition(
        &self,
        lease: &ReplyLease,
        expected: ReplyState,
        next: ReplyState,
        error: Option<ContractError>,
        m2: Option<M2TraceMetadata>,
    ) -> Result<(), ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        let Some(active) = inner.active.as_mut() else {
            return Err(ContractError::WxRequestStale);
        };
        if !lease_matches(active, lease) {
            if let Err(error) = record_stale(active, lease) {
                inner.active = None;
                return Err(error);
            }
            return Err(ContractError::WxRequestStale);
        }
        if active.state.state() != expected {
            return fail_closed(&mut inner, ContractError::WxContractViolation);
        }
        if *active.cancel.borrow() || Instant::now() >= active.deadline {
            let result = transition_terminal(
                active,
                ReplyState::Cancelled,
                Some(ContractError::WxRequestCancelled),
            );
            if result.is_err() {
                inner.active = None;
            }
            return result;
        }
        if next == ReplyState::ReplyReady && active.model_transport_calls != 1 {
            return fail_closed(&mut inner, ContractError::WxContractViolation);
        }
        let mut candidate = active.state.clone();
        if candidate
            .advance(next, candidate.stage_seq().saturating_add(1))
            .is_err()
        {
            return fail_closed(&mut inner, ContractError::WxContractViolation);
        }
        let event = event_for(active, next, error, m2)?;
        if let Err(error) = active.trace.append(event) {
            inner.active = None;
            return Err(error);
        }
        active.state = candidate;
        Ok(())
    }

    /// Binds the version allocated by the guarded capture before the OCR event
    /// is written. This keeps the lease, capture slices, and trace on one
    /// request without exposing mutable runtime state to callers.
    pub(crate) fn enter_ocr_after_capture(
        &self,
        lease: &ReplyLease,
        capture_version: CaptureVersion,
    ) -> Result<(), ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        let Some(active) = inner.active.as_mut() else {
            return Err(ContractError::WxRequestStale);
        };
        if !lease_matches(active, lease) {
            if let Err(error) = record_stale(active, lease) {
                inner.active = None;
                return Err(error);
            }
            return Err(ContractError::WxRequestStale);
        }
        if active.state.state() != ReplyState::Capturing {
            return fail_closed(&mut inner, ContractError::WxContractViolation);
        }
        if *active.cancel.borrow() || Instant::now() >= active.deadline {
            let result = transition_terminal(
                active,
                ReplyState::Cancelled,
                Some(ContractError::WxRequestCancelled),
            );
            if result.is_err() {
                inner.active = None;
            }
            return result;
        }
        let mut candidate = active.state.clone();
        if candidate
            .advance(ReplyState::Ocr, candidate.stage_seq().saturating_add(1))
            .is_err()
        {
            return fail_closed(&mut inner, ContractError::WxContractViolation);
        }
        active.capture_version = Some(capture_version);
        let event = event_for(active, ReplyState::Ocr, None, None)?;
        if let Err(error) = active.trace.append(event) {
            inner.active = None;
            return Err(error);
        }
        active.state = candidate;
        Ok(())
    }

    /// M2 is the one non-generic edge: the existing state machine verifies that
    /// the retrieved envelope belongs to this request before generation starts.
    pub(crate) fn complete_retrieval(
        &self,
        lease: &ReplyLease,
        reply: &RetrievedReply,
        m2: M2TraceMetadata,
    ) -> Result<(), ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        let Some(active) = inner.active.as_mut() else {
            return Err(ContractError::WxRequestStale);
        };
        if !lease_matches(active, lease) {
            if let Err(error) = record_stale(active, lease) {
                inner.active = None;
                return Err(error);
            }
            return Err(ContractError::WxRequestStale);
        }
        if *active.cancel.borrow() || Instant::now() >= active.deadline {
            let result = transition_terminal(
                active,
                ReplyState::Cancelled,
                Some(ContractError::WxRequestCancelled),
            );
            if result.is_err() {
                inner.active = None;
            }
            return result;
        }
        let mut candidate = active.state.clone();
        if candidate
            .complete_retrieval(reply, candidate.stage_seq().saturating_add(1))
            .is_err()
        {
            return fail_closed(&mut inner, ContractError::WxContractViolation);
        }
        let event = event_for(active, ReplyState::Generating, None, Some(m2))?;
        if let Err(error) = active.trace.append(event) {
            inner.active = None;
            return Err(error);
        }
        active.state = candidate;
        Ok(())
    }

    pub(crate) fn cancel_reply(&self, lease: &ReplyLease) -> Result<(), ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        let Some(active) = inner.active.as_mut() else {
            return Err(ContractError::WxRequestStale);
        };
        if !lease_matches(active, lease) {
            if let Err(error) = record_stale(active, lease) {
                inner.active = None;
                return Err(error);
            }
            return Err(ContractError::WxRequestStale);
        }
        let _ = active.cancel.send(true);
        let result = transition_terminal(
            active,
            ReplyState::Cancelled,
            Some(ContractError::WxRequestCancelled),
        );
        if result.is_err() {
            inner.active = None;
        }
        result
    }

    /// Releases the active slot only after a terminal stage has been persisted.
    pub(crate) fn finish_reply(&self, lease: ReplyLease) -> Result<(), ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        let Some(active) = inner.active.as_mut() else {
            return Err(ContractError::WxRequestStale);
        };
        if !lease_matches(active, &lease) {
            if let Err(error) = record_stale(active, &lease) {
                inner.active = None;
                return Err(error);
            }
            return Err(ContractError::WxRequestStale);
        }
        if !matches!(
            active.state.state(),
            ReplyState::ReplyReady | ReplyState::Failed | ReplyState::Cancelled
        ) {
            return Err(ContractError::WxContractViolation);
        }
        inner.active = None;
        Ok(())
    }

    pub(crate) fn cancellation_receiver(
        &self,
        lease: &ReplyLease,
    ) -> Result<tokio::sync::watch::Receiver<bool>, ContractError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        let active = inner.active.as_ref().ok_or(ContractError::WxRequestStale)?;
        if !lease_matches(active, lease) {
            return Err(ContractError::WxRequestStale);
        }
        Ok(active.cancel.subscribe())
    }

    pub(crate) fn record_model_transport_call(
        &self,
        lease: &ReplyLease,
    ) -> Result<(), ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        let active = inner.active.as_mut().ok_or(ContractError::WxRequestStale)?;
        if !lease_matches(active, lease) {
            return Err(ContractError::WxRequestStale);
        }
        if active.state.state() != ReplyState::Generating || active.model_transport_calls >= 1 {
            return fail_closed(&mut inner, ContractError::WxContractViolation);
        }
        active.model_transport_calls += 1;
        Ok(())
    }

    /// Commits a generated reply only for the active generation after exactly
    /// one transport call. The reply itself carries no observation data, so the
    /// current observation is read only while this lock is held.
    pub(crate) fn complete_generated_reply(
        &self,
        lease: &ReplyLease,
        reply: &GeneratedReply,
    ) -> Result<(), ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        let Some(active) = inner.active.as_mut() else {
            return Err(ContractError::WxRequestStale);
        };
        if !lease_matches(active, lease) {
            if let Err(error) = record_stale(active, lease) {
                inner.active = None;
                return Err(error);
            }
            return Err(ContractError::WxRequestStale);
        }
        let binding =
            (active.state.mode() == ReplyMode::M2).then_some(active.state.binding_generation());
        if active.model_transport_calls != 1
            || !reply.is_current(lease.request_id(), lease.suggestion_generation(), binding)
            || !active.state.accepts_reply(
                lease.request_id(),
                lease.suggestion_generation(),
                binding,
                (active.state.mode() == ReplyMode::M2)
                    .then_some(active.state.observation_version()),
            )
        {
            return fail_closed(&mut inner, ContractError::WxContractViolation);
        }
        if *active.cancel.borrow() || Instant::now() >= active.deadline {
            let result = transition_terminal(
                active,
                ReplyState::Cancelled,
                Some(ContractError::WxRequestCancelled),
            );
            inner.active = None;
            return result.and(Err(ContractError::WxRequestCancelled));
        }
        let mut candidate = active.state.clone();
        candidate
            .advance(
                ReplyState::ReplyReady,
                candidate.stage_seq().saturating_add(1),
            )
            .map_err(|_| ContractError::WxContractViolation)?;
        let event = event_for(active, ReplyState::ReplyReady, None, None)?;
        if let Err(error) = active.trace.append(event) {
            inner.active = None;
            return Err(error);
        }
        active.state = candidate;
        Ok(())
    }

    #[cfg(feature = "wechat-m1")]
    pub(super) async fn generate_m1_reply_with_client(
        &self,
        client: &WechatReplyModelClient,
        config: WechatConfig,
        profiles: Vec<TextModelProfile>,
        input: M1ReplyInput,
        lease: &ReplyLease,
    ) -> Result<GeneratedReply, ContractError> {
        if let Err(error) = selected_verified_model(&config, &profiles) {
            self.fail_model_generation(lease, error)?;
            return Err(error);
        }
        self.record_model_transport_call(lease)?;
        match client
            .generate_m1(&config, &profiles, input, lease.suggestion_generation())
            .await
        {
            Ok(reply) => {
                self.complete_generated_reply(lease, &reply)?;
                Ok(reply)
            }
            Err(_) => {
                self.fail_model_generation(lease, ContractError::LlmFailed)?;
                Err(ContractError::LlmFailed)
            }
        }
    }

    /// Runs exactly one private M2 model call after retrieval has created a
    /// trusted context and the caller has copied the configuration snapshot.
    #[cfg(feature = "wechat-m2")]
    pub(crate) async fn generate_m2_reply(
        &self,
        config: WechatConfig,
        profiles: Vec<TextModelProfile>,
        context: ModelKnowledgeContext,
        lease: &ReplyLease,
    ) -> Result<GeneratedReply, ContractError> {
        self.generate_m2_reply_with_client(
            &WechatReplyModelClient::new(),
            config,
            profiles,
            context,
            lease,
        )
        .await
    }

    #[cfg(feature = "wechat-m2")]
    async fn generate_m2_reply_with_client(
        &self,
        client: &WechatReplyModelClient,
        config: WechatConfig,
        profiles: Vec<TextModelProfile>,
        context: ModelKnowledgeContext,
        lease: &ReplyLease,
    ) -> Result<GeneratedReply, ContractError> {
        if let Err(error) = selected_verified_model(&config, &profiles) {
            self.fail_model_generation(lease, error)?;
            return Err(error);
        }
        self.record_model_transport_call(lease)?;
        match client
            .generate_m2(&config, &profiles, context, lease.suggestion_generation())
            .await
        {
            Ok(reply) => {
                self.complete_generated_reply(lease, &reply)?;
                Ok(reply)
            }
            Err(_) => {
                self.fail_model_generation(lease, ContractError::LlmFailed)?;
                Err(ContractError::LlmFailed)
            }
        }
    }

    fn fail_model_generation(
        &self,
        lease: &ReplyLease,
        error: ContractError,
    ) -> Result<(), ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        let Some(active) = inner.active.as_mut() else {
            return Err(ContractError::WxRequestStale);
        };
        if !lease_matches(active, lease) {
            if let Err(trace_error) = record_stale(active, lease) {
                inner.active = None;
                return Err(trace_error);
            }
            return Err(ContractError::WxRequestStale);
        }
        if active.state.state() != ReplyState::Generating {
            return fail_closed(&mut inner, ContractError::WxContractViolation);
        }
        let result = if *active.cancel.borrow() || Instant::now() >= active.deadline {
            transition_terminal(
                active,
                ReplyState::Cancelled,
                Some(ContractError::WxRequestCancelled),
            )
        } else {
            transition_terminal(active, ReplyState::Failed, Some(error))
        };
        inner.active = None;
        result
    }

    /// The only content-write path is runtime-owned: a current lease, the
    /// completed source stage, and the latest retention snapshot are all
    /// checked under the active-request lock. Disabling retention also removes
    /// any earlier body persisted for this request.
    pub(super) fn retain_ocr_content(
        &self,
        lease: &ReplyLease,
        store: &WechatContentStore,
        reply: &OcrReadyReply,
        retention_enabled: bool,
        retention_days: u16,
    ) -> Result<bool, ContractError> {
        if reply.request_id() != lease.request_id() {
            return Err(ContractError::WxRequestStale);
        }
        self.retain_content(
            store,
            lease,
            WechatContentKind::OcrText,
            reply.text().as_bytes(),
            retention_enabled,
            retention_days,
        )
    }

    pub(super) fn retain_suggestion_content(
        &self,
        lease: &ReplyLease,
        store: &WechatContentStore,
        reply: &GeneratedReply,
        retention_enabled: bool,
        retention_days: u16,
    ) -> Result<bool, ContractError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        let active = inner.active.as_ref().ok_or(ContractError::WxRequestStale)?;
        if !lease_matches(active, lease)
            || !reply.is_current(
                lease.request_id(),
                lease.suggestion_generation(),
                (active.state.mode() == ReplyMode::M2).then_some(active.state.binding_generation()),
            )
        {
            return Err(ContractError::WxRequestStale);
        }
        drop(inner);
        self.retain_content(
            store,
            lease,
            WechatContentKind::Suggestion,
            reply.text().as_bytes(),
            retention_enabled,
            retention_days,
        )
    }

    /// Keeps the raw-byte operation private so callers cannot pair an
    /// arbitrary request UUID with arbitrary data. The typed helpers above are
    /// the only capability-bearing entry points.
    fn retain_content(
        &self,
        store: &WechatContentStore,
        lease: &ReplyLease,
        kind: WechatContentKind,
        body: &[u8],
        retention_enabled: bool,
        retention_days: u16,
    ) -> Result<bool, ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        let active = inner.active.as_mut().ok_or(ContractError::WxRequestStale)?;
        if !lease_matches(active, lease) {
            return Err(ContractError::WxRequestStale);
        }
        if *active.cancel.borrow() || Instant::now() >= active.deadline {
            let result = transition_terminal(
                active,
                ReplyState::Cancelled,
                Some(ContractError::WxRequestCancelled),
            );
            if result.is_err() {
                inner.active = None;
            }
            return result.map(|_| false);
        }
        if !retention_enabled || retention_days == 0 || retention_days > 30 {
            if store.delete_request(lease.request_id()).is_err() {
                return fail_closed(&mut inner, ContractError::WxContentPersistFailed)
                    .map(|_| false);
            }
            return Ok(false);
        }
        if !content_stage_is_valid(kind, active.state.state()) {
            return fail_closed(&mut inner, ContractError::WxContractViolation).map(|_| false);
        }
        match store.retain(
            lease.request_id(),
            kind,
            body,
            retention_enabled,
            retention_days,
        ) {
            Ok(retained) => Ok(retained),
            Err(_) => fail_closed(&mut inner, ContractError::WxContentPersistFailed).map(|_| false),
        }
    }
}

fn lease_matches(active: &ActiveReply, lease: &ReplyLease) -> bool {
    active.state.request_id() == lease.request_id()
        && active.state.suggestion_generation() == lease.suggestion_generation()
}

fn event_for(
    active: &ActiveReply,
    next: ReplyState,
    error: Option<ContractError>,
    m2: Option<M2TraceMetadata>,
) -> Result<ReplyTraceEvent, ContractError> {
    Ok(ReplyTraceEvent::stage(
        active.state.request_id(),
        active.state.stage_seq().saturating_add(1),
        next,
        active.capture_version,
        active.state.suggestion_generation(),
        active.state.binding_generation(),
        active.state.observation_version(),
        active.model_transport_calls,
        error,
        m2,
    ))
}

fn transition_terminal(
    active: &mut ActiveReply,
    next: ReplyState,
    error: Option<ContractError>,
) -> Result<(), ContractError> {
    if matches!(
        active.state.state(),
        ReplyState::ReplyReady | ReplyState::Failed | ReplyState::Cancelled
    ) {
        return Ok(());
    }
    let mut candidate = active.state.clone();
    candidate.advance(next, candidate.stage_seq().saturating_add(1))?;
    let event = event_for(active, next, error, None)?;
    active.trace.append(event)?;
    active.state = candidate;
    Ok(())
}

/// A protocol violation belongs to the current lease, so it must not leave the
/// single-flight slot waiting for a best-effort caller cleanup. If persistence
/// of the terminal event fails, the slot is still released and the trace error
/// is returned instead of pretending the request was auditable.
fn fail_closed(inner: &mut RuntimeInner, error: ContractError) -> Result<(), ContractError> {
    let terminal = inner
        .active
        .as_mut()
        .ok_or(ContractError::WxRequestStale)
        .and_then(|active| transition_terminal(active, ReplyState::Failed, Some(error)));
    inner.active = None;
    match terminal {
        Ok(()) => Err(error),
        Err(trace_error) => Err(trace_error),
    }
}

fn content_stage_is_valid(kind: WechatContentKind, state: ReplyState) -> bool {
    match kind {
        WechatContentKind::Capture => matches!(
            state,
            ReplyState::Ocr
                | ReplyState::Retrieving
                | ReplyState::Generating
                | ReplyState::ReplyReady
        ),
        WechatContentKind::OcrText => matches!(
            state,
            ReplyState::Retrieving | ReplyState::Generating | ReplyState::ReplyReady
        ),
        WechatContentKind::RetrievalExcerpt => {
            matches!(state, ReplyState::Generating | ReplyState::ReplyReady)
        }
        WechatContentKind::Suggestion => state == ReplyState::ReplyReady,
    }
}

fn record_stale(active: &mut ActiveReply, _lease: &ReplyLease) -> Result<(), ContractError> {
    active.trace.append(ReplyTraceEvent::stale_result(
        active.state.request_id(),
        active.state.stage_seq().max(1),
        active.state.state(),
        active.state.suggestion_generation(),
        active.state.binding_generation(),
        active.state.observation_version(),
        active.model_transport_calls,
    ))
}

impl WechatReplyRuntime {
    /// Private OCR hand-off for a verified capture. It has no Tauri command and
    /// cannot progress to retrieval or a model after an OCR failure.
    pub(crate) fn recognize_captured_wechat<S: super::ocr::WechatOcrAuditSink>(
        &self,
        slices: &super::capture::WechatCaptureSlices,
        identity: &super::window_identity::WechatWindowIdentity,
        captured: super::types::CapturedWechat,
        state: &mut super::state_machine::StateMachine,
        stage_seq: u64,
        audit_sink: &mut S,
    ) -> Result<super::types::OcrReadyReply, super::types::ContractError> {
        let mut dispatcher = super::ocr::WechatOcrDispatcher::new(
            super::ocr::WindowsMemoryPrimary,
            super::ocr::DisabledLocalFallback,
        );
        match dispatcher.recognize(slices, identity, captured, audit_sink) {
            Ok(reply) => Ok(reply),
            Err(
                error @ (super::types::ContractError::WxOcrEmpty
                | super::types::ContractError::WxOcrUnavailable
                | super::types::ContractError::WxOcrFailed),
            ) => {
                state.fail_ocr(error, stage_seq)?;
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

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

/// Serializes Work Review captures and the private WeChat capture transaction.
/// Callers must use `try_acquire` so background recording never queues work.
pub(crate) struct CaptureCoordinator {
    permit: std::sync::Arc<tokio::sync::Semaphore>,
    latest_capture_version: std::sync::atomic::AtomicU64,
}

impl Default for CaptureCoordinator {
    fn default() -> Self {
        Self {
            permit: std::sync::Arc::new(tokio::sync::Semaphore::new(1)),
            latest_capture_version: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl CaptureCoordinator {
    pub(crate) fn try_acquire(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.permit.clone().try_acquire_owned().ok()
    }

    /// Allocate only after a complete capture has passed revalidation and ROI
    /// checks, so every returned slice has a strictly newer version.
    pub(crate) fn next_capture_version(&self) -> super::types::CaptureVersion {
        let value = self
            .latest_capture_version
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .saturating_add(1);
        super::types::CaptureVersion::new(value)
    }

    pub(crate) fn is_current_capture(&self, version: super::types::CaptureVersion) -> bool {
        version.value()
            == self
                .latest_capture_version
                .load(std::sync::atomic::Ordering::Relaxed)
    }
}

async fn cancellation_requested(cancel: &mut Option<tokio::sync::watch::Receiver<bool>>) {
    let Some(receiver) = cancel else {
        std::future::pending::<()>().await;
        return;
    };
    loop {
        if *receiver.borrow() {
            return;
        }
        if receiver.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

async fn wait_for_capture_worker<T>(
    worker: &mut tokio::task::JoinHandle<Result<T, super::types::ContractError>>,
    timeout: std::time::Duration,
    cancel: &mut Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<T, super::types::ContractError> {
    if cancel.as_ref().is_some_and(|receiver| *receiver.borrow()) {
        let _ = worker.await;
        return Err(super::types::ContractError::WxRequestCancelled);
    }

    tokio::select! {
        result = tokio::time::timeout(timeout, &mut *worker) => match result {
            Ok(Ok(Ok(frame))) => Ok(frame),
            Ok(Ok(Err(error))) => Err(error),
            Ok(Err(_)) => Err(super::types::ContractError::WxCaptureFailed),
            // spawn_blocking cannot be aborted safely. Joining before restore
            // prevents an overdue worker from capturing an already restored overlay.
            Err(_) => {
                let _ = worker.await;
                Err(super::types::ContractError::WxCaptureTimeout)
            }
        },
        _ = cancellation_requested(cancel) => {
            let _ = worker.await;
            Err(super::types::ContractError::WxRequestCancelled)
        },
    }
}

async fn run_guarded_capture<T, V, P, Before, Worker, After>(
    mut guard: super::capture::WechatCaptureGuard<P>,
    before_worker: Before,
    worker: Worker,
    after_worker: After,
    timeout: std::time::Duration,
    cancel: &mut Option<tokio::sync::watch::Receiver<bool>>,
) -> Result<(T, V), super::types::ContractError>
where
    P: super::capture::WechatWindowPort,
    Before: FnOnce() -> Result<(), super::types::ContractError>,
    Worker: FnOnce() -> tokio::task::JoinHandle<Result<T, super::types::ContractError>>,
    After: FnOnce() -> Result<V, super::types::ContractError>,
{
    let outcome = async {
        before_worker()?;
        let mut worker = worker();
        let frame = wait_for_capture_worker(&mut worker, timeout, cancel).await?;
        Ok((frame, after_worker()?))
    }
    .await;
    guard.finish_after_worker();
    outcome
}

impl WechatReplyRuntime {
    /// Runs the private, memory-only capture transaction. There is deliberately
    /// no Tauri command for this method: a later authorized step owns its input.
    #[cfg(target_os = "windows")]
    pub(crate) async fn capture_foreground_wechat(
        &self,
        app: tauri::AppHandle,
        coordinator: &CaptureCoordinator,
        screenshot_service: crate::screenshot::ScreenshotService,
        identity: super::window_identity::WechatWindowIdentity,
        request_id: super::types::RequestId,
        timeout: std::time::Duration,
        mut cancel: Option<tokio::sync::watch::Receiver<bool>>,
    ) -> Result<
        (
            super::capture::WechatCaptureSlices,
            super::window_identity::WechatWindowIdentity,
        ),
        super::types::ContractError,
    > {
        let _permit = coordinator
            .try_acquire()
            .ok_or(super::types::ContractError::WxBusy)?;
        let guard = super::capture::WechatCaptureGuard::begin(
            super::capture::TauriWechatWindowPort::new(app),
        )?;
        let (frame, current) = run_guarded_capture(
            guard,
            || self.revalidate_foreground_wechat(&identity).map(|_| ()),
            || {
                let target_window = identity.capture_target_window();
                tokio::task::spawn_blocking(move || {
                    screenshot_service
                        .capture_ephemeral_for_window(&target_window)
                        .map_err(|_| super::types::ContractError::WxCaptureFailed)
                })
            },
            || self.revalidate_foreground_wechat(&identity),
            timeout,
            &mut cancel,
        )
        .await?;
        let (chat_rgba, header_identity_rgba) = super::capture::cropped_images_for_identity(
            super::capture::EphemeralCapturedFrame::new(frame),
            &current,
        )?;
        Ok((
            super::capture::WechatCaptureSlices {
                request_id,
                capture_version: coordinator.next_capture_version(),
                chat_rgba,
                header_identity_rgba,
            },
            current,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier, Mutex};

    #[derive(Clone)]
    struct FakeWindowPort {
        calls: Arc<Mutex<Vec<&'static str>>>,
        avatar_hidden: Arc<std::sync::atomic::AtomicBool>,
    }

    impl FakeWindowPort {
        fn new() -> Self {
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                avatar_hidden: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            }
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl super::super::capture::WechatWindowPort for FakeWindowPort {
        fn is_visible(
            &self,
            label: &'static str,
        ) -> Result<Option<bool>, super::super::types::ContractError> {
            Ok((label == crate::avatar_engine::AVATAR_WINDOW_LABEL)
                .then_some(!self.avatar_hidden.load(std::sync::atomic::Ordering::SeqCst)))
        }

        fn hide(&self, _label: &'static str) -> Result<(), super::super::types::ContractError> {
            self.calls.lock().unwrap().push("hide");
            self.avatar_hidden
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }

        fn show(&self, _label: &'static str) {
            self.calls.lock().unwrap().push("show");
            self.avatar_hidden
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    fn guard(port: FakeWindowPort) -> super::super::capture::WechatCaptureGuard<FakeWindowPort> {
        super::super::capture::WechatCaptureGuard::begin(port).unwrap()
    }

    #[test]
    fn successful_captures_receive_monotonic_versions_and_reject_old_results() {
        let coordinator = CaptureCoordinator::default();
        let first = coordinator.next_capture_version();
        let second = coordinator.next_capture_version();

        assert!(second > first);
        assert!(!coordinator.is_current_capture(first));
        assert!(coordinator.is_current_capture(second));
    }

    #[tokio::test]
    async fn stale_identity_does_not_start_a_worker_and_restores_once() {
        let port = FakeWindowPort::new();
        let worker_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started = worker_started.clone();
        let mut cancel = None;

        let result = run_guarded_capture(
            guard(port.clone()),
            || Err(super::super::types::ContractError::WxRequestStale),
            move || {
                started.store(true, std::sync::atomic::Ordering::SeqCst);
                tokio::task::spawn_blocking(|| Ok(()))
            },
            || Ok(()),
            std::time::Duration::from_millis(20),
            &mut cancel,
        )
        .await;

        assert_eq!(
            result,
            Err(super::super::types::ContractError::WxRequestStale)
        );
        assert!(!worker_started.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(port.calls(), vec!["hide", "show"]);
    }

    #[tokio::test]
    async fn provider_failure_and_worker_panic_restore_once() {
        for worker in [
            tokio::task::spawn_blocking(|| {
                Err(super::super::types::ContractError::WxCaptureFailed)
            }),
            tokio::task::spawn_blocking(|| -> Result<(), super::super::types::ContractError> {
                panic!("test worker panic")
            }),
        ] {
            let port = FakeWindowPort::new();
            let mut cancel = None;
            let result = run_guarded_capture(
                guard(port.clone()),
                || Ok(()),
                || worker,
                || Ok(()),
                std::time::Duration::from_millis(20),
                &mut cancel,
            )
            .await;

            assert_eq!(
                result,
                Err(super::super::types::ContractError::WxCaptureFailed)
            );
            assert_eq!(port.calls(), vec!["hide", "show"]);
        }
    }

    #[tokio::test]
    async fn timeout_waits_for_worker_before_restoring() {
        let port = FakeWindowPort::new();
        let release = Arc::new(Barrier::new(2));
        let worker_release = release.clone();
        let task_port = port.clone();
        let task = tokio::spawn(async move {
            let mut cancel = None;
            run_guarded_capture(
                guard(task_port),
                || Ok(()),
                move || {
                    tokio::task::spawn_blocking(move || {
                        worker_release.wait();
                        Ok(())
                    })
                },
                || Ok(()),
                std::time::Duration::from_millis(10),
                &mut cancel,
            )
            .await
        });

        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert_eq!(port.calls(), vec!["hide"]);
        release.wait();
        assert_eq!(
            task.await.unwrap(),
            Err(super::super::types::ContractError::WxCaptureTimeout)
        );
        assert_eq!(port.calls(), vec!["hide", "show"]);
    }

    #[tokio::test]
    async fn cancellation_waits_for_worker_before_restoring() {
        let port = FakeWindowPort::new();
        let release = Arc::new(Barrier::new(2));
        let worker_release = release.clone();
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let task_port = port.clone();
        let task = tokio::spawn(async move {
            let mut cancel = Some(cancel_rx);
            run_guarded_capture(
                guard(task_port),
                || Ok(()),
                move || {
                    tokio::task::spawn_blocking(move || {
                        worker_release.wait();
                        Ok(())
                    })
                },
                || Ok(()),
                std::time::Duration::from_secs(1),
                &mut cancel,
            )
            .await
        });

        cancel_tx.send(true).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        assert_eq!(port.calls(), vec!["hide"]);
        release.wait();
        assert_eq!(
            task.await.unwrap(),
            Err(super::super::types::ContractError::WxRequestCancelled)
        );
        assert_eq!(port.calls(), vec!["hide", "show"]);
    }
}

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

#[cfg(test)]
mod reply_runtime_tests {
    use super::*;
    #[cfg(feature = "wechat-m1")]
    use crate::agent::model::{SingleTurnTextRequest, SingleTurnTextTransport};
    #[cfg(feature = "wechat-m1")]
    use crate::config::{AiProvider, ModelConfig, TextModelProfile, WechatConfig};
    #[cfg(feature = "wechat-m1")]
    use crate::error::AppError;
    use crate::knowledge::types::{retrieval_fixture, RetrievalOutcome, RetrievalStatus};
    use crate::wechat::trace::{RetrievalMode, TraceQuery};
    #[cfg(feature = "wechat-m1")]
    use async_trait::async_trait;
    #[cfg(feature = "wechat-m1")]
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn snapshot() -> BeginReplySnapshot {
        BeginReplySnapshot {
            mode: ReplyMode::M1,
            binding_generation: BindingGeneration::new(4),
            observation_version: BindingObservationVersion::new(5),
            capture_version: Some(CaptureVersion::new(6)),
            timeout: Duration::from_secs(5),
        }
    }

    #[cfg(feature = "wechat-m1")]
    struct FakeTransport {
        calls: Arc<AtomicUsize>,
        succeeds: bool,
    }

    #[cfg(feature = "wechat-m1")]
    #[async_trait]
    impl SingleTurnTextTransport for FakeTransport {
        async fn complete(
            &self,
            _model: &ModelConfig,
            _request: SingleTurnTextRequest,
        ) -> Result<String, AppError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.succeeds {
                Ok("建议回复".into())
            } else {
                Err(AppError::Unknown("fixture transport failure".into()))
            }
        }
    }

    #[cfg(feature = "wechat-m1")]
    fn selected_model_snapshot() -> (WechatConfig, Vec<TextModelProfile>) {
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
        (config, vec![profile])
    }

    #[cfg(feature = "wechat-m1")]
    fn m1_input(lease: &ReplyLease) -> M1ReplyInput {
        M1ReplyInput::from(
            OcrReadyReply::from_backend(
                super::super::types::CapturedWechat {
                    request_id: lease.request_id().clone(),
                    capture_version: CaptureVersion::new(6),
                    stable_message_id: "fixture".into(),
                    is_single_chat: true,
                },
                super::super::types::OcrBackendResult::Text(
                    super::super::types::NormalizedOcrText::parse("请确认").unwrap(),
                ),
            )
            .unwrap(),
        )
    }

    #[cfg(feature = "wechat-m1")]
    fn advance_m1_to_generating(
        runtime: &WechatReplyRuntime,
        trace: ReplyTraceStore,
    ) -> ReplyLease {
        let lease = runtime.begin_reply(snapshot(), trace).unwrap();
        runtime
            .transition(
                &lease,
                ReplyState::Validating,
                ReplyState::Capturing,
                None,
                None,
            )
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Capturing, ReplyState::Ocr, None, None)
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Ocr, ReplyState::Generating, None, None)
            .unwrap();
        lease
    }

    #[test]
    fn capture_handoff_binds_the_lease_version_before_ocr() {
        let directory =
            std::env::temp_dir().join(format!("wechat-runtime-{}", uuid::Uuid::new_v4()));
        let trace = ReplyTraceStore::new(&directory);
        let runtime = WechatReplyRuntime::default();
        let lease = runtime
            .begin_reply(
                BeginReplySnapshot {
                    capture_version: None,
                    ..snapshot()
                },
                trace,
            )
            .unwrap();
        runtime
            .transition(
                &lease,
                ReplyState::Validating,
                ReplyState::Capturing,
                None,
                None,
            )
            .unwrap();
        runtime
            .enter_ocr_after_capture(&lease, CaptureVersion::new(9))
            .unwrap();
        runtime
            .transition(
                &lease,
                ReplyState::Ocr,
                ReplyState::Failed,
                Some(ContractError::WxOcrEmpty),
                None,
            )
            .unwrap();
        runtime.finish_reply(lease).unwrap();
        let trace_file = directory
            .join("wechat_reply")
            .join("trace")
            .join(format!("{}.jsonl", chrono::Utc::now().format("%F")));
        let trace = std::fs::read_to_string(trace_file).unwrap();
        assert!(trace.contains("\"captureVersion\":9"));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn single_flight_stages_are_monotonic_and_busy_is_untraced() {
        let directory =
            std::env::temp_dir().join(format!("wechat-runtime-{}", uuid::Uuid::new_v4()));
        let trace = ReplyTraceStore::new(&directory);
        let runtime = WechatReplyRuntime::default();
        let lease = runtime.begin_reply(snapshot(), trace.clone()).unwrap();
        assert!(matches!(
            runtime.begin_reply(snapshot(), trace.clone()),
            Err(ContractError::WxBusy)
        ));
        runtime
            .transition(
                &lease,
                ReplyState::Validating,
                ReplyState::Capturing,
                None,
                None,
            )
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Capturing, ReplyState::Ocr, None, None)
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Ocr, ReplyState::Generating, None, None)
            .unwrap();
        runtime.record_model_transport_call(&lease).unwrap();
        runtime
            .transition(
                &lease,
                ReplyState::Generating,
                ReplyState::ReplyReady,
                None,
                None,
            )
            .unwrap();
        runtime.finish_reply(lease).unwrap();
        let page = trace
            .list(TraceQuery {
                request_id: None,
                occurred_after: None,
                occurred_before: None,
                cursor: None,
                limit: 100,
            })
            .unwrap();
        assert_eq!(page.entry_count(), 5);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn cancelled_lease_cannot_progress() {
        let directory =
            std::env::temp_dir().join(format!("wechat-runtime-{}", uuid::Uuid::new_v4()));
        let trace = ReplyTraceStore::new(&directory);
        let runtime = WechatReplyRuntime::default();
        let lease = runtime.begin_reply(snapshot(), trace.clone()).unwrap();
        runtime.cancel_reply(&lease).unwrap();
        assert_eq!(
            runtime.transition(
                &lease,
                ReplyState::Validating,
                ReplyState::Capturing,
                None,
                None
            ),
            Err(ContractError::WxContractViolation)
        );
        assert_eq!(
            runtime.finish_reply(lease),
            Err(ContractError::WxRequestStale)
        );
        assert!(runtime.begin_reply(snapshot(), trace).is_ok());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn illegal_transition_records_failure_and_releases_the_slot() {
        let directory =
            std::env::temp_dir().join(format!("wechat-runtime-{}", uuid::Uuid::new_v4()));
        let trace = ReplyTraceStore::new(&directory);
        let runtime = WechatReplyRuntime::default();
        let lease = runtime.begin_reply(snapshot(), trace.clone()).unwrap();

        assert_eq!(
            runtime.transition(&lease, ReplyState::Capturing, ReplyState::Ocr, None, None),
            Err(ContractError::WxContractViolation)
        );
        assert!(runtime.begin_reply(snapshot(), trace.clone()).is_ok());
        let page = trace
            .list(TraceQuery {
                request_id: Some(lease.request_id().clone()),
                occurred_after: None,
                occurred_before: None,
                cursor: None,
                limit: 100,
            })
            .unwrap();
        assert_eq!(page.entry_count(), 2);
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn duplicate_model_call_records_failure_and_releases_the_slot() {
        let directory =
            std::env::temp_dir().join(format!("wechat-runtime-{}", uuid::Uuid::new_v4()));
        let trace = ReplyTraceStore::new(&directory);
        let runtime = WechatReplyRuntime::default();
        let lease = runtime.begin_reply(snapshot(), trace).unwrap();
        runtime
            .transition(
                &lease,
                ReplyState::Validating,
                ReplyState::Capturing,
                None,
                None,
            )
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Capturing, ReplyState::Ocr, None, None)
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Ocr, ReplyState::Generating, None, None)
            .unwrap();
        runtime.record_model_transport_call(&lease).unwrap();

        assert_eq!(
            runtime.record_model_transport_call(&lease),
            Err(ContractError::WxContractViolation)
        );
        assert!(runtime
            .begin_reply(snapshot(), ReplyTraceStore::new(&directory))
            .is_ok());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn generated_reply_requires_the_current_lease_and_one_transport_call() {
        let directory =
            std::env::temp_dir().join(format!("wechat-runtime-{}", uuid::Uuid::new_v4()));
        let trace = ReplyTraceStore::new(&directory);
        let runtime = WechatReplyRuntime::default();
        let lease = runtime.begin_reply(snapshot(), trace).unwrap();
        runtime
            .transition(
                &lease,
                ReplyState::Validating,
                ReplyState::Capturing,
                None,
                None,
            )
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Capturing, ReplyState::Ocr, None, None)
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Ocr, ReplyState::Generating, None, None)
            .unwrap();
        let reply = GeneratedReply::m1(
            lease.request_id().clone(),
            lease.suggestion_generation(),
            "fixture".into(),
        );

        assert_eq!(
            runtime.complete_generated_reply(&lease, &reply),
            Err(ContractError::WxContractViolation)
        );
        assert!(runtime
            .begin_reply(snapshot(), ReplyTraceStore::new(&directory))
            .is_ok());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[cfg(feature = "wechat-m1")]
    #[tokio::test]
    async fn model_orchestration_commits_one_current_reply() {
        let directory =
            std::env::temp_dir().join(format!("wechat-runtime-{}", uuid::Uuid::new_v4()));
        let trace = ReplyTraceStore::new(&directory);
        let runtime = WechatReplyRuntime::default();
        let lease = advance_m1_to_generating(&runtime, trace);
        let calls = Arc::new(AtomicUsize::new(0));
        let client = WechatReplyModelClient::with_transport(Arc::new(FakeTransport {
            calls: calls.clone(),
            succeeds: true,
        }));
        let (config, profiles) = selected_model_snapshot();

        let reply = runtime
            .generate_m1_reply_with_client(&client, config, profiles, m1_input(&lease), &lease)
            .await
            .unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(reply.is_current(lease.request_id(), lease.suggestion_generation(), None));
        runtime.finish_reply(lease).unwrap();
        let _ = std::fs::remove_dir_all(directory);
    }

    #[cfg(feature = "wechat-m1")]
    #[tokio::test]
    async fn model_orchestration_fails_once_and_releases_the_slot() {
        let directory =
            std::env::temp_dir().join(format!("wechat-runtime-{}", uuid::Uuid::new_v4()));
        let trace = ReplyTraceStore::new(&directory);
        let runtime = WechatReplyRuntime::default();
        let lease = advance_m1_to_generating(&runtime, trace.clone());
        let calls = Arc::new(AtomicUsize::new(0));
        let client = WechatReplyModelClient::with_transport(Arc::new(FakeTransport {
            calls: calls.clone(),
            succeeds: false,
        }));
        let (config, profiles) = selected_model_snapshot();

        assert_eq!(
            runtime
                .generate_m1_reply_with_client(&client, config, profiles, m1_input(&lease), &lease)
                .await,
            Err(ContractError::LlmFailed),
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(runtime
            .begin_reply(snapshot(), ReplyTraceStore::new(&directory))
            .is_ok());
        let trace_file = directory
            .join("wechat_reply")
            .join("trace")
            .join(format!("{}.jsonl", chrono::Utc::now().format("%F")));
        assert!(std::fs::read_to_string(trace_file)
            .unwrap()
            .contains("LLM_FAILED"));
        let _ = std::fs::remove_dir_all(directory);
    }

    #[cfg(feature = "wechat-m1")]
    #[tokio::test]
    async fn unavailable_model_fails_before_transport_and_releases_the_slot() {
        let directory =
            std::env::temp_dir().join(format!("wechat-runtime-{}", uuid::Uuid::new_v4()));
        let trace = ReplyTraceStore::new(&directory);
        let runtime = WechatReplyRuntime::default();
        let lease = advance_m1_to_generating(&runtime, trace);
        let calls = Arc::new(AtomicUsize::new(0));
        let client = WechatReplyModelClient::with_transport(Arc::new(FakeTransport {
            calls: calls.clone(),
            succeeds: true,
        }));
        let mut config = WechatConfig::default();
        config.text_model_profile_id = Some("missing".into());

        assert_eq!(
            runtime
                .generate_m1_reply_with_client(
                    &client,
                    config,
                    Vec::new(),
                    m1_input(&lease),
                    &lease
                )
                .await,
            Err(ContractError::WxTextModelUnavailable),
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(runtime
            .begin_reply(snapshot(), ReplyTraceStore::new(&directory))
            .is_ok());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn mismatched_m2_retrieval_records_failure_and_releases_the_slot() {
        let directory =
            std::env::temp_dir().join(format!("wechat-runtime-{}", uuid::Uuid::new_v4()));
        let trace = ReplyTraceStore::new(&directory);
        let runtime = WechatReplyRuntime::default();
        let lease = runtime
            .begin_reply(
                BeginReplySnapshot {
                    mode: ReplyMode::M2,
                    ..snapshot()
                },
                trace,
            )
            .unwrap();
        runtime
            .transition(
                &lease,
                ReplyState::Validating,
                ReplyState::Capturing,
                None,
                None,
            )
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Capturing, ReplyState::Ocr, None, None)
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Ocr, ReplyState::Retrieving, None, None)
            .unwrap();
        let wrong_reply = retrieval_fixture(
            RequestId::new(),
            "fixture",
            RetrievalOutcome::Retrieved(RetrievalStatus::Success),
            &[("excerpt", 1)],
            1,
        )
        .unwrap();
        let metadata = M2TraceMetadata::new(
            None,
            Some(1),
            None,
            None,
            RetrievalMode::NoHit,
            vec![],
            vec![],
            None,
        )
        .unwrap();

        assert_eq!(
            runtime.complete_retrieval(&lease, &wrong_reply, metadata),
            Err(ContractError::WxContractViolation)
        );
        assert!(runtime
            .begin_reply(snapshot(), ReplyTraceStore::new(&directory))
            .is_ok());
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn disabling_retention_deletes_only_the_current_request_content() {
        let directory =
            std::env::temp_dir().join(format!("wechat-runtime-{}", uuid::Uuid::new_v4()));
        let trace = ReplyTraceStore::new(&directory);
        let content = WechatContentStore::new(&directory);
        let runtime = WechatReplyRuntime::default();
        let lease = runtime.begin_reply(snapshot(), trace).unwrap();
        runtime
            .transition(
                &lease,
                ReplyState::Validating,
                ReplyState::Capturing,
                None,
                None,
            )
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Capturing, ReplyState::Ocr, None, None)
            .unwrap();
        runtime
            .transition(&lease, ReplyState::Ocr, ReplyState::Generating, None, None)
            .unwrap();

        let ocr = OcrReadyReply::from_backend(
            super::super::types::CapturedWechat {
                request_id: lease.request_id().clone(),
                capture_version: CaptureVersion::new(6),
                stable_message_id: "fixture".into(),
                is_single_chat: true,
            },
            super::super::types::OcrBackendResult::Text(
                super::super::types::NormalizedOcrText::parse("private").unwrap(),
            ),
        )
        .unwrap();
        assert!(runtime
            .retain_ocr_content(&lease, &content, &ocr, true, 1)
            .unwrap());
        assert!(!runtime
            .retain_ocr_content(&lease, &content, &ocr, false, 0)
            .unwrap());
        assert!(!content.request_exists(lease.request_id()));
        let _ = std::fs::remove_dir_all(directory);
    }
}
