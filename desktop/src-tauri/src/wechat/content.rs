use super::types::{ContractError, RequestId};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WechatContentKind {
    Capture,
    OcrText,
    RetrievalExcerpt,
    Suggestion,
}

impl WechatContentKind {
    fn file_name(self) -> &'static str {
        match self {
            Self::Capture => "capture.bin",
            Self::OcrText => "ocr_text.txt",
            Self::RetrievalExcerpt => "retrieval_excerpt.txt",
            Self::Suggestion => "suggestion.txt",
        }
    }
}

#[derive(Clone)]
pub(crate) struct WechatContentStore {
    root: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContentDeleteResult {
    deleted_request_directories: u64,
    failed_entries: u64,
}

impl WechatContentStore {
    pub(crate) fn new(data_dir: impl AsRef<Path>) -> Self {
        Self { root: data_dir.as_ref().join("wechat_reply").join("content") }
    }

    /// A disabled or invalid retention setting is intentionally a no-op before
    /// any directory is created. Callers never supply a path or file name.
    pub(super) fn retain(
        &self,
        request_id: &RequestId,
        kind: WechatContentKind,
        body: &[u8],
        retention_enabled: bool,
        retention_days: u16,
    ) -> Result<bool, ContractError> {
        if !retention_enabled || retention_days == 0 || retention_days > 30 {
            return Ok(false);
        }
        fs::create_dir_all(&self.root).map_err(|_| ContractError::WxContentPersistFailed)?;
        self.require_managed_root()?;
        let request_dir = self.root.join(request_id.to_string());
        fs::create_dir_all(&request_dir).map_err(|_| ContractError::WxContentPersistFailed)?;
        let request_metadata = fs::symlink_metadata(&request_dir)
            .map_err(|_| ContractError::WxContentPersistFailed)?;
        if request_metadata.file_type().is_symlink() || !request_metadata.is_dir() {
            return Err(ContractError::WxContentPersistFailed);
        }
        let temporary = request_dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let destination = request_dir.join(kind.file_name());
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| ContractError::WxContentPersistFailed)?;
        let outcome = (|| {
            file.write_all(body).map_err(|_| ContractError::WxContentPersistFailed)?;
            file.sync_all().map_err(|_| ContractError::WxContentPersistFailed)?;
            fs::rename(&temporary, &destination).map_err(|_| ContractError::WxContentPersistFailed)
        })();
        if outcome.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        outcome.map(|_| true)
    }

    /// Removes only UUID-named request directories below the dedicated content
    /// root. A symlink or unexpected entry is left in place and reported.
    pub(crate) fn delete_all(&self) -> Result<ContentDeleteResult, ContractError> {
        if !self.managed_root_exists()? {
            return Ok(ContentDeleteResult { deleted_request_directories: 0, failed_entries: 0 });
        }
        let mut result = ContentDeleteResult { deleted_request_directories: 0, failed_entries: 0 };
        for entry in fs::read_dir(&self.root).map_err(|_| ContractError::WxContentPersistFailed)? {
            let entry = entry.map_err(|_| ContractError::WxContentPersistFailed)?;
            let path = entry.path();
            let name = entry.file_name();
            let valid_request = name.to_str().is_some_and(|value| uuid::Uuid::parse_str(value).is_ok());
            let metadata = fs::symlink_metadata(&path).map_err(|_| ContractError::WxContentPersistFailed)?;
            if valid_request && metadata.is_dir() && !metadata.file_type().is_symlink() {
                match fs::remove_dir_all(path) {
                    Ok(()) => result.deleted_request_directories += 1,
                    Err(_) => result.failed_entries += 1,
                }
            } else {
                result.failed_entries += 1;
            }
        }
        if result.failed_entries == 0 {
            let _ = fs::remove_dir(&self.root);
        }
        Ok(result)
    }

    /// Cleanup only considers managed UUID directories and their filesystem
    /// creation/modification timestamp; ambiguous entries are deliberately kept.
    pub(crate) fn cleanup_expired(&self, retention_enabled: bool, retention_days: u16) -> Result<ContentDeleteResult, ContractError> {
        if !retention_enabled || retention_days == 0 || retention_days > 30 || !self.managed_root_exists()? {
            return Ok(ContentDeleteResult { deleted_request_directories: 0, failed_entries: 0 });
        }
        let cutoff = Utc::now() - Duration::days(i64::from(retention_days));
        let mut result = ContentDeleteResult { deleted_request_directories: 0, failed_entries: 0 };
        for entry in fs::read_dir(&self.root).map_err(|_| ContractError::WxContentPersistFailed)? {
            let entry = entry.map_err(|_| ContractError::WxContentPersistFailed)?;
            let path = entry.path();
            let valid_request = entry.file_name().to_str().is_some_and(|value| uuid::Uuid::parse_str(value).is_ok());
            let metadata = fs::symlink_metadata(&path).map_err(|_| ContractError::WxContentPersistFailed)?;
            let created = metadata.modified().ok().map(DateTime::<Utc>::from);
            if !valid_request || metadata.file_type().is_symlink() || !metadata.is_dir() || created.is_none() {
                result.failed_entries += 1;
            } else if created.is_some_and(|created| created < cutoff) {
                match fs::remove_dir_all(path) {
                    Ok(()) => result.deleted_request_directories += 1,
                    Err(_) => result.failed_entries += 1,
                }
            }
        }
        Ok(result)
    }

    /// A request may only be removed through the runtime-held lease. This
    /// helper refuses a symlink root or request directory before deletion.
    pub(super) fn delete_request(&self, request_id: &RequestId) -> Result<(), ContractError> {
        if !self.managed_root_exists()? {
            return Ok(());
        }
        let request_dir = self.root.join(request_id.to_string());
        let metadata = match fs::symlink_metadata(&request_dir) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(ContractError::WxContentPersistFailed),
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ContractError::WxContentPersistFailed);
        }
        fs::remove_dir_all(request_dir).map_err(|_| ContractError::WxContentPersistFailed)
    }

    fn managed_root_exists(&self) -> Result<bool, ContractError> {
        match fs::symlink_metadata(&self.root) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(ContractError::WxContentPersistFailed);
                }
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(ContractError::WxContentPersistFailed),
        }
    }

    fn require_managed_root(&self) -> Result<(), ContractError> {
        self.managed_root_exists()?
            .then_some(())
            .ok_or(ContractError::WxContentPersistFailed)
    }

    #[cfg(test)]
    pub(super) fn request_exists(&self, request_id: &RequestId) -> bool {
        self.root.join(request_id.to_string()).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_retention_does_not_create_a_directory() {
        let directory = std::env::temp_dir().join(format!("wechat-content-{}", uuid::Uuid::new_v4()));
        let store = WechatContentStore::new(&directory);
        assert!(!store.retain(&RequestId::new(), WechatContentKind::OcrText, b"private", false, 0).unwrap());
        assert!(!directory.exists());
    }

    #[test]
    fn delete_only_removes_uuid_request_directories() {
        let directory = std::env::temp_dir().join(format!("wechat-content-{}", uuid::Uuid::new_v4()));
        let store = WechatContentStore::new(&directory);
        store.retain(&RequestId::new(), WechatContentKind::Suggestion, b"private", true, 1).unwrap();
        fs::write(store.root.join("do-not-delete"), b"marker").unwrap();
        let result = store.delete_all().unwrap();
        assert_eq!(result.deleted_request_directories, 1);
        assert_eq!(result.failed_entries, 1);
        assert!(store.root.join("do-not-delete").exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn cleanup_removes_only_expired_uuid_directories() {
        use std::time::{Duration as StdDuration, SystemTime};

        let directory = std::env::temp_dir().join(format!("wechat-content-{}", uuid::Uuid::new_v4()));
        let store = WechatContentStore::new(&directory);
        let expired = RequestId::new();
        let current = RequestId::new();
        store.retain(&expired, WechatContentKind::OcrText, b"old", true, 1).unwrap();
        store.retain(&current, WechatContentKind::OcrText, b"current", true, 1).unwrap();
        let expired_dir = store.root.join(expired.to_string());
        fs::File::open(&expired_dir)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(SystemTime::now() - StdDuration::from_secs(48 * 60 * 60)))
            .unwrap();
        fs::write(store.root.join("not-a-request"), b"marker").unwrap();

        let result = store.cleanup_expired(true, 1).unwrap();
        assert_eq!(result.deleted_request_directories, 1);
        assert_eq!(result.failed_entries, 1);
        assert!(!expired_dir.exists());
        assert!(store.root.join(current.to_string()).exists());
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_refuses_a_symlinked_content_root() {
        use std::os::unix::fs::symlink;

        let directory = std::env::temp_dir().join(format!("wechat-content-{}", uuid::Uuid::new_v4()));
        let outside = std::env::temp_dir().join(format!("wechat-content-outside-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&outside).unwrap();
        let expired_request = outside.join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&expired_request).unwrap();
        let store = WechatContentStore::new(&directory);
        fs::create_dir_all(store.root.parent().unwrap()).unwrap();
        symlink(&outside, &store.root).unwrap();

        assert!(matches!(
            store.cleanup_expired(true, 1),
            Err(ContractError::WxContentPersistFailed)
        ));
        assert!(expired_request.exists());
        let _ = fs::remove_dir_all(directory);
        let _ = fs::remove_dir_all(outside);
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_keeps_a_symlinked_request_directory() {
        use std::os::unix::fs::symlink;

        let directory = std::env::temp_dir().join(format!("wechat-content-{}", uuid::Uuid::new_v4()));
        let outside = std::env::temp_dir().join(format!("wechat-content-outside-{}", uuid::Uuid::new_v4()));
        let store = WechatContentStore::new(&directory);
        fs::create_dir_all(&store.root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let request = RequestId::new();
        symlink(&outside, store.root.join(request.to_string())).unwrap();

        let result = store.cleanup_expired(true, 1).unwrap();
        assert_eq!(result.deleted_request_directories, 0);
        assert_eq!(result.failed_entries, 1);
        assert!(outside.exists());
        let _ = fs::remove_dir_all(directory);
        let _ = fs::remove_dir_all(outside);
    }
}
