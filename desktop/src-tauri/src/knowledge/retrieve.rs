use super::chunk::token_count_v1;
#[cfg(test)]
use super::chunk::TOKEN_COUNTER_VERSION;
use super::embedding::{query_active_vector, QueryVectorAttempt};
use super::store::{AuthorizedHitPayload, FrozenRetrievalRead, KnowledgeStore, RetrievalCandidate};
use super::types::KnowledgeScope;
use crate::wechat::types::{BindingGeneration, ContractError, RequestId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};
#[cfg(test)]
use std::sync::Mutex;
use std::time::Instant;
use work_review_core::semantic::reciprocal_rank_fusion;

const MAX_QUERY_BYTES: usize = 32 * 1024;
const MAX_QUERY_SCALARS: usize = 8_192;
const MAX_HIT_TOKENS: usize = 512;
const MIN_VECTOR_SCORE_V1: f64 = 0.20;
const SAME_CONVERSATION_BOOST_V1: f64 = 0.001;
const FROZEN_RESULT_SCHEMA_V1: &str = "knowledge-retrieval-result-v1";

#[cfg(test)]
type RetrievalStageHook = (String, &'static str, Box<dyn FnOnce() + Send>);

#[cfg(test)]
static RETRIEVAL_STAGE_HOOKS: Mutex<Vec<RetrievalStageHook>> = Mutex::new(Vec::new());

#[cfg(test)]
fn set_retrieval_stage_hook(
    request_id: &RequestId,
    stage: &'static str,
    hook: impl FnOnce() + Send + 'static,
) {
    RETRIEVAL_STAGE_HOOKS
        .lock()
        .unwrap()
        .push((request_id.to_string(), stage, Box::new(hook)));
}

#[cfg(test)]
fn run_retrieval_stage_hook(request_id: &RequestId, stage: &'static str) {
    let mut hooks = RETRIEVAL_STAGE_HOOKS.lock().unwrap();
    if let Some(position) = hooks.iter().position(|(target, target_stage, _)| {
        target == &request_id.to_string() && *target_stage == stage
    }) {
        let (_, _, hook) = hooks.remove(position);
        drop(hooks);
        hook();
    }
}

#[cfg(not(test))]
fn run_retrieval_stage_hook(_request_id: &RequestId, _stage: &'static str) {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnowledgeRetrieveRequest {
    pub(crate) request_id: RequestId,
    pub(crate) query_text: String,
    pub(crate) binding_generation: BindingGeneration,
    pub(crate) bound_conversation_id: Option<String>,
    pub(crate) scope: KnowledgeScope,
    pub(crate) top_k: u8,
    pub(crate) token_budget: u32,
    pub(crate) token_counter_version: String,
    pub(crate) same_conversation_boost: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RetrievalStatus {
    Success,
    NoHit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RetrievalMode {
    Hybrid,
    FtsFallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum KnowledgeError {
    #[serde(rename = "KB_NOT_READY")]
    NotReady,
    #[serde(rename = "KB_SCOPE_UNRESOLVED")]
    ScopeUnresolved,
    #[serde(rename = "KB_RETRIEVAL_FAILED")]
    RetrievalFailed,
    #[serde(rename = "WX_AUDIT_PERSIST_FAILED")]
    AuditPersistFailed,
}

impl From<ContractError> for KnowledgeError {
    fn from(error: ContractError) -> Self {
        match error {
            ContractError::KbNotReady => Self::NotReady,
            ContractError::KbScopeUnresolved => Self::ScopeUnresolved,
            ContractError::WxAuditPersistFailed => Self::AuditPersistFailed,
            _ => Self::RetrievalFailed,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceMessageRange {
    first: String,
    last: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceTimeRange {
    started_at_ms: i64,
    ended_at_ms: i64,
}

#[derive(Clone, Debug, PartialEq)]
struct LocalKnowledgeHit {
    knowledge_chunk_id: String,
    conversation_id: String,
    source_message_range: SourceMessageRange,
    source_time_range: SourceTimeRange,
    source_paths: Vec<String>,
    excerpt: String,
    token_count: u32,
    score: f64,
    context_lines: Vec<RetrievedContextLine>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetrievedContextDirection {
    Self_,
    Other,
}

impl RetrievedContextDirection {
    pub(crate) fn role(self) -> &'static str {
        match self {
            Self::Self_ => "self",
            Self::Other => "other",
        }
    }

    pub(crate) fn direction(self) -> &'static str {
        match self {
            Self::Self_ => "outgoing",
            Self::Other => "incoming",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RetrievedContextLine {
    pub(crate) occurred_at_ms: i64,
    pub(crate) direction: RetrievedContextDirection,
    pub(crate) text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RetrievedContextHit {
    pub(crate) hit_id: String,
    pub(crate) score: f64,
    pub(crate) started_at_ms: i64,
    pub(crate) ended_at_ms: i64,
    pub(crate) lines: Vec<RetrievedContextLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RetrievedContextParts {
    pub(crate) request_id: RequestId,
    pub(crate) query: String,
    pub(crate) binding_generation: BindingGeneration,
    pub(crate) catalog_generation: u64,
    pub(crate) index_generation_id: String,
    pub(crate) active_snapshot_hash: String,
    pub(crate) frozen_result_hash: String,
    pub(crate) status: RetrievalStatus,
    pub(crate) retrieval_mode: RetrievalMode,
    pub(crate) token_counter_version: String,
    pub(crate) token_budget: u32,
    pub(crate) hits: Vec<RetrievedContextHit>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RetrievedReply {
    request_id: RequestId,
    normalized_query: String,
    binding_generation: BindingGeneration,
    catalog_generation: u64,
    index_generation_id: String,
    active_snapshot_hash: String,
    frozen_result_hash: String,
    status: RetrievalStatus,
    retrieval_mode: RetrievalMode,
    token_counter_version: String,
    token_budget: u32,
    hits: Vec<LocalKnowledgeHit>,
    elapsed_ms: u64,
}

impl RetrievedReply {
    fn success(
        request: &KnowledgeRetrieveRequest,
        normalized_query: String,
        frozen: &FrozenRetrievalRead,
        retrieval_mode: RetrievalMode,
        hits: Vec<LocalKnowledgeHit>,
        elapsed_ms: u64,
        frozen_result_hash: String,
    ) -> Result<Self, KnowledgeError> {
        if hits.is_empty() {
            return Err(KnowledgeError::RetrievalFailed);
        }
        Self::create(
            request,
            normalized_query,
            frozen,
            RetrievalStatus::Success,
            retrieval_mode,
            hits,
            elapsed_ms,
            frozen_result_hash,
        )
    }

    fn no_hit(
        request: &KnowledgeRetrieveRequest,
        normalized_query: String,
        frozen: &FrozenRetrievalRead,
        retrieval_mode: RetrievalMode,
        elapsed_ms: u64,
        frozen_result_hash: String,
    ) -> Result<Self, KnowledgeError> {
        Self::create(
            request,
            normalized_query,
            frozen,
            RetrievalStatus::NoHit,
            retrieval_mode,
            Vec::new(),
            elapsed_ms,
            frozen_result_hash,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create(
        request: &KnowledgeRetrieveRequest,
        normalized_query: String,
        frozen: &FrozenRetrievalRead,
        status: RetrievalStatus,
        retrieval_mode: RetrievalMode,
        hits: Vec<LocalKnowledgeHit>,
        elapsed_ms: u64,
        frozen_result_hash: String,
    ) -> Result<Self, KnowledgeError> {
        let total_tokens = hits.iter().try_fold(0_u32, |total, hit| {
            total
                .checked_add(hit.token_count)
                .ok_or(KnowledgeError::RetrievalFailed)
        })?;
        if hits.len() > usize::from(request.top_k)
            || hits.iter().any(|hit| {
                hit.token_count as usize > MAX_HIT_TOKENS
                    || hit.excerpt.is_empty()
                    || !hit.score.is_finite()
            })
            || total_tokens > request.token_budget
            || (status == RetrievalStatus::NoHit) != hits.is_empty()
        {
            return Err(KnowledgeError::RetrievalFailed);
        }
        Ok(Self {
            request_id: request.request_id.clone(),
            normalized_query,
            binding_generation: request.binding_generation,
            catalog_generation: frozen.scope.catalog_generation,
            index_generation_id: frozen.scope.index_generation_id.clone(),
            active_snapshot_hash: frozen.scope.snapshot_hash.clone(),
            frozen_result_hash,
            status,
            retrieval_mode,
            token_counter_version: request.token_counter_version.clone(),
            token_budget: request.token_budget,
            hits,
            elapsed_ms,
        })
    }

    pub(crate) fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    pub(crate) fn query(&self) -> &str {
        &self.normalized_query
    }

    pub(crate) fn binding_generation(&self) -> BindingGeneration {
        self.binding_generation
    }

    pub(crate) fn frozen_result_hash(&self) -> &str {
        &self.frozen_result_hash
    }

    pub(crate) fn is_no_hit(&self) -> bool {
        self.status == RetrievalStatus::NoHit
    }

    pub(crate) fn excerpts(&self) -> Vec<String> {
        self.hits.iter().map(|hit| hit.excerpt.clone()).collect()
    }

    pub(crate) fn into_context_parts(self) -> RetrievedContextParts {
        RetrievedContextParts {
            request_id: self.request_id,
            query: self.normalized_query,
            binding_generation: self.binding_generation,
            catalog_generation: self.catalog_generation,
            index_generation_id: self.index_generation_id,
            active_snapshot_hash: self.active_snapshot_hash,
            frozen_result_hash: self.frozen_result_hash,
            status: self.status,
            retrieval_mode: self.retrieval_mode,
            token_counter_version: self.token_counter_version,
            token_budget: self.token_budget,
            hits: self
                .hits
                .into_iter()
                .map(|hit| RetrievedContextHit {
                    hit_id: hit.knowledge_chunk_id,
                    score: quantize_score(hit.score),
                    started_at_ms: hit.source_time_range.started_at_ms,
                    ended_at_ms: hit.source_time_range.ended_at_ms,
                    lines: hit.context_lines,
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
struct RankedCandidate {
    candidate: RetrievalCandidate,
    score: f64,
}

struct RetrievalTraceSummary {
    request_audit_tag: String,
    catalog_generation: u64,
    index_generation_id: String,
    retrieval_mode: RetrievalMode,
    hit_ids: Vec<String>,
    hit_scores: Vec<f64>,
    elapsed_ms: u64,
}

impl RetrievalTraceSummary {
    fn emit(self) {
        log::info!(
            "knowledge_retrieval request_audit_tag={} catalog_generation={} index_generation_id={} retrieval_mode={:?} hit_ids={:?} hit_scores={:?} elapsed_ms={}",
            self.request_audit_tag,
            self.catalog_generation,
            self.index_generation_id,
            self.retrieval_mode,
            self.hit_ids,
            self.hit_scores,
            self.elapsed_ms
        );
    }
}

impl KnowledgeStore {
    pub(crate) async fn knowledge_retrieve(
        &self,
        request: KnowledgeRetrieveRequest,
    ) -> Result<RetrievedReply, KnowledgeError> {
        self.knowledge_retrieve_with_audit(request, None).await
    }

    pub(crate) async fn knowledge_retrieve_with_audit(
        &self,
        request: KnowledgeRetrieveRequest,
        audit: Option<&dyn crate::wechat::observability::M2AuditSink>,
    ) -> Result<RetrievedReply, KnowledgeError> {
        let started = Instant::now();
        request.scope.validate().map_err(KnowledgeError::from)?;
        let normalized_query = normalize_query(&request.query_text)?;
        let frozen = self
            .begin_authorized_retrieval(
                &request.scope,
                &request.token_counter_version,
                request.token_budget,
                request.top_k,
            )
            .map_err(KnowledgeError::from)?;
        if let Some(bound) = request.bound_conversation_id.as_deref() {
            if !self
                .retrieval_scope_authorizes(&frozen, bound)
                .map_err(KnowledgeError::from)?
            {
                return Err(KnowledgeError::ScopeUnresolved);
            }
        }

        let candidate_k = request.top_k.saturating_mul(4).max(20);
        run_retrieval_stage_hook(&request.request_id, "before_fts");
        let fts = self
            .search_authorized_fts(&frozen, &normalized_query, candidate_k)
            .map_err(KnowledgeError::from)?;
        let (mut ranked, retrieval_mode) =
            match query_active_vector(&frozen, &normalized_query, &request.request_id, audit)
                .await
                .map_err(KnowledgeError::from)?
            {
                QueryVectorAttempt::Available(vector) => {
                    let vectors = self
                        .search_authorized_vectors(&frozen, &vector, candidate_k)
                        .map_err(KnowledgeError::from)?
                        .into_iter()
                        .filter(|candidate| candidate.score >= MIN_VECTOR_SCORE_V1)
                        .collect::<Vec<_>>();
                    (fuse_candidates(vectors, fts)?, RetrievalMode::Hybrid)
                }
                QueryVectorAttempt::Unavailable => {
                    (rank_fts_fallback(fts)?, RetrievalMode::FtsFallback)
                }
            };

        let chunks_before = ranked
            .iter()
            .map(|candidate| candidate.candidate.chunk_id.clone())
            .collect::<BTreeSet<_>>();
        let conversations_before = ranked
            .iter()
            .map(|candidate| candidate.candidate.conversation_id.clone())
            .collect::<BTreeSet<_>>();
        apply_same_conversation_boost(
            &mut ranked,
            request.bound_conversation_id.as_deref(),
            request.same_conversation_boost,
        );
        if chunks_before
            != ranked
                .iter()
                .map(|candidate| candidate.candidate.chunk_id.clone())
                .collect()
            || conversations_before
                != ranked
                    .iter()
                    .map(|candidate| candidate.candidate.conversation_id.clone())
                    .collect()
        {
            return Err(KnowledgeError::RetrievalFailed);
        }
        ranked.truncate(usize::from(request.top_k));
        let selected_ids = ranked
            .iter()
            .map(|candidate| candidate.candidate.chunk_id.clone())
            .collect::<Vec<_>>();
        run_retrieval_stage_hook(&request.request_id, "before_payload");
        let payloads = self
            .read_authorized_hit_payloads(&frozen, &selected_ids)
            .map_err(KnowledgeError::from)?;
        if payloads
            .iter()
            .map(|payload| payload.chunk_id.as_str())
            .ne(selected_ids.iter().map(String::as_str))
        {
            return Err(KnowledgeError::RetrievalFailed);
        }
        let hits = assemble_hits(&request, payloads, &ranked)?;
        run_retrieval_stage_hook(&request.request_id, "before_final_revalidation");
        self.ensure_retrieval_still_active(&frozen)
            .map_err(KnowledgeError::from)?;
        let status = if hits.is_empty() {
            RetrievalStatus::NoHit
        } else {
            RetrievalStatus::Success
        };
        let frozen_result_hash = result_hash(
            &request,
            &normalized_query,
            &frozen,
            status,
            retrieval_mode,
            &hits,
        )?;
        let elapsed_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
        let reply = if status == RetrievalStatus::NoHit {
            RetrievedReply::no_hit(
                &request,
                normalized_query,
                &frozen,
                retrieval_mode,
                elapsed_ms,
                frozen_result_hash,
            )?
        } else {
            RetrievedReply::success(
                &request,
                normalized_query,
                &frozen,
                retrieval_mode,
                hits,
                elapsed_ms,
                frozen_result_hash,
            )?
        };
        RetrievalTraceSummary {
            request_audit_tag: request.request_id.audit_tag(),
            catalog_generation: reply.catalog_generation,
            index_generation_id: reply.index_generation_id.clone(),
            retrieval_mode: reply.retrieval_mode,
            hit_ids: reply
                .hits
                .iter()
                .map(|hit| hit.knowledge_chunk_id.clone())
                .collect(),
            hit_scores: reply
                .hits
                .iter()
                .map(|hit| quantize_score(hit.score))
                .collect(),
            elapsed_ms: reply.elapsed_ms,
        }
        .emit();
        Ok(reply)
    }
}

fn apply_same_conversation_boost(
    ranked: &mut [RankedCandidate],
    bound_conversation_id: Option<&str>,
    enabled: bool,
) {
    if enabled {
        let Some(bound_conversation_id) = bound_conversation_id else {
            ranked.sort_by(|left, right| {
                right
                    .score
                    .total_cmp(&left.score)
                    .then_with(|| left.candidate.chunk_id.cmp(&right.candidate.chunk_id))
            });
            return;
        };
        for candidate in ranked.iter_mut() {
            if candidate.candidate.conversation_id == bound_conversation_id {
                candidate.score += SAME_CONVERSATION_BOOST_V1;
            }
        }
    }
    ranked.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.candidate.chunk_id.cmp(&right.candidate.chunk_id))
    });
}

fn normalize_query(raw: &str) -> Result<String, KnowledgeError> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut lines = Vec::new();
    let mut previous_blank = false;
    for line in normalized.split('\n') {
        let cleaned = line
            .chars()
            .filter(|character| *character == '\t' || !character.is_control())
            .collect::<String>();
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
    let normalized = lines.join("\n").trim().to_owned();
    if normalized.is_empty()
        || normalized.len() > MAX_QUERY_BYTES
        || normalized.chars().count() > MAX_QUERY_SCALARS
    {
        return Err(KnowledgeError::RetrievalFailed);
    }
    Ok(normalized)
}

fn fuse_candidates(
    vectors: Vec<RetrievalCandidate>,
    fts: Vec<RetrievalCandidate>,
) -> Result<Vec<RankedCandidate>, KnowledgeError> {
    let vector_keys = vectors
        .iter()
        .map(|candidate| candidate.chunk_id.clone())
        .collect::<Vec<_>>();
    let fts_keys = fts
        .iter()
        .map(|candidate| candidate.chunk_id.clone())
        .collect::<Vec<_>>();
    let mut candidates = BTreeMap::new();
    for candidate in vectors.into_iter().chain(fts) {
        if let Some(existing) = candidates.get(&candidate.chunk_id) {
            let existing: &RetrievalCandidate = existing;
            if existing.conversation_id != candidate.conversation_id
                || existing.started_at_ms != candidate.started_at_ms
                || existing.ended_at_ms != candidate.ended_at_ms
            {
                return Err(KnowledgeError::RetrievalFailed);
            }
        } else {
            candidates.insert(candidate.chunk_id.clone(), candidate);
        }
    }
    reciprocal_rank_fusion(&[vector_keys, fts_keys], candidates.len())
        .map_err(|_| KnowledgeError::RetrievalFailed)?
        .into_iter()
        .map(|score| {
            Ok(RankedCandidate {
                candidate: candidates
                    .remove(&score.key)
                    .ok_or(KnowledgeError::RetrievalFailed)?,
                score: score.score,
            })
        })
        .collect()
}

fn rank_fts_fallback(fts: Vec<RetrievalCandidate>) -> Result<Vec<RankedCandidate>, KnowledgeError> {
    let mut seen = BTreeSet::new();
    fts.into_iter()
        .enumerate()
        .map(|(rank, candidate)| {
            if !seen.insert(candidate.chunk_id.clone()) {
                return Err(KnowledgeError::RetrievalFailed);
            }
            Ok(RankedCandidate {
                candidate,
                score: 1.0 / (61.0 + rank as f64),
            })
        })
        .collect()
}

fn assemble_hits(
    request: &KnowledgeRetrieveRequest,
    payloads: Vec<AuthorizedHitPayload>,
    ranked: &[RankedCandidate],
) -> Result<Vec<LocalKnowledgeHit>, KnowledgeError> {
    let mut remaining = request.token_budget as usize;
    let mut hits = Vec::with_capacity(payloads.len());
    for (payload, ranked) in payloads.into_iter().zip(ranked) {
        if payload.chunk_id != ranked.candidate.chunk_id
            || payload.conversation_id != ranked.candidate.conversation_id
            || payload.started_at_ms != ranked.candidate.started_at_ms
            || payload.ended_at_ms != ranked.candidate.ended_at_ms
            || payload.started_at_ms > payload.ended_at_ms
            || !ranked.score.is_finite()
        {
            return Err(KnowledgeError::RetrievalFailed);
        }
        validate_source_paths(&payload.source_paths)?;
        if payload.context_lines.is_empty()
            || payload
                .context_lines
                .first()
                .map(|line| line.occurred_at_ms)
                != Some(payload.started_at_ms)
            || payload.context_lines.last().map(|line| line.occurred_at_ms)
                != Some(payload.ended_at_ms)
        {
            return Err(KnowledgeError::RetrievalFailed);
        }
        let cap = remaining.min(MAX_HIT_TOKENS);
        if cap == 0 {
            break;
        }
        let token_count = token_count_v1(&payload.content);
        if token_count == 0 || token_count > MAX_HIT_TOKENS {
            return Err(KnowledgeError::RetrievalFailed);
        }
        // A model-visible hit is an indivisible sequence of complete turns.
        // Never truncate only the rendered excerpt while retaining longer
        // structured turns: if the next hit does not fit, omit that tail hit.
        if token_count > cap {
            break;
        }
        remaining = remaining
            .checked_sub(token_count)
            .ok_or(KnowledgeError::RetrievalFailed)?;
        hits.push(LocalKnowledgeHit {
            knowledge_chunk_id: payload.chunk_id,
            conversation_id: payload.conversation_id,
            source_message_range: SourceMessageRange {
                first: payload.first_message_id,
                last: payload.last_message_id,
            },
            source_time_range: SourceTimeRange {
                started_at_ms: payload.started_at_ms,
                ended_at_ms: payload.ended_at_ms,
            },
            source_paths: payload.source_paths,
            excerpt: payload.content,
            token_count: u32::try_from(token_count).map_err(|_| KnowledgeError::RetrievalFailed)?,
            score: ranked.score,
            context_lines: payload
                .context_lines
                .into_iter()
                .map(|line| RetrievedContextLine {
                    occurred_at_ms: line.occurred_at_ms,
                    direction: match line.direction {
                        super::chunk::Direction::Self_ => RetrievedContextDirection::Self_,
                        super::chunk::Direction::Other => RetrievedContextDirection::Other,
                    },
                    text: line.text,
                })
                .collect(),
        });
    }
    Ok(hits)
}

fn truncate_utf8(content: &str, max_bytes: usize) -> String {
    if content.len() <= max_bytes {
        return content.to_owned();
    }
    let mut end = max_bytes;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    content[..end].to_owned()
}

fn validate_source_paths(paths: &[String]) -> Result<(), KnowledgeError> {
    if paths.is_empty() || paths.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(KnowledgeError::RetrievalFailed);
    }
    for path in paths {
        if path.is_empty()
            || path.contains('\0')
            || path.contains('\\')
            || Path::new(path).is_absolute()
            || Path::new(path)
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(KnowledgeError::RetrievalFailed);
        }
    }
    Ok(())
}

fn result_hash(
    request: &KnowledgeRetrieveRequest,
    normalized_query: &str,
    frozen: &FrozenRetrievalRead,
    status: RetrievalStatus,
    mode: RetrievalMode,
    hits: &[LocalKnowledgeHit],
) -> Result<String, KnowledgeError> {
    // elapsed_ms is intentionally excluded: this freezes deterministic request/result content,
    // so an identical retry remains comparable even when wall-clock latency changes.
    let mut hasher = Sha256::new();
    for field in [
        FROZEN_RESULT_SCHEMA_V1.into(),
        request.request_id.to_string(),
        normalized_query.to_owned(),
        request.binding_generation.value().to_string(),
        request.bound_conversation_id.clone().unwrap_or_default(),
        request.same_conversation_boost.to_string(),
        frozen.scope.catalog_generation.to_string(),
        frozen.scope.authorization_epoch.to_string(),
        frozen.scope.index_generation_id.clone(),
        frozen.scope.snapshot_hash.clone(),
        canonical_scope_json(&request.scope)?,
        "retrieval-policy-v1".into(),
        request.token_counter_version.clone(),
        request.token_budget.to_string(),
        request.top_k.to_string(),
        serde_json::to_string(&status).map_err(|_| KnowledgeError::RetrievalFailed)?,
        serde_json::to_string(&mode).map_err(|_| KnowledgeError::RetrievalFailed)?,
        hits.len().to_string(),
    ] {
        hash_field(&mut hasher, &field);
    }
    for hit in hits {
        let score = format!("{:.6}", quantize_score(hit.score));
        for field in [
            hit.knowledge_chunk_id.as_str(),
            hit.conversation_id.as_str(),
            hit.source_message_range.first.as_str(),
            hit.source_message_range.last.as_str(),
        ] {
            hash_field(&mut hasher, field);
        }
        for field in [
            hit.source_time_range.started_at_ms.to_string(),
            hit.source_time_range.ended_at_ms.to_string(),
            hit.source_paths.len().to_string(),
        ] {
            hash_field(&mut hasher, &field);
        }
        for path in &hit.source_paths {
            hash_field(&mut hasher, path);
        }
        for field in [hit.excerpt.as_str(), &hit.token_count.to_string(), &score] {
            hash_field(&mut hasher, field);
        }
        hash_field(&mut hasher, &hit.context_lines.len().to_string());
        for line in &hit.context_lines {
            hash_field(&mut hasher, &line.occurred_at_ms.to_string());
            hash_field(&mut hasher, line.direction.role());
            hash_field(&mut hasher, line.direction.direction());
            hash_field(&mut hasher, &line.text);
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

fn hash_field(hasher: &mut Sha256, field: &str) {
    hasher.update((field.len() as u64).to_be_bytes());
    hasher.update(field.as_bytes());
}

fn quantize_score(score: f64) -> f64 {
    (score * 1_000_000.0).round() / 1_000_000.0
}

fn canonical_scope_json(scope: &KnowledgeScope) -> Result<String, KnowledgeError> {
    let canonical = match scope {
        KnowledgeScope::SelectedConversations { ids } => {
            let mut ids = ids.clone();
            ids.sort();
            KnowledgeScope::SelectedConversations { ids }
        }
        other => other.clone(),
    };
    serde_json::to_string(&canonical).map_err(|_| KnowledgeError::RetrievalFailed)
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetrievalOutcome {
    Retrieved(RetrievalStatus),
    Failed(ContractError),
}

#[cfg(test)]
pub(crate) fn retrieval_fixture(
    request_id: RequestId,
    query: &str,
    outcome: RetrievalOutcome,
    excerpts: &[(&str, u32)],
    token_budget: u32,
) -> Result<RetrievedReply, ContractError> {
    let RetrievalOutcome::Retrieved(status) = outcome else {
        return match outcome {
            RetrievalOutcome::Failed(error) => Err(error),
            RetrievalOutcome::Retrieved(_) => unreachable!(),
        };
    };
    let mut remaining = token_budget;
    let hits = excerpts
        .iter()
        .enumerate()
        .filter_map(|(index, (excerpt, tokens))| {
            if *tokens > remaining {
                return None;
            }
            remaining -= *tokens;
            Some(LocalKnowledgeHit {
                knowledge_chunk_id: format!("fixture-chunk-{index}"),
                conversation_id: "fixture-conversation".into(),
                source_message_range: SourceMessageRange {
                    first: "fixture-first".into(),
                    last: "fixture-last".into(),
                },
                source_time_range: SourceTimeRange {
                    started_at_ms: 1,
                    ended_at_ms: 1,
                },
                source_paths: vec!["fixture/messages.jsonl".into()],
                excerpt: (*excerpt).into(),
                token_count: *tokens,
                score: 1.0,
                context_lines: vec![RetrievedContextLine {
                    occurred_at_ms: 1,
                    direction: RetrievedContextDirection::Other,
                    text: (*excerpt).into(),
                }],
            })
        })
        .collect();
    Ok(RetrievedReply {
        request_id,
        normalized_query: query.into(),
        binding_generation: BindingGeneration::new(1),
        catalog_generation: 1,
        index_generation_id: "fixture-index".into(),
        active_snapshot_hash: "a".repeat(64),
        frozen_result_hash: "b".repeat(64),
        status,
        retrieval_mode: RetrievalMode::Hybrid,
        token_counter_version: TOKEN_COUNTER_VERSION.into(),
        token_budget,
        hits,
        elapsed_ms: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::archive_schema::CoverageKind;
    use crate::knowledge::archive_store::CompletenessVerdict;
    use crate::knowledge::chunk::{chunk_messages, CHUNK_SCHEMA_VERSION, FTS_PRETOKEN_VERSION};
    use crate::knowledge::store::{
        fail_reader_at, knowledge_scope_key, set_active_fts_after_scan_hook,
        set_active_vector_after_scan_hook, CandidateChecks, DeletionRequest, EncodedEmbedding,
        FrozenEmbeddingIdentity, FrozenIndexBuildSpec, IncomingMessage, NewSource,
        StableConversationKey,
    };
    use std::fs;
    use std::net::SocketAddr;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_embedding_fixture(
        vector: [f32; 2],
        fingerprint: &'static str,
    ) -> (
        SocketAddr,
        Arc<Mutex<Vec<String>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_task = calls.clone();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = vec![0_u8; 16 * 1024];
                let read = stream.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap()
                    .to_owned();
                calls_task.lock().unwrap().push(path.clone());
                let body = if path == "/api/tags" {
                    format!(
                        "{{\"models\":[{{\"name\":\"fixture-model\",\"digest\":\"{fingerprint}\"}}]}}"
                    )
                } else {
                    serde_json::json!({"embeddings": [vector]}).to_string()
                };
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(), body
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        (address, calls, server)
    }

    async fn spawn_raw_embedding_fixture(
        embed_body: String,
    ) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = vec![0_u8; 16 * 1024];
                let read = stream.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap();
                let body = if path == "/api/tags" {
                    "{\"models\":[{\"name\":\"fixture-model\",\"digest\":\"fixture-fingerprint\"}]}"
                } else {
                    embed_body.as_str()
                };
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(), body
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        (address, server)
    }

    fn database_path(data_dir: &Path) -> PathBuf {
        data_dir.join("wechat_knowledge/knowledge.sqlite")
    }

    fn fixture_ids(data_dir: &Path) -> (String, String, String) {
        let connection = rusqlite::Connection::open(database_path(data_dir)).unwrap();
        (
            connection
                .query_row("SELECT id FROM knowledge_sources LIMIT 1", [], |row| {
                    row.get(0)
                })
                .unwrap(),
            connection
                .query_row(
                    "SELECT id FROM knowledge_conversations LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .unwrap(),
            connection
                .query_row("SELECT id FROM knowledge_messages LIMIT 1", [], |row| {
                    row.get(0)
                })
                .unwrap(),
        )
    }

    fn build_retrieval_fixture(endpoint: String) -> (PathBuf, KnowledgeStore) {
        build_retrieval_fixture_with_messages(endpoint, &["周五同步虚构项目进度".into()])
    }

    fn build_retrieval_fixture_with_messages(
        endpoint: String,
        contents: &[String],
    ) -> (PathBuf, KnowledgeStore) {
        let data_dir =
            std::env::temp_dir().join(format!("knowledge_retrieval_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&data_dir).unwrap();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let staging = store
            .begin_staging_source(NewSource {
                account_stable_id: "fixture-account".into(),
                conversation_stable_id: "fixture-conversation".into(),
                export_id: "fixture-export".into(),
                schema_version: "v1".into(),
                manifest_hash: "fixture-manifest".into(),
                coverage_hash: "fixture-coverage".into(),
                exported_at_ms: 1,
                coverage_kind: CoverageKind::Full,
                display_metadata_json: Some(
                    serde_json::json!({
                        "schemaVersion": "conversation-display-v1",
                        "displayName": "虚构会话",
                        "isGroup": false
                    })
                    .to_string(),
                ),
            })
            .unwrap();
        store
            .append_staging_messages(
                &staging,
                &contents
                    .iter()
                    .enumerate()
                    .map(|(index, content)| {
                        let content_hash = hex::encode(Sha256::digest(content.as_bytes()));
                        IncomingMessage {
                            stable_id: Some(format!("fixture-message-{index}")),
                            fallback_key: None,
                            content: content.clone(),
                            normalized_content: content.clone(),
                            content_hash: content_hash.clone(),
                            source_member_token: "archive/messages.jsonl".into(),
                            created_at_ms: 1 + index as i64 * 2_000_000,
                            source_ordinal: index as u64,
                            sort_key: format!("{:020}|{:020}|fixture", index + 1, index),
                            message_kind: "text".into(),
                            render_kind: "text".into(),
                            sender_key: "fixture-sender".into(),
                            text_hash: content_hash,
                            reference_json: None,
                            extra_json: None,
                            media_refs: Vec::new(),
                        }
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        store
            .set_source_verdict(staging.source_id(), CompletenessVerdict::FullDeclared)
            .unwrap();
        store
            .mark_ready_candidate(
                staging,
                CandidateChecks {
                    expected_message_count: contents.len() as u64,
                },
            )
            .unwrap();
        let build = store
            .begin_or_resume_index_build(FrozenIndexBuildSpec {
                chunk_schema_version: CHUNK_SCHEMA_VERSION.into(),
                token_counter_version: TOKEN_COUNTER_VERSION.into(),
                fts_pretoken_version: FTS_PRETOKEN_VERSION.into(),
                retrieval_token_budget: 512,
                embedding: FrozenEmbeddingIdentity {
                    provider: "ollama_loopback".into(),
                    endpoint,
                    model: "fixture-model".into(),
                    fingerprint: "fixture-fingerprint".into(),
                    dimension: 2,
                },
            })
            .unwrap();
        let scope = store
            .list_build_conversations(&build.index_generation_id)
            .unwrap()
            .pop()
            .unwrap();
        let page = store
            .read_build_message_page(&build.index_generation_id, &scope, None, 256)
            .unwrap();
        let drafts = chunk_messages(
            &build.import_snapshot_hash,
            build.spec.retrieval_token_budget,
            &page.messages,
        )
        .unwrap();
        store
            .write_chunk_batch(&build.index_generation_id, &drafts)
            .unwrap();
        let pending = store
            .list_pending_build_embeddings(&build.index_generation_id, None, 32)
            .unwrap();
        let blob = work_review_core::semantic::encode_embedding(&[1.0, 0.0]);
        store
            .write_build_embeddings(
                &build.index_generation_id,
                2,
                &pending
                    .into_iter()
                    .map(|chunk| EncodedEmbedding {
                        chunk_key: chunk.chunk_key,
                        content_hash: chunk.content_hash,
                        blob: blob.clone(),
                    })
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let validation = store
            .validate_candidate_index(&build.index_generation_id)
            .unwrap();
        store
            .activate_validated_candidate(&build.index_generation_id, &validation)
            .unwrap();
        (data_dir, store)
    }

    fn fixture_scope_key() -> String {
        knowledge_scope_key(&StableConversationKey {
            account_stable_id: "fixture-account".into(),
            conversation_stable_id: "fixture-conversation".into(),
        })
    }

    fn request(query: &str) -> KnowledgeRetrieveRequest {
        let scope_key = fixture_scope_key();
        KnowledgeRetrieveRequest {
            request_id: RequestId::new(),
            query_text: query.into(),
            binding_generation: BindingGeneration::new(7),
            bound_conversation_id: Some(scope_key.clone()),
            scope: KnowledgeScope::Conversation { id: scope_key },
            top_k: 4,
            token_budget: 512,
            token_counter_version: TOKEN_COUNTER_VERSION.into(),
            same_conversation_boost: true,
        }
    }

    #[test]
    fn query_normalization_and_limits_are_deterministic() {
        assert_eq!(
            normalize_query("  一\r\n\r\n\u{0}\r\n 二  ").unwrap(),
            "一\n\n二"
        );
        assert_eq!(
            normalize_query(" \u{0} "),
            Err(KnowledgeError::RetrievalFailed)
        );
        assert_eq!(
            normalize_query(&"a".repeat(MAX_QUERY_BYTES + 1)),
            Err(KnowledgeError::RetrievalFailed)
        );
    }

    #[test]
    fn utf8_truncation_and_source_paths_fail_closed() {
        assert_eq!(truncate_utf8("甲乙丙", 7), "甲乙");
        assert!(validate_source_paths(&["archive/messages.jsonl".into()]).is_ok());
        for path in ["/absolute", "../escape", "bad\\escape", ""] {
            assert_eq!(
                validate_source_paths(&[path.into()]),
                Err(KnowledgeError::RetrievalFailed)
            );
        }
    }

    #[test]
    fn boost_changes_only_scores_and_order() {
        let first = RetrievalCandidate {
            chunk_id: "a".into(),
            conversation_id: "bound".into(),
            started_at_ms: 1,
            ended_at_ms: 2,
            score: 0.5,
        };
        let second = RetrievalCandidate {
            chunk_id: "b".into(),
            conversation_id: "other".into(),
            started_at_ms: 1,
            ended_at_ms: 2,
            score: 0.5,
        };
        let ranked = fuse_candidates(vec![first.clone(), second], vec![first]).unwrap();
        let mut disabled = ranked.clone();
        let mut enabled = ranked;
        apply_same_conversation_boost(&mut disabled, Some("bound"), false);
        apply_same_conversation_boost(&mut enabled, Some("bound"), true);
        assert_eq!(
            disabled
                .iter()
                .map(|item| item.candidate.chunk_id.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["a", "b"])
        );
        assert_eq!(
            disabled
                .iter()
                .map(|item| item.candidate.chunk_id.as_str())
                .collect::<BTreeSet<_>>(),
            enabled
                .iter()
                .map(|item| item.candidate.chunk_id.as_str())
                .collect::<BTreeSet<_>>()
        );
        assert_eq!(enabled[0].candidate.chunk_id, "a");
        assert_eq!(
            enabled[0].score,
            disabled[0].score + SAME_CONVERSATION_BOOST_V1
        );
    }

    #[test]
    fn test_fixture_preserves_no_hit_and_budget_contract() {
        let reply = retrieval_fixture(
            RequestId::new(),
            "虚构查询",
            RetrievalOutcome::Retrieved(RetrievalStatus::NoHit),
            &[],
            0,
        )
        .unwrap();
        assert!(reply.is_no_hit());
        let reply = retrieval_fixture(
            RequestId::new(),
            "虚构查询",
            RetrievalOutcome::Retrieved(RetrievalStatus::Success),
            &[("预算内", 3), ("超预算", 2)],
            4,
        )
        .unwrap();
        assert_eq!(reply.excerpts(), ["预算内"]);
    }

    #[test]
    fn request_wire_is_snake_case_and_rejects_unknown_fields() {
        let request = request("虚构查询");
        let value = serde_json::to_value(&request).unwrap();
        for field in [
            "request_id",
            "query_text",
            "binding_generation",
            "bound_conversation_id",
            "scope",
            "top_k",
            "token_budget",
            "token_counter_version",
            "same_conversation_boost",
        ] {
            assert!(value.get(field).is_some(), "missing {field}");
        }
        let mut value = value;
        value["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<KnowledgeRetrieveRequest>(value).is_err());
    }

    #[test]
    fn frozen_result_hash_binds_every_result_and_request_policy_field() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        let base_request = request("虚构项目");
        let frozen = store
            .begin_authorized_retrieval(
                &base_request.scope,
                &base_request.token_counter_version,
                base_request.token_budget,
                base_request.top_k,
            )
            .unwrap();
        let hit = LocalKnowledgeHit {
            knowledge_chunk_id: "chunk-a".into(),
            conversation_id: "fixture-conversation".into(),
            source_message_range: SourceMessageRange {
                first: "message-a".into(),
                last: "message-b".into(),
            },
            source_time_range: SourceTimeRange {
                started_at_ms: 1,
                ended_at_ms: 2,
            },
            source_paths: vec!["archive/messages.jsonl".into()],
            excerpt: "虚构正文一".into(),
            token_count: 15,
            score: 0.5,
            context_lines: vec![
                RetrievedContextLine {
                    occurred_at_ms: 1,
                    direction: RetrievedContextDirection::Other,
                    text: "虚构正文一".into(),
                },
                RetrievedContextLine {
                    occurred_at_ms: 2,
                    direction: RetrievedContextDirection::Self_,
                    text: "虚构正文二".into(),
                },
            ],
        };
        let second = LocalKnowledgeHit {
            knowledge_chunk_id: "chunk-b".into(),
            ..hit.clone()
        };
        let base_hits = vec![hit, second];
        let hash = |request: &KnowledgeRetrieveRequest,
                    status: RetrievalStatus,
                    mode: RetrievalMode,
                    hits: &[LocalKnowledgeHit]| {
            result_hash(request, "虚构项目", &frozen, status, mode, hits).unwrap()
        };
        let base = hash(
            &base_request,
            RetrievalStatus::Success,
            RetrievalMode::Hybrid,
            &base_hits,
        );

        let mut request_variants = Vec::new();
        let mut changed = base_request.clone();
        changed.bound_conversation_id = Some("other-bound".into());
        request_variants.push(("bound conversation", changed));
        let mut changed = base_request.clone();
        changed.same_conversation_boost = false;
        request_variants.push(("boost policy", changed));
        let mut changed = base_request.clone();
        changed.binding_generation = BindingGeneration::new(8);
        request_variants.push(("binding generation", changed));
        let mut changed = base_request.clone();
        changed.top_k = 3;
        request_variants.push(("top k", changed));
        let mut changed = base_request.clone();
        changed.token_budget = 511;
        request_variants.push(("token budget", changed));
        let mut changed = base_request.clone();
        changed.token_counter_version = "v2".into();
        request_variants.push(("token counter", changed));
        for (field, changed) in request_variants {
            assert_ne!(
                base,
                hash(
                    &changed,
                    RetrievalStatus::Success,
                    RetrievalMode::Hybrid,
                    &base_hits
                ),
                "hash ignored {field}"
            );
        }
        assert_ne!(
            base,
            result_hash(
                &base_request,
                "different query",
                &frozen,
                RetrievalStatus::Success,
                RetrievalMode::Hybrid,
                &base_hits,
            )
            .unwrap(),
            "hash ignored normalized query"
        );

        let mut hit_variants = Vec::new();
        let mut changed = base_hits.clone();
        changed[0].conversation_id = "other-conversation".into();
        hit_variants.push(("conversation", changed));
        let mut changed = base_hits.clone();
        changed[0].source_message_range.first = "other-first".into();
        hit_variants.push(("first message", changed));
        let mut changed = base_hits.clone();
        changed[0].source_message_range.last = "other-last".into();
        hit_variants.push(("last message", changed));
        let mut changed = base_hits.clone();
        changed[0].source_time_range.started_at_ms = 0;
        hit_variants.push(("start time", changed));
        let mut changed = base_hits.clone();
        changed[0].source_time_range.ended_at_ms = 3;
        hit_variants.push(("end time", changed));
        let mut changed = base_hits.clone();
        changed[0].source_paths = vec!["other/messages.jsonl".into()];
        hit_variants.push(("source paths", changed));
        let mut changed = base_hits.clone();
        changed[0].excerpt = "另一段正文".into();
        hit_variants.push(("excerpt", changed));
        let mut changed = base_hits.clone();
        changed[0].token_count += 1;
        hit_variants.push(("token count", changed));
        let mut changed = base_hits.clone();
        changed[0].score += 0.01;
        hit_variants.push(("score", changed));
        let mut changed = base_hits.clone();
        changed[0].context_lines[0].text = "另一条实际入模正文".into();
        hit_variants.push(("model context text", changed));
        let mut changed = base_hits.clone();
        changed[0].context_lines[0].direction = RetrievedContextDirection::Self_;
        hit_variants.push(("model context direction", changed));
        let mut changed = base_hits.clone();
        changed.reverse();
        hit_variants.push(("hit order", changed));
        for (field, changed) in hit_variants {
            assert_ne!(
                base,
                hash(
                    &base_request,
                    RetrievalStatus::Success,
                    RetrievalMode::Hybrid,
                    &changed
                ),
                "hash ignored {field}"
            );
        }
        assert_ne!(
            base,
            hash(
                &base_request,
                RetrievalStatus::Success,
                RetrievalMode::FtsFallback,
                &base_hits
            )
        );
        assert_ne!(
            base,
            hash(
                &base_request,
                RetrievalStatus::NoHit,
                RetrievalMode::Hybrid,
                &[]
            )
        );

        let mut selected_a = base_request.clone();
        selected_a.scope = KnowledgeScope::SelectedConversations {
            ids: vec!["z".into(), "a".into()],
        };
        let mut selected_b = selected_a.clone();
        selected_b.scope = KnowledgeScope::SelectedConversations {
            ids: vec!["a".into(), "z".into()],
        };
        assert_eq!(
            hash(
                &selected_a,
                RetrievalStatus::Success,
                RetrievalMode::Hybrid,
                &base_hits
            ),
            hash(
                &selected_b,
                RetrievalStatus::Success,
                RetrievalMode::Hybrid,
                &base_hits
            )
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn hybrid_success_binds_active_generations_and_complete_hit() {
        use crate::wechat::observability::{M2AuditKind, SpyM2AuditSink};

        let (address, calls, server) =
            spawn_embedding_fixture([1.0, 0.0], "fixture-fingerprint").await;
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        let request = request("虚构项目");
        let audit = SpyM2AuditSink::default();
        let reply = store
            .knowledge_retrieve_with_audit(request.clone(), Some(&audit))
            .await
            .unwrap();
        server.await.unwrap();

        assert_eq!(reply.status, RetrievalStatus::Success);
        assert_eq!(reply.retrieval_mode, RetrievalMode::Hybrid);
        assert_eq!(reply.binding_generation(), request.binding_generation);
        assert!(!reply.frozen_result_hash().is_empty());
        assert_eq!(reply.hits.len(), 1);
        assert_eq!(reply.hits[0].conversation_id, fixture_scope_key());
        assert_eq!(reply.hits[0].source_paths, ["archive/messages.jsonl"]);
        let source = store
            .rehydrate_active_source_excerpt(&reply.hits[0].knowledge_chunk_id)
            .unwrap()
            .unwrap();
        assert_eq!(source.lines.len(), 1);
        assert_eq!(source.lines[0].text, "周五同步虚构项目进度");
        assert!(!source.lines[0].text.contains("fixture-sender"));
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            ["/api/tags", "/api/embed"]
        );
        let events = audit.snapshot();
        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .all(|event| event.kind() == M2AuditKind::EmbeddingTransport));
        assert!(events
            .iter()
            .all(|event| event.request_id() == request.request_id.to_string()));
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn unavailable_vector_explicitly_falls_back_to_authorized_fts() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));

        let reply = store.knowledge_retrieve(request("虚构项目")).await.unwrap();
        assert_eq!(reply.status, RetrievalStatus::Success);
        assert_eq!(reply.retrieval_mode, RetrievalMode::FtsFallback);
        assert_eq!(reply.hits.len(), 1);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn vector_no_hit_is_success_but_fingerprint_change_fails_closed() {
        let (address, _, server) =
            spawn_embedding_fixture([-1.0, 0.0], "fixture-fingerprint").await;
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        let reply = store
            .knowledge_retrieve(request("完全不相关词"))
            .await
            .unwrap();
        server.await.unwrap();
        assert_eq!(reply.status, RetrievalStatus::NoHit);
        assert!(reply.hits.is_empty());
        let _ = fs::remove_dir_all(data_dir);

        let (address, _, server) = spawn_embedding_fixture([1.0, 0.0], "changed").await;
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        assert_eq!(
            store.knowledge_retrieve(request("虚构项目")).await,
            Err(KnowledgeError::RetrievalFailed)
        );
        server.abort();
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn unresolved_scope_stops_before_embedding_transport() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        let mut cases = Vec::new();
        let mut outside = request("虚构项目");
        outside.scope = KnowledgeScope::Conversation {
            id: "outside-scope".into(),
        };
        cases.push(outside);
        let mut blank = request("虚构项目");
        blank.bound_conversation_id = Some("  ".into());
        cases.push(blank);
        let mut nul = request("虚构项目");
        nul.bound_conversation_id = Some("fixture\0conversation".into());
        cases.push(nul);
        for request in cases {
            assert_eq!(
                store.knowledge_retrieve(request).await,
                Err(KnowledgeError::ScopeUnresolved)
            );
        }
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn global_cross_account_same_stable_id_keeps_opaque_scope_unique() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        let connection = rusqlite::Connection::open(database_path(&data_dir)).unwrap();
        let source_id: String = connection
            .query_row("SELECT id FROM knowledge_sources LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        let index_id: String = connection
            .query_row(
                "SELECT active_index_generation_id FROM knowledge_catalog_state WHERE singleton_id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        connection.execute_batch("BEGIN IMMEDIATE").unwrap();
        connection.execute("INSERT INTO knowledge_conversations(id,account_stable_id,conversation_stable_id,display_metadata_json) VALUES('duplicate-conversation','other-account','fixture-conversation','{}')", []).unwrap();
        connection.execute("INSERT INTO knowledge_import_generations(id,trigger_source_id,conversation_id,parent_generation_id,source_set_hash,merge_mode,status,message_count,created_at_ms) VALUES('duplicate-import',?1,'duplicate-conversation',NULL,'duplicate-set','full','active',0,1)", [&source_id]).unwrap();
        connection.execute("UPDATE knowledge_conversations SET active_import_generation_id='duplicate-import' WHERE id='duplicate-conversation'", []).unwrap();
        connection.execute("INSERT INTO knowledge_import_generation_sources(import_generation_id,source_id,precedence,coverage_role) VALUES('duplicate-import',?1,0,'primary')", [&source_id]).unwrap();
        connection.execute("INSERT INTO knowledge_index_generation_imports(index_generation_id,conversation_id,import_generation_id) VALUES(?1,'duplicate-conversation','duplicate-import')", [&index_id]).unwrap();
        connection.execute_batch("COMMIT").unwrap();

        let mut request = request("虚构项目");
        request.scope = KnowledgeScope::GlobalUserSelected;
        let reply = store.knowledge_retrieve(request).await.unwrap();
        assert_eq!(reply.hits.len(), 1);
        assert_eq!(reply.hits[0].conversation_id, fixture_scope_key());
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn selected_and_global_scopes_never_expand_beyond_active_mapping() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        for scope in [
            KnowledgeScope::SelectedConversations {
                ids: vec![fixture_scope_key()],
            },
            KnowledgeScope::GlobalUserSelected,
        ] {
            let mut request = request("虚构项目");
            request.scope = scope;
            let reply = store.knowledge_retrieve(request).await.unwrap();
            assert_eq!(reply.hits.len(), 1);
            assert!(reply
                .hits
                .iter()
                .all(|hit| hit.conversation_id == fixture_scope_key()));
        }
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn retired_only_provenance_is_not_retrievable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        rusqlite::Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite"))
            .unwrap()
            .execute("UPDATE knowledge_sources SET source_state='retired'", [])
            .unwrap();

        assert_eq!(
            store.knowledge_retrieve(request("虚构项目")).await,
            Err(KnowledgeError::ScopeUnresolved)
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn source_conversation_and_message_denials_never_return_payloads() {
        for denied_kind in ["source", "conversation", "message"] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            drop(listener);
            let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
            let (source_id, conversation_id, message_id) = fixture_ids(&data_dir);
            let denial_request = match denied_kind {
                "source" => DeletionRequest {
                    source_id: Some(source_id),
                    conversation_id: None,
                    message_id: None,
                    reason: "fixture-source-denial".into(),
                },
                "conversation" => DeletionRequest {
                    source_id: None,
                    conversation_id: Some(conversation_id),
                    message_id: None,
                    reason: "fixture-conversation-denial".into(),
                },
                "message" => DeletionRequest {
                    source_id: None,
                    conversation_id: None,
                    message_id: Some(message_id),
                    reason: "fixture-message-denial".into(),
                },
                _ => unreachable!(),
            };
            store.deny_or_delete(denial_request).unwrap();
            assert_eq!(
                store.knowledge_retrieve(request("虚构项目")).await,
                Err(KnowledgeError::ScopeUnresolved),
                "{denied_kind}"
            );
            let _ = fs::remove_dir_all(data_dir);
        }
    }

    #[tokio::test]
    async fn a_second_active_provenance_survives_retiring_one_source() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        let connection = rusqlite::Connection::open(database_path(&data_dir)).unwrap();
        let (source_id, _, _) = fixture_ids(&data_dir);
        let import_id: String = connection
            .query_row(
                "SELECT active_import_generation_id FROM knowledge_conversations LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        connection.execute("INSERT INTO knowledge_sources(id,account_stable_id,export_id,schema_version,manifest_hash,coverage_hash,snapshot_kind,scope_filters_json,integrity_json,priority,source_state,import_status,checked_at_ms,error_code,error_summary,exported_at_ms,coverage_kind,member_audit_count,member_audit_digest) SELECT 'fixture-source-two',account_stable_id,export_id||'-two',schema_version,manifest_hash||'-two',coverage_hash||'-two',snapshot_kind,scope_filters_json,integrity_json,priority,'active',import_status,checked_at_ms,error_code,error_summary,exported_at_ms,coverage_kind,member_audit_count,member_audit_digest FROM knowledge_sources WHERE id=?1", [&source_id]).unwrap();
        connection.execute("INSERT INTO knowledge_import_generation_sources(import_generation_id,source_id,precedence,coverage_role) VALUES(?1,'fixture-source-two',1,'supplemental')", [&import_id]).unwrap();
        connection.execute("INSERT INTO knowledge_message_sources(message_version_id,source_id,source_relative_path) SELECT id,'fixture-source-two','archive-two/messages.jsonl' FROM knowledge_message_versions", []).unwrap();

        store.retire_source(&source_id).unwrap();
        let reply = store.knowledge_retrieve(request("虚构项目")).await.unwrap();
        assert_eq!(reply.status, RetrievalStatus::Success);
        assert_eq!(reply.hits.len(), 1);
        assert_eq!(reply.hits[0].source_paths, ["archive-two/messages.jsonl"]);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn authorization_revocation_after_payload_read_discards_the_entire_reply() {
        for revoked_kind in ["retire", "source", "conversation", "message"] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            drop(listener);
            let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
            let (source_id, conversation_id, message_id) = fixture_ids(&data_dir);
            let request = request("虚构项目");
            let hook_store = store.clone();
            set_retrieval_stage_hook(
                &request.request_id,
                "before_final_revalidation",
                move || match revoked_kind {
                    "retire" => hook_store.retire_source(&source_id).unwrap(),
                    "source" => hook_store
                        .deny_or_delete(DeletionRequest {
                            source_id: Some(source_id),
                            conversation_id: None,
                            message_id: None,
                            reason: "concurrent-source-denial".into(),
                        })
                        .unwrap(),
                    "conversation" => hook_store
                        .deny_or_delete(DeletionRequest {
                            source_id: None,
                            conversation_id: Some(conversation_id),
                            message_id: None,
                            reason: "concurrent-conversation-denial".into(),
                        })
                        .unwrap(),
                    "message" => hook_store
                        .deny_or_delete(DeletionRequest {
                            source_id: None,
                            conversation_id: None,
                            message_id: Some(message_id),
                            reason: "concurrent-message-denial".into(),
                        })
                        .unwrap(),
                    _ => unreachable!(),
                },
            );
            assert_eq!(
                store.knowledge_retrieve(request).await,
                Err(KnowledgeError::RetrievalFailed),
                "{revoked_kind}"
            );
            let _ = fs::remove_dir_all(data_dir);
        }
    }

    #[tokio::test]
    async fn corrupt_vector_and_generation_switch_discard_all_results() {
        let (address, _, server) = spawn_embedding_fixture([1.0, 0.0], "fixture-fingerprint").await;
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        rusqlite::Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite"))
            .unwrap()
            .execute("UPDATE knowledge_chunks SET embedding=x'0000'", [])
            .unwrap();
        assert_eq!(
            store.knowledge_retrieve(request("虚构项目")).await,
            Err(KnowledgeError::RetrievalFailed)
        );
        server.await.unwrap();
        let _ = fs::remove_dir_all(data_dir);

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        let request = request("虚构项目");
        let frozen = store
            .begin_authorized_retrieval(
                &request.scope,
                &request.token_counter_version,
                request.token_budget,
                request.top_k,
            )
            .unwrap();
        rusqlite::Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite"))
            .unwrap()
            .execute(
                "UPDATE knowledge_catalog_state SET catalog_generation_seq=catalog_generation_seq+1",
                [],
            )
            .unwrap();
        assert_eq!(
            store.ensure_retrieval_still_active(&frozen),
            Err(ContractError::KbRetrievalFailed)
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn malformed_vector_responses_never_degrade_to_fts() {
        for body in [
            "{".to_owned(),
            r#"{"embeddings":[[1.0]]}"#.to_owned(),
            r#"{"embeddings":[[NaN,0.0]]}"#.to_owned(),
            r#"{"embeddings":[[1e999,0.0]]}"#.to_owned(),
        ] {
            let (address, server) = spawn_raw_embedding_fixture(body).await;
            let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
            assert_eq!(
                store.knowledge_retrieve(request("虚构项目")).await,
                Err(KnowledgeError::RetrievalFailed)
            );
            server.await.unwrap();
            let _ = fs::remove_dir_all(data_dir);
        }
    }

    #[tokio::test]
    async fn catalog_switches_at_each_facade_stage_fail_closed() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        let path = database_path(&data_dir);
        set_active_fts_after_scan_hook(move || {
            rusqlite::Connection::open(path)
                .unwrap()
                .execute(
                    "UPDATE knowledge_catalog_state SET catalog_generation_seq=catalog_generation_seq+1",
                    [],
                )
                .unwrap();
        });
        assert_eq!(
            store.knowledge_retrieve(request("虚构项目")).await,
            Err(KnowledgeError::RetrievalFailed)
        );
        let _ = fs::remove_dir_all(data_dir);

        let (address, _, server) = spawn_embedding_fixture([1.0, 0.0], "fixture-fingerprint").await;
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        let path = database_path(&data_dir);
        set_active_vector_after_scan_hook(move || {
            rusqlite::Connection::open(path)
                .unwrap()
                .execute(
                    "UPDATE knowledge_catalog_state SET catalog_generation_seq=catalog_generation_seq+1",
                    [],
                )
                .unwrap();
        });
        assert_eq!(
            store.knowledge_retrieve(request("虚构项目")).await,
            Err(KnowledgeError::RetrievalFailed)
        );
        server.await.unwrap();
        let _ = fs::remove_dir_all(data_dir);

        for stage in ["before_payload", "before_final_revalidation"] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            drop(listener);
            let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
            let request = request("虚构项目");
            let path = database_path(&data_dir);
            set_retrieval_stage_hook(&request.request_id, stage, move || {
                rusqlite::Connection::open(path)
                    .unwrap()
                    .execute(
                        "UPDATE knowledge_catalog_state SET catalog_generation_seq=catalog_generation_seq+1",
                        [],
                    )
                    .unwrap();
            });
            assert_eq!(
                store.knowledge_retrieve(request).await,
                Err(KnowledgeError::RetrievalFailed),
                "{stage}"
            );
            let _ = fs::remove_dir_all(data_dir);
        }
    }

    #[tokio::test]
    async fn initial_and_mid_retrieval_reader_busy_map_to_distinct_errors() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        fail_reader_at(1);
        assert_eq!(
            store.knowledge_retrieve(request("虚构项目")).await,
            Err(KnowledgeError::NotReady)
        );
        fail_reader_at(3);
        assert_eq!(
            store.knowledge_retrieve(request("虚构项目")).await,
            Err(KnowledgeError::RetrievalFailed)
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn fts_and_payload_corruption_return_no_partial_reply() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        let fts_request = request("虚构项目");
        let path = database_path(&data_dir);
        set_retrieval_stage_hook(&fts_request.request_id, "before_fts", move || {
            rusqlite::Connection::open(path)
                .unwrap()
                .execute("DROP TABLE knowledge_chunks_fts", [])
                .unwrap();
        });
        assert_eq!(
            store.knowledge_retrieve(fts_request).await,
            Err(KnowledgeError::RetrievalFailed)
        );
        let _ = fs::remove_dir_all(data_dir);

        for corruption in ["message", "path"] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            drop(listener);
            let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
            let request = request("虚构项目");
            let path = database_path(&data_dir);
            set_retrieval_stage_hook(&request.request_id, "before_payload", move || {
                let connection = rusqlite::Connection::open(path).unwrap();
                if corruption == "message" {
                    connection
                        .execute("DELETE FROM knowledge_chunk_messages", [])
                        .unwrap();
                } else {
                    connection
                        .execute("DELETE FROM knowledge_message_sources", [])
                        .unwrap();
                }
            });
            assert_eq!(
                store.knowledge_retrieve(request).await,
                Err(KnowledgeError::RetrievalFailed),
                "{corruption}"
            );
            let _ = fs::remove_dir_all(data_dir);
        }
    }

    #[tokio::test]
    async fn production_assembly_omits_a_tail_hit_instead_of_leaking_uncapped_turns() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture_with_messages(
            format!("http://{address}"),
            &[
                format!("虚构项目{}", "a".repeat(260)),
                format!("虚构项目{}TAIL_CANARY", "b".repeat(260)),
            ],
        );

        let reply = store.knowledge_retrieve(request("虚构项目")).await.unwrap();
        assert_eq!(reply.status, RetrievalStatus::Success);
        assert_eq!(reply.hits.len(), 1);
        assert!(reply.hits[0].token_count <= 512);
        let result_hash = reply.frozen_result_hash().to_owned();
        let parts = reply.into_context_parts();
        let actual_model_text = parts
            .hits
            .iter()
            .flat_map(|hit| hit.lines.iter())
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!actual_model_text.contains("TAIL_CANARY"));
        assert_eq!(parts.frozen_result_hash, result_hash);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn unavailable_store_and_frozen_budget_fail_before_http() {
        assert_eq!(
            KnowledgeStore::default()
                .knowledge_retrieve(request("虚构项目"))
                .await,
            Err(KnowledgeError::NotReady)
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let (data_dir, store) = build_retrieval_fixture(format!("http://{address}"));
        let mut request = request("虚构项目");
        request.token_budget = 256;
        assert_eq!(
            store.knowledge_retrieve(request).await,
            Err(KnowledgeError::NotReady)
        );
        let _ = fs::remove_dir_all(data_dir);
    }
}
