#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

/// 在 sudo 下运行时，恢复原用户的 Wayland/DBus 环境变量。
///
/// sudo 会剥离 WAYLAND_DISPLAY、XDG_RUNTIME_DIR 等关键变量，
/// 导致截图工具无法连接 compositor（黑屏）且会话检测失败。
/// 此函数从原用户进程的环境中恢复这些变量。
#[cfg(target_os = "linux")]
pub fn fix_wayland_env_if_sudo() {
    let sudo_uid = match std::env::var("SUDO_UID") {
        Ok(uid) if !uid.trim().is_empty() => uid,
        _ => return, // 不是通过 sudo 运行，无需修复
    };

    // 需要恢复的环境变量
    let needed_vars = [
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "XDG_SESSION_TYPE",
        "XDG_CURRENT_DESKTOP",
        "DESKTOP_SESSION",
        "DISPLAY",
    ];

    // 检查是否已经有这些变量（sudo -E 场景）
    let missing: Vec<_> = needed_vars
        .iter()
        .filter(|v| std::env::var(v).is_err())
        .copied()
        .collect();

    if missing.is_empty() {
        return;
    }

    let uid_trimmed = sudo_uid.trim();

    // 尝试从原用户进程的 /proc/{pid}/environ 读取环境变量
    let user_env = read_user_process_env(uid_trimmed);

    let mut fixed = 0;

    // 第一轮：恢复可从 user_env 读到的变量，以及 XDG_RUNTIME_DIR 的回退
    // （XDG_RUNTIME_DIR 必须先于 WAYLAND_DISPLAY 恢复，后者依赖前者）
    for var in &missing {
        if let Some(val) = user_env.get(*var) {
            std::env::set_var(var, val);
            fixed += 1;
            continue;
        }
        if *var == "XDG_RUNTIME_DIR" {
            let runtime_dir = format!("/run/user/{uid_trimmed}");
            if std::path::Path::new(&runtime_dir).is_dir() {
                std::env::set_var("XDG_RUNTIME_DIR", &runtime_dir);
                fixed += 1;
            }
        }
    }

    // 第二轮：恢复依赖第一轮结果的变量
    for var in &missing {
        if std::env::var(var).is_ok() {
            continue; // 已在第一轮恢复
        }
        match *var {
            "WAYLAND_DISPLAY" => {
                if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
                    for name in &["wayland-0", "wayland-1"] {
                        let socket = std::path::Path::new(&runtime_dir).join(name);
                        if socket.exists() {
                            std::env::set_var("WAYLAND_DISPLAY", *name);
                            fixed += 1;
                            break;
                        }
                    }
                }
            }
            "DBUS_SESSION_BUS_ADDRESS" => {
                let bus_path = format!("unix:path=/run/user/{uid_trimmed}/bus");
                if std::path::Path::new(&format!("/run/user/{uid_trimmed}/bus")).exists() {
                    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &bus_path);
                    fixed += 1;
                }
            }
            "XDG_SESSION_TYPE" => {
                if std::env::var("WAYLAND_DISPLAY").is_ok() {
                    std::env::set_var("XDG_SESSION_TYPE", "wayland");
                    fixed += 1;
                }
            }
            _ => {}
        }
    }

    if fixed > 0 {
        log::info!("sudo 环境修复：已恢复 {fixed} 个 Wayland/DBus 环境变量 (uid={uid_trimmed})");
    }
}

/// 从原用户进程的 /proc/{pid}/environ 读取环境变量
#[cfg(target_os = "linux")]
fn read_user_process_env(uid: &str) -> std::collections::HashMap<String, String> {
    let mut env = std::collections::HashMap::new();
    let uid_num: u32 = match uid.parse() {
        Ok(n) => n,
        Err(_) => return env,
    };

    let Ok(entries) = std::fs::read_dir("/proc") else {
        return env;
    };

    for entry in entries.flatten() {
        let pid_dir = entry.path();

        // 检查是否是进程目录
        let _file_name = match pid_dir.file_name().and_then(|n| n.to_str()) {
            Some(name) if name.chars().all(|c| c.is_ascii_digit()) => name,
            _ => continue,
        };

        // 读取进程的 status 获取 UID
        let status_path = pid_dir.join("status");
        let Ok(status) = std::fs::read_to_string(&status_path) else {
            continue;
        };

        let mut proc_uid: Option<u32> = None;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("Uid:") {
                // Uid 行格式: Uid:\t1000\t1000\t1000\t1000
                proc_uid = rest.split_whitespace().next().and_then(|s| s.parse().ok());
                break;
            }
        }

        let Some(proc_uid) = proc_uid else { continue };
        if proc_uid != uid_num {
            continue;
        }

        // 读取 environ
        let environ_path = pid_dir.join("environ");
        let Ok(raw) = std::fs::read(&environ_path) else {
            continue;
        };

        // environ 以 null 字节分隔 KEY=VALUE 条目
        for entry_bytes in raw.split(|&b| b == 0) {
            if entry_bytes.is_empty() {
                continue;
            }
            let entry = String::from_utf8_lossy(entry_bytes);
            if let Some((key, value)) = entry.split_once('=') {
                env.insert(key.to_string(), value.to_string());
            }
        }

        // 找到第一个匹配的进程就够了
        break;
    }

    env
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxDesktopSession {
    X11,
    Wayland,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxDesktopEnvironment {
    Gnome,
    Kde,
    Sway,
    Hyprland,
    Unknown,
}

impl LinuxDesktopSession {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X11 => "x11",
            Self::Wayland => "wayland",
            Self::Unknown => "unknown",
        }
    }

    pub fn supports_active_window_tracking(self) -> bool {
        matches!(self, Self::X11)
    }

    pub fn supports_screenshot_capture(self) -> bool {
        matches!(self, Self::X11 | Self::Wayland)
    }
}

impl LinuxDesktopEnvironment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gnome => "gnome",
            Self::Kde => "kde",
            Self::Sway => "sway",
            Self::Hyprland => "hyprland",
            Self::Unknown => "unknown",
        }
    }
}

fn detect_linux_desktop_environment(
    xdg_current_desktop: Option<&str>,
    desktop_session: Option<&str>,
) -> LinuxDesktopEnvironment {
    let current_desktop = xdg_current_desktop
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let desktop_session = desktop_session.unwrap_or_default().trim().to_lowercase();

    let detect = |value: &str| -> Option<LinuxDesktopEnvironment> {
        if value.contains("gnome") {
            Some(LinuxDesktopEnvironment::Gnome)
        } else if value.contains("kde") || value.contains("plasma") {
            Some(LinuxDesktopEnvironment::Kde)
        } else if value.contains("sway") {
            Some(LinuxDesktopEnvironment::Sway)
        } else if value.contains("hyprland") || value.contains("hypr") {
            Some(LinuxDesktopEnvironment::Hyprland)
        } else {
            None
        }
    };

    detect(&current_desktop)
        .or_else(|| detect(&desktop_session))
        .unwrap_or(LinuxDesktopEnvironment::Unknown)
}

#[cfg(target_os = "linux")]
pub fn current_linux_desktop_session() -> LinuxDesktopSession {
    let session_type = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .trim()
        .to_lowercase();

    match session_type.as_str() {
        "wayland" => return LinuxDesktopSession::Wayland,
        "x11" => return LinuxDesktopSession::X11,
        _ => {}
    }

    let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_default();
    if !wayland_display.trim().is_empty() {
        return LinuxDesktopSession::Wayland;
    }

    let x11_display = std::env::var("DISPLAY").unwrap_or_default();
    if !x11_display.trim().is_empty() {
        return LinuxDesktopSession::X11;
    }

    LinuxDesktopSession::Unknown
}

#[cfg(not(target_os = "linux"))]
pub fn current_linux_desktop_session() -> LinuxDesktopSession {
    LinuxDesktopSession::Unknown
}

#[cfg(target_os = "linux")]
pub fn current_linux_desktop_environment() -> LinuxDesktopEnvironment {
    detect_linux_desktop_environment(
        std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref(),
        std::env::var("DESKTOP_SESSION").ok().as_deref(),
    )
}

#[cfg(not(target_os = "linux"))]
pub fn current_linux_desktop_environment() -> LinuxDesktopEnvironment {
    LinuxDesktopEnvironment::Unknown
}

#[cfg(test)]
mod tests {
    use super::{detect_linux_desktop_environment, LinuxDesktopEnvironment, LinuxDesktopSession};

    #[test]
    fn xdg_current_desktop应优先识别_gnome() {
        let detected = detect_linux_desktop_environment(Some("ubuntu:GNOME"), Some("plasma"));
        assert_eq!(detected, LinuxDesktopEnvironment::Gnome);
    }

    #[test]
    fn desktop_session应兜底识别_sway() {
        let detected = detect_linux_desktop_environment(None, Some("sway"));
        assert_eq!(detected, LinuxDesktopEnvironment::Sway);
    }

    #[test]
    fn 未命中桌面环境时应回退_unknown() {
        let detected = detect_linux_desktop_environment(Some(""), Some("custom-session"));
        assert_eq!(detected, LinuxDesktopEnvironment::Unknown);
    }

    #[test]
    fn x11会话仍应支持活动窗口追踪() {
        assert!(LinuxDesktopSession::X11.supports_active_window_tracking());
        assert!(!LinuxDesktopSession::Wayland.supports_active_window_tracking());
    }
}
