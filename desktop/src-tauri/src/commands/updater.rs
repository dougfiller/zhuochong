//! Update settings and the deliberately disabled updater boundary.

use crate::error::AppError;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, State};

const DEFAULT_UPDATE_CHECK_INTERVAL_HOURS: u64 = 24;
const AUTO_UPDATE_DISABLED_MESSAGE: &str = "当前发行未启用自动更新";

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GithubUpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    pub auto_update_ready: bool,
    pub disabled: bool,
    pub release_url: String,
    pub body: Option<String>,
    pub source: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GithubUpdateInstallResult {
    pub updated: bool,
    pub available: bool,
    pub version: Option<String>,
    pub source: Option<String>,
    pub message: String,
    pub attempted_sources: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettings {
    pub auto_check: bool,
    pub last_check_time: u64,
    #[serde(default = "default_update_check_interval")]
    pub check_interval_hours: u64,
}

fn default_update_check_interval() -> u64 {
    DEFAULT_UPDATE_CHECK_INTERVAL_HOURS
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            auto_check: false,
            last_check_time: 0,
            check_interval_hours: DEFAULT_UPDATE_CHECK_INTERVAL_HOURS,
        }
    }
}

fn update_settings_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("update_settings.json")
}

fn load_update_settings_from_dir(data_dir: &Path) -> Result<UpdateSettings, AppError> {
    let settings_path = update_settings_path(data_dir);

    if !settings_path.exists() {
        return Ok(UpdateSettings::default());
    }

    let content = std::fs::read_to_string(&settings_path)
        .map_err(|e| AppError::Unknown(format!("读取更新设置失败: {e}")))?;

    serde_json::from_str(&content).map_err(|e| AppError::Unknown(format!("解析更新设置失败: {e}")))
}

fn save_update_settings_to_dir(data_dir: &Path, settings: &UpdateSettings) -> Result<(), AppError> {
    let settings_path = update_settings_path(data_dir);
    let content = serde_json::to_string_pretty(settings)
        .map_err(|e| AppError::Unknown(format!("序列化更新设置失败: {e}")))?;

    std::fs::write(&settings_path, content)
        .map_err(|e| AppError::Unknown(format!("保存更新设置失败: {e}")))
}

/// 获取更新检查设置。
#[tauri::command]
pub async fn get_update_settings(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<UpdateSettings, AppError> {
    let data_dir = {
        let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        state.data_dir.clone()
    };

    load_update_settings_from_dir(&data_dir)
}

/// 保存更新检查设置，以便未来受控更新链启用后继续兼容现有设置文件。
#[tauri::command]
pub async fn save_update_settings(
    settings: UpdateSettings,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), AppError> {
    let data_dir = {
        let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        state.data_dir.clone()
    };

    save_update_settings_to_dir(&data_dir, &settings)
}

/// 本产品更新端点和公钥尚未冻结，禁止启动时发起任何更新网络请求。
#[tauri::command]
pub async fn should_check_updates(
    _state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<bool, AppError> {
    Ok(false)
}

/// 仅在未来启用受控更新链后使用；保留该命令以兼容现有前端调用。
#[tauri::command]
pub async fn update_last_check_time(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), AppError> {
    let data_dir = {
        let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        state.data_dir.clone()
    };
    let mut settings = load_update_settings_from_dir(&data_dir)?;
    settings.last_check_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    save_update_settings_to_dir(&data_dir, &settings)
}

#[tauri::command]
pub async fn quit_app_for_update(app: AppHandle) -> Result<(), AppError> {
    app.exit(0);
    Ok(())
}

/// 不构造 HTTP 客户端或 updater client，直到本产品的更新信任链完成审查。
#[tauri::command]
pub async fn check_github_update() -> Result<GithubUpdateInfo, AppError> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();

    Ok(GithubUpdateInfo {
        current_version: current_version.clone(),
        latest_version: current_version,
        available: false,
        auto_update_ready: false,
        disabled: true,
        release_url: String::new(),
        body: None,
        source: Some("disabled".to_string()),
    })
}

/// 防御性保留：旧前端即使错误调用安装命令，也不会下载或执行任何更新。
#[tauri::command]
pub async fn download_and_install_github_update(
    _expected_version: Option<String>,
) -> Result<GithubUpdateInstallResult, AppError> {
    Err(AppError::Unknown(AUTO_UPDATE_DISABLED_MESSAGE.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn 未配置本产品信任链时更新检查必须明确禁用() {
        let update = check_github_update().await.expect("disabled response");

        assert!(update.disabled);
        assert!(!update.available);
        assert!(!update.auto_update_ready);
        assert!(update.release_url.is_empty());
        assert_eq!(update.source.as_deref(), Some("disabled"));
    }
}
