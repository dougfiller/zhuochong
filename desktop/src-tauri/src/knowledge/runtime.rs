use super::archive_importer::{ArchiveImportSummary, WechatJsonArchiveImporter};
use crate::wechat::types::ContractError;
use std::path::Path;

/// Managed knowledge facade. Archive connections are created only for an
/// explicit import and only below the caller-provided product data directory.
#[derive(Default)]
pub(crate) struct KnowledgeStore;

impl KnowledgeStore {
    pub(crate) fn import_wechat_json_archive(
        &self,
        source_root: &Path,
        data_dir: &Path,
    ) -> Result<ArchiveImportSummary, ContractError> {
        WechatJsonArchiveImporter::open(source_root, data_dir)?.import()
    }
}

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
