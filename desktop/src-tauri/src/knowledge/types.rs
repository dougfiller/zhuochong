use crate::wechat::types::{BindingGeneration, ContractError, RequestId};
use serde::{Deserialize, Serialize};

macro_rules! generation_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(transparent)]
        pub(crate) struct $name(u64);
    };
}

generation_type!(CatalogGeneration);
generation_type!(IndexGeneration);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub(crate) enum KnowledgeScope {
    #[serde(rename = "conversation")]
    Conversation { id: String },
    #[serde(rename = "selected_conversations")]
    SelectedConversations { ids: Vec<String> },
    #[serde(rename = "global_user_selected")]
    GlobalUserSelected,
}

impl KnowledgeScope {
    pub(crate) fn validate(&self) -> Result<(), ContractError> {
        if matches!(self, Self::SelectedConversations { ids } if ids.is_empty()) {
            return Err(ContractError::KbScopeUnresolved);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KnowledgeRetrieveRequest {
    pub(crate) request_id: RequestId,
    pub(crate) query: String,
    pub(crate) scope: KnowledgeScope,
    pub(crate) binding_generation: BindingGeneration,
    pub(crate) catalog_generation: CatalogGeneration,
    pub(crate) index_generation: IndexGeneration,
    pub(crate) top_k: u8,
    pub(crate) token_budget: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum RetrievalStatus {
    #[serde(rename = "success")]
    Success,
    #[serde(rename = "no_hit")]
    NoHit,
    #[serde(rename = "fts_fallback")]
    FtsFallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RetrievalOutcome {
    Retrieved(RetrievalStatus),
    Failed(ContractError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct KnowledgeHit {
    excerpt: String,
    token_count: u32,
}

#[derive(Clone, Debug)]
pub(crate) struct KnowledgeRetrieveResult {
    request_id: RequestId,
    query: String,
    outcome: RetrievalOutcome,
    hits: Vec<KnowledgeHit>,
    token_budget: u32,
}

impl KnowledgeRetrieveResult {
    #[cfg(test)]
    fn fixture(
        request_id: RequestId,
        query: &str,
        outcome: RetrievalOutcome,
        excerpts: &[(&str, u32)],
        token_budget: u32,
    ) -> Self {
        Self {
            request_id,
            query: query.into(),
            outcome,
            hits: excerpts
                .iter()
                .map(|(excerpt, token_count)| KnowledgeHit {
                    excerpt: (*excerpt).into(),
                    token_count: *token_count,
                })
                .collect(),
            token_budget,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RetrievedReply {
    request_id: RequestId,
    query: String,
    status: RetrievalStatus,
    excerpts: Vec<String>,
}

impl RetrievedReply {
    pub(crate) fn request_id(&self) -> &RequestId {
        &self.request_id
    }
    pub(crate) fn query(&self) -> &str {
        &self.query
    }
    pub(crate) fn is_no_hit(&self) -> bool {
        self.status == RetrievalStatus::NoHit
    }
    pub(crate) fn excerpts(&self) -> Vec<String> {
        self.excerpts.clone()
    }
}

// Future retrieval code may create this envelope only after a successful query.
pub(in crate::knowledge) fn knowledge_retrieve(
    result: KnowledgeRetrieveResult,
) -> Result<RetrievedReply, ContractError> {
    match result.outcome {
        RetrievalOutcome::Retrieved(status) => {
            let mut remaining_tokens = result.token_budget;
            let excerpts = result
                .hits
                .into_iter()
                .filter_map(|hit| {
                    if hit.token_count > remaining_tokens {
                        return None;
                    }
                    remaining_tokens -= hit.token_count;
                    Some(hit.excerpt)
                })
                .collect();

            Ok(RetrievedReply {
                request_id: result.request_id,
                query: result.query,
                status,
                excerpts,
            })
        }
        RetrievalOutcome::Failed(error) => Err(error),
    }
}

#[cfg(test)]
pub(crate) fn retrieval_fixture(
    request_id: RequestId,
    query: &str,
    outcome: RetrievalOutcome,
    excerpts: &[(&str, u32)],
    token_budget: u32,
) -> Result<RetrievedReply, ContractError> {
    knowledge_retrieve(KnowledgeRetrieveResult::fixture(
        request_id,
        query,
        outcome,
        excerpts,
        token_budget,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_wire_shape_is_explicit_and_empty_selection_is_rejected() {
        assert_eq!(
            serde_json::to_string(&KnowledgeScope::Conversation {
                id: "conversation-stable-id".into()
            })
            .unwrap(),
            "{\"kind\":\"conversation\",\"id\":\"conversation-stable-id\"}"
        );
        assert_eq!(
            serde_json::to_string(&KnowledgeScope::SelectedConversations {
                ids: vec!["conversation-a".into(), "conversation-b".into()]
            })
            .unwrap(),
            "{\"kind\":\"selected_conversations\",\"ids\":[\"conversation-a\",\"conversation-b\"]}"
        );
        assert_eq!(
            serde_json::to_string(&KnowledgeScope::GlobalUserSelected).unwrap(),
            "{\"kind\":\"global_user_selected\"}"
        );
        assert_eq!(
            KnowledgeScope::SelectedConversations { ids: vec![] }.validate(),
            Err(ContractError::KbScopeUnresolved)
        );
    }

    #[test]
    fn no_hit_is_a_retrieved_reply_not_an_error() {
        let reply = knowledge_retrieve(KnowledgeRetrieveResult::fixture(
            RequestId::new(),
            "请回复",
            RetrievalOutcome::Retrieved(RetrievalStatus::NoHit),
            &[],
            0,
        ))
        .unwrap();
        assert!(reply.is_no_hit());
        assert!(reply.excerpts().is_empty());
    }

    #[test]
    fn retrieved_reply_excerpts_do_not_exceed_the_token_budget() {
        let reply = knowledge_retrieve(KnowledgeRetrieveResult::fixture(
            RequestId::new(),
            "请回复",
            RetrievalOutcome::Retrieved(RetrievalStatus::Success),
            &[("预算内摘要", 3), ("超预算摘要", 2)],
            4,
        ))
        .unwrap();

        assert_eq!(reply.excerpts(), vec!["预算内摘要"]);
    }
}
