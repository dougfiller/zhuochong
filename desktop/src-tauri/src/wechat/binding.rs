use super::ocr::HeaderIdentityClue;
use super::types::{BindingGeneration, BindingObservationVersion, ContractError};
use super::window_identity::{WechatWindowIdentity, WindowInstanceToken};
use crate::knowledge::store::{KnowledgeStore, RequestedScopeKeys, ResolvedActiveScope};
use crate::monitor::WindowBounds;
use serde::Serialize;
use std::sync::Mutex;
use uuid::Uuid;

pub(crate) struct KnowledgeScopeBinding {
    inner: Mutex<BindingInner>,
}

struct BindingInner {
    session_nonce: Uuid,
    binding_generation: BindingGeneration,
    observation_version: BindingObservationVersion,
    state: BindingState,
    pending_observation: Option<PendingHeaderObservation>,
    next_request_confirmation: Option<OneShotScopeConfirmation>,
    next_confirmation_epoch: u64,
}

enum BindingState {
    Unbound,
    Bound(BoundScope),
}

struct BoundScope {
    window: BindingWindowIdentity,
    header: HeaderIdentityClue,
    scope: ResolvedActiveScope,
    requires_per_request_confirmation: bool,
}

#[derive(Clone)]
struct PendingHeaderObservation {
    window: BindingWindowIdentity,
    header: HeaderIdentityClue,
    observation_version: BindingObservationVersion,
}

struct OneShotScopeConfirmation {
    generation: BindingGeneration,
    observation_version: BindingObservationVersion,
    epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BindingWindowIdentity {
    instance: WindowInstanceToken,
    pid: u32,
    bounds_px: WindowBounds,
    dpi: u32,
    profile_id: String,
    profile_version: String,
}

impl From<&WechatWindowIdentity> for BindingWindowIdentity {
    fn from(identity: &WechatWindowIdentity) -> Self {
        Self {
            instance: identity.instance(),
            pid: identity.pid(),
            bounds_px: identity.bounds_px(),
            dpi: identity.dpi(),
            profile_id: identity.profile_id().to_owned(),
            profile_version: identity.profile_version().to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnowledgeScopeBindingStatus {
    pub(crate) session_nonce: String,
    pub(crate) binding_generation: u64,
    pub(crate) binding_observation_version: u64,
    pub(crate) state: &'static str,
    pub(crate) scope_kind: Option<&'static str>,
    pub(crate) selected_count: u64,
    pub(crate) requires_per_request_confirmation: bool,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HeaderObservationResponse {
    pub(crate) binding_generation: u64,
    pub(crate) binding_observation_version: u64,
    pub(crate) header_clue: String,
    pub(crate) header_reliable: bool,
    pub(crate) profile_label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingMutationReason {
    UserScopeChanged,
    UserUnbound,
    WindowInstanceChanged,
    HeaderChanged,
    BoundsChanged,
    ProfileChanged,
    ActiveCatalogChanged,
    ScopeResolutionFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BindingMutation {
    pub(crate) old_generation: BindingGeneration,
    pub(crate) new_generation: BindingGeneration,
    pub(crate) reason: BindingMutationReason,
}

pub(crate) struct ConfirmBindingInput {
    pub(crate) session_nonce: String,
    pub(crate) expected_binding_generation: u64,
    pub(crate) expected_observation_version: u64,
    pub(crate) expected_catalog_generation: u64,
    pub(crate) requested_scope: RequestedScopeKeys,
    pub(crate) header_confirmed: bool,
    pub(crate) global_confirmed: bool,
}

#[derive(Debug)]
pub(crate) struct BindingRequestSnapshot {
    pub(crate) session_nonce: String,
    pub(crate) binding_generation: BindingGeneration,
    pub(crate) observation_version: BindingObservationVersion,
    pub(crate) resolved_scope: ResolvedActiveScope,
    next_stage: Option<BindingStage>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingStage {
    BeforeRetrieval,
    BeforeModelTransport,
}

#[derive(Debug)]
pub(crate) struct BindingFailure {
    pub(crate) error: ContractError,
    pub(crate) mutation: Option<BindingMutation>,
}

impl Default for KnowledgeScopeBinding {
    fn default() -> Self {
        Self {
            inner: Mutex::new(BindingInner {
                session_nonce: Uuid::new_v4(),
                binding_generation: BindingGeneration::new(0),
                observation_version: BindingObservationVersion::new(0),
                state: BindingState::Unbound,
                pending_observation: None,
                next_request_confirmation: None,
                next_confirmation_epoch: 0,
            }),
        }
    }
}

impl KnowledgeScopeBinding {
    pub(crate) fn status(&self) -> Result<KnowledgeScopeBindingStatus, ContractError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        Ok(status_for(&inner))
    }

    pub(crate) fn record_observation(
        &self,
        session_nonce: &str,
        expected_generation: u64,
        window: BindingWindowIdentity,
        header: HeaderIdentityClue,
    ) -> Result<(HeaderObservationResponse, Option<BindingMutation>), ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        validate_session_generation(&inner, session_nonce, expected_generation)?;
        let next_observation = inner
            .observation_version
            .value()
            .checked_add(1)
            .ok_or(ContractError::WxContractViolation)?;
        inner.observation_version = BindingObservationVersion::new(next_observation);
        let reason = match &inner.state {
            BindingState::Unbound => None,
            BindingState::Bound(bound) => observation_change_reason(bound, &window, &header),
        };
        let mutation = reason
            .map(|reason| mutate_unbound(&mut inner, reason))
            .transpose()?;
        let pending = PendingHeaderObservation {
            window,
            header: header.clone(),
            observation_version: inner.observation_version,
        };
        let response = HeaderObservationResponse {
            binding_generation: inner.binding_generation.value(),
            binding_observation_version: inner.observation_version.value(),
            header_clue: header.text().to_owned(),
            header_reliable: header.reliable(),
            profile_label: format!(
                "{} / {}",
                pending.window.profile_id, pending.window.profile_version
            ),
        };
        inner.pending_observation = Some(pending);
        Ok((response, mutation))
    }

    pub(crate) fn confirm_binding(
        &self,
        store: &KnowledgeStore,
        input: ConfirmBindingInput,
    ) -> Result<(KnowledgeScopeBindingStatus, BindingMutation), ContractError> {
        {
            let inner = self
                .inner
                .lock()
                .map_err(|_| ContractError::WxContractViolation)?;
            validate_session_generation(
                &inner,
                &input.session_nonce,
                input.expected_binding_generation,
            )?;
            let pending = inner
                .pending_observation
                .as_ref()
                .ok_or(ContractError::WxRequestStale)?;
            if pending.observation_version.value() != input.expected_observation_version
                || !input.header_confirmed
                || matches!(
                    input.requested_scope,
                    RequestedScopeKeys::GlobalUserSelected
                ) && !input.global_confirmed
            {
                return Err(ContractError::KbScopeUnresolved);
            }
        }
        let resolved = store
            .resolve_scope_keys(input.expected_catalog_generation, &input.requested_scope)
            .map_err(|_| ContractError::KbScopeUnresolved)?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        validate_session_generation(
            &inner,
            &input.session_nonce,
            input.expected_binding_generation,
        )?;
        let pending = inner
            .pending_observation
            .take()
            .ok_or(ContractError::WxRequestStale)?;
        if pending.observation_version.value() != input.expected_observation_version {
            return Err(ContractError::WxRequestStale);
        }
        let old_generation = inner.binding_generation;
        let new_generation = checked_next_generation(old_generation)?;
        let requires_per_request_confirmation = !pending.header.reliable();
        inner.binding_generation = new_generation;
        inner.state = BindingState::Bound(BoundScope {
            window: pending.window,
            header: pending.header,
            scope: resolved,
            requires_per_request_confirmation,
        });
        inner.next_request_confirmation = None;
        let mutation = BindingMutation {
            old_generation,
            new_generation,
            reason: BindingMutationReason::UserScopeChanged,
        };
        Ok((status_for(&inner), mutation))
    }

    pub(crate) fn clear(
        &self,
        session_nonce: &str,
        expected_generation: u64,
    ) -> Result<(KnowledgeScopeBindingStatus, BindingMutation), ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        validate_session_generation(&inner, session_nonce, expected_generation)?;
        let mutation = mutate_unbound(&mut inner, BindingMutationReason::UserUnbound)?;
        inner.pending_observation = None;
        Ok((status_for(&inner), mutation))
    }

    pub(crate) fn confirm_scope_for_next_request(
        &self,
        session_nonce: &str,
        expected_generation: u64,
        expected_observation: u64,
    ) -> Result<u64, ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        validate_session_generation(&inner, session_nonce, expected_generation)?;
        let BindingState::Bound(bound) = &inner.state else {
            return Err(ContractError::KbScopeUnresolved);
        };
        if !bound.requires_per_request_confirmation
            || inner.observation_version.value() != expected_observation
        {
            return Err(ContractError::KbScopeUnresolved);
        }
        let epoch = inner
            .next_confirmation_epoch
            .checked_add(1)
            .ok_or(ContractError::WxContractViolation)?;
        inner.next_confirmation_epoch = epoch;
        inner.next_request_confirmation = Some(OneShotScopeConfirmation {
            generation: inner.binding_generation,
            observation_version: inner.observation_version,
            epoch,
        });
        Ok(epoch)
    }

    pub(super) fn begin_m2_request(
        &self,
        store: &KnowledgeStore,
    ) -> Result<BindingRequestSnapshot, BindingFailure> {
        let (
            session_nonce,
            generation,
            observation,
            requested,
            catalog_generation,
            frozen_digest,
            frozen_count,
            requires_confirmation,
        ) = {
            let inner = self
                .inner
                .lock()
                .map_err(|_| binding_failure(ContractError::WxContractViolation))?;
            let BindingState::Bound(bound) = &inner.state else {
                return Err(binding_failure(ContractError::KbScopeUnresolved));
            };
            if bound.scope.bound_conversation_is_group {
                return Err(binding_failure(ContractError::WxGroupChatUnsupported));
            }
            let requested = match &bound.scope.knowledge_scope {
                crate::knowledge::types::KnowledgeScope::Conversation { id } => {
                    RequestedScopeKeys::Conversation(id.clone())
                }
                crate::knowledge::types::KnowledgeScope::SelectedConversations { ids } => {
                    RequestedScopeKeys::Selected(ids.clone())
                }
                crate::knowledge::types::KnowledgeScope::GlobalUserSelected => {
                    RequestedScopeKeys::GlobalUserSelected
                }
            };
            (
                inner.session_nonce.to_string(),
                inner.binding_generation,
                inner.observation_version,
                requested,
                bound.scope.catalog_generation,
                bound.scope.active_scope_digest.clone(),
                bound.scope.conversation_count,
                bound.requires_per_request_confirmation,
            )
        };
        let resolved = match store.resolve_scope_keys(catalog_generation, &requested) {
            Ok(resolved) => resolved,
            Err(_) => {
                let mutation = self
                    .invalidate_if_current(generation, BindingMutationReason::ScopeResolutionFailed)
                    .map_err(binding_failure)?;
                return Err(BindingFailure {
                    error: ContractError::KbScopeUnresolved,
                    mutation,
                });
            }
        };
        if resolved.active_scope_digest != frozen_digest
            || resolved.conversation_count != frozen_count
        {
            let mutation = self
                .invalidate_if_current(generation, BindingMutationReason::ActiveCatalogChanged)
                .map_err(binding_failure)?;
            return Err(BindingFailure {
                error: ContractError::KbScopeUnresolved,
                mutation,
            });
        }
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| binding_failure(ContractError::WxContractViolation))?;
        if inner.binding_generation != generation {
            return Err(binding_failure(ContractError::WxRequestStale));
        }
        consume_one_shot_confirmation(&mut inner, generation, observation, requires_confirmation)
            .map_err(binding_failure)?;
        Ok(BindingRequestSnapshot {
            session_nonce,
            binding_generation: generation,
            observation_version: observation,
            resolved_scope: resolved,
            next_stage: Some(BindingStage::BeforeRetrieval),
        })
    }

    pub(super) fn record_stage_observation(
        &self,
        expected: &mut BindingRequestSnapshot,
        stage: BindingStage,
        window: BindingWindowIdentity,
        header: HeaderIdentityClue,
    ) -> Result<BindingObservationVersion, BindingFailure> {
        if expected.next_stage != Some(stage) {
            return Err(binding_failure(ContractError::WxContractViolation));
        }
        let (observation, mutation) = self
            .record_observation(
                &expected.session_nonce,
                expected.binding_generation.value(),
                window,
                header,
            )
            .map_err(binding_failure)?;
        if let Some(mutation) = mutation {
            return Err(BindingFailure {
                error: ContractError::KbScopeUnresolved,
                mutation: Some(mutation),
            });
        }
        if observation.binding_generation != expected.binding_generation.value() {
            return Err(binding_failure(ContractError::WxRequestStale));
        }
        let observation_version =
            BindingObservationVersion::new(observation.binding_observation_version);
        if observation_version.value() <= expected.observation_version.value() {
            return Err(binding_failure(ContractError::WxContractViolation));
        }
        expected.observation_version = observation_version;
        expected.next_stage = match stage {
            BindingStage::BeforeRetrieval => Some(BindingStage::BeforeModelTransport),
            BindingStage::BeforeModelTransport => None,
        };
        Ok(observation_version)
    }

    fn invalidate_if_current(
        &self,
        expected: BindingGeneration,
        reason: BindingMutationReason,
    ) -> Result<Option<BindingMutation>, ContractError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContractError::WxContractViolation)?;
        if inner.binding_generation != expected {
            return Ok(None);
        }
        mutate_unbound(&mut inner, reason).map(Some)
    }
}

fn binding_failure(error: ContractError) -> BindingFailure {
    BindingFailure {
        error,
        mutation: None,
    }
}

fn consume_one_shot_confirmation(
    inner: &mut BindingInner,
    generation: BindingGeneration,
    observation: BindingObservationVersion,
    required: bool,
) -> Result<(), ContractError> {
    if !required {
        return Ok(());
    }
    let confirmation = inner
        .next_request_confirmation
        .take()
        .ok_or(ContractError::KbScopeUnresolved)?;
    if confirmation.generation != generation
        || confirmation.observation_version != observation
        || confirmation.epoch == 0
    {
        return Err(ContractError::KbScopeUnresolved);
    }
    Ok(())
}

fn validate_session_generation(
    inner: &BindingInner,
    session_nonce: &str,
    expected_generation: u64,
) -> Result<(), ContractError> {
    if inner.session_nonce.to_string() != session_nonce
        || inner.binding_generation.value() != expected_generation
    {
        return Err(ContractError::WxRequestStale);
    }
    Ok(())
}

fn checked_next_generation(current: BindingGeneration) -> Result<BindingGeneration, ContractError> {
    current
        .value()
        .checked_add(1)
        .map(BindingGeneration::new)
        .ok_or(ContractError::WxContractViolation)
}

fn mutate_unbound(
    inner: &mut BindingInner,
    reason: BindingMutationReason,
) -> Result<BindingMutation, ContractError> {
    let old_generation = inner.binding_generation;
    let new_generation = checked_next_generation(old_generation)?;
    inner.binding_generation = new_generation;
    inner.state = BindingState::Unbound;
    inner.next_request_confirmation = None;
    Ok(BindingMutation {
        old_generation,
        new_generation,
        reason,
    })
}

fn observation_change_reason(
    bound: &BoundScope,
    window: &BindingWindowIdentity,
    header: &HeaderIdentityClue,
) -> Option<BindingMutationReason> {
    if bound.window.instance != window.instance || bound.window.pid != window.pid {
        Some(BindingMutationReason::WindowInstanceChanged)
    } else if bound.window.bounds_px != window.bounds_px || bound.window.dpi != window.dpi {
        Some(BindingMutationReason::BoundsChanged)
    } else if bound.window.profile_id != window.profile_id
        || bound.window.profile_version != window.profile_version
    {
        Some(BindingMutationReason::ProfileChanged)
    } else if bound.header != *header {
        Some(BindingMutationReason::HeaderChanged)
    } else {
        None
    }
}

fn status_for(inner: &BindingInner) -> KnowledgeScopeBindingStatus {
    let (state, scope_kind, selected_count, requires) = match &inner.state {
        BindingState::Unbound => ("unbound", None, 0, false),
        BindingState::Bound(bound) => {
            let (kind, count) = match &bound.scope.knowledge_scope {
                crate::knowledge::types::KnowledgeScope::Conversation { .. } => ("conversation", 1),
                crate::knowledge::types::KnowledgeScope::SelectedConversations { ids } => {
                    ("selected_conversations", ids.len() as u64)
                }
                crate::knowledge::types::KnowledgeScope::GlobalUserSelected => {
                    ("global_user_selected", bound.scope.conversation_count)
                }
            };
            (
                "bound",
                Some(kind),
                count,
                bound.requires_per_request_confirmation,
            )
        }
    };
    KnowledgeScopeBindingStatus {
        session_nonce: inner.session_nonce.to_string(),
        binding_generation: inner.binding_generation.value(),
        binding_observation_version: inner.observation_version.value(),
        state,
        scope_kind,
        selected_count,
        requires_per_request_confirmation: requires,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(hwnd: usize, width: u32) -> BindingWindowIdentity {
        BindingWindowIdentity {
            instance: WindowInstanceToken::new(hwnd, 7, Some(9)),
            pid: 7,
            bounds_px: WindowBounds {
                x: 0,
                y: 0,
                width,
                height: 900,
            },
            dpi: 96,
            profile_id: "fixture-profile".into(),
            profile_version: "1".into(),
        }
    }

    fn bound_fixture() -> KnowledgeScopeBinding {
        let binding = KnowledgeScopeBinding::default();
        {
            let mut inner = binding.inner.lock().unwrap();
            inner.binding_generation = BindingGeneration::new(1);
            inner.observation_version = BindingObservationVersion::new(2);
            inner.state = BindingState::Bound(BoundScope {
                window: window(42, 1280),
                header: HeaderIdentityClue::fixture("虚构联系人", true),
                scope: ResolvedActiveScope {
                    knowledge_scope: crate::knowledge::types::KnowledgeScope::Conversation {
                        id: format!("ksc1_{}", "a".repeat(64)),
                    },
                    scope_keys: vec![format!("ksc1_{}", "a".repeat(64))],
                    catalog_generation: 4,
                    active_scope_digest: "fixture-digest".into(),
                    conversation_count: 1,
                    bound_conversation_key: Some(format!("ksc1_{}", "a".repeat(64))),
                    bound_conversation_is_group: false,
                },
                requires_per_request_confirmation: false,
            });
        }
        binding
    }

    fn request_snapshot(binding: &KnowledgeScopeBinding) -> BindingRequestSnapshot {
        let inner = binding.inner.lock().unwrap();
        let BindingState::Bound(bound) = &inner.state else {
            unreachable!();
        };
        BindingRequestSnapshot {
            session_nonce: inner.session_nonce.to_string(),
            binding_generation: inner.binding_generation,
            observation_version: inner.observation_version,
            resolved_scope: bound.scope.clone(),
            next_stage: Some(BindingStage::BeforeRetrieval),
        }
    }

    #[test]
    fn every_process_binding_starts_unbound_with_a_new_nonce() {
        let first = KnowledgeScopeBinding::default().status().unwrap();
        let second = KnowledgeScopeBinding::default().status().unwrap();
        assert_eq!(first.state, "unbound");
        assert_eq!(first.binding_generation, 0);
        assert_ne!(first.session_nonce, second.session_nonce);
    }

    #[test]
    fn generation_overflow_fails_closed() {
        assert_eq!(
            checked_next_generation(BindingGeneration::new(u64::MAX)),
            Err(ContractError::WxContractViolation)
        );
    }

    #[test]
    fn same_header_only_advances_observation_but_window_change_unbinds() {
        let binding = bound_fixture();
        let nonce = binding.status().unwrap().session_nonce;
        let (same, mutation) = binding
            .record_observation(
                &nonce,
                1,
                window(42, 1280),
                HeaderIdentityClue::fixture("虚构联系人", true),
            )
            .unwrap();
        assert_eq!(same.binding_generation, 1);
        assert_eq!(same.binding_observation_version, 3);
        assert!(mutation.is_none());

        let (changed, mutation) = binding
            .record_observation(
                &nonce,
                1,
                window(43, 1280),
                HeaderIdentityClue::fixture("虚构联系人", true),
            )
            .unwrap();
        assert_eq!(changed.binding_generation, 2);
        assert_eq!(changed.binding_observation_version, 4);
        assert_eq!(
            mutation.unwrap().reason,
            BindingMutationReason::WindowInstanceChanged
        );
        assert_eq!(binding.status().unwrap().state, "unbound");
    }

    #[test]
    fn resize_and_header_change_have_distinct_generation_reasons() {
        for (next_window, next_header, expected) in [
            (
                window(42, 1270),
                HeaderIdentityClue::fixture("虚构联系人", true),
                BindingMutationReason::BoundsChanged,
            ),
            (
                window(42, 1280),
                HeaderIdentityClue::fixture("另一虚构联系人", true),
                BindingMutationReason::HeaderChanged,
            ),
        ] {
            let binding = bound_fixture();
            let status = binding.status().unwrap();
            let (_, mutation) = binding
                .record_observation(
                    &status.session_nonce,
                    status.binding_generation,
                    next_window,
                    next_header,
                )
                .unwrap();
            assert_eq!(mutation.unwrap().reason, expected);
        }
    }

    #[test]
    fn stage_gate_requires_fresh_ordered_observations_for_each_stage() {
        let binding = bound_fixture();
        let mut request = request_snapshot(&binding);
        let first = binding
            .record_stage_observation(
                &mut request,
                BindingStage::BeforeRetrieval,
                window(42, 1280),
                HeaderIdentityClue::fixture("虚构联系人", true),
            )
            .unwrap();
        let second = binding
            .record_stage_observation(
                &mut request,
                BindingStage::BeforeModelTransport,
                window(42, 1280),
                HeaderIdentityClue::fixture("虚构联系人", true),
            )
            .unwrap();
        assert!(second.value() > first.value());
        let repeated = binding.record_stage_observation(
            &mut request,
            BindingStage::BeforeModelTransport,
            window(42, 1280),
            HeaderIdentityClue::fixture("虚构联系人", true),
        );
        assert!(matches!(
            repeated,
            Err(BindingFailure {
                error: ContractError::WxContractViolation,
                mutation: None
            })
        ));
    }

    #[test]
    fn scope_resolution_failure_returns_the_mutation_to_the_orchestrator() {
        let binding = bound_fixture();
        match binding.begin_m2_request(&KnowledgeStore::default()) {
            Err(BindingFailure {
                error: ContractError::KbScopeUnresolved,
                mutation: Some(mutation),
            }) => {
                assert_eq!(
                    mutation.reason,
                    BindingMutationReason::ScopeResolutionFailed
                );
                assert_eq!(mutation.old_generation.value(), 1);
                assert_eq!(mutation.new_generation.value(), 2);
            }
            _ => panic!("scope resolution failure must return its binding mutation"),
        }
        assert_eq!(binding.status().unwrap().state, "unbound");
    }

    #[test]
    fn second_stage_window_or_header_change_returns_the_mutation() {
        for (next_window, next_header, expected_reason) in [
            (
                window(43, 1280),
                HeaderIdentityClue::fixture("虚构联系人", true),
                BindingMutationReason::WindowInstanceChanged,
            ),
            (
                window(42, 1280),
                HeaderIdentityClue::fixture("另一虚构联系人", true),
                BindingMutationReason::HeaderChanged,
            ),
        ] {
            let binding = bound_fixture();
            let mut request = request_snapshot(&binding);
            binding
                .record_stage_observation(
                    &mut request,
                    BindingStage::BeforeRetrieval,
                    window(42, 1280),
                    HeaderIdentityClue::fixture("虚构联系人", true),
                )
                .unwrap();
            let changed = binding.record_stage_observation(
                &mut request,
                BindingStage::BeforeModelTransport,
                next_window,
                next_header,
            );
            match changed {
                Err(BindingFailure {
                    error: ContractError::KbScopeUnresolved,
                    mutation: Some(mutation),
                }) => assert_eq!(mutation.reason, expected_reason),
                _ => panic!("stage change must return its binding mutation"),
            }
            assert_eq!(binding.status().unwrap().state, "unbound");
        }
    }

    #[test]
    fn unreliable_header_confirmation_is_one_shot_and_observation_scoped() {
        let binding = bound_fixture();
        {
            let mut inner = binding.inner.lock().unwrap();
            let BindingState::Bound(bound) = &mut inner.state else {
                unreachable!();
            };
            bound.requires_per_request_confirmation = true;
        }
        let status = binding.status().unwrap();
        assert_eq!(
            consume_one_shot_confirmation(
                &mut binding.inner.lock().unwrap(),
                BindingGeneration::new(status.binding_generation),
                BindingObservationVersion::new(status.binding_observation_version),
                true,
            ),
            Err(ContractError::KbScopeUnresolved)
        );
        binding
            .confirm_scope_for_next_request(
                &status.session_nonce,
                status.binding_generation,
                status.binding_observation_version,
            )
            .unwrap();
        {
            let mut inner = binding.inner.lock().unwrap();
            assert!(consume_one_shot_confirmation(
                &mut inner,
                BindingGeneration::new(status.binding_generation),
                BindingObservationVersion::new(status.binding_observation_version),
                true,
            )
            .is_ok());
            assert_eq!(
                consume_one_shot_confirmation(
                    &mut inner,
                    BindingGeneration::new(status.binding_generation),
                    BindingObservationVersion::new(status.binding_observation_version),
                    true,
                ),
                Err(ContractError::KbScopeUnresolved)
            );
        }
        binding
            .confirm_scope_for_next_request(
                &status.session_nonce,
                status.binding_generation,
                status.binding_observation_version,
            )
            .unwrap();
        assert_eq!(
            consume_one_shot_confirmation(
                &mut binding.inner.lock().unwrap(),
                BindingGeneration::new(status.binding_generation),
                BindingObservationVersion::new(status.binding_observation_version + 1),
                true,
            ),
            Err(ContractError::KbScopeUnresolved)
        );
    }
}
