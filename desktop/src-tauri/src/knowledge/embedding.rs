use super::store::{
    ActiveFtsRequest, ActiveVectorRequest, EncodedEmbedding, FrozenEmbeddingIdentity,
    FrozenRetrievalRead, FtsHit, KnowledgeStore, KnowledgeVectorHit, StableConversationKey,
};
use crate::config::LocalEmbeddingConfig;
use crate::embedding::{embedding_payload, parse_embedding_response, EmbeddingWireFormat};
use crate::wechat::types::ContractError;
use async_trait::async_trait;
use reqwest::redirect::Policy;
use serde_json::Value;
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};
use url::{Host, Url};
use work_review_core::semantic::{
    encode_embedding, normalize_embedding_strict, reciprocal_rank_fusion,
};

const EMBEDDING_BATCH_LIMIT: u8 = 32;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, PartialEq)]
pub(in crate::knowledge) enum QueryVectorAttempt {
    Available(Vec<f32>),
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndpointClass {
    Ipv4Loopback,
    Ipv6Loopback,
    LocalhostPinned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EmbeddingOperation {
    MetadataProbe,
    EmbedBatch,
    Query,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EmbeddingCallAudit {
    pub(crate) endpoint_class: EndpointClass,
    pub(crate) operation: EmbeddingOperation,
    pub(crate) call_count: u32,
    pub(crate) batch_size: u8,
    pub(crate) elapsed_ms: u64,
    pub(crate) outcome_code: &'static str,
}

pub(crate) trait EmbeddingAuditSink: Send + Sync {
    fn record(&self, event: EmbeddingCallAudit);
}

struct LogEmbeddingAuditSink;

impl EmbeddingAuditSink for LogEmbeddingAuditSink {
    fn record(&self, event: EmbeddingCallAudit) {
        log::info!(
            "knowledge_embedding endpoint_class={:?} operation={:?} call_count={} batch_size={} elapsed_ms={} outcome_code={}",
            event.endpoint_class,
            event.operation,
            event.call_count,
            event.batch_size,
            event.elapsed_ms,
            event.outcome_code
        );
    }
}

#[async_trait]
pub(crate) trait EndpointResolver: Send + Sync {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, ContractError>;
}

pub(crate) struct SystemEndpointResolver;

#[async_trait]
impl EndpointResolver for SystemEndpointResolver {
    async fn resolve(&self, host: &str, port: u16) -> Result<Vec<SocketAddr>, ContractError> {
        tokio::net::lookup_host((host, port))
            .await
            .map(|addresses| addresses.collect())
            .map_err(|_| ContractError::KbNotReady)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PinnedLoopbackEndpoint {
    base: Url,
    host: String,
    addresses: Vec<SocketAddr>,
    class: EndpointClass,
}

pub(crate) async fn validate_and_pin_loopback<R: EndpointResolver + ?Sized>(
    raw: &str,
    resolver: &R,
) -> Result<PinnedLoopbackEndpoint, ContractError> {
    let base = Url::parse(raw.trim()).map_err(|_| ContractError::KbNotReady)?;
    if base.scheme() != "http"
        || !base.username().is_empty()
        || base.password().is_some()
        || base.path() != "/"
        || base.query().is_some()
        || base.fragment().is_some()
        || base.port().is_some_and(|port| port == 0)
    {
        return Err(ContractError::KbNotReady);
    }
    let port = base
        .port_or_known_default()
        .filter(|port| *port != 0)
        .ok_or(ContractError::KbNotReady)?;
    let (host, addresses, class) = match base.host() {
        Some(Host::Ipv4(address)) if address.octets() == [127, 0, 0, 1] => (
            address.to_string(),
            vec![SocketAddr::new(IpAddr::V4(address), port)],
            EndpointClass::Ipv4Loopback,
        ),
        Some(Host::Ipv6(address)) if address.is_loopback() => (
            address.to_string(),
            vec![SocketAddr::new(IpAddr::V6(address), port)],
            EndpointClass::Ipv6Loopback,
        ),
        Some(Host::Domain("localhost")) => {
            let mut resolved = resolver.resolve("localhost", port).await?;
            resolved.sort_unstable();
            resolved.dedup();
            if resolved.is_empty() || resolved.iter().any(|address| !address.ip().is_loopback()) {
                return Err(ContractError::KbNotReady);
            }
            ("localhost".into(), resolved, EndpointClass::LocalhostPinned)
        }
        _ => return Err(ContractError::KbNotReady),
    };
    Ok(PinnedLoopbackEndpoint {
        base,
        host,
        addresses,
        class,
    })
}

pub(crate) fn build_knowledge_embedding_client(
    endpoint: &PinnedLoopbackEndpoint,
) -> Result<reqwest::Client, ContractError> {
    build_client_with_timeouts(endpoint, CONNECT_TIMEOUT, REQUEST_TIMEOUT)
}

fn build_client_with_timeouts(
    endpoint: &PinnedLoopbackEndpoint,
    connect_timeout: Duration,
    request_timeout: Duration,
) -> Result<reqwest::Client, ContractError> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .connect_timeout(connect_timeout)
        .timeout(request_timeout)
        .resolve_to_addrs(&endpoint.host, &endpoint.addresses)
        .build()
        .map_err(|_| ContractError::KbNotReady)
}

fn endpoint_url(
    endpoint: &PinnedLoopbackEndpoint,
    path: &'static str,
) -> Result<Url, ContractError> {
    endpoint
        .base
        .join(path)
        .map_err(|_| ContractError::KbNotReady)
}

async fn model_fingerprint(
    client: &reqwest::Client,
    endpoint: &PinnedLoopbackEndpoint,
    model: &str,
    audit: &dyn EmbeddingAuditSink,
) -> Result<String, ContractError> {
    let started = Instant::now();
    let result = async {
        let response = client
            .get(endpoint_url(endpoint, "api/tags")?)
            .send()
            .await
            .map_err(|_| ContractError::KbNotReady)?;
        if !response.status().is_success() || response.status().is_redirection() {
            return Err(ContractError::KbNotReady);
        }
        let payload: Value = response
            .json()
            .await
            .map_err(|_| ContractError::KbNotReady)?;
        parse_model_fingerprint(&payload, model)
    }
    .await;
    audit.record(audit_call(
        endpoint,
        EmbeddingOperation::MetadataProbe,
        0,
        started,
        if result.is_ok() {
            "METADATA_OK"
        } else {
            "METADATA_FAILED"
        },
    ));
    result
}

fn parse_model_fingerprint(payload: &Value, model: &str) -> Result<String, ContractError> {
    let candidates = payload
        .get("models")
        .and_then(Value::as_array)
        .ok_or(ContractError::KbNotReady)?
        .iter()
        .filter(|candidate| candidate.get("name").and_then(Value::as_str) == Some(model))
        .collect::<Vec<_>>();
    if candidates.len() != 1 {
        return Err(ContractError::KbNotReady);
    }
    candidates[0]
        .get("digest")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|digest| !digest.is_empty())
        .map(ToOwned::to_owned)
        .ok_or(ContractError::KbNotReady)
}

async fn embed_batch(
    client: &reqwest::Client,
    endpoint: &PinnedLoopbackEndpoint,
    model: &str,
    texts: &[String],
    operation: EmbeddingOperation,
    audit: &dyn EmbeddingAuditSink,
) -> Result<Vec<Vec<f32>>, ContractError> {
    if texts.is_empty() || texts.len() > usize::from(EMBEDDING_BATCH_LIMIT) {
        return Err(ContractError::KbNotReady);
    }
    let started = Instant::now();
    let result = async {
        let body = embedding_payload(EmbeddingWireFormat::OllamaBatchV1, model, texts)
            .map_err(|_| ContractError::KbNotReady)?;
        let response = client
            .post(endpoint_url(endpoint, "api/embed")?)
            .json(&body)
            .send()
            .await
            .map_err(|_| ContractError::KbNotReady)?;
        if !response.status().is_success() || response.status().is_redirection() {
            return Err(ContractError::KbNotReady);
        }
        parse_embedding_response(
            EmbeddingWireFormat::OllamaBatchV1,
            response
                .json()
                .await
                .map_err(|_| ContractError::KbNotReady)?,
            texts.len(),
        )
        .map_err(|_| ContractError::KbNotReady)?
        .into_iter()
        .map(|vector| normalize_embedding_strict(vector).map_err(|_| ContractError::KbNotReady))
        .collect()
    }
    .await;
    audit.record(audit_call(
        endpoint,
        operation,
        u8::try_from(texts.len()).unwrap_or(u8::MAX),
        started,
        if result.is_ok() {
            "EMBED_OK"
        } else {
            "EMBED_FAILED"
        },
    ));
    result
}

pub(in crate::knowledge) async fn query_active_vector(
    frozen: &FrozenRetrievalRead,
    query: &str,
) -> Result<QueryVectorAttempt, ContractError> {
    let endpoint = validate_and_pin_loopback(&frozen.embedding.endpoint, &SystemEndpointResolver)
        .await
        .map_err(|_| ContractError::KbRetrievalFailed)?;
    let client = build_knowledge_embedding_client(&endpoint)
        .map_err(|_| ContractError::KbRetrievalFailed)?;

    let metadata_started = Instant::now();
    let metadata_response = match client
        .get(endpoint_url(&endpoint, "api/tags").map_err(|_| ContractError::KbRetrievalFailed)?)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) if error.is_connect() || error.is_timeout() => {
            LogEmbeddingAuditSink.record(audit_call(
                &endpoint,
                EmbeddingOperation::MetadataProbe,
                0,
                metadata_started,
                "METADATA_UNAVAILABLE",
            ));
            return Ok(QueryVectorAttempt::Unavailable);
        }
        Err(_) => return Err(ContractError::KbRetrievalFailed),
    };
    if !metadata_response.status().is_success() || metadata_response.status().is_redirection() {
        return Err(ContractError::KbRetrievalFailed);
    }
    let metadata: Value = metadata_response
        .json()
        .await
        .map_err(|_| ContractError::KbRetrievalFailed)?;
    let fingerprint = parse_model_fingerprint(&metadata, &frozen.embedding.model)
        .map_err(|_| ContractError::KbRetrievalFailed)?;
    LogEmbeddingAuditSink.record(audit_call(
        &endpoint,
        EmbeddingOperation::MetadataProbe,
        0,
        metadata_started,
        "METADATA_OK",
    ));
    if fingerprint != frozen.embedding.fingerprint {
        return Err(ContractError::KbRetrievalFailed);
    }

    let embed_started = Instant::now();
    let texts = vec![query.to_owned()];
    let body = embedding_payload(
        EmbeddingWireFormat::OllamaBatchV1,
        &frozen.embedding.model,
        &texts,
    )
    .map_err(|_| ContractError::KbRetrievalFailed)?;
    let embed_response = match client
        .post(endpoint_url(&endpoint, "api/embed").map_err(|_| ContractError::KbRetrievalFailed)?)
        .json(&body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) if error.is_connect() || error.is_timeout() => {
            LogEmbeddingAuditSink.record(audit_call(
                &endpoint,
                EmbeddingOperation::Query,
                1,
                embed_started,
                "QUERY_UNAVAILABLE",
            ));
            return Ok(QueryVectorAttempt::Unavailable);
        }
        Err(_) => return Err(ContractError::KbRetrievalFailed),
    };
    if !embed_response.status().is_success() || embed_response.status().is_redirection() {
        return Err(ContractError::KbRetrievalFailed);
    }
    let payload = embed_response
        .json()
        .await
        .map_err(|_| ContractError::KbRetrievalFailed)?;
    let vector = parse_embedding_response(EmbeddingWireFormat::OllamaBatchV1, payload, 1)
        .map_err(|_| ContractError::KbRetrievalFailed)?
        .pop()
        .ok_or(ContractError::KbRetrievalFailed)?;
    let vector =
        normalize_embedding_strict(vector).map_err(|_| ContractError::KbRetrievalFailed)?;
    if vector.len() != frozen.embedding.dimension as usize {
        return Err(ContractError::KbRetrievalFailed);
    }
    LogEmbeddingAuditSink.record(audit_call(
        &endpoint,
        EmbeddingOperation::Query,
        1,
        embed_started,
        "QUERY_OK",
    ));
    Ok(QueryVectorAttempt::Available(vector))
}

pub(crate) async fn probe_and_freeze_candidate(
    candidate: &LocalEmbeddingConfig,
) -> Result<FrozenEmbeddingIdentity, ContractError> {
    probe_and_freeze_candidate_with(candidate, &SystemEndpointResolver, &LogEmbeddingAuditSink)
        .await
}

async fn probe_and_freeze_candidate_with(
    candidate: &LocalEmbeddingConfig,
    resolver: &dyn EndpointResolver,
    audit: &dyn EmbeddingAuditSink,
) -> Result<FrozenEmbeddingIdentity, ContractError> {
    if candidate.provider != "ollama_loopback" || candidate.model.trim().is_empty() {
        return Err(ContractError::KbNotReady);
    }
    let endpoint = validate_and_pin_loopback(&candidate.endpoint, resolver).await?;
    let client = build_knowledge_embedding_client(&endpoint)?;
    let fingerprint = model_fingerprint(&client, &endpoint, &candidate.model, audit).await?;
    let probe_texts = vec![
        "虚构项目在周五下午同步进度".to_string(),
        "虚构同事约在地铁站出口见面".to_string(),
    ];
    let vectors = embed_batch(
        &client,
        &endpoint,
        &candidate.model,
        &probe_texts,
        EmbeddingOperation::EmbedBatch,
        audit,
    )
    .await?;
    let dimension = vectors
        .first()
        .map(Vec::len)
        .ok_or(ContractError::KbNotReady)?;
    if dimension == 0 || vectors.iter().any(|vector| vector.len() != dimension) {
        return Err(ContractError::KbNotReady);
    }
    Ok(FrozenEmbeddingIdentity {
        provider: candidate.provider.clone(),
        endpoint: endpoint.base.to_string(),
        model: candidate.model.clone(),
        fingerprint,
        dimension: u32::try_from(dimension).map_err(|_| ContractError::KbNotReady)?,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KnowledgeEmbeddingProgress {
    pub(crate) embedded_chunks: u64,
    pub(crate) pending_chunks: u64,
}

pub(crate) async fn index_building_generation(
    store: &KnowledgeStore,
    index_generation_id: &str,
) -> Result<KnowledgeEmbeddingProgress, ContractError> {
    index_building_generation_with(
        store,
        index_generation_id,
        &SystemEndpointResolver,
        &LogEmbeddingAuditSink,
    )
    .await
}

async fn index_building_generation_with(
    store: &KnowledgeStore,
    index_generation_id: &str,
    resolver: &dyn EndpointResolver,
    audit: &dyn EmbeddingAuditSink,
) -> Result<KnowledgeEmbeddingProgress, ContractError> {
    let frozen = store.read_building_embedding_config(index_generation_id)?;
    let endpoint = validate_and_pin_loopback(&frozen.config.endpoint, resolver).await?;
    let client = build_knowledge_embedding_client(&endpoint)?;
    if model_fingerprint(&client, &endpoint, &frozen.config.model, audit).await?
        != frozen.config.fingerprint
    {
        return Err(ContractError::KbNotReady);
    }
    let mut embedded_chunks = 0_u64;
    loop {
        let pending = store.list_pending_build_embeddings(
            index_generation_id,
            None,
            EMBEDDING_BATCH_LIMIT,
        )?;
        if pending.is_empty() {
            let validation = store.validate_candidate_index(index_generation_id)?;
            store.activate_validated_candidate(index_generation_id, &validation)?;
            return Ok(KnowledgeEmbeddingProgress {
                embedded_chunks,
                pending_chunks: 0,
            });
        }
        let texts = pending
            .iter()
            .map(|chunk| chunk.content.clone())
            .collect::<Vec<_>>();
        let vectors = embed_batch(
            &client,
            &endpoint,
            &frozen.config.model,
            &texts,
            EmbeddingOperation::EmbedBatch,
            audit,
        )
        .await?;
        if vectors
            .iter()
            .any(|vector| vector.len() != frozen.config.dimension as usize)
        {
            return Err(ContractError::KbNotReady);
        }
        let rows = pending
            .into_iter()
            .zip(vectors)
            .map(|(chunk, vector)| EncodedEmbedding {
                chunk_key: chunk.chunk_key,
                content_hash: chunk.content_hash,
                blob: encode_embedding(&vector),
            })
            .collect::<Vec<_>>();
        store.write_build_embeddings(index_generation_id, frozen.config.dimension, &rows)?;
        embedded_chunks += rows.len() as u64;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ActiveHybridRequest {
    pub(crate) scope: Vec<StableConversationKey>,
    pub(crate) query: String,
    pub(crate) from_ms: Option<i64>,
    pub(crate) to_ms: Option<i64>,
    pub(crate) top_k: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct HybridHit {
    pub(crate) chunk_key: String,
    pub(crate) content: String,
    pub(crate) token_count: u32,
    pub(crate) started_at_ms: i64,
    pub(crate) ended_at_ms: i64,
    pub(crate) score: f64,
}

pub(crate) async fn search_active_hybrid(
    store: &KnowledgeStore,
    request: ActiveHybridRequest,
) -> Result<Vec<HybridHit>, ContractError> {
    search_active_hybrid_with(
        store,
        request,
        &SystemEndpointResolver,
        &LogEmbeddingAuditSink,
    )
    .await
}

async fn search_active_hybrid_with(
    store: &KnowledgeStore,
    request: ActiveHybridRequest,
    resolver: &dyn EndpointResolver,
    audit: &dyn EmbeddingAuditSink,
) -> Result<Vec<HybridHit>, ContractError> {
    let frozen = store.preflight_active_embedding_config(
        &request.scope,
        request.from_ms,
        request.to_ms,
        request.top_k,
    )?;
    let endpoint = validate_and_pin_loopback(&frozen.config.endpoint, resolver)
        .await
        .map_err(|_| ContractError::KbRetrievalFailed)?;
    let client = build_knowledge_embedding_client(&endpoint)
        .map_err(|_| ContractError::KbRetrievalFailed)?;
    if model_fingerprint(&client, &endpoint, &frozen.config.model, audit)
        .await
        .map_err(|_| ContractError::KbRetrievalFailed)?
        != frozen.config.fingerprint
    {
        return Err(ContractError::KbRetrievalFailed);
    }
    let query_vector = embed_batch(
        &client,
        &endpoint,
        &frozen.config.model,
        std::slice::from_ref(&request.query),
        EmbeddingOperation::Query,
        audit,
    )
    .await
    .map_err(|_| ContractError::KbRetrievalFailed)?
    .pop()
    .ok_or(ContractError::KbRetrievalFailed)?;
    let candidate_k = request.top_k.saturating_mul(4).max(20);
    let vectors = store.search_active_vectors_for_generation(
        ActiveVectorRequest {
            scope: request.scope.clone(),
            from_ms: request.from_ms,
            to_ms: request.to_ms,
            top_k: request.top_k,
        },
        &query_vector,
        Some(&frozen),
    )?;
    let fts = store.search_active_fts_for_generation(
        ActiveFtsRequest {
            scope: request.scope,
            query: request.query,
            from_ms: request.from_ms,
            to_ms: request.to_ms,
            top_k: candidate_k,
        },
        Some(&frozen),
    )?;
    let end = store
        .read_active_embedding_config()
        .map_err(|_| ContractError::KbRetrievalFailed)?;
    if frozen.catalog_generation != end.catalog_generation
        || frozen.index_generation_id != end.index_generation_id
    {
        return Err(ContractError::KbRetrievalFailed);
    }
    fuse_hybrid(vectors, fts, usize::from(request.top_k))
}

fn fuse_hybrid(
    vectors: Vec<KnowledgeVectorHit>,
    fts: Vec<FtsHit>,
    limit: usize,
) -> Result<Vec<HybridHit>, ContractError> {
    let vector_keys = vectors
        .iter()
        .map(|hit| hit.chunk_key.clone())
        .collect::<Vec<_>>();
    let fts_keys = fts
        .iter()
        .map(|hit| hit.chunk_key.clone())
        .collect::<Vec<_>>();
    let mut payloads = BTreeMap::new();
    for hit in vectors {
        payloads.insert(
            hit.chunk_key.clone(),
            HybridHit {
                chunk_key: hit.chunk_key,
                content: hit.content,
                token_count: hit.token_count,
                started_at_ms: hit.started_at_ms,
                ended_at_ms: hit.ended_at_ms,
                score: 0.0,
            },
        );
    }
    for hit in fts {
        payloads.entry(hit.chunk_key.clone()).or_insert(HybridHit {
            chunk_key: hit.chunk_key,
            content: hit.content,
            token_count: hit.token_count,
            started_at_ms: hit.started_at_ms,
            ended_at_ms: hit.ended_at_ms,
            score: 0.0,
        });
    }
    reciprocal_rank_fusion(&[vector_keys, fts_keys], limit)
        .map_err(|_| ContractError::KbRetrievalFailed)?
        .into_iter()
        .map(|score| {
            let mut hit = payloads
                .remove(&score.key)
                .ok_or(ContractError::KbRetrievalFailed)?;
            hit.score = score.score;
            Ok(hit)
        })
        .collect()
}

pub(crate) fn audit_call(
    endpoint: &PinnedLoopbackEndpoint,
    operation: EmbeddingOperation,
    batch_size: u8,
    started: Instant,
    outcome_code: &'static str,
) -> EmbeddingCallAudit {
    EmbeddingCallAudit {
        endpoint_class: endpoint.class,
        operation,
        call_count: 1,
        batch_size,
        elapsed_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        outcome_code,
    }
}

#[cfg(test)]
mod tests {
    use super::super::archive_schema::CoverageKind;
    use super::super::archive_store::CompletenessVerdict;
    use super::super::chunk::{
        chunk_messages, CHUNK_SCHEMA_VERSION, FTS_PRETOKEN_VERSION, TOKEN_COUNTER_VERSION,
    };
    use super::super::store::{CandidateChecks, FrozenIndexBuildSpec, IncomingMessage, NewSource};
    use super::*;
    use rusqlite::Connection;
    use sha2::{Digest, Sha256};
    use std::ffi::OsString;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[derive(Default)]
    struct TestAudit(Mutex<Vec<EmbeddingCallAudit>>);

    impl EmbeddingAuditSink for TestAudit {
        fn record(&self, event: EmbeddingCallAudit) {
            self.0.lock().unwrap().push(event);
        }
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.take() {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    struct FakeResolver {
        calls: AtomicUsize,
        addresses: Vec<SocketAddr>,
    }

    #[async_trait]
    impl EndpointResolver for FakeResolver {
        async fn resolve(&self, _host: &str, _port: u16) -> Result<Vec<SocketAddr>, ContractError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.addresses.clone())
        }
    }

    fn resolver(addresses: &[&str]) -> FakeResolver {
        FakeResolver {
            calls: AtomicUsize::new(0),
            addresses: addresses
                .iter()
                .map(|value| value.parse().unwrap())
                .collect(),
        }
    }

    async fn spawn_ollama_fixture(
        expected_calls: usize,
    ) -> (
        SocketAddr,
        Arc<Mutex<Vec<(String, usize)>>>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let calls_task = calls.clone();
        let server = tokio::spawn(async move {
            for _ in 0..expected_calls {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = vec![0_u8; 16 * 1024];
                let read = stream.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..read]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap()
                    .to_string();
                let body = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .unwrap_or("");
                let batch_size = serde_json::from_str::<Value>(body)
                    .ok()
                    .and_then(|payload| {
                        payload.get("input").and_then(Value::as_array).map(Vec::len)
                    })
                    .unwrap_or(0);
                calls_task.lock().unwrap().push((path.clone(), batch_size));
                let response_body = if path == "/api/tags" {
                    r#"{"models":[{"name":"fixture-model","digest":"digest-1"}]}"#.to_string()
                } else {
                    let vectors = std::iter::repeat(serde_json::json!([1.0, 0.0]))
                        .take(batch_size)
                        .collect::<Vec<_>>();
                    serde_json::json!({"embeddings":vectors}).to_string()
                };
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            response_body.len(), response_body
                        )
                        .as_bytes(),
                    )
                    .await
                    .unwrap();
            }
        });
        (address, calls, server)
    }

    async fn spawn_json_responses(
        responses: Vec<String>,
    ) -> (SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for body in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = [0_u8; 4096];
                let _ = stream.read(&mut buffer).await.unwrap();
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

    fn build_fixture(endpoint: String) -> (PathBuf, KnowledgeStore, String, StableConversationKey) {
        let data_dir =
            std::env::temp_dir().join(format!("knowledge_embedding_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&data_dir).unwrap();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let content = "周五同步虚构项目进度";
        let hash = hex::encode(Sha256::digest(content.as_bytes()));
        let staging = store
            .begin_staging_source(NewSource {
                account_stable_id: "acct".into(),
                conversation_stable_id: "conv".into(),
                export_id: "embedding-fixture".into(),
                schema_version: "v1".into(),
                manifest_hash: "manifest".into(),
                coverage_hash: "coverage".into(),
                exported_at_ms: 1,
                coverage_kind: CoverageKind::Full,
                display_metadata_json: None,
            })
            .unwrap();
        let first = IncomingMessage {
            stable_id: Some("message-1".into()),
            fallback_key: None,
            content: content.into(),
            normalized_content: content.into(),
            content_hash: hash.clone(),
            source_member_token: "member-1".into(),
            created_at_ms: 1,
            source_ordinal: 0,
            sort_key: "00000000000000000001|00000000000000000000|fixture".into(),
            message_kind: "text".into(),
            render_kind: "text".into(),
            sender_key: "fixture".into(),
            text_hash: hash,
            reference_json: None,
            extra_json: None,
            media_refs: Vec::new(),
        };
        let second_content = "下周复盘虚构项目结果";
        let second_hash = hex::encode(Sha256::digest(second_content.as_bytes()));
        let mut second = first.clone();
        second.stable_id = Some("message-2".into());
        second.content = second_content.into();
        second.normalized_content = second_content.into();
        second.content_hash = second_hash.clone();
        second.text_hash = second_hash;
        second.source_member_token = "member-2".into();
        second.created_at_ms = 2_000_000;
        second.source_ordinal = 1;
        second.sort_key = "00000000000002000000|00000000000000000001|fixture".into();
        store
            .append_staging_messages(&staging, &[first, second])
            .unwrap();
        store
            .set_source_verdict(staging.source_id(), CompletenessVerdict::FullDeclared)
            .unwrap();
        store
            .mark_ready_candidate(
                staging,
                CandidateChecks {
                    expected_message_count: 2,
                },
            )
            .unwrap();
        let frozen = store
            .begin_or_resume_index_build(FrozenIndexBuildSpec {
                chunk_schema_version: CHUNK_SCHEMA_VERSION.into(),
                token_counter_version: TOKEN_COUNTER_VERSION.into(),
                fts_pretoken_version: FTS_PRETOKEN_VERSION.into(),
                retrieval_token_budget: 512,
                embedding: FrozenEmbeddingIdentity {
                    provider: "ollama_loopback".into(),
                    endpoint,
                    model: "fixture-model".into(),
                    fingerprint: "digest-1".into(),
                    dimension: 2,
                },
            })
            .unwrap();
        let scope = store
            .list_build_conversations(&frozen.index_generation_id)
            .unwrap()
            .pop()
            .unwrap();
        let page = store
            .read_build_message_page(&frozen.index_generation_id, &scope, None, 256)
            .unwrap();
        let drafts = chunk_messages(
            &frozen.import_snapshot_hash,
            frozen.spec.retrieval_token_budget,
            &page.messages,
        )
        .unwrap();
        store
            .write_chunk_batch(&frozen.index_generation_id, &drafts)
            .unwrap();
        (data_dir, store, frozen.index_generation_id, scope)
    }

    #[tokio::test]
    async fn endpoint_policy_rejects_escape_paths_without_resolution() {
        let resolver = resolver(&["127.0.0.1:11434"]);
        for rejected in [
            "http://user:pass@localhost:11434",
            "https://localhost:11434",
            "http://localhost:11434/api/embed",
            "http://localhost:11434?x=1",
            "http://localhost:11434#fragment",
            "http://0.0.0.0:11434",
            "http://127.0.0.2:11434",
            "http://10.0.0.2:11434",
            "http://172.16.0.2:11434",
            "http://192.168.1.2:11434",
            "http://169.254.1.1:11434",
            "http://example.com:11434",
            "http://localhost.evil.example:11434",
            "http://localhost:0",
        ] {
            assert!(validate_and_pin_loopback(rejected, &resolver)
                .await
                .is_err());
        }
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn localhost_is_resolved_once_and_mixed_dns_is_rejected() {
        let good = resolver(&["127.0.0.1:11434", "[::1]:11434"]);
        let pinned = validate_and_pin_loopback("http://localhost:11434", &good)
            .await
            .unwrap();
        assert_eq!(good.calls.load(Ordering::SeqCst), 1);
        assert_eq!(pinned.class, EndpointClass::LocalhostPinned);

        let mixed = resolver(&["127.0.0.1:11434", "10.0.0.1:11434"]);
        assert!(validate_and_pin_loopback("http://localhost:11434", &mixed)
            .await
            .is_err());
        assert_eq!(mixed.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn pinned_client_uses_the_single_validated_resolution() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer).await.unwrap();
            let body = r#"{"models":[{"name":"fixture","digest":"digest-1"}]}"#;
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
        });
        let resolver = resolver(&[&address.to_string()]);
        let endpoint =
            validate_and_pin_loopback(&format!("http://localhost:{}", address.port()), &resolver)
                .await
                .unwrap();
        let client = build_knowledge_embedding_client(&endpoint).unwrap();
        let audit = TestAudit::default();
        assert_eq!(
            model_fingerprint(&client, &endpoint, "fixture", &audit)
                .await
                .unwrap(),
            "digest-1"
        );
        server.await.unwrap();
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
        let events = audit.0.lock().unwrap().clone();
        assert_eq!(
            events,
            &[EmbeddingCallAudit {
                endpoint_class: EndpointClass::LocalhostPinned,
                operation: EmbeddingOperation::MetadataProbe,
                call_count: 1,
                batch_size: 0,
                elapsed_ms: events[0].elapsed_ms,
                outcome_code: "METADATA_OK",
            }]
        );
    }

    #[tokio::test]
    async fn knowledge_client_ignores_proxy_environment_and_calls_only_pinned_loopback() {
        let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_address = target.local_addr().unwrap();
        let proxy_address = proxy.local_addr().unwrap();
        let target_calls = Arc::new(AtomicUsize::new(0));
        let target_calls_task = target_calls.clone();
        let proxy_calls = Arc::new(AtomicUsize::new(0));
        let proxy_calls_task = proxy_calls.clone();
        let target_server = tokio::spawn(async move {
            let (mut stream, _) = target.accept().await.unwrap();
            target_calls_task.fetch_add(1, Ordering::SeqCst);
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer).await.unwrap();
            let body = r#"{"models":[{"name":"fixture","digest":"digest-1"}]}"#;
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
        });
        let proxy_server = tokio::spawn(async move {
            if tokio::time::timeout(Duration::from_millis(250), proxy.accept())
                .await
                .is_ok()
            {
                proxy_calls_task.fetch_add(1, Ordering::SeqCst);
            }
        });
        let proxy_url = format!("http://{proxy_address}");
        let _http_proxy = EnvVarGuard::set("HTTP_PROXY", &proxy_url);
        let _http_proxy_lower = EnvVarGuard::set("http_proxy", &proxy_url);
        let _all_proxy = EnvVarGuard::set("ALL_PROXY", &proxy_url);
        let _all_proxy_lower = EnvVarGuard::set("all_proxy", &proxy_url);
        let _no_proxy = EnvVarGuard::set("NO_PROXY", "");
        let _no_proxy_lower = EnvVarGuard::set("no_proxy", "");
        let endpoint = validate_and_pin_loopback(
            &format!("http://127.0.0.1:{}", target_address.port()),
            &resolver(&[]),
        )
        .await
        .unwrap();
        let client = build_knowledge_embedding_client(&endpoint).unwrap();
        let audit = TestAudit::default();
        assert_eq!(
            model_fingerprint(&client, &endpoint, "fixture", &audit)
                .await
                .unwrap(),
            "digest-1"
        );
        target_server.await.unwrap();
        proxy_server.await.unwrap();
        assert_eq!(target_calls.load(Ordering::SeqCst), 1);
        assert_eq!(proxy_calls.load(Ordering::SeqCst), 0);
        assert_eq!(audit.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn fingerprint_requires_exactly_one_named_candidate_with_nonempty_digest() {
        let accepted = serde_json::json!({
            "models": [
                {"name":"other","digest":"ignored"},
                {"name":"fixture","digest":" digest-1 "}
            ]
        });
        assert_eq!(
            parse_model_fingerprint(&accepted, "fixture").unwrap(),
            "digest-1"
        );
        for rejected in [
            serde_json::json!({"models":[]}),
            serde_json::json!({"models":[{"name":"fixture"}]}),
            serde_json::json!({"models":[{"name":"fixture","digest":""}]}),
            serde_json::json!({"models":[{"name":"fixture","digest":7}]}),
            serde_json::json!({"models":[
                {"name":"fixture","digest":"digest-1"},
                {"name":"fixture"}
            ]}),
            serde_json::json!({"models":[
                {"name":"fixture","digest":"digest-1"},
                {"name":"fixture","digest":"digest-2"}
            ]}),
        ] {
            assert_eq!(
                parse_model_fingerprint(&rejected, "fixture"),
                Err(ContractError::KbNotReady)
            );
        }
    }

    #[tokio::test]
    async fn candidate_probe_freezes_fingerprint_and_dimension_with_audited_calls() {
        let (address, calls, server) = spawn_ollama_fixture(2).await;
        let candidate = LocalEmbeddingConfig {
            provider: "ollama_loopback".into(),
            endpoint: format!("http://{address}"),
            model: "fixture-model".into(),
        };
        let frozen = probe_and_freeze_candidate(&candidate).await.unwrap();
        server.await.unwrap();
        assert_eq!(frozen.fingerprint, "digest-1");
        assert_eq!(frozen.dimension, 2);
        assert_eq!(
            calls.lock().unwrap().as_slice(),
            &[("/api/tags".into(), 0), ("/api/embed".into(), 2)]
        );
    }

    #[tokio::test]
    async fn candidate_probe_rejects_fingerprint_count_and_dimension_errors() {
        let cases = [
            vec![r#"{"models":[{"name":"fixture-model"}]}"#.to_string()],
            vec![
                r#"{"models":[{"name":"fixture-model","digest":"digest-1"}]}"#.to_string(),
                r#"{"embeddings":[[1,0]]}"#.to_string(),
            ],
            vec![
                r#"{"models":[{"name":"fixture-model","digest":"digest-1"}]}"#.to_string(),
                r#"{"embeddings":[[1,0],[1,0,0]]}"#.to_string(),
            ],
        ];
        for responses in cases {
            let (address, server) = spawn_json_responses(responses).await;
            let candidate = LocalEmbeddingConfig {
                provider: "ollama_loopback".into(),
                endpoint: format!("http://{address}"),
                model: "fixture-model".into(),
            };
            assert_eq!(
                probe_and_freeze_candidate(&candidate).await,
                Err(ContractError::KbNotReady)
            );
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn batch_limits_execute_one_eight_thirty_two_and_reject_thirty_three() {
        let (address, calls, server) = spawn_ollama_fixture(3).await;
        let endpoint = validate_and_pin_loopback(&format!("http://{address}"), &resolver(&[]))
            .await
            .unwrap();
        let client = build_knowledge_embedding_client(&endpoint).unwrap();
        let audit = TestAudit::default();
        for size in [1_usize, 8, 32] {
            let texts = (0..size)
                .map(|index| format!("虚构批次{index}"))
                .collect::<Vec<_>>();
            assert_eq!(
                embed_batch(
                    &client,
                    &endpoint,
                    "fixture-model",
                    &texts,
                    EmbeddingOperation::EmbedBatch,
                    &audit,
                )
                .await
                .unwrap()
                .len(),
                size
            );
        }
        let too_many = (0..33).map(|index| index.to_string()).collect::<Vec<_>>();
        assert_eq!(
            embed_batch(
                &client,
                &endpoint,
                "fixture-model",
                &too_many,
                EmbeddingOperation::EmbedBatch,
                &audit,
            )
            .await,
            Err(ContractError::KbNotReady)
        );
        server.await.unwrap();
        assert_eq!(
            calls
                .lock()
                .unwrap()
                .iter()
                .map(|(_, size)| *size)
                .collect::<Vec<_>>(),
            vec![1, 8, 32]
        );
        assert_eq!(audit.0.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn building_resume_and_active_hybrid_execute_high_level_orchestration() {
        let (address, calls, server) = spawn_ollama_fixture(4).await;
        let (data_dir, store, generation, scope) = build_fixture(format!("http://{address}"));
        let pending = store
            .list_pending_build_embeddings(&generation, None, 32)
            .unwrap();
        assert_eq!(pending.len(), 2);
        store
            .write_build_embeddings(
                &generation,
                2,
                &[EncodedEmbedding {
                    chunk_key: pending[0].chunk_key.clone(),
                    content_hash: pending[0].content_hash.clone(),
                    blob: encode_embedding(&[1.0, 0.0]),
                }],
            )
            .unwrap();
        let first = index_building_generation(&store, &generation)
            .await
            .unwrap();
        assert_eq!(first.embedded_chunks, 1);
        assert_eq!(first.pending_chunks, 0);
        let hits = search_active_hybrid(
            &store,
            ActiveHybridRequest {
                scope: vec![scope],
                query: "周五同步".into(),
                from_ms: None,
                to_ms: None,
                top_k: 12,
            },
        )
        .await
        .unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|hit| hit.content.contains("周五同步")));
        server.await.unwrap();
        assert_eq!(
            calls
                .lock()
                .unwrap()
                .iter()
                .map(|(path, batch)| (path.as_str(), *batch))
                .collect::<Vec<_>>(),
            vec![
                ("/api/tags", 0),
                ("/api/embed", 1),
                ("/api/tags", 0),
                ("/api/embed", 1),
            ]
        );
        let _ = fs::remove_dir_all(data_dir);
    }

    #[tokio::test]
    async fn incomplete_active_combinations_fail_before_any_embedding_transport() {
        for corruption in [
            "catalog_null",
            "building",
            "failed",
            "snapshot_mismatch",
            "missing_completed",
            "missing_activated",
            "missing_mapping",
            "pointer_mismatch",
            "import_not_active",
        ] {
            let (data_dir, store, generation, scope) = build_fixture("http://127.0.0.1:9".into());
            let pending = store
                .list_pending_build_embeddings(&generation, None, 32)
                .unwrap();
            let rows = pending
                .iter()
                .map(|chunk| EncodedEmbedding {
                    chunk_key: chunk.chunk_key.clone(),
                    content_hash: chunk.content_hash.clone(),
                    blob: encode_embedding(&[1.0, 0.0]),
                })
                .collect::<Vec<_>>();
            store.write_build_embeddings(&generation, 2, &rows).unwrap();
            store
                .activate_completed_build_for_test(&generation)
                .unwrap();
            let connection =
                Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
            match corruption {
                "catalog_null" => connection.execute(
                    "UPDATE knowledge_catalog_state SET active_index_generation_id=NULL,active_snapshot_hash=NULL,activated_at_ms=NULL WHERE singleton_id=1",
                    [],
                ),
                "building" => connection.execute(
                    "UPDATE knowledge_index_generations SET status='building',completed_at_ms=NULL WHERE id=?1",
                    [&generation],
                ),
                "failed" => connection.execute(
                    "UPDATE knowledge_index_generations SET status='failed',completed_at_ms=NULL,error_code='KB_NOT_READY',error_summary='FIXTURE' WHERE id=?1",
                    [&generation],
                ),
                "snapshot_mismatch" => connection.execute(
                    "UPDATE knowledge_catalog_state SET active_snapshot_hash='mismatch' WHERE singleton_id=1",
                    [],
                ),
                "missing_completed" => connection.execute(
                    "UPDATE knowledge_index_generations SET completed_at_ms=NULL WHERE id=?1",
                    [&generation],
                ),
                "missing_activated" => connection.execute(
                    "UPDATE knowledge_catalog_state SET activated_at_ms=NULL WHERE singleton_id=1",
                    [],
                ),
                "missing_mapping" => connection.execute(
                    "DELETE FROM knowledge_index_generation_imports WHERE index_generation_id=?1",
                    [&generation],
                ),
                "pointer_mismatch" => connection.execute(
                    "UPDATE knowledge_conversations SET active_import_generation_id=NULL",
                    [],
                ),
                "import_not_active" => connection.execute(
                    "UPDATE knowledge_import_generations SET status='ready_candidate' WHERE status='active'",
                    [],
                ),
                _ => unreachable!(),
            }
            .unwrap();
            drop(connection);
            let audit = TestAudit::default();
            let endpoint_resolver = resolver(&[]);
            assert_eq!(
                search_active_hybrid_with(
                    &store,
                    ActiveHybridRequest {
                        scope: vec![scope],
                        query: "虚构查询".into(),
                        from_ms: None,
                        to_ms: None,
                        top_k: 12,
                    },
                    &endpoint_resolver,
                    &audit,
                )
                .await,
                Err(ContractError::KbNotReady),
                "corruption {corruption}"
            );
            assert_eq!(endpoint_resolver.calls.load(Ordering::SeqCst), 0);
            assert!(audit.0.lock().unwrap().is_empty());
            let _ = fs::remove_dir_all(data_dir);
        }
    }

    #[tokio::test]
    async fn redirects_are_not_followed() {
        let target = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let redirect = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_address = target.local_addr().unwrap();
        let redirect_address = redirect.local_addr().unwrap();
        let redirected_calls = Arc::new(AtomicUsize::new(0));
        let redirected_calls_task = redirected_calls.clone();
        let redirect_server = tokio::spawn(async move {
            if tokio::time::timeout(Duration::from_millis(250), redirect.accept())
                .await
                .is_ok()
            {
                redirected_calls_task.fetch_add(1, Ordering::SeqCst);
            }
        });
        let target_server = tokio::spawn(async move {
            let (mut stream, _) = target.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer).await.unwrap();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 302 Found\r\nLocation: http://{redirect_address}/api/tags\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let resolver = resolver(&[&target_address.to_string()]);
        let endpoint = validate_and_pin_loopback(
            &format!("http://localhost:{}", target_address.port()),
            &resolver,
        )
        .await
        .unwrap();
        let client = build_knowledge_embedding_client(&endpoint).unwrap();
        assert!(
            model_fingerprint(&client, &endpoint, "fixture", &LogEmbeddingAuditSink)
                .await
                .is_err()
        );
        target_server.await.unwrap();
        redirect_server.await.unwrap();
        assert_eq!(redirected_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn timeout_and_unavailable_loopback_fail_without_retry() {
        let stalled = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let stalled_address = stalled.local_addr().unwrap();
        let stalled_server = tokio::spawn(async move {
            let (_stream, _) = stalled.accept().await.unwrap();
            tokio::time::sleep(Duration::from_millis(150)).await;
        });
        let stalled_resolver = resolver(&[&stalled_address.to_string()]);
        let endpoint = validate_and_pin_loopback(
            &format!("http://localhost:{}", stalled_address.port()),
            &stalled_resolver,
        )
        .await
        .unwrap();
        let client = build_client_with_timeouts(
            &endpoint,
            Duration::from_millis(50),
            Duration::from_millis(50),
        )
        .unwrap();
        assert!(
            model_fingerprint(&client, &endpoint, "fixture", &LogEmbeddingAuditSink)
                .await
                .is_err()
        );
        stalled_server.await.unwrap();

        let unused = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let unused_address = unused.local_addr().unwrap();
        drop(unused);
        let unavailable_resolver = resolver(&[&unused_address.to_string()]);
        let endpoint = validate_and_pin_loopback(
            &format!("http://localhost:{}", unused_address.port()),
            &unavailable_resolver,
        )
        .await
        .unwrap();
        let client = build_client_with_timeouts(
            &endpoint,
            Duration::from_millis(50),
            Duration::from_millis(100),
        )
        .unwrap();
        assert!(
            model_fingerprint(&client, &endpoint, "fixture", &LogEmbeddingAuditSink)
                .await
                .is_err()
        );
    }

    #[test]
    fn hybrid_rrf_deduplicates_only_by_stable_chunk_key() {
        let vectors = vec![KnowledgeVectorHit {
            chunk_key: "chunk-b".into(),
            content: "相同正文".into(),
            token_count: 2,
            started_at_ms: 1,
            ended_at_ms: 2,
            score: 1.0,
        }];
        let fts = vec![
            FtsHit {
                chunk_key: "chunk-b".into(),
                content: "相同正文".into(),
                token_count: 2,
                started_at_ms: 1,
                ended_at_ms: 2,
                rank: -1.0,
            },
            FtsHit {
                chunk_key: "chunk-a".into(),
                content: "相同正文".into(),
                token_count: 2,
                started_at_ms: 1,
                ended_at_ms: 2,
                rank: -0.5,
            },
        ];
        let fused = fuse_hybrid(vectors, fts, 3).unwrap();
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].chunk_key, "chunk-b");
    }

    #[tokio::test]
    #[ignore = "requires explicit local Ollama endpoint and model"]
    async fn real_chinese_quality_probe() {
        use sha2::{Digest, Sha256};

        let Ok(raw_endpoint) = std::env::var("AICH8_KNOWLEDGE_PROBE_ENDPOINT") else {
            println!(
                "KNOWLEDGE_LOOPBACK_PROBE status=not-run reason=missing_explicit_local_config"
            );
            return;
        };
        let Ok(model) = std::env::var("AICH8_KNOWLEDGE_PROBE_MODEL") else {
            println!(
                "KNOWLEDGE_LOOPBACK_PROBE status=not-run reason=missing_explicit_local_config"
            );
            return;
        };
        let endpoint = validate_and_pin_loopback(&raw_endpoint, &SystemEndpointResolver)
            .await
            .unwrap();
        let client = build_knowledge_embedding_client(&endpoint).unwrap();
        let fingerprint = model_fingerprint(&client, &endpoint, &model, &LogEmbeddingAuditSink)
            .await
            .unwrap();
        let fixtures = [
            ("周五项目同步改到几点", "项目进度会安排在周五下午三点同步"),
            ("地铁站在哪里碰面", "我们约在人民广场地铁站七号出口见"),
            ("差旅费用怎么报销", "出差回来请在系统提交交通与住宿报销单"),
            ("新版本什么时候上线", "客户端二点零版本计划下周二发布"),
            (
                "测试环境的登录口令",
                "测试账号凭据由管理员在本机安全渠道提供",
            ),
            (
                "design review 改期了吗",
                "设计评审 design review 调整到了星期三",
            ),
            ("午餐订了哪家店", "中午预订了园区东门的素食餐厅"),
            ("合同需要谁盖章", "采购合同完成法务审核后交行政盖章"),
            ("会议纪要放在哪", "讨论记录已保存到项目共享目录的纪要文件夹"),
            ("服务器维护窗口", "服务端将在凌晨一点到两点执行例行维护"),
            ("客户演示用哪个分支", "演示环境使用 release demo 分支构建"),
            ("快递寄到什么地址", "样品请寄往创新园二号楼前台"),
        ];
        let mut latencies = Vec::new();
        let mut dimension = None;
        for batch_size in [1_usize, 8, 32] {
            let inputs = (0..batch_size)
                .map(|index| format!("完全虚构的本地质量探针句子编号{index}"))
                .collect::<Vec<_>>();
            let started = Instant::now();
            let vectors = embed_batch(
                &client,
                &endpoint,
                &model,
                &inputs,
                EmbeddingOperation::EmbedBatch,
                &LogEmbeddingAuditSink,
            )
            .await
            .unwrap();
            latencies.push(started.elapsed().as_millis() as u64);
            let current = vectors[0].len();
            assert!(vectors.iter().all(|vector| vector.len() == current));
            assert!(dimension.replace(current).is_none_or(|old| old == current));
        }
        let queries = fixtures
            .iter()
            .map(|(query, _)| (*query).to_string())
            .collect::<Vec<_>>();
        let documents = fixtures
            .iter()
            .map(|(_, document)| (*document).to_string())
            .collect::<Vec<_>>();
        let query_vectors = embed_batch(
            &client,
            &endpoint,
            &model,
            &queries,
            EmbeddingOperation::Query,
            &LogEmbeddingAuditSink,
        )
        .await
        .unwrap();
        let document_vectors = embed_batch(
            &client,
            &endpoint,
            &model,
            &documents,
            EmbeddingOperation::EmbedBatch,
            &LogEmbeddingAuditSink,
        )
        .await
        .unwrap();
        let matched = query_vectors
            .iter()
            .enumerate()
            .filter(|(expected, query)| {
                let mut scores = document_vectors
                    .iter()
                    .enumerate()
                    .map(|(index, document)| {
                        let score = query
                            .iter()
                            .zip(document)
                            .map(|(left, right)| f64::from(*left) * f64::from(*right))
                            .sum::<f64>();
                        (index, score)
                    })
                    .collect::<Vec<_>>();
                scores.sort_by(|left, right| right.1.total_cmp(&left.1));
                scores.iter().take(5).any(|(index, _)| index == expected)
            })
            .count();
        let recall = matched as f64 / fixtures.len() as f64;
        latencies.sort_unstable();
        let p50 = latencies[latencies.len() / 2];
        let p95 = latencies[latencies.len() - 1];
        let digest = hex::encode(Sha256::digest(fingerprint.as_bytes()));
        println!(
            "KNOWLEDGE_LOOPBACK_PROBE status={} digest_short={} dimension={} batch_sizes=1,8,32 p50_ms={} p95_ms={} recall_at_5={:.3}",
            if recall >= 0.80 { "pass" } else { "fail" },
            &digest[..12],
            dimension.unwrap(),
            p50,
            p95,
            recall
        );
        assert!(recall >= 0.80);
    }
}
