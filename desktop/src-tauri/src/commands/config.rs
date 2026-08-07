//! Auto-extracted from the historical `commands.rs`. Behavior unchanged.

use crate::config::AppConfig;
use crate::database::Database;
use crate::error::AppError;
use crate::privacy::PrivacyFilter;
use crate::screenshot::ScreenshotService;
use crate::storage::StorageManager;
use crate::AppState;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};
use uuid::Uuid;

use super::shared::persist_app_config;

const MANAGED_DATA_ENTRIES: &[&str] = &[
    "config.json",
    "workreview.db",
    "screenshots",
    "ocr_logs",
    "background.jpg",
    "update_settings.json",
];

const LIVE_DATABASE_FILES: &[&str] = &["workreview.db", "workreview.db-shm", "workreview.db-wal"];

#[derive(Clone, Copy, Debug)]
pub(crate) enum RecoveryReason {
    PreUpgrade,
    PreRollback,
}

impl RecoveryReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::PreUpgrade => "pre_upgrade",
            Self::PreRollback => "pre_rollback",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryBundleReceipt {
    bundle_id: String,
    created_at: String,
    installed_version: String,
    release_batch_id: String,
    reason: &'static str,
    verified: bool,
    files: Vec<RecoveryFileReceipt>,
    core_database: CoreDatabaseReceipt,
    knowledge_inventory_policy: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecoveryFileReceipt {
    relative_name: String,
    sha256: String,
    bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CoreDatabaseReceipt {
    integrity: &'static str,
    table_count: u64,
    activity_count: u64,
}

fn hash_file(path: &Path) -> Result<(String, u64), AppError> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        bytes += read as u64;
    }
    Ok((format!("{:x}", digest.finalize()), bytes))
}

fn verify_core_backup(path: &Path) -> Result<CoreDatabaseReceipt, AppError> {
    // FTS5's integrity_check may need a temporary write even when the database
    // is valid, so validate the private backup copy read-write, never the live DB.
    let connection = rusqlite::Connection::open(path)?;
    let integrity: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if !integrity.trim().eq_ignore_ascii_case("ok") {
        return Err(AppError::Config("RECOVERY_BACKUP_FAILED".into()));
    }
    let table_count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    let activity_count: i64 =
        connection.query_row("SELECT COUNT(*) FROM activities", [], |row| row.get(0))?;
    Ok(CoreDatabaseReceipt {
        integrity: "ok",
        table_count: u64::try_from(table_count)
            .map_err(|_| AppError::Config("RECOVERY_BACKUP_FAILED".into()))?,
        activity_count: u64::try_from(activity_count)
            .map_err(|_| AppError::Config("RECOVERY_BACKUP_FAILED".into()))?,
    })
}

fn canonical_or_absolute(path: &Path) -> Result<PathBuf, AppError> {
    if path.exists() {
        Ok(path.canonicalize()?)
    } else {
        let absolute = to_absolute_path(path)?;
        let mut cursor = absolute.as_path();
        let mut missing = Vec::new();
        while !cursor.exists() {
            missing.push(
                cursor
                    .file_name()
                    .ok_or_else(|| AppError::Config("RECOVERY_BACKUP_FAILED".into()))?
                    .to_owned(),
            );
            cursor = cursor
                .parent()
                .ok_or_else(|| AppError::Config("RECOVERY_BACKUP_FAILED".into()))?;
        }
        let mut normalized = cursor.canonicalize()?;
        for component in missing.into_iter().rev() {
            normalized.push(component);
        }
        Ok(normalized)
    }
}

/// Creates a verified, private recovery bundle in a directory already chosen
/// by the native picker. This narrow seam intentionally is not a Tauri command:
/// frontend strings cannot select backup paths without a picker receipt.
#[allow(dead_code)]
pub(crate) fn create_recovery_bundle(
    source_data_dir: &Path,
    selected_directory: &Path,
    database: &Database,
    installed_version: &str,
    release_batch_id: &str,
    reason: RecoveryReason,
) -> Result<RecoveryBundleReceipt, AppError> {
    if installed_version.trim().is_empty() || release_batch_id.trim().is_empty() {
        return Err(AppError::Config("RECOVERY_BACKUP_FAILED".into()));
    }
    let source = source_data_dir.canonicalize()?;
    let selected = canonical_or_absolute(selected_directory)?;
    if source == selected
        || source.starts_with(&selected)
        || selected.starts_with(&source)
        || fs::symlink_metadata(selected_directory)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
    {
        return Err(AppError::Config("RECOVERY_BACKUP_FAILED".into()));
    }
    fs::create_dir_all(&selected)?;
    let selected = selected.canonicalize()?;
    let bundle_id = format!("recovery-{}", Uuid::new_v4().simple());
    let temporary = selected.join(format!(".{bundle_id}.pending"));
    let committed = selected.join(&bundle_id);
    fs::create_dir(&temporary)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o700))?;
    }
    let result = (|| {
        let mut files = Vec::new();
        for name in ["config.json", "config.json.bak"] {
            let source_file = source.join(name);
            if !source_file.exists() {
                continue;
            }
            if source_file.is_symlink() || !source_file.is_file() {
                return Err(AppError::Config("RECOVERY_BACKUP_FAILED".into()));
            }
            let target = temporary.join(name);
            fs::copy(&source_file, &target)?;
            let (sha256, bytes) = hash_file(&target)?;
            files.push(RecoveryFileReceipt {
                relative_name: name.into(),
                sha256,
                bytes,
            });
        }
        let backup = temporary.join("workreview.db");
        database.backup_to(&backup)?;
        let core_database = verify_core_backup(&backup)?;
        let (sha256, bytes) = hash_file(&backup)?;
        files.push(RecoveryFileReceipt {
            relative_name: "workreview.db".into(),
            sha256,
            bytes,
        });
        files.sort_by(|left, right| left.relative_name.cmp(&right.relative_name));
        let receipt = RecoveryBundleReceipt {
            bundle_id: bundle_id.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            installed_version: installed_version.into(),
            release_batch_id: release_batch_id.into(),
            reason: reason.as_str(),
            verified: true,
            files,
            core_database,
            knowledge_inventory_policy: "derived-layer-inventory-only; source exports excluded",
        };
        let manifest = temporary.join("recovery-manifest.v1.json");
        fs::write(&manifest, serde_json::to_vec_pretty(&receipt)?)?;
        File::open(&manifest)?.sync_all()?;
        fs::rename(&temporary, &committed)?;
        File::open(&selected)?.sync_all()?;
        Ok(receipt)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

/// 获取配置
#[tauri::command]
pub async fn get_config(state: State<'_, Arc<Mutex<AppState>>>) -> Result<AppConfig, AppError> {
    let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    Ok(state.config.clone())
}

/// 保存配置
#[tauri::command]
pub async fn save_config(
    config: AppConfig,
    app: AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), AppError> {
    // 模型端点安全校验：远程端点必须使用 https，明文 http 仅允许本机/内网（Ollama 等）
    super::ai::validate_model_endpoint(&config.text_model.endpoint)?;
    super::ai::validate_model_endpoint(&config.vision_model.endpoint)?;
    super::ai::validate_model_endpoint(&config.ai_provider.endpoint)?;
    for profile in &config.text_model_profiles {
        super::ai::validate_model_endpoint(&profile.model_config.endpoint)?;
    }
    persist_app_config(config, app, state.inner())
}

/// 获取数据目录
#[tauri::command]
pub async fn get_data_dir(state: State<'_, Arc<Mutex<AppState>>>) -> Result<String, AppError> {
    let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    Ok(path_for_display(&state.data_dir))
}

/// 获取默认数据目录
#[tauri::command]
pub async fn get_default_data_dir() -> Result<String, AppError> {
    Ok(path_for_display(&crate::default_data_dir()))
}

fn path_for_display(path: &Path) -> String {
    let raw = path.to_string_lossy().to_string();

    #[cfg(target_os = "windows")]
    {
        raw.strip_prefix(r"\\?\")
            .or_else(|| raw.strip_prefix(r"\??\"))
            .unwrap_or(&raw)
            .to_string()
    }

    #[cfg(not(target_os = "windows"))]
    {
        raw
    }
}

fn is_ignorable_dir_entry(name: &str) -> bool {
    name.starts_with('.') || name == "Thumbs.db"
}

fn is_managed_dir_entry(name: &str) -> bool {
    MANAGED_DATA_ENTRIES.contains(&name)
}

fn is_cleanup_managed_dir_entry(name: &str) -> bool {
    MANAGED_DATA_ENTRIES.contains(&name) || LIVE_DATABASE_FILES.contains(&name)
}

fn to_absolute_path(path: &Path) -> Result<PathBuf, AppError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn ensure_target_dir_ready(target_dir: &Path) -> Result<bool, AppError> {
    std::fs::create_dir_all(target_dir)?;

    let mut has_existing_app_data = false;

    for entry in std::fs::read_dir(target_dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if is_ignorable_dir_entry(&name) {
            continue;
        }

        if !is_managed_dir_entry(&name) {
            return Err(AppError::Config(format!(
                "目标目录包含非 Work Review 数据（{name}），为避免误覆盖，请选择空目录或旧的数据目录"
            )));
        }

        has_existing_app_data = true;
    }

    if !has_existing_app_data {
        return Ok(false);
    }

    // 目标目录若已存在旧版应用数据，先清空受管条目，再完整覆盖为当前数据。
    for entry_name in MANAGED_DATA_ENTRIES {
        let path = target_dir.join(entry_name);
        if !path.exists() {
            continue;
        }

        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }

    Ok(true)
}

fn copy_managed_data_without_live_db(
    source_dir: &Path,
    target_dir: &Path,
) -> Result<u64, AppError> {
    let mut copied_files = 0u64;

    for entry_name in MANAGED_DATA_ENTRIES {
        if LIVE_DATABASE_FILES.contains(entry_name) {
            continue;
        }

        let source_path = source_dir.join(entry_name);
        if !source_path.exists() {
            continue;
        }

        let target_path = target_dir.join(entry_name);
        if source_path.is_dir() {
            copied_files += crate::copy_dir_contents(&source_path, &target_path, true)?;
        } else {
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(&source_path, &target_path)?;
            copied_files += 1;
        }
    }

    Ok(copied_files)
}

fn remove_app_managed_entries(target_dir: &Path) -> Result<(u64, Vec<String>), AppError> {
    let mut removed_entries = 0u64;
    let mut preserved_entries = Vec::new();

    if !target_dir.exists() {
        return Ok((0, preserved_entries));
    }

    for entry in std::fs::read_dir(target_dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if is_ignorable_dir_entry(&name) {
            continue;
        }

        if is_cleanup_managed_dir_entry(&name) {
            if path.is_dir() {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
            removed_entries += 1;
            continue;
        }

        preserved_entries.push(name);
    }

    if preserved_entries.is_empty() {
        let mut remaining_entries = std::fs::read_dir(target_dir)?;
        if remaining_entries.next().is_none() {
            let _ = std::fs::remove_dir(target_dir);
        }
    }

    Ok((removed_entries, preserved_entries))
}

/// 切换数据目录，并迁移当前数据
#[tauri::command]
pub async fn change_data_dir(
    target_dir: String,
    app: AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<serde_json::Value, AppError> {
    let requested_dir = target_dir.trim();
    if requested_dir.is_empty() {
        return Err(AppError::Config("目标目录不能为空".to_string()));
    }

    let requested_path = to_absolute_path(Path::new(requested_dir))?;
    let current_dir = {
        let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        state
            .data_dir
            .canonicalize()
            .unwrap_or_else(|_| state.data_dir.clone())
    };

    if requested_path == current_dir {
        return Ok(serde_json::json!({
            "dataDir": current_dir.to_string_lossy().to_string(),
            "copiedFiles": 0,
            "message": "数据目录未变化",
        }));
    }

    if requested_path.starts_with(&current_dir) || current_dir.starts_with(&requested_path) {
        return Err(AppError::Config(
            "新旧数据目录不能互为父子目录，请选择独立目录".to_string(),
        ));
    }

    let target_dir = {
        std::fs::create_dir_all(&requested_path)?;
        requested_path
            .canonicalize()
            .unwrap_or_else(|_| requested_path.clone())
    };

    // 先清空目标目录中已有的受管条目（必须在 backup_to 之前，否则会删掉刚备份的数据库）
    let replaced_existing_data = ensure_target_dir_ready(&target_dir)?;

    // 复制截图等文件（在锁外执行，不阻塞截图循环）
    let copied_files = copy_managed_data_without_live_db(&current_dir, &target_dir)?;

    // 短暂获取锁，做安全 SQLite 备份，然后立即释放
    let config = {
        let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        // SQLite 备份必须在持锁状态下执行（backup_to 内部做 WAL checkpoint + VACUUM INTO）
        state
            .database
            .backup_to(&target_dir.join("workreview.db"))?;
        state.config.clone()
    };

    let config_path = target_dir.join("config.json");
    config.save(&config_path)?;
    crate::save_data_dir_preference(&target_dir)?;

    // 重新获取锁，仅做轻量状态更新
    let mut state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
    state.database = Database::new(&target_dir.join("workreview.db"))?;
    if let Err(e) = state.database.rebuild_fts_index() {
        log::warn!("迁移后 FTS 索引重建失败: {e}");
    }
    state.privacy_filter = PrivacyFilter::from_config(&config.privacy);
    state.screenshot_service = ScreenshotService::new(&target_dir, &config.storage);
    state.storage_manager = StorageManager::new(&target_dir, config.storage.clone());
    state.data_dir = target_dir.clone();
    state.config_path = config_path;

    log::info!("数据目录已切换到: {target_dir:?}");
    drop(state);
    crate::emit_recording_state_changed(&app);

    Ok(serde_json::json!({
        "dataDir": target_dir.to_string_lossy().to_string(),
        "oldDataDir": current_dir.to_string_lossy().to_string(),
        "copiedFiles": copied_files,
        "replacedExistingData": replaced_existing_data,
        "message": format!(
            "数据目录已更新，已迁移 {} 个文件{}",
            copied_files,
            if replaced_existing_data { "，并覆盖旧目录中的 Work Review 数据" } else { "" }
        ),
    }))
}

#[tauri::command]
pub async fn cleanup_old_data_dir(
    target_dir: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<serde_json::Value, AppError> {
    let requested_dir = target_dir.trim();
    if requested_dir.is_empty() {
        return Err(AppError::Config("旧目录不能为空".to_string()));
    }

    let requested_path = to_absolute_path(Path::new(requested_dir))?;
    if !requested_path.exists() {
        return Ok(serde_json::json!({
            "removedEntries": 0,
            "preservedEntries": [],
            "message": "旧目录不存在，无需清理",
        }));
    }

    let current_dir = {
        let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        state
            .data_dir
            .canonicalize()
            .unwrap_or_else(|_| state.data_dir.clone())
    };

    let cleanup_dir = requested_path
        .canonicalize()
        .unwrap_or_else(|_| requested_path.clone());

    if cleanup_dir == current_dir {
        return Err(AppError::Config(
            "不能清理当前正在使用的数据目录".to_string(),
        ));
    }

    if cleanup_dir.starts_with(&current_dir) || current_dir.starts_with(&cleanup_dir) {
        return Err(AppError::Config(
            "为避免误删，当前数据目录与待清理目录不能互为父子目录".to_string(),
        ));
    }

    let (removed_entries, preserved_entries) = remove_app_managed_entries(&cleanup_dir)?;
    let message = if preserved_entries.is_empty() {
        if cleanup_dir.exists() {
            format!("已清理旧目录中的 {removed_entries} 项 Work Review 数据")
        } else {
            format!("已清理旧目录中的 {removed_entries} 项 Work Review 数据，并移除空目录")
        }
    } else {
        format!(
            "已清理旧目录中的 {} 项 Work Review 数据，保留其他文件：{}",
            removed_entries,
            preserved_entries.join("、")
        )
    };

    Ok(serde_json::json!({
        "removedEntries": removed_entries,
        "preservedEntries": preserved_entries,
        "message": message,
    }))
}

/// 在系统文件管理器中打开数据目录
/// plugin-shell 的 open 对本地路径在部分平台不可靠，改用系统命令直接打开
#[tauri::command]
pub async fn open_data_dir(state: State<'_, Arc<Mutex<AppState>>>) -> Result<(), AppError> {
    let data_dir = {
        let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        state.data_dir.clone()
    };

    // 目录不存在时先创建，避免打开失败
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| AppError::Unknown(format!("创建数据目录失败: {e}")))?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&data_dir)
            .spawn()
            .map_err(|e| AppError::Unknown(format!("打开数据目录失败: {e}")))?;
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&data_dir)
            .spawn()
            .map_err(|e| AppError::Unknown(format!("打开数据目录失败: {e}")))?;
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&data_dir)
            .spawn()
            .map_err(|e| AppError::Unknown(format!("打开数据目录失败: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("task27-{label}-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn recovery_bundle_is_verified_committed_and_contains_no_absolute_paths() {
        let source = temp_dir("recovery-source");
        let destination = temp_dir("recovery-destination");
        fs::write(source.join("config.json"), br#"{"secret":"local-only"}"#).unwrap();
        fs::write(source.join("config.json.bak"), br#"{"safe":true}"#).unwrap();
        let database = Database::new(&source.join("workreview.db")).unwrap();
        let source_canonical = source.canonicalize().unwrap();
        let destination_canonical = destination.canonicalize().unwrap();
        assert!(!source_canonical.starts_with(&destination_canonical));
        assert!(!destination_canonical.starts_with(&source_canonical));
        let receipt = create_recovery_bundle(
            &source,
            &destination,
            &database,
            "1.0.0",
            "fixture-batch",
            RecoveryReason::PreRollback,
        )
        .unwrap();

        assert!(receipt.verified);
        assert_eq!(receipt.core_database.integrity, "ok");
        let bundle = destination.join(&receipt.bundle_id);
        let manifest = fs::read_to_string(bundle.join("recovery-manifest.v1.json")).unwrap();
        assert!(!manifest.contains(&source.to_string_lossy().to_string()));
        assert!(!manifest.contains(&destination.to_string_lossy().to_string()));
        assert_eq!(
            fs::read(bundle.join("config.json")).unwrap(),
            br#"{"secret":"local-only"}"#
        );
        assert!(Database::new(&bundle.join("workreview.db")).is_ok());
    }

    #[test]
    fn recovery_bundle_rejects_data_root_relatives_without_changing_source() {
        let source = temp_dir("recovery-reject");
        fs::write(source.join("config.json"), b"original").unwrap();
        let database = Database::new(&source.join("workreview.db")).unwrap();
        let before = fs::read(source.join("config.json")).unwrap();

        assert!(create_recovery_bundle(
            &source,
            &source.join("recovery"),
            &database,
            "1.0.0",
            "fixture-batch",
            RecoveryReason::PreUpgrade,
        )
        .is_err());
        assert_eq!(fs::read(source.join("config.json")).unwrap(), before);
    }
}
