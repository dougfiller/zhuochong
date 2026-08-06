use crate::wechat::types::ContractError;
use serde::Deserialize;

pub(crate) const WECHAT_ARCHIVE_V1: &str = "wechat_archive_v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArchiveSchemaVersion {
    WechatArchiveV1,
}

pub(crate) fn route_manifest(
    manifest: &ManifestProbeV1,
) -> Result<ArchiveSchemaVersion, ContractError> {
    (manifest.schema_version == WECHAT_ARCHIVE_V1
        && manifest.source.kind == "user_selected"
        && matches!(manifest.scope.kind.as_str(), "selected" | "full")
        && !manifest.options.include_media)
        .then_some(ArchiveSchemaVersion::WechatArchiveV1)
        .ok_or(ContractError::KbSourceUnsupported)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ManifestProbeV1 {
    pub(crate) schema_version: String,
    pub(crate) exported_at: String,
    pub(crate) export_id: String,
    pub(crate) account: ArchiveAccountV1,
    pub(crate) source: SourceProbeV1,
    pub(crate) format: FormatProbeV1,
    pub(crate) scope: ScopeProbeV1,
    pub(crate) filters: FiltersProbeV1,
    pub(crate) options: OptionsProbeV1,
    pub(crate) stats: StatsProbeV1,
    pub(crate) accounts_available: Vec<String>,
    pub(crate) integrity_files: Vec<DeclaredMemberV1>,
}

impl ManifestProbeV1 {
    pub(crate) fn declared_manifest_hash(&self) -> Option<&str> {
        self.format.manifest_content_hash.as_deref()
    }

    pub(crate) fn coverage(&self) -> CoverageKind {
        if self.scope.kind == "selected" {
            CoverageKind::Selected
        } else if self.scope.kind == "full"
            && self.filters.is_empty()
            && self.stats.missing_count == 0
        {
            CoverageKind::Full
        } else {
            CoverageKind::Filtered
        }
    }

    pub(crate) fn declared_conversations(&self) -> &[DeclaredConversationV1] {
        &self.scope.conversations
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ArchiveAccountV1 {
    pub(crate) stable_id: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SourceProbeV1 {
    pub(crate) kind: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FormatProbeV1 {
    pub(crate) manifest_content_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ScopeProbeV1 {
    pub(crate) kind: String,
    pub(crate) conversations: Vec<DeclaredConversationV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DeclaredConversationV1 {
    pub(crate) stable_id: String,
    pub(crate) meta_path: String,
    pub(crate) messages_path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DeclaredMemberV1 {
    pub(crate) path: String,
    pub(crate) declared_hash: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FiltersProbeV1 {
    #[serde(default)]
    pub(crate) start_at: Option<String>,
    #[serde(default)]
    pub(crate) end_at: Option<String>,
    #[serde(default)]
    pub(crate) message_types: Vec<String>,
}

impl FiltersProbeV1 {
    fn is_empty(&self) -> bool {
        self.start_at.is_none() && self.end_at.is_none() && self.message_types.is_empty()
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OptionsProbeV1 {
    #[serde(default)]
    pub(crate) include_media: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StatsProbeV1 {
    pub(crate) conversation_count: u64,
    pub(crate) message_count: u64,
    #[serde(default)]
    pub(crate) missing_count: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReportProbeV1 {
    pub(crate) schema_version: String,
    pub(crate) export_id: String,
    pub(crate) account: ArchiveAccountV1,
    pub(crate) created_at: String,
    #[serde(default)]
    pub(crate) missing_media: u64,
    #[serde(default)]
    pub(crate) errors: Vec<ReportErrorV1>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReportErrorV1 {
    pub(crate) code: String,
}

impl ReportProbeV1 {
    pub(crate) fn matches_manifest(&self, manifest: &ManifestProbeV1) -> bool {
        self.schema_version == manifest.schema_version
            && self.export_id == manifest.export_id
            && self.account.stable_id == manifest.account.stable_id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoverageKind {
    Full,
    Filtered,
    Selected,
}

impl CoverageKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Filtered => "filtered",
            Self::Selected => "selected",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(scope: &str, filters: &str) -> ManifestProbeV1 {
        serde_json::from_str(&format!(
            r#"{{"schemaVersion":"wechat_archive_v1","exportedAt":"2026-01-01T00:00:00Z","exportId":"export_fixture","account":{{"stableId":"acct_fixture_01"}},"source":{{"kind":"user_selected"}},"format":{{"manifestContentHash":"declared_fixture_hash"}},"scope":{{"kind":"{scope}","conversations":[]}},"filters":{filters},"options":{{"includeMedia":false}},"stats":{{"conversationCount":0,"messageCount":0,"missingCount":0}},"accountsAvailable":[],"integrityFiles":[]}}"#
        ))
        .unwrap()
    }

    #[test]
    fn unknown_schema_and_future_fields_fail_closed() {
        let unsupported = manifest("full", "{}");
        let mut unsupported = unsupported;
        unsupported.schema_version = "wechat_archive_v2".into();
        assert_eq!(
            route_manifest(&unsupported),
            Err(ContractError::KbSourceUnsupported)
        );
        assert!(serde_json::from_str::<ManifestProbeV1>(
            r#"{"schemaVersion":"wechat_archive_v1","unknown":true}"#
        )
        .is_err());
    }

    #[test]
    fn selected_and_filters_never_upgrade_to_full() {
        assert_eq!(
            manifest("selected", "{}").coverage(),
            CoverageKind::Selected
        );
        assert_eq!(
            manifest("full", r#"{"messageTypes":["text"]}"#).coverage(),
            CoverageKind::Filtered
        );
        assert_eq!(manifest("full", "{}").coverage(), CoverageKind::Full);
    }

    #[test]
    fn unsupported_source_scope_and_media_contracts_fail_closed() {
        let mut unsupported_source = manifest("selected", "{}");
        unsupported_source.source.kind = "future_source".into();
        assert_eq!(
            route_manifest(&unsupported_source),
            Err(ContractError::KbSourceUnsupported)
        );

        let unsupported_scope = manifest("future_scope", "{}");
        assert_eq!(
            route_manifest(&unsupported_scope),
            Err(ContractError::KbSourceUnsupported)
        );

        let mut media_requested = manifest("selected", "{}");
        media_requested.options.include_media = true;
        assert_eq!(
            route_manifest(&media_requested),
            Err(ContractError::KbSourceUnsupported)
        );
    }
}
