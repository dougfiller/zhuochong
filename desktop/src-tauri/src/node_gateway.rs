use crate::error::{AppError, Result};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const NODE_IDENTITY_FILE: &str = "node_gateway_identity.json";
pub const NODE_GATEWAY_PROTOCOL_VERSION: &str = "wr-node-gateway/v1alpha1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIdentity {
    pub device_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeGatewayStatusPayload {
    pub protocol_version: String,
    pub device_id: String,
    pub device_name: String,
}

fn node_identity_path(data_dir: &Path) -> PathBuf {
    data_dir.join(NODE_IDENTITY_FILE)
}

pub(crate) fn ensure_node_identity(state: &Arc<Mutex<AppState>>) -> Result<NodeIdentity> {
    let identity_path = {
        let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        node_identity_path(&state.data_dir)
    };
    ensure_node_identity_for_path(&identity_path)
}

pub(crate) fn get_node_gateway_status(
    state: &Arc<Mutex<AppState>>,
) -> Result<NodeGatewayStatusPayload> {
    let identity = ensure_node_identity(state)?;
    let (device_name_override, data_dir) = {
        let state = state.lock().map_err(|e| AppError::Unknown(e.to_string()))?;
        (
            state.config.node_gateway.device_name.clone(),
            state.data_dir.clone(),
        )
    };
    let _ = data_dir;

    let system_name = system_device_name();
    let device_name = resolve_node_device_name(device_name_override.as_deref(), &system_name);

    Ok(NodeGatewayStatusPayload {
        protocol_version: NODE_GATEWAY_PROTOCOL_VERSION.to_string(),
        device_id: identity.device_id,
        device_name,
    })
}

pub(crate) fn resolve_node_device_name(override_name: Option<&str>, fallback_host: &str) -> String {
    let override_name = override_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(override_name) = override_name {
        return override_name;
    }

    let fallback_host = fallback_host.trim();
    if !fallback_host.is_empty() {
        return fallback_host.to_string();
    }

    default_device_name()
}

pub(crate) fn ensure_node_identity_for_path(path: &Path) -> Result<NodeIdentity> {
    if let Some(existing) = read_node_identity_from_path(path)? {
        return Ok(existing);
    }

    let identity = NodeIdentity {
        device_id: format!("wr-{}", &Uuid::new_v4().simple().to_string()[..12]),
        created_at: chrono::Utc::now().timestamp(),
    };
    write_node_identity_to_path(path, &identity)?;
    Ok(identity)
}

fn read_node_identity_from_path(path: &Path) -> Result<Option<NodeIdentity>> {
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path)?;
    let identity = serde_json::from_str::<NodeIdentity>(&content)?;
    if identity.device_id.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(identity))
    }
}

fn write_node_identity_to_path(path: &Path, identity: &NodeIdentity) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(identity)?;
    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, &content)?;
    std::fs::rename(&temp_path, path)?;
    Ok(())
}

fn system_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(read_etc_hostname)
        .unwrap_or_else(default_device_name)
}

#[cfg(target_family = "unix")]
fn read_etc_hostname() -> Option<String> {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(not(target_family = "unix"))]
fn read_etc_hostname() -> Option<String> {
    None
}

fn default_device_name() -> String {
    match platform_label() {
        "macOS" => "Work Review Mac".to_string(),
        "Windows" => "Work Review Windows".to_string(),
        "Linux" => "Work Review Linux".to_string(),
        _ => "Work Review Device".to_string(),
    }
}

fn platform_label() -> &'static str {
    #[cfg(target_os = "macos")]
    return "macOS";
    #[cfg(target_os = "windows")]
    return "Windows";
    #[cfg(target_os = "linux")]
    return "Linux";
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return "Unknown";
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{ensure_node_identity_for_path, resolve_node_device_name};

    fn temp_identity_path(label: &str) -> PathBuf {
        let unique = format!("work-review-node-gateway-{label}-{}", uuid::Uuid::new_v4());
        std::env::temp_dir().join(unique).join("node_identity.json")
    }

    #[test]
    fn 设备身份文件存在时应稳定复用同一device_id() {
        let path = temp_identity_path("stable-device-id");

        let first = ensure_node_identity_for_path(&path).expect("首次生成 device id 失败");
        let second = ensure_node_identity_for_path(&path).expect("二次读取 device id 失败");

        assert_eq!(first.device_id, second.device_id);

        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn 设备显示名应优先使用用户配置覆盖值() {
        let resolved = resolve_node_device_name(Some("  我的工作主机  "), "fallback-host");
        assert_eq!(resolved, "我的工作主机");
    }
}
