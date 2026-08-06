use super::archive_schema::{
    route_manifest, ArchiveSchemaVersion, CoverageKind, DeclaredConversationV1, ManifestProbeV1,
    ReportProbeV1,
};
use super::archive_store::{
    coverage_signature, member_path_token, CompletenessVerdict, ImportFingerprint, MemberAudit,
    WechatArchiveStore,
};
use crate::wechat::types::ContractError;
use serde::de::{DeserializeSeed, Error as DeError, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use std::collections::HashSet;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufReader, Read};
use std::path::{Component, Path, PathBuf};

const MAX_JSON_STRING_BYTES: usize = 64 * 1024;

/// Read-only capability for the v1 allow-list. It intentionally has no write,
/// rename, deletion, directory-walk, or media API.
pub(crate) struct WechatArchiveReadGuard {
    root: PathBuf,
    declared_members: HashSet<String>,
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
            declared_members: HashSet::new(),
        })
    }

    pub(crate) fn open_manifest(&self) -> Result<File, ContractError> {
        self.open_fixed("manifest.json")
    }

    pub(crate) fn open_report(&self) -> Result<File, ContractError> {
        self.open_fixed("report.json")
    }

    pub(crate) fn bind_manifest(
        &mut self,
        manifest: &ManifestProbeV1,
    ) -> Result<(), ContractError> {
        let mut declared = HashSet::from(["manifest.json".to_owned(), "report.json".to_owned()]);
        for integrity in &manifest.integrity_files {
            if !integrity.path.starts_with("_integrity/") {
                return Err(ContractError::KbSourceUnsupported);
            }
            declared.insert(normalize_member_path(&integrity.path)?);
        }
        for conversation in manifest.declared_conversations() {
            declared.insert(normalize_member_path(&conversation.meta_path)?);
            declared.insert(normalize_member_path(&conversation.messages_path)?);
        }
        self.declared_members = declared;
        Ok(())
    }

    pub(crate) fn open_declared_integrity(&self, member: &str) -> Result<File, ContractError> {
        if !member.starts_with("_integrity/") {
            return Err(ContractError::KbSourceUnsupported);
        }
        self.open_declared(member)
    }

    pub(crate) fn open_conversation_meta(
        &self,
        conversation: &DeclaredConversationV1,
    ) -> Result<File, ContractError> {
        self.open_declared(&conversation.meta_path)
    }

    fn open_messages_stream(
        &self,
        conversation: &DeclaredConversationV1,
    ) -> Result<BufReader<JsonStringLimitReader<File>>, ContractError> {
        Ok(BufReader::new(JsonStringLimitReader::new(
            self.open_declared(&conversation.messages_path)?,
        )))
    }

    fn open_fixed(&self, member: &str) -> Result<File, ContractError> {
        self.open_safe(&normalize_member_path(member)?)
    }

    fn open_declared(&self, member: &str) -> Result<File, ContractError> {
        let member = normalize_member_path(member)?;
        if !self.declared_members.contains(&member) {
            return Err(ContractError::KbSourceUnsupported);
        }
        self.open_safe(&member)
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
        if member != "manifest.json"
            && member != "report.json"
            && !self.declared_members.contains(&member)
        {
            return Err(ContractError::KbSourceUnsupported);
        }
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

pub(crate) struct WechatJsonArchiveImporter {
    guard: WechatArchiveReadGuard,
    derived_store: WechatArchiveStore,
}

impl WechatJsonArchiveImporter {
    pub(crate) fn open(
        source_root: impl AsRef<Path>,
        data_dir: impl AsRef<Path>,
    ) -> Result<Self, ContractError> {
        Ok(Self {
            guard: WechatArchiveReadGuard::open(source_root)?,
            derived_store: WechatArchiveStore::open(data_dir.as_ref())?,
        })
    }

    /// Imports only the manifest-declared JSON members. Source paths and bodies
    /// never leave this method; only anonymous IDs, counts, and verdicts reach
    /// the derived store.
    pub(crate) fn import(&mut self) -> Result<ArchiveImportSummary, ContractError> {
        let manifest: ManifestProbeV1 = serde_json::from_reader(self.guard.open_manifest()?)
            .map_err(|_| ContractError::KbSourceUnsupported)?;
        let schema = route_manifest(&manifest)?;
        let declared_hash = manifest
            .declared_manifest_hash()
            .ok_or(ContractError::KbSourceUnsupported)?
            .to_owned();
        self.guard.bind_manifest(&manifest)?;
        let report: ReportProbeV1 = serde_json::from_reader(self.guard.open_report()?)
            .map_err(|_| ContractError::KbSourceUnsupported)?;
        if !report.matches_manifest(&manifest) {
            return Err(ContractError::KbSourceUnsupported);
        }
        let coverage = manifest.coverage();
        let before = self.member_audits(&manifest)?;
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
        if self.derived_store.fast_verify(&fingerprint, &before)? {
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

        let mut conversation_count = 0;
        let mut message_count = 0;
        let mut stable_ids = HashSet::new();
        let mut message_ids = HashSet::new();
        for conversation in manifest.declared_conversations() {
            if !stable_ids.insert(conversation.stable_id.clone()) {
                return Err(ContractError::KbSourceUnsupported);
            }
            let meta: ConversationMetaV1 =
                serde_json::from_reader(self.guard.open_conversation_meta(conversation)?)
                    .map_err(|_| ContractError::KbSourceUnsupported)?;
            if meta.schema_version != manifest.schema_version
                || meta.username != conversation.stable_id
            {
                return Err(ContractError::KbSourceUnsupported);
            }
            let count = stream_messages(
                self.guard.open_messages_stream(conversation)?,
                &manifest,
                conversation,
                &mut message_ids,
            )?;
            if count != meta.message_count {
                return Err(ContractError::KbSourceUnsupported);
            }
            conversation_count += 1;
            message_count += count;
        }
        if conversation_count != manifest.stats.conversation_count
            || message_count != manifest.stats.message_count
        {
            return Err(ContractError::KbSourceUnsupported);
        }
        let after = self.member_audits(&manifest)?;
        if before != after {
            return Err(ContractError::KbSourceUnsupported);
        }
        let verdict = usable_verdict(coverage, &report);
        let import_id = format!("import_{}", uuid::Uuid::new_v4().simple());
        self.derived_store.record_import(
            &import_id,
            &fingerprint,
            coverage,
            verdict,
            &safe_scope_filters_json(&manifest),
            &safe_integrity_json(&manifest),
            conversation_count,
            message_count,
            &after,
        )?;
        Ok(ArchiveImportSummary {
            import_id,
            schema,
            coverage,
            verdict,
            conversation_count,
            message_count,
            fast_verified: false,
        })
    }

    fn member_audits(&self, manifest: &ManifestProbeV1) -> Result<Vec<MemberAudit>, ContractError> {
        let mut audits = vec![
            self.guard.member_audit(
                "manifest.json",
                "manifest",
                manifest.declared_manifest_hash().map(str::to_owned),
            )?,
            self.guard.member_audit("report.json", "report", None)?,
        ];
        for integrity in &manifest.integrity_files {
            // Opening is deliberate: only already-declared integrity statements are consumed.
            let mut reader = self.guard.open_declared_integrity(&integrity.path)?;
            let mut first_byte = [0_u8; 1];
            let _ = reader
                .read(&mut first_byte)
                .map_err(|_| ContractError::KbSourceUnsupported)?;
            audits.push(self.guard.member_audit(
                &integrity.path,
                "integrity",
                integrity.declared_hash.clone(),
            )?);
        }
        for conversation in manifest.declared_conversations() {
            audits.push(
                self.guard
                    .member_audit(&conversation.meta_path, "meta", None)?,
            );
            audits.push(
                self.guard
                    .member_audit(&conversation.messages_path, "messages", None)?,
            );
        }
        Ok(audits)
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

fn safe_scope_filters_json(manifest: &ManifestProbeV1) -> String {
    format!(
        r#"{{"coverage":"{}","hasFilters":{},"declaredConversationCount":{}}}"#,
        manifest.coverage().as_str(),
        !manifest.filters.message_types.is_empty()
            || manifest.filters.start_at.is_some()
            || manifest.filters.end_at.is_some(),
        manifest.scope.conversations.len()
    )
}

fn safe_integrity_json(manifest: &ManifestProbeV1) -> String {
    format!(
        r#"{{"declaredMemberCount":{},"hasManifestHash":{}}}"#,
        manifest.integrity_files.len(),
        manifest.declared_manifest_hash().is_some()
    )
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
    id: String,
    #[serde(rename = "type")]
    kind: String,
    sender: String,
    created_at: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    media: Option<String>,
}

fn stream_messages<R: Read>(
    reader: BufReader<R>,
    manifest: &ManifestProbeV1,
    conversation: &DeclaredConversationV1,
    message_ids: &mut HashSet<String>,
) -> Result<u64, ContractError> {
    let mut count = 0;
    let mut on_message = |message: MessageV1| -> Result<(), serde_json::Error> {
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
        ) || !message_ids.insert(message.id)
        {
            return Err(serde_json::Error::custom("unsupported message contract"));
        }
        // Fields are intentionally read only to enforce the v1 shape, never logged or retained.
        let _ = (
            message.sender,
            message.created_at,
            message.text,
            message.media,
        );
        count += 1;
        Ok(())
    };
    let header = MessagesEnvelopeSeed {
        on_message: &mut on_message,
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
mod tests {
    use super::*;
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

    #[test]
    fn fixture_import_is_read_only_and_exact_repeat_uses_fast_verify() {
        let data_dir = temp_dir();
        let source = fixture_root();
        let source_entries_before: Vec<_> = fs::read_dir(&source)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        let mut importer = WechatJsonArchiveImporter::open(&source, &data_dir).unwrap();
        let first = importer.import().unwrap();
        assert_eq!(first.coverage, CoverageKind::Selected);
        assert_eq!(first.verdict, CompletenessVerdict::FilteredSelected);
        assert!(!first.fast_verified);
        let second = importer.import().unwrap();
        assert!(second.fast_verified);
        let source_entries_after: Vec<_> = fs::read_dir(&source)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(source_entries_before, source_entries_after);
        assert!(data_dir.join("wechat_knowledge/knowledge.sqlite").is_file());
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
    fn source_path_is_not_persisted_in_the_derived_database() {
        let data_dir = temp_dir();
        let mut importer = WechatJsonArchiveImporter::open(fixture_root(), &data_dir).unwrap();
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
        let mut importer = WechatJsonArchiveImporter::open(&source, &data_dir).unwrap();
        assert_eq!(importer.import(), Err(ContractError::KbSourceUnsupported));
        let database =
            rusqlite::Connection::open(data_dir.join("wechat_knowledge/knowledge.sqlite")).unwrap();
        let imports: u64 = database
            .query_row("SELECT COUNT(*) FROM archive_imports", [], |row| row.get(0))
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
