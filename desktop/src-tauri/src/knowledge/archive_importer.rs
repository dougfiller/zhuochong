use super::archive_schema::{
    ArchiveAccountV1, ArchiveSchemaVersion, CoverageKind, DeclaredConversationV1, DeclaredMemberV1,
    FiltersProbeV1, FormatProbeV1, ManifestProbeV1, OptionsProbeV1, ReportProbeV1, SourceProbeV1,
    StatsProbeV1, WECHAT_ARCHIVE_V1,
};
use super::archive_store::{
    coverage_signature, member_path_token, CompletenessVerdict, ImportFingerprint, MemberAudit,
    SourceAuditAccumulator, SourceAuditDigest,
};
use super::store::{IncomingMediaRef, IncomingMessage, KnowledgeStore, NewSource};
use crate::wechat::types::ContractError;
use serde::de::{DeserializeSeed, Error as DeError, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufReader, Read};
use std::path::{Component, Path, PathBuf};

const MAX_JSON_STRING_BYTES: usize = 64 * 1024;

/// Read-only capability for the v1 allow-list. It intentionally has no write,
/// rename, deletion, directory-walk, or media API.
pub(crate) struct WechatArchiveReadGuard {
    root: PathBuf,
}

impl WechatArchiveReadGuard {
    pub(crate) fn open(root: impl AsRef<Path>) -> Result<Self, ContractError> {
        let root = root.as_ref();
        let metadata =
            fs::symlink_metadata(root).map_err(|_| ContractError::KbSourceUnsupported)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ContractError::KbSourceUnsupported);
        }
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    pub(crate) fn open_manifest(&self) -> Result<File, ContractError> {
        self.open_fixed("manifest.json")
    }

    pub(crate) fn open_report(&self) -> Result<File, ContractError> {
        self.open_fixed("report.json")
    }

    pub(crate) fn open_declared_integrity(&self, member: &str) -> Result<File, ContractError> {
        if !member.starts_with("_integrity/") {
            return Err(ContractError::KbSourceUnsupported);
        }
        self.open_safe(&normalize_member_path(member)?)
    }

    pub(crate) fn open_conversation_meta(
        &self,
        conversation: &DeclaredConversationV1,
    ) -> Result<File, ContractError> {
        self.open_safe(&normalize_member_path(&conversation.meta_path)?)
    }

    fn open_messages_stream(
        &self,
        conversation: &DeclaredConversationV1,
    ) -> Result<BufReader<JsonStringLimitReader<File>>, ContractError> {
        Ok(BufReader::new(JsonStringLimitReader::new(self.open_safe(
            &normalize_member_path(&conversation.messages_path)?,
        )?)))
    }

    fn open_fixed(&self, member: &str) -> Result<File, ContractError> {
        self.open_safe(&normalize_member_path(member)?)
    }

    fn open_safe(&self, member: &str) -> Result<File, ContractError> {
        let file = open_member_no_follow(&self.root, member)?;
        let metadata = file
            .metadata()
            .map_err(|_| ContractError::KbSourceUnsupported)?;
        if !metadata.is_file() {
            return Err(ContractError::KbSourceUnsupported);
        }
        Ok(file)
    }

    fn member_audit(
        &self,
        member: &str,
        kind: &'static str,
        declared_hash: Option<String>,
    ) -> Result<MemberAudit, ContractError> {
        let member = normalize_member_path(member)?;
        let metadata = self
            .open_safe(&member)?
            .metadata()
            .map_err(|_| ContractError::KbSourceUnsupported)?;
        let mtime_ms = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or_default();
        Ok(MemberAudit {
            member_path_token: member_path_token(&member),
            member_kind: kind,
            size_bytes: metadata.len(),
            mtime_ms,
            declared_hash,
        })
    }
}

#[cfg(unix)]
fn open_member_no_follow(root: &Path, member: &str) -> Result<File, ContractError> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;

    let mut directory = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(root)
        .map_err(|_| ContractError::KbSourceUnsupported)?;
    let components: Vec<_> = Path::new(member).components().collect();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(component) = component else {
            return Err(ContractError::KbSourceUnsupported);
        };
        let component =
            CString::new(component.as_bytes()).map_err(|_| ContractError::KbSourceUnsupported)?;
        let last = index + 1 == components.len();
        let flags = libc::O_RDONLY | libc::O_NOFOLLOW | if last { 0 } else { libc::O_DIRECTORY };
        let fd = unsafe { libc::openat(directory.as_raw_fd(), component.as_ptr(), flags) };
        if fd < 0 {
            return Err(ContractError::KbSourceUnsupported);
        }
        let opened = unsafe { File::from_raw_fd(fd) };
        if last {
            return Ok(opened);
        }
        directory = opened;
    }
    Err(ContractError::KbSourceUnsupported)
}

#[cfg(windows)]
fn open_member_no_follow(root: &Path, member: &str) -> Result<File, ContractError> {
    use std::os::windows::fs::{FileTypeExt, OpenOptionsExt};

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(0x0020_0000)
        .open(root.join(member))
        .map_err(|_| ContractError::KbSourceUnsupported)?;
    if file
        .metadata()
        .map_err(|_| ContractError::KbSourceUnsupported)?
        .file_type()
        .is_reparse_point()
    {
        return Err(ContractError::KbSourceUnsupported);
    }
    Ok(file)
}

#[cfg(not(any(unix, windows)))]
fn open_member_no_follow(_root: &Path, _member: &str) -> Result<File, ContractError> {
    Err(ContractError::KbSourceUnsupported)
}

fn normalize_member_path(raw: &str) -> Result<String, ContractError> {
    let path = Path::new(raw);
    if raw.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ContractError::KbSourceUnsupported);
    }
    let normalized = path
        .to_str()
        .ok_or(ContractError::KbSourceUnsupported)?
        .replace('\\', "/");
    if normalized.starts_with('/') || normalized.contains("//") {
        return Err(ContractError::KbSourceUnsupported);
    }
    Ok(normalized)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArchiveImportSummary {
    pub(crate) import_id: String,
    pub(crate) schema: ArchiveSchemaVersion,
    pub(crate) coverage: CoverageKind,
    pub(crate) verdict: CompletenessVerdict,
    pub(crate) conversation_count: u64,
    pub(crate) message_count: u64,
    pub(crate) fast_verified: bool,
}

#[derive(Clone, Debug)]
struct ManifestHeader {
    schema_version: String,
    exported_at: String,
    export_id: String,
    account: ArchiveAccountV1,
    source: SourceProbeV1,
    format: FormatProbeV1,
    scope_kind: String,
    filters: FiltersProbeV1,
    options: OptionsProbeV1,
    stats: StatsProbeV1,
    conversation_count: u64,
}

impl ManifestHeader {
    fn route(&self) -> Result<ArchiveSchemaVersion, ContractError> {
        (self.schema_version == WECHAT_ARCHIVE_V1
            && self.source.kind == "user_selected"
            && matches!(self.scope_kind.as_str(), "selected" | "full")
            && !self.options.include_media)
            .then_some(ArchiveSchemaVersion::WechatArchiveV1)
            .ok_or(ContractError::KbSourceUnsupported)
    }

    fn exported_at_ms(&self) -> Result<i64, ContractError> {
        chrono::DateTime::parse_from_rfc3339(&self.exported_at)
            .map(|value| value.timestamp_millis())
            .map_err(|_| ContractError::KbSourceUnsupported)
    }

    fn coverage(&self) -> CoverageKind {
        if self.scope_kind == "selected" {
            CoverageKind::Selected
        } else if self.scope_kind == "full"
            && self.filters.start_at.is_none()
            && self.filters.end_at.is_none()
            && self.filters.message_types.is_empty()
            && self.stats.missing_count == 0
        {
            CoverageKind::Full
        } else {
            CoverageKind::Filtered
        }
    }

    fn manifest_hash(&self) -> Result<&str, ContractError> {
        self.format
            .manifest_content_hash
            .as_deref()
            .ok_or(ContractError::KbSourceUnsupported)
    }
}

fn stream_manifest<R: Read, C, I>(
    reader: R,
    on_conversation: &mut C,
    on_integrity: &mut I,
) -> Result<ManifestHeader, ContractError>
where
    C: FnMut(DeclaredConversationV1) -> Result<(), ContractError>,
    I: FnMut(DeclaredMemberV1) -> Result<(), ContractError>,
{
    ManifestSeed {
        on_conversation,
        on_integrity,
    }
    .deserialize(&mut serde_json::Deserializer::from_reader(reader))
    .map_err(|_| ContractError::KbSourceUnsupported)
}

struct ManifestSeed<'a, C, I> {
    on_conversation: &'a mut C,
    on_integrity: &'a mut I,
}
impl<'de, C, I> DeserializeSeed<'de> for ManifestSeed<'_, C, I>
where
    C: FnMut(DeclaredConversationV1) -> Result<(), ContractError>,
    I: FnMut(DeclaredMemberV1) -> Result<(), ContractError>,
{
    type Value = ManifestHeader;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(ManifestVisitor {
            on_conversation: self.on_conversation,
            on_integrity: self.on_integrity,
        })
    }
}
struct ManifestVisitor<'a, C, I> {
    on_conversation: &'a mut C,
    on_integrity: &'a mut I,
}
impl<'de, C, I> Visitor<'de> for ManifestVisitor<'_, C, I>
where
    C: FnMut(DeclaredConversationV1) -> Result<(), ContractError>,
    I: FnMut(DeclaredMemberV1) -> Result<(), ContractError>,
{
    type Value = ManifestHeader;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a strict streaming wechat archive manifest")
    }
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut schema_version = None;
        let mut exported_at = None;
        let mut export_id = None;
        let mut account = None;
        let mut source = None;
        let mut format = None;
        let mut scope = None;
        let mut filters = None;
        let mut options = None;
        let mut stats = None;
        let mut accounts_available = false;
        let mut integrity_files = false;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "schemaVersion" if schema_version.is_none() => {
                    schema_version = Some(map.next_value()?)
                }
                "exportedAt" if exported_at.is_none() => exported_at = Some(map.next_value()?),
                "exportId" if export_id.is_none() => export_id = Some(map.next_value()?),
                "account" if account.is_none() => account = Some(map.next_value()?),
                "source" if source.is_none() => source = Some(map.next_value()?),
                "format" if format.is_none() => format = Some(map.next_value()?),
                "scope" if scope.is_none() => {
                    scope = Some(map.next_value_seed(ManifestScopeSeed {
                        on_conversation: self.on_conversation,
                    })?)
                }
                "filters" if filters.is_none() => filters = Some(map.next_value()?),
                "options" if options.is_none() => options = Some(map.next_value()?),
                "stats" if stats.is_none() => stats = Some(map.next_value()?),
                "accountsAvailable" if !accounts_available => {
                    let _: IgnoredAny = map.next_value()?;
                    accounts_available = true;
                }
                "integrityFiles" if !integrity_files => {
                    map.next_value_seed(ManifestIntegritySeed {
                        on_integrity: self.on_integrity,
                    })?;
                    integrity_files = true;
                }
                _ => return Err(A::Error::custom("unknown or duplicate manifest field")),
            }
        }
        let (scope_kind, conversation_count) =
            scope.ok_or_else(|| A::Error::missing_field("scope"))?;
        Ok(ManifestHeader {
            schema_version: schema_version
                .ok_or_else(|| A::Error::missing_field("schemaVersion"))?,
            exported_at: exported_at.ok_or_else(|| A::Error::missing_field("exportedAt"))?,
            export_id: export_id.ok_or_else(|| A::Error::missing_field("exportId"))?,
            account: account.ok_or_else(|| A::Error::missing_field("account"))?,
            source: source.ok_or_else(|| A::Error::missing_field("source"))?,
            format: format.ok_or_else(|| A::Error::missing_field("format"))?,
            scope_kind,
            filters: filters.ok_or_else(|| A::Error::missing_field("filters"))?,
            options: options.ok_or_else(|| A::Error::missing_field("options"))?,
            stats: stats.ok_or_else(|| A::Error::missing_field("stats"))?,
            conversation_count,
        })
    }
}
struct ManifestScopeSeed<'a, C> {
    on_conversation: &'a mut C,
}
impl<'de, C> DeserializeSeed<'de> for ManifestScopeSeed<'_, C>
where
    C: FnMut(DeclaredConversationV1) -> Result<(), ContractError>,
{
    type Value = (String, u64);
    fn deserialize<D>(self, d: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        d.deserialize_map(ManifestScopeVisitor {
            on_conversation: self.on_conversation,
        })
    }
}
struct ManifestScopeVisitor<'a, C> {
    on_conversation: &'a mut C,
}
impl<'de, C> Visitor<'de> for ManifestScopeVisitor<'_, C>
where
    C: FnMut(DeclaredConversationV1) -> Result<(), ContractError>,
{
    type Value = (String, u64);
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a manifest scope")
    }
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut kind = None;
        let mut seen = false;
        let mut count = 0;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "kind" if kind.is_none() => kind = Some(map.next_value()?),
                "conversations" if !seen => {
                    map.next_value_seed(ManifestConversationSeed {
                        on_conversation: self.on_conversation,
                        count: &mut count,
                    })?;
                    seen = true;
                }
                _ => return Err(A::Error::custom("unknown or duplicate scope field")),
            }
        }
        Ok((kind.ok_or_else(|| A::Error::missing_field("kind"))?, count))
    }
}
struct ManifestConversationSeed<'a, C> {
    on_conversation: &'a mut C,
    count: &'a mut u64,
}
impl<'de, C> DeserializeSeed<'de> for ManifestConversationSeed<'_, C>
where
    C: FnMut(DeclaredConversationV1) -> Result<(), ContractError>,
{
    type Value = ();
    fn deserialize<D>(self, d: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        d.deserialize_seq(ManifestConversationVisitor {
            on_conversation: self.on_conversation,
            count: self.count,
        })
    }
}
struct ManifestConversationVisitor<'a, C> {
    on_conversation: &'a mut C,
    count: &'a mut u64,
}
impl<'de, C> Visitor<'de> for ManifestConversationVisitor<'_, C>
where
    C: FnMut(DeclaredConversationV1) -> Result<(), ContractError>,
{
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("manifest conversations")
    }
    fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        while let Some(conversation) = seq.next_element::<DeclaredConversationV1>()? {
            (self.on_conversation)(conversation)
                .map_err(|_| A::Error::custom("manifest conversation rejected"))?;
            *self.count += 1;
        }
        Ok(())
    }
}
struct ManifestIntegritySeed<'a, I> {
    on_integrity: &'a mut I,
}
impl<'de, I> DeserializeSeed<'de> for ManifestIntegritySeed<'_, I>
where
    I: FnMut(DeclaredMemberV1) -> Result<(), ContractError>,
{
    type Value = ();
    fn deserialize<D>(self, d: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        d.deserialize_seq(ManifestIntegrityVisitor {
            on_integrity: self.on_integrity,
        })
    }
}
struct ManifestIntegrityVisitor<'a, I> {
    on_integrity: &'a mut I,
}
impl<'de, I> Visitor<'de> for ManifestIntegrityVisitor<'_, I>
where
    I: FnMut(DeclaredMemberV1) -> Result<(), ContractError>,
{
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("manifest integrity files")
    }
    fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        while let Some(member) = seq.next_element::<DeclaredMemberV1>()? {
            (self.on_integrity)(member)
                .map_err(|_| A::Error::custom("manifest integrity rejected"))?;
        }
        Ok(())
    }
}

pub(crate) struct WechatJsonArchiveImporter<'a> {
    guard: WechatArchiveReadGuard,
    store: &'a KnowledgeStore,
}

impl<'a> WechatJsonArchiveImporter<'a> {
    pub(crate) fn open(
        source_root: impl AsRef<Path>,
        store: &'a KnowledgeStore,
    ) -> Result<Self, ContractError> {
        Ok(Self {
            guard: WechatArchiveReadGuard::open(source_root)?,
            store,
        })
    }

    /// Imports only the manifest-declared JSON members. Source paths and bodies
    /// never leave this method; only anonymous IDs, counts, and verdicts reach
    /// the derived store.
    pub(crate) fn import(&mut self) -> Result<ArchiveImportSummary, ContractError> {
        let mut declared_conversations = 0;
        let mut validate_conversation = |conversation: DeclaredConversationV1| {
            normalize_member_path(&conversation.meta_path)?;
            normalize_member_path(&conversation.messages_path)?;
            declared_conversations += 1;
            Ok(())
        };
        let mut validate_integrity = |member: DeclaredMemberV1| {
            if !member.path.starts_with("_integrity/") {
                return Err(ContractError::KbSourceUnsupported);
            }
            normalize_member_path(&member.path).map(|_| ())
        };
        let manifest = stream_manifest(
            self.guard.open_manifest()?,
            &mut validate_conversation,
            &mut validate_integrity,
        )?;
        let schema = manifest.route()?;
        if declared_conversations != manifest.conversation_count
            || manifest.conversation_count != manifest.stats.conversation_count
        {
            return Err(ContractError::KbSourceUnsupported);
        }
        let declared_hash = manifest.manifest_hash()?.to_owned();
        let report: ReportProbeV1 = serde_json::from_reader(self.guard.open_report()?)
            .map_err(|_| ContractError::KbSourceUnsupported)?;
        if report.schema_version != manifest.schema_version
            || report.export_id != manifest.export_id
            || report.account.stable_id != manifest.account.stable_id
        {
            return Err(ContractError::KbSourceUnsupported);
        }
        let coverage = manifest.coverage();
        let exported_at_ms = manifest.exported_at_ms()?;
        let before = self.source_audit(&manifest, None)?;
        let fingerprint = ImportFingerprint {
            account_stable_id: manifest.account.stable_id.clone(),
            export_id: manifest.export_id.clone(),
            schema_version: manifest.schema_version.clone(),
            manifest_content_hash: declared_hash,
            coverage_signature: coverage_signature(
                coverage,
                &safe_scope_filters_json(&manifest),
                (
                    manifest.stats.conversation_count,
                    manifest.stats.message_count,
                ),
            ),
        };
        if self.store.fast_verify_archive(&fingerprint, &before)? {
            return Ok(ArchiveImportSummary {
                import_id: "fast_verified".into(),
                schema,
                coverage,
                verdict: usable_verdict(coverage, &report),
                conversation_count: manifest.stats.conversation_count,
                message_count: manifest.stats.message_count,
                fast_verified: true,
            });
        }

        let source = NewSource {
            account_stable_id: manifest.account.stable_id.clone(),
            conversation_stable_id: String::new(),
            export_id: manifest.export_id.clone(),
            schema_version: manifest.schema_version.clone(),
            manifest_hash: fingerprint.manifest_content_hash.clone(),
            coverage_hash: fingerprint.coverage_signature.clone(),
            exported_at_ms,
            coverage_kind: coverage,
            display_metadata_json: None,
        };
        let mut conversation_count = 0;
        let mut message_count = 0;
        let mut failed_conversations = 0;
        let mut source_id = None;
        let mut import_conversation = |conversation: DeclaredConversationV1| {
            conversation_count += 1;
            let outcome = self.import_conversation(
                &manifest,
                &conversation,
                &source,
                usable_verdict(coverage, &report),
            );
            match outcome {
                Ok((count, conversation_source_id)) => {
                    message_count += count;
                    source_id.get_or_insert(conversation_source_id);
                }
                Err(_) => failed_conversations += 1,
            }
            Ok(())
        };
        let mut ignore_integrity = |_member: DeclaredMemberV1| Ok(());
        let second_manifest = stream_manifest(
            self.guard.open_manifest()?,
            &mut import_conversation,
            &mut ignore_integrity,
        )?;
        if !same_manifest_header(&manifest, &second_manifest) {
            return Err(ContractError::KbSourceUnsupported);
        }
        if failed_conversations == conversation_count {
            return Err(ContractError::KbSourceUnsupported);
        }
        if conversation_count != manifest.stats.conversation_count
            || (failed_conversations == 0 && message_count != manifest.stats.message_count)
        {
            if let Some(source_id) = source_id.as_deref() {
                self.store.discard_source_candidates(source_id)?;
            }
            return Err(ContractError::KbSourceUnsupported);
        }
        let after = match self.source_audit(&manifest, source_id.as_deref()) {
            Ok(after) => after,
            Err(error) => {
                if let Some(source_id) = source_id.as_deref() {
                    self.store.discard_source_candidates(source_id)?;
                }
                return Err(error);
            }
        };
        if before != after {
            if let Some(source_id) = source_id.as_deref() {
                self.store.discard_source_candidates(source_id)?;
            }
            return Err(ContractError::KbSourceUnsupported);
        }
        if let Some(source_id) = source_id.as_deref() {
            self.store.finalize_source_candidates(
                source_id,
                if failed_conversations == 0 {
                    usable_verdict(coverage, &report)
                } else {
                    CompletenessVerdict::Incomplete
                },
                &after,
            )?;
        }
        let import_id = fingerprint.manifest_content_hash.clone();
        Ok(ArchiveImportSummary {
            import_id,
            schema,
            coverage,
            verdict: if failed_conversations == 0 {
                usable_verdict(coverage, &report)
            } else {
                CompletenessVerdict::Incomplete
            },
            conversation_count,
            message_count,
            fast_verified: false,
        })
    }

    fn import_conversation(
        &self,
        manifest: &ManifestHeader,
        conversation: &DeclaredConversationV1,
        source: &NewSource,
        verdict: CompletenessVerdict,
    ) -> Result<(u64, String), ContractError> {
        let meta: ConversationMetaV1 =
            serde_json::from_reader(self.guard.open_conversation_meta(conversation)?)
                .map_err(|_| ContractError::KbSourceUnsupported)?;
        if meta.schema_version != manifest.schema_version || meta.username != conversation.stable_id
        {
            return Err(ContractError::KbSourceUnsupported);
        }
        let staging = self.store.begin_staging_source(NewSource {
            conversation_stable_id: conversation.stable_id.clone(),
            display_metadata_json: Some(
                serde_json::json!({
                    "schemaVersion": "conversation-display-v1",
                    "displayName": meta.display_name,
                    "isGroup": meta.is_group,
                })
                .to_string(),
            ),
            ..source.clone()
        })?;
        let source_id = staging.source_id().to_owned();
        let result = (|| {
            let mut imported_count = 0;
            let mut batch = Vec::with_capacity(256);
            let source_member_token = member_path_token(&conversation.messages_path);
            let count = stream_messages(
                self.guard.open_messages_stream(conversation)?,
                manifest,
                conversation,
                |message, source_ordinal| {
                    #[cfg(test)]
                    observe_streamed_message();
                    batch.push(normalize_message(
                        message,
                        &source_member_token,
                        &manifest.account.stable_id,
                        &conversation.stable_id,
                        source_ordinal,
                    )?);
                    imported_count += 1;
                    if batch.len() == 256 {
                        #[cfg(test)]
                        observe_batch_commit(batch.len());
                        self.store.append_staging_messages(&staging, &batch)?;
                        batch.clear();
                    }
                    Ok(())
                },
            )?;
            if !batch.is_empty() {
                #[cfg(test)]
                observe_batch_commit(batch.len());
                self.store.append_staging_messages(&staging, &batch)?;
            }
            if count != meta.message_count || imported_count != meta.message_count {
                return Err(ContractError::KbSourceUnsupported);
            }
            let _ = verdict;
            Ok(count)
        })();
        if result.is_err() {
            self.store.discard_stagings(&[staging])?;
        }
        result.map(|count| (count, source_id))
    }

    fn source_audit(
        &self,
        manifest: &ManifestHeader,
        source_id: Option<&str>,
    ) -> Result<SourceAuditDigest, ContractError> {
        let accumulator = RefCell::new(SourceAuditAccumulator::new());
        self.add_audit(
            source_id,
            &mut accumulator.borrow_mut(),
            self.guard.member_audit(
                "manifest.json",
                "manifest",
                manifest.format.manifest_content_hash.clone(),
            )?,
        )?;
        self.add_audit(
            source_id,
            &mut accumulator.borrow_mut(),
            self.guard.member_audit("report.json", "report", None)?,
        )?;
        let mut audit_conversation = |conversation: DeclaredConversationV1| {
            self.add_audit(
                source_id,
                &mut accumulator.borrow_mut(),
                self.guard
                    .member_audit(&conversation.meta_path, "meta", None)?,
            )?;
            self.add_audit(
                source_id,
                &mut accumulator.borrow_mut(),
                self.guard
                    .member_audit(&conversation.messages_path, "messages", None)?,
            )
        };
        let mut audit_integrity = |integrity: DeclaredMemberV1| {
            let mut reader = self.guard.open_declared_integrity(&integrity.path)?;
            let mut first_byte = [0_u8; 1];
            let _ = reader
                .read(&mut first_byte)
                .map_err(|_| ContractError::KbSourceUnsupported)?;
            self.add_audit(
                source_id,
                &mut accumulator.borrow_mut(),
                self.guard
                    .member_audit(&integrity.path, "integrity", integrity.declared_hash)?,
            )
        };
        let audited = stream_manifest(
            self.guard.open_manifest()?,
            &mut audit_conversation,
            &mut audit_integrity,
        )?;
        if !same_manifest_header(manifest, &audited) {
            return Err(ContractError::KbSourceUnsupported);
        }
        Ok(accumulator.into_inner().finish())
    }

    fn add_audit(
        &self,
        source_id: Option<&str>,
        accumulator: &mut SourceAuditAccumulator,
        audit: MemberAudit,
    ) -> Result<(), ContractError> {
        accumulator.add(&audit);
        if let Some(source_id) = source_id {
            self.store.record_source_audit(source_id, &audit)?;
        }
        Ok(())
    }
}

fn usable_verdict(coverage: CoverageKind, report: &ReportProbeV1) -> CompletenessVerdict {
    if report.missing_media != 0 || !report.errors.is_empty() {
        return CompletenessVerdict::Incomplete;
    }
    match coverage {
        CoverageKind::Full => CompletenessVerdict::FullDeclared,
        CoverageKind::Filtered | CoverageKind::Selected => CompletenessVerdict::FilteredSelected,
    }
}

fn safe_scope_filters_json(manifest: &ManifestHeader) -> String {
    format!(
        r#"{{"coverage":"{}","hasFilters":{},"declaredConversationCount":{}}}"#,
        manifest.coverage().as_str(),
        !manifest.filters.message_types.is_empty()
            || manifest.filters.start_at.is_some()
            || manifest.filters.end_at.is_some(),
        manifest.conversation_count
    )
}

fn same_manifest_header(left: &ManifestHeader, right: &ManifestHeader) -> bool {
    left.schema_version == right.schema_version
        && left.export_id == right.export_id
        && left.account.stable_id == right.account.stable_id
        && left.manifest_hash().ok() == right.manifest_hash().ok()
        && left.coverage() == right.coverage()
        && left.stats.conversation_count == right.stats.conversation_count
        && left.stats.message_count == right.stats.message_count
        && left.conversation_count == right.conversation_count
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConversationMetaV1 {
    schema_version: String,
    username: String,
    display_name: String,
    avatar_path: Option<String>,
    is_group: bool,
    exported_at: String,
    message_count: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MessageV1 {
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    sender: String,
    created_at: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    media: Option<String>,
    #[serde(default)]
    render_type: Option<String>,
    #[serde(default)]
    reference: Option<MessageReferenceV1>,
    #[serde(default)]
    extra: Option<BTreeMap<String, serde_json::Value>>,
    #[serde(default)]
    media_refs: Vec<MediaRefV1>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MessageReferenceV1 {
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    sender: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct MediaRefV1 {
    kind: String,
    #[serde(default)]
    relative_path: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    extra: Option<BTreeMap<String, serde_json::Value>>,
}

fn stream_messages<R: Read>(
    reader: BufReader<R>,
    manifest: &ManifestHeader,
    conversation: &DeclaredConversationV1,
    mut on_message: impl FnMut(MessageV1, u64) -> Result<(), ContractError>,
) -> Result<u64, ContractError> {
    let mut count = 0;
    let mut consume = |message: MessageV1| -> Result<(), serde_json::Error> {
        on_message(message, count)
            .map_err(|_| serde_json::Error::custom("store rejected message"))?;
        count += 1;
        Ok(())
    };
    let header = MessagesEnvelopeSeed {
        on_message: &mut consume,
    }
    .deserialize(&mut serde_json::Deserializer::from_reader(reader))
    .map_err(|_| ContractError::KbSourceUnsupported)?;
    if header.schema_version != manifest.schema_version
        || header.export_id != manifest.export_id
        || header.account_stable_id != manifest.account.stable_id
        || header.conversation_stable_id != conversation.stable_id
    {
        return Err(ContractError::KbSourceUnsupported);
    }
    Ok(count)
}

fn normalize_message(
    message: MessageV1,
    source_member_token: &str,
    account_stable_id: &str,
    conversation_stable_id: &str,
    source_ordinal: u64,
) -> Result<IncomingMessage, ContractError> {
    if !matches!(
        message.kind.as_str(),
        "text"
            | "image"
            | "video"
            | "voice"
            | "file"
            | "location"
            | "link"
            | "emoji"
            | "quote"
            | "reply"
            | "system"
            | "recall"
            | "unknown"
    ) {
        return Err(ContractError::KbSourceUnsupported);
    }
    let sender_key = normalize_text(&message.sender, 512)?;
    if sender_key.is_empty() {
        return Err(ContractError::KbSourceUnsupported);
    }
    let created_at_ms = chrono::DateTime::parse_from_rfc3339(&message.created_at)
        .map_err(|_| ContractError::KbSourceUnsupported)?
        .timestamp_millis();
    let render_kind = message.render_type.unwrap_or_else(|| message.kind.clone());
    if !matches!(
        render_kind.as_str(),
        "text"
            | "image"
            | "video"
            | "voice"
            | "file"
            | "location"
            | "link"
            | "emoji"
            | "quote"
            | "reply"
            | "system"
            | "recall"
            | "unknown"
    ) {
        return Err(ContractError::KbSourceUnsupported);
    }
    let normalized_content = match message.kind.as_str() {
        "text" => {
            let text = normalize_text(
                message
                    .text
                    .as_deref()
                    .ok_or(ContractError::KbSourceUnsupported)?,
                MAX_JSON_STRING_BYTES,
            )?;
            if text.is_empty() {
                return Err(ContractError::KbSourceUnsupported);
            }
            text
        }
        "image" | "video" | "voice" | "file" | "unknown" => format!("[{}]", message.kind),
        _ => message
            .text
            .as_deref()
            .map(|text| normalize_text(text, MAX_JSON_STRING_BYTES))
            .transpose()?
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| format!("[{}]", message.kind)),
    };
    let stable_id = message
        .id
        .filter(|id| !id.trim().is_empty())
        .map(|id| normalize_text(&id, 512))
        .transpose()?;
    let fallback_key = match stable_id {
        Some(_) => None,
        None => Some(format!(
            "{:x}",
            Sha256::digest(
                format!(
                    "{}|{}|{}|{}|{}|{}|{}",
                    account_stable_id,
                    conversation_stable_id,
                    sender_key,
                    created_at_ms,
                    message.kind,
                    hex_hash(&normalized_content),
                    source_ordinal
                )
                .as_bytes()
            )
        )),
    };
    let reference_json = canonical_reference(message.reference)?;
    let extra_json = canonical_extra(message.extra)?;
    if message.media_refs.len() > 64 {
        return Err(ContractError::KbSourceUnsupported);
    }
    let media_refs = message
        .media_refs
        .into_iter()
        .enumerate()
        .map(|(ordinal, media)| {
            let relative_path = media
                .relative_path
                .map(|path| {
                    if path.len() > 4096 {
                        return Err(ContractError::KbSourceUnsupported);
                    }
                    normalize_member_path(&path)
                })
                .transpose()?;
            let label = media
                .label
                .map(|value| normalize_text(&value, 512))
                .transpose()?;
            let extra = canonical_extra(media.extra)?;
            let metadata_json = match (label, extra) {
                (None, None) => None,
                (label, extra) => {
                    Some(serde_json::json!({"label": label, "extra": extra}).to_string())
                }
            };
            Ok(IncomingMediaRef {
                ordinal: ordinal as u64,
                kind: normalize_text(&media.kind, 64)?,
                relative_path,
                metadata_json,
            })
        })
        .collect::<Result<Vec<_>, ContractError>>()?;
    let text_hash = hex_hash(&normalized_content);
    let identity = stable_id
        .as_deref()
        .or(fallback_key.as_deref())
        .ok_or(ContractError::KbSourceUnsupported)?;
    let sort_key = format!(
        "{created_at_ms:020}|{source_ordinal:020}|{}",
        hex_hash(identity)
    );
    let canonical = serde_json::json!({"identity": identity, "createdAtMs": created_at_ms, "sourceOrdinal": source_ordinal, "kind": message.kind, "render": render_kind, "sender": sender_key, "text": normalized_content, "reference": reference_json, "extra": extra_json, "source": source_member_token, "media": media_refs.iter().map(|item| serde_json::json!({"ordinal": item.ordinal, "kind": item.kind, "path": item.relative_path, "metadata": item.metadata_json})).collect::<Vec<_>>()});
    let content_hash = hex_hash(&canonical.to_string());
    let _ = message.media;
    Ok(IncomingMessage {
        stable_id,
        fallback_key,
        content: normalized_content.clone(),
        normalized_content,
        content_hash,
        source_member_token: source_member_token.to_owned(),
        created_at_ms,
        source_ordinal,
        sort_key,
        message_kind: message.kind,
        render_kind,
        sender_key,
        text_hash,
        reference_json,
        extra_json,
        media_refs,
    })
}

fn hex_hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn normalize_text(value: &str, max_bytes: usize) -> Result<String, ContractError> {
    if value.len() > max_bytes {
        return Err(ContractError::KbSourceUnsupported);
    }
    Ok(value
        .replace("\r\n", "\n")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" "))
}

fn canonical_reference(
    reference: Option<MessageReferenceV1>,
) -> Result<Option<String>, ContractError> {
    let Some(reference) = reference else {
        return Ok(None);
    };
    let message_id = reference
        .message_id
        .map(|value| normalize_text(&value, 512))
        .transpose()?;
    let sender = reference
        .sender
        .map(|value| normalize_text(&value, 512))
        .transpose()?;
    let created_at = reference
        .created_at
        .map(|value| {
            chrono::DateTime::parse_from_rfc3339(&value)
                .map(|date| date.timestamp_millis())
                .map_err(|_| ContractError::KbSourceUnsupported)
        })
        .transpose()?;
    if message_id.is_none() && sender.is_none() && created_at.is_none() {
        return Err(ContractError::KbSourceUnsupported);
    }
    Ok(Some(
        serde_json::json!({"createdAtMs": created_at, "messageId": message_id, "sender": sender})
            .to_string(),
    ))
}

fn canonical_extra(
    extra: Option<BTreeMap<String, serde_json::Value>>,
) -> Result<Option<String>, ContractError> {
    let Some(extra) = extra else {
        return Ok(None);
    };
    let allowed = [
        "title",
        "description",
        "url",
        "fileName",
        "durationMs",
        "width",
        "height",
    ];
    if extra.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(ContractError::KbSourceUnsupported);
    }
    for value in extra.values() {
        match value {
            serde_json::Value::String(text) if text.len() <= MAX_JSON_STRING_BYTES => {}
            serde_json::Value::Number(_) | serde_json::Value::Bool(_) => {}
            _ => return Err(ContractError::KbSourceUnsupported),
        }
    }
    let value = serde_json::to_string(&extra).map_err(|_| ContractError::KbSourceUnsupported)?;
    (value.len() <= 8 * 1024)
        .then_some(value)
        .ok_or(ContractError::KbSourceUnsupported)
        .map(Some)
}

struct JsonStringLimitReader<R> {
    inner: R,
    in_string: bool,
    escaped: bool,
    string_bytes: usize,
}

impl<R> JsonStringLimitReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            in_string: false,
            escaped: false,
            string_bytes: 0,
        }
    }
}

impl<R: Read> Read for JsonStringLimitReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buffer)?;
        for byte in &buffer[..read] {
            if self.in_string {
                self.string_bytes += 1;
                if self.string_bytes > MAX_JSON_STRING_BYTES {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "archive JSON string exceeds the v1 limit",
                    ));
                }
                if self.escaped {
                    self.escaped = false;
                } else if *byte == b'\\' {
                    self.escaped = true;
                } else if *byte == b'"' {
                    self.in_string = false;
                }
            } else if *byte == b'"' {
                self.in_string = true;
                self.string_bytes = 0;
            }
        }
        Ok(read)
    }
}

struct EnvelopeHeader {
    schema_version: String,
    export_id: String,
    account_stable_id: String,
    conversation_stable_id: String,
}

struct MessagesEnvelopeSeed<'a, F> {
    on_message: &'a mut F,
}

impl<'de, F> DeserializeSeed<'de> for MessagesEnvelopeSeed<'_, F>
where
    F: FnMut(MessageV1) -> Result<(), serde_json::Error>,
{
    type Value = EnvelopeHeader;
    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(MessagesEnvelopeVisitor {
            on_message: self.on_message,
        })
    }
}

struct MessagesEnvelopeVisitor<'a, F> {
    on_message: &'a mut F,
}

impl<'de, F> Visitor<'de> for MessagesEnvelopeVisitor<'_, F>
where
    F: FnMut(MessageV1) -> Result<(), serde_json::Error>,
{
    type Value = EnvelopeHeader;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a strict wechat archive v1 messages envelope")
    }
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut schema_version = None;
        let mut export_id = None;
        let mut account_stable_id = None;
        let mut conversation_stable_id = None;
        let mut filters_seen = false;
        let mut messages_seen = false;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "schemaVersion" if schema_version.is_none() => {
                    schema_version = Some(map.next_value()?)
                }
                "exportId" if export_id.is_none() => export_id = Some(map.next_value()?),
                "account" if account_stable_id.is_none() => {
                    account_stable_id = Some(
                        map.next_value::<super::archive_schema::ArchiveAccountV1>()?
                            .stable_id,
                    )
                }
                "conversation" if conversation_stable_id.is_none() => {
                    conversation_stable_id = Some(map.next_value::<ConversationRefV1>()?.stable_id)
                }
                "filters" if !filters_seen => {
                    let _: super::archive_schema::FiltersProbeV1 = map.next_value()?;
                    filters_seen = true;
                }
                "messages" if !messages_seen => {
                    if schema_version.is_none()
                        || export_id.is_none()
                        || account_stable_id.is_none()
                        || conversation_stable_id.is_none()
                        || !filters_seen
                    {
                        return Err(A::Error::custom(
                            "messages must follow a complete envelope header",
                        ));
                    }
                    map.next_value_seed(MessageArraySeed {
                        on_message: self.on_message,
                    })?;
                    messages_seen = true;
                }
                _ => {
                    return Err(A::Error::custom(
                        "unknown, duplicate, or misplaced envelope field",
                    ))
                }
            }
        }
        Ok(EnvelopeHeader {
            schema_version: schema_version
                .ok_or_else(|| A::Error::missing_field("schemaVersion"))?,
            export_id: export_id.ok_or_else(|| A::Error::missing_field("exportId"))?,
            account_stable_id: account_stable_id
                .ok_or_else(|| A::Error::missing_field("account"))?,
            conversation_stable_id: conversation_stable_id
                .ok_or_else(|| A::Error::missing_field("conversation"))?,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConversationRefV1 {
    stable_id: String,
}

struct MessageArraySeed<'a, F> {
    on_message: &'a mut F,
}
impl<'de, F> DeserializeSeed<'de> for MessageArraySeed<'_, F>
where
    F: FnMut(MessageV1) -> Result<(), serde_json::Error>,
{
    type Value = ();
    fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(MessageArrayVisitor {
            on_message: self.on_message,
        })
    }
}
struct MessageArrayVisitor<'a, F> {
    on_message: &'a mut F,
}
impl<'de, F> Visitor<'de> for MessageArrayVisitor<'_, F>
where
    F: FnMut(MessageV1) -> Result<(), serde_json::Error>,
{
    type Value = ();
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a stream of strict messages")
    }
    fn visit_seq<A>(self, mut sequence: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        while let Some(message) = sequence.next_element::<MessageV1>()? {
            (self.on_message)(message).map_err(A::Error::custom)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[derive(Default, Debug, PartialEq, Eq)]
struct ImportMemoryProbe {
    peak_batch_len: usize,
    peak_message_dto: usize,
    parsed_messages: u64,
    batch_commits: u64,
}

#[cfg(test)]
thread_local! {
    static IMPORT_MEMORY_PROBE: RefCell<Option<ImportMemoryProbe>> = const { RefCell::new(None) };
}

#[cfg(test)]
fn begin_import_memory_probe() {
    IMPORT_MEMORY_PROBE.with(|probe| *probe.borrow_mut() = Some(ImportMemoryProbe::default()));
}

#[cfg(test)]
fn take_import_memory_probe() -> ImportMemoryProbe {
    IMPORT_MEMORY_PROBE.with(|probe| probe.borrow_mut().take().unwrap())
}

#[cfg(test)]
fn observe_streamed_message() {
    IMPORT_MEMORY_PROBE.with(|probe| {
        if let Some(probe) = probe.borrow_mut().as_mut() {
            probe.peak_message_dto = probe.peak_message_dto.max(1);
            probe.parsed_messages += 1;
        }
    });
}

#[cfg(test)]
fn observe_batch_commit(batch_len: usize) {
    IMPORT_MEMORY_PROBE.with(|probe| {
        if let Some(probe) = probe.borrow_mut().as_mut() {
            probe.peak_batch_len = probe.peak_batch_len.max(batch_len);
            probe.batch_commits += 1;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::store;
    use std::path::PathBuf;

    fn fixture_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wechat_archive_v1")
    }
    fn temp_dir() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("wechat_archive_import_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn synthetic_archive(message_count: usize) -> PathBuf {
        let source = temp_dir();
        let conversation = source.join("conversations/conv_fixture_01");
        fs::create_dir_all(source.join("_integrity")).unwrap();
        fs::create_dir_all(&conversation).unwrap();
        for member in [
            "manifest.json",
            "report.json",
            "_integrity/declarations.json",
            "conversations/conv_fixture_01/meta.json",
        ] {
            fs::copy(fixture_root().join(member), source.join(member)).unwrap();
        }
        let mut manifest: serde_json::Value =
            serde_json::from_reader(File::open(fixture_root().join("manifest.json")).unwrap())
                .unwrap();
        manifest["format"]["manifestContentHash"] =
            serde_json::Value::String(format!("synthetic-{message_count}"));
        manifest["stats"]["messageCount"] = serde_json::json!(message_count);
        serde_json::to_writer(
            File::create(source.join("manifest.json")).unwrap(),
            &manifest,
        )
        .unwrap();

        let mut meta: serde_json::Value = serde_json::from_reader(
            File::open(fixture_root().join("conversations/conv_fixture_01/meta.json")).unwrap(),
        )
        .unwrap();
        meta["messageCount"] = serde_json::json!(message_count);
        serde_json::to_writer(File::create(conversation.join("meta.json")).unwrap(), &meta)
            .unwrap();

        let messages: serde_json::Value = serde_json::from_reader(
            File::open(fixture_root().join("conversations/conv_fixture_01/messages.json")).unwrap(),
        )
        .unwrap();
        let template = messages["messages"][0].clone();
        let messages = (0..message_count)
            .map(|ordinal| {
                let mut message = template.clone();
                message["id"] = serde_json::Value::String(format!("synthetic-{ordinal}"));
                message
            })
            .collect::<Vec<_>>();
        let document: serde_json::Value = serde_json::from_reader(
            File::open(fixture_root().join("conversations/conv_fixture_01/messages.json")).unwrap(),
        )
        .unwrap();
        let messages = serde_json::to_string(&messages).unwrap();
        let envelope = format!(
            r#"{{"schemaVersion":{},"exportId":{},"account":{},"conversation":{},"filters":{},"messages":{messages}}}"#,
            document["schemaVersion"],
            document["exportId"],
            document["account"],
            document["conversation"],
            document["filters"],
        );
        fs::write(conversation.join("messages.json"), envelope).unwrap();
        source
    }

    struct SyntheticMessagesReader {
        count: u64,
        next_ordinal: u64,
        current: io::Cursor<Vec<u8>>,
        finished: bool,
    }

    impl SyntheticMessagesReader {
        fn new(count: u64) -> Self {
            Self {
                count,
                next_ordinal: 0,
                current: io::Cursor::new(
                    br#"{"schemaVersion":"wechat_archive_v1","exportId":"export_fixture_01","account":{"stableId":"acct_fixture_01"},"conversation":{"stableId":"conv_fixture_01"},"filters":{},"messages":["#.to_vec(),
                ),
                finished: false,
            }
        }
    }

    impl Read for SyntheticMessagesReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            loop {
                let read = self.current.read(buffer)?;
                if read != 0 {
                    return Ok(read);
                }
                if self.next_ordinal < self.count {
                    let comma = if self.next_ordinal == 0 { "" } else { "," };
                    self.current = io::Cursor::new(
                        format!(
                            r#"{comma}{{"id":"synthetic-{}","type":"text","sender":"sender","createdAt":"2026-01-01T00:00:00Z","text":"body"}}"#,
                            self.next_ordinal
                        )
                        .into_bytes(),
                    );
                    self.next_ordinal += 1;
                } else if !self.finished {
                    self.current = io::Cursor::new(b"]}".to_vec());
                    self.finished = true;
                } else {
                    return Ok(0);
                }
            }
        }
    }

    #[test]
    fn fixture_import_is_read_only_and_exact_repeat_uses_fast_verify() {
        let data_dir = temp_dir();
        let source = fixture_root();
        let source_entries_before: Vec<_> = fs::read_dir(&source)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let mut importer = WechatJsonArchiveImporter::open(&source, &store).unwrap();
        let first = importer.import().unwrap();
        assert_eq!(first.coverage, CoverageKind::Selected);
        assert_eq!(first.verdict, CompletenessVerdict::FilteredSelected);
        assert!(!first.fast_verified);
        let database = data_dir.join("wechat_knowledge/knowledge.sqlite");
        let before: Vec<i64> = [
            "knowledge_message_versions",
            "knowledge_import_generation_members",
            "knowledge_media_refs",
            "knowledge_chunks",
        ]
        .iter()
        .map(|table| {
            rusqlite::Connection::open(&database)
                .unwrap()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap()
        })
        .collect();
        let second = importer.import().unwrap();
        assert!(second.fast_verified);
        let after: Vec<i64> = [
            "knowledge_message_versions",
            "knowledge_import_generation_members",
            "knowledge_media_refs",
            "knowledge_chunks",
        ]
        .iter()
        .map(|table| {
            rusqlite::Connection::open(&database)
                .unwrap()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap()
        })
        .collect();
        assert_eq!(after, before);
        let source_entries_after: Vec<_> = fs::read_dir(&source)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(source_entries_before, source_entries_after);
        assert!(data_dir.join("wechat_knowledge/knowledge.sqlite").is_file());
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn large_message_stream_stays_at_the_256_batch_boundary() {
        let source = synthetic_archive(513);
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        begin_import_memory_probe();
        let result = WechatJsonArchiveImporter::open(&source, &store)
            .unwrap()
            .import();
        let summary = result.unwrap();
        let probe = take_import_memory_probe();
        assert_eq!(summary.message_count, 513);
        assert_eq!(probe.peak_batch_len, 256);
        assert_eq!(probe.peak_message_dto, 1);
        assert_eq!(probe.parsed_messages, 513);
        assert_eq!(probe.batch_commits, 3);
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn million_message_envelope_is_consumed_without_a_message_vec() {
        let manifest: ManifestProbeV1 =
            serde_json::from_reader(File::open(fixture_root().join("manifest.json")).unwrap())
                .unwrap();
        let header = ManifestHeader {
            schema_version: manifest.schema_version.clone(),
            exported_at: manifest.exported_at.clone(),
            export_id: manifest.export_id.clone(),
            account: manifest.account.clone(),
            source: manifest.source.clone(),
            format: manifest.format.clone(),
            scope_kind: manifest.scope.kind.clone(),
            filters: manifest.filters.clone(),
            options: manifest.options.clone(),
            stats: manifest.stats.clone(),
            conversation_count: manifest.scope.conversations.len() as u64,
        };
        let conversation = manifest.declared_conversations()[0].clone();
        let count = stream_messages(
            BufReader::new(SyntheticMessagesReader::new(1_000_000)),
            &header,
            &conversation,
            |_, _| Ok(()),
        )
        .unwrap();
        assert_eq!(count, 1_000_000);
    }

    #[test]
    fn failed_batch_is_cleaned_and_a_reopen_replays_from_the_conversation_start() {
        let source = synthetic_archive(257);
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        store::fail_on_append_batch(2);
        let mut importer = WechatJsonArchiveImporter::open(&source, &store).unwrap();
        assert_eq!(importer.import(), Err(ContractError::KbSourceUnsupported));
        store::clear_append_batch_failure();
        let database = data_dir.join("wechat_knowledge/knowledge.sqlite");
        for table in [
            "knowledge_import_generations",
            "knowledge_import_generation_members",
            "knowledge_message_versions",
            "knowledge_media_refs",
        ] {
            let count: i64 = rusqlite::Connection::open(&database)
                .unwrap()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table}");
        }
        assert_eq!(importer.import().unwrap().message_count, 257);
        let members: i64 = rusqlite::Connection::open(&database)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM knowledge_import_generation_members",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(members, 257);
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn traversal_and_media_requests_are_rejected_without_opening_them() {
        let guard = WechatArchiveReadGuard::open(fixture_root()).unwrap();
        assert!(guard.open_declared_integrity("../media.bin").is_err());
        assert!(guard.open_declared_integrity("media/avatar.png").is_err());
    }

    #[test]
    fn report_declared_missing_data_is_incomplete_not_full() {
        let report: ReportProbeV1 = serde_json::from_str(
            r#"{"schemaVersion":"wechat_archive_v1","exportId":"export_fixture_01","account":{"stableId":"acct_fixture_01"},"createdAt":"2026-01-01T00:00:00Z","missingMedia":1,"errors":[]}"#,
        )
        .unwrap();
        assert_eq!(
            usable_verdict(CoverageKind::Full, &report),
            CompletenessVerdict::Incomplete
        );
    }

    #[test]
    fn message_normalization_is_deterministic_and_keeps_media_as_metadata() {
        let message = || MessageV1 {
            id: Some("stable-message".into()),
            kind: "image".into(),
            sender: " sender\r\nname ".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
            text: Some("ignored for media".into()),
            media: Some("legacy-token".into()),
            render_type: Some("image".into()),
            reference: None,
            extra: None,
            media_refs: vec![MediaRefV1 {
                kind: "image".into(),
                relative_path: Some("media/does-not-need-to-exist.jpg".into()),
                label: Some(" preview ".into()),
                extra: None,
            }],
        };
        let first =
            normalize_message(message(), "member-token", "account", "conversation", 3).unwrap();
        let second =
            normalize_message(message(), "member-token", "account", "conversation", 3).unwrap();
        assert_eq!(first.content, "[image]");
        assert_eq!(first.sender_key, "sender name");
        assert_eq!(first.content_hash, second.content_hash);
        assert_eq!(first.media_refs.len(), 1);
        assert_eq!(
            first.media_refs[0].relative_path.as_deref(),
            Some("media/does-not-need-to-exist.jpg")
        );
    }

    #[test]
    fn message_ordinals_are_scoped_to_each_conversation() {
        let envelope = |conversation: &str| {
            format!(
                r#"{{"schemaVersion":"wechat_archive_v1","exportId":"export","account":{{"stableId":"account"}},"conversation":{{"stableId":"{conversation}"}},"filters":{{}},"messages":[{{"id":"shared","type":"text","sender":"sender","createdAt":"2026-01-01T00:00:00Z","text":"body"}}]}}"#
            )
        };
        let manifest: ManifestProbeV1 = serde_json::from_str(
            r#"{"schemaVersion":"wechat_archive_v1","exportedAt":"2026-01-01T00:00:00Z","exportId":"export","account":{"stableId":"account"},"source":{"kind":"user_selected"},"format":{"manifestContentHash":"hash"},"scope":{"kind":"selected","conversations":[{"stableId":"one","metaPath":"one-meta","messagesPath":"one-messages"},{"stableId":"two","metaPath":"two-meta","messagesPath":"two-messages"}]},"filters":{},"options":{"includeMedia":false},"stats":{"conversationCount":2,"messageCount":2},"accountsAvailable":[],"integrityFiles":[]}"#,
        )
        .unwrap();
        let header = ManifestHeader {
            schema_version: manifest.schema_version.clone(),
            exported_at: manifest.exported_at.clone(),
            export_id: manifest.export_id.clone(),
            account: manifest.account.clone(),
            source: manifest.source.clone(),
            format: manifest.format.clone(),
            scope_kind: manifest.scope.kind.clone(),
            filters: manifest.filters.clone(),
            options: manifest.options.clone(),
            stats: manifest.stats.clone(),
            conversation_count: manifest.scope.conversations.len() as u64,
        };
        for conversation in manifest.declared_conversations() {
            let mut ordinals = Vec::new();
            let count = stream_messages(
                BufReader::new(envelope(&conversation.stable_id).as_bytes()),
                &header,
                conversation,
                |_, ordinal| {
                    ordinals.push(ordinal);
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(count, 1);
            assert_eq!(ordinals, vec![0]);
        }
    }

    #[test]
    fn source_path_is_not_persisted_in_the_derived_database() {
        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let mut importer = WechatJsonArchiveImporter::open(fixture_root(), &store).unwrap();
        importer.import().unwrap();
        let database = data_dir.join("wechat_knowledge/knowledge.sqlite");
        let content = fs::read(database).unwrap();
        assert!(!String::from_utf8_lossy(&content).contains("/Users/example/archive"));
        let _ = fs::remove_dir_all(data_dir);
    }

    #[test]
    fn oversized_message_string_is_rejected_without_recording_an_import() {
        let source = temp_dir();
        let conversation = source.join("conversations/conv_fixture_01");
        fs::create_dir_all(source.join("_integrity")).unwrap();
        fs::create_dir_all(&conversation).unwrap();
        for member in [
            "manifest.json",
            "report.json",
            "_integrity/declarations.json",
            "conversations/conv_fixture_01/meta.json",
        ] {
            fs::copy(fixture_root().join(member), source.join(member)).unwrap();
        }
        let mut messages: serde_json::Value = serde_json::from_reader(
            File::open(fixture_root().join("conversations/conv_fixture_01/messages.json")).unwrap(),
        )
        .unwrap();
        messages["messages"][0]["text"] =
            serde_json::Value::String("x".repeat(MAX_JSON_STRING_BYTES + 1));
        serde_json::to_writer(
            File::create(conversation.join("messages.json")).unwrap(),
            &messages,
        )
        .unwrap();

        let data_dir = temp_dir();
        let store = KnowledgeStore::open(&data_dir).unwrap();
        let mut importer = WechatJsonArchiveImporter::open(&source, &store).unwrap();
        assert_eq!(importer.import(), Err(ContractError::KbSourceUnsupported));
        let database =
            rusqlite::Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
        let imports: u64 = database
            .query_row("SELECT COUNT(*) FROM knowledge_sources", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(imports, 0);
        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(data_dir);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_member_is_rejected_by_no_follow_open() {
        use std::os::unix::fs::symlink;

        let source = temp_dir();
        fs::create_dir_all(&source).unwrap();
        let manifest = source.join("manifest.json");
        symlink("/etc/passwd", &manifest).unwrap();
        let guard = WechatArchiveReadGuard::open(&source).unwrap();
        assert!(matches!(
            guard.open_manifest(),
            Err(ContractError::KbSourceUnsupported)
        ));
        let _ = fs::remove_dir_all(source);
    }
}
