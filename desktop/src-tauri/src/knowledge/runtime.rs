#[allow(dead_code)]
pub(crate) trait KnowledgeStorePort {
    fn retrieval_is_unavailable(&self) -> &'static str;
}

#[allow(dead_code)]
pub(crate) struct UninitializedKnowledgeStore;

impl KnowledgeStorePort for UninitializedKnowledgeStore {
    fn retrieval_is_unavailable(&self) -> &'static str {
        "KB_NOT_READY"
    }
}
