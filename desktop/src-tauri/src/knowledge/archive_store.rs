use super::archive_schema::CoverageKind;
use sha2::{Digest, Sha256};

/// Archive DTOs deliberately contain no SQLite connection or SQL. The single
/// database owner is `KnowledgeStore`.
#[derive(Clone, Debug)]
pub(crate) struct ImportFingerprint {
    pub(crate) account_stable_id: String,
    pub(crate) export_id: String,
    pub(crate) schema_version: String,
    pub(crate) manifest_content_hash: String,
    pub(crate) coverage_signature: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MemberAudit {
    pub(crate) member_path_token: String,
    pub(crate) member_kind: &'static str,
    pub(crate) size_bytes: u64,
    pub(crate) mtime_ms: i64,
    pub(crate) declared_hash: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompletenessVerdict {
    FullDeclared,
    FilteredSelected,
    Incomplete,
    Failed,
}

impl CompletenessVerdict {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::FullDeclared => "full_declared",
            Self::FilteredSelected => "filtered_selected",
            Self::Incomplete => "incomplete",
            Self::Failed => "failed",
        }
    }
}

pub(crate) fn member_path_token(member_path: &str) -> String {
    format!("{:x}", Sha256::digest(member_path.as_bytes()))
}

pub(crate) fn coverage_signature(
    coverage: CoverageKind,
    filters_json: &str,
    stats: (u64, u64),
) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}|{}|{}|{}",
                coverage.as_str(),
                filters_json,
                stats.0,
                stats.1
            )
            .as_bytes()
        )
    )
}
