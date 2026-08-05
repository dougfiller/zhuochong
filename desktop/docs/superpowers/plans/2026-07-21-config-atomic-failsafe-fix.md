# 配置原子保存与故障安全修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 防止配置写入中断破坏 `config.json`，并在主配置损坏时从备份恢复或以关闭采集和网络服务的安全状态启动。

**Architecture:** `work-review-core` 增加配置加载状态、单代 `.bak` 恢复和安全配置构造；保存流程在目标目录完成安全临时文件写入、同步、备份更新和原子替换。Tauri 启动根据加载状态决定是否允许自动保存以及初始录制状态，从而避免损坏文件被默认值静默覆盖。

**Tech Stack:** Rust 2021、serde/serde_json、std::fs/std::io、uuid 1.6（现有依赖）、Tauri 2、Cargo 单元测试。

## Global Constraints

- 不修改 Ask/Agent 当前未提交代码。
- 文件不存在属于首次启动；文件存在但无法读取或解析属于配置损坏。
- 备份路径固定为 `<config path>.bak`，即 `config.json.bak`，只保留最近一代。
- Unix 下配置临时文件、主文件和备份文件创建时即使用 `0600`。
- 保存必须执行完整写入、`flush`、`sync_all`、同目录替换，并尽可能同步父目录。
- 保存失败必须清理临时文件并保留原配置。
- 主配置损坏时先尝试 `.bak`；主配置与备份均不可用时不得覆盖主文件。
- 故障安全配置必须关闭截图、Telegram/飞书/企微/钉钉 Bot、Localhost API、MCP Server 和远程上传。
- 故障安全启动必须设置 `is_recording = false`、`is_paused = true`。
- 严格执行 RED → GREEN → REFACTOR；单次后台测试最长 60 秒。
- 不新增依赖、不新增完整恢复 UI、不进行无关重构。

---

### Task 1: 建立可区分首次启动、备份恢复和损坏状态的加载 API

**Files:**
- Modify: `crates/core/src/config.rs:1-4,587-596,638-785,1058-1137,1618-末尾`
- Test: `crates/core/src/config.rs` 内现有 `tests` 模块

**Interfaces:**
- Produces: `pub enum ConfigLoadStatus { Loaded, Missing, RecoveredFromBackup, Corrupted }`。
- Produces: `pub struct ConfigLoadResult { pub config: AppConfig, pub status: ConfigLoadStatus, pub primary_error: Option<String>, pub backup_error: Option<String> }`。
- Produces: `pub fn config_backup_path(path: &Path) -> PathBuf`，把 `.bak` 追加到完整文件名。
- Produces: `AppConfig::load_with_recovery(path: &Path) -> ConfigLoadResult`。
- Produces: `ConfigLoadStatus::allows_automatic_save(self) -> bool` 与 `ConfigLoadStatus::requires_fail_safe(self) -> bool`。

- [ ] **Step 1: 写出首次启动与正常加载的失败测试**

新增唯一临时目录辅助函数，并加入：

```rust
#[test]
fn 配置文件不存在时应返回首次启动状态() {
    let dir = unique_temp_dir("config-missing");
    let result = AppConfig::load_with_recovery(&dir.join("config.json"));
    assert_eq!(result.status, ConfigLoadStatus::Missing);
    assert!(!result.status.requires_fail_safe());
    assert!(result.status.allows_automatic_save());
}

#[test]
fn 合法主配置应返回正常加载状态() {
    let dir = unique_temp_dir("config-loaded");
    std::fs::create_dir_all(&dir).expect("应创建临时目录");
    let path = dir.join("config.json");
    std::fs::write(&path, serde_json::to_vec(&AppConfig::default()).unwrap()).unwrap();
    let result = AppConfig::load_with_recovery(&path);
    assert_eq!(result.status, ConfigLoadStatus::Loaded);
    assert!(result.primary_error.is_none());
    std::fs::remove_dir_all(dir).unwrap();
}
```

- [ ] **Step 2: 运行加载状态测试并确认 RED**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-core 配置文件不存在 -- --nocapture && CC=/usr/bin/cc cargo test -p work-review-core 合法主配置 -- --nocapture'`

Expected: FAIL，原因是恢复类型和 `load_with_recovery` 尚不存在。

- [ ] **Step 3: 定义加载状态、结果和备份路径函数**

实现状态类型与方法：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigLoadStatus {
    Loaded,
    Missing,
    RecoveredFromBackup,
    Corrupted,
}

impl ConfigLoadStatus {
    pub fn allows_automatic_save(self) -> bool {
        matches!(self, Self::Loaded | Self::Missing)
    }

    pub fn requires_fail_safe(self) -> bool {
        matches!(self, Self::Corrupted)
    }
}
```

`config_backup_path` 必须通过 `OsString::push(".bak")` 得到 `config.json.bak`，不能使用 `with_extension("bak")` 生成 `config.bak`。

- [ ] **Step 4: 实现正常加载和首次启动分支并确认 GREEN**

`load_with_recovery` 首先检查 `path.exists()`：不存在时返回默认配置和 `Missing`；存在时复用一个“读取、反序列化、normalize”的私有函数，成功返回 `Loaded`。

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-core 配置文件不存在 -- --nocapture && CC=/usr/bin/cc cargo test -p work-review-core 合法主配置 -- --nocapture'`

Expected: 两个测试 PASS。

- [ ] **Step 5: 提交基础加载状态 API**

```bash
git add crates/core/src/config.rs
git commit -m "feat: expose config load status"
```

---

### Task 2: 增加备份恢复和最小安全配置

**Files:**
- Modify: `crates/core/src/config.rs:587-596,1058-1137,1618-末尾`
- Test: `crates/core/src/config.rs` 内现有 `tests` 模块

**Interfaces:**
- Consumes: Task 1 的 `ConfigLoadStatus`、`ConfigLoadResult`、`config_backup_path`。
- Produces: `fn fail_safe_config() -> AppConfig`（私有）或等价的 `AppConfig::fail_safe()` 私有构造路径。
- Produces: `load_with_recovery` 的 `RecoveredFromBackup` 与 `Corrupted` 分支。

- [ ] **Step 1: 写出主配置损坏但备份有效的失败测试**

```rust
#[test]
fn 主配置损坏时应从备份恢复且不覆盖主文件() {
    let dir = unique_temp_dir("config-backup-recovery");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    let backup_path = config_backup_path(&path);
    std::fs::write(&path, b"{ broken json").unwrap();
    let mut backup = AppConfig::default();
    backup.locale = "en".to_string();
    std::fs::write(&backup_path, serde_json::to_vec(&backup).unwrap()).unwrap();

    let result = AppConfig::load_with_recovery(&path);

    assert_eq!(result.status, ConfigLoadStatus::RecoveredFromBackup);
    assert_eq!(result.config.locale, "en");
    assert!(result.primary_error.is_some());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ broken json");
    assert!(!result.status.allows_automatic_save());
    std::fs::remove_dir_all(dir).unwrap();
}
```

- [ ] **Step 2: 写出主配置与备份都损坏时的失败测试**

```rust
#[test]
fn 主配置与备份都损坏时应返回关闭采集和网络服务的安全配置() {
    let dir = unique_temp_dir("config-fail-safe");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    std::fs::write(&path, b"broken-primary").unwrap();
    std::fs::write(config_backup_path(&path), b"broken-backup").unwrap();

    let result = AppConfig::load_with_recovery(&path);

    assert_eq!(result.status, ConfigLoadStatus::Corrupted);
    assert!(result.status.requires_fail_safe());
    assert!(!result.config.storage.screenshots_enabled);
    assert!(!result.config.localhost_api_enabled);
    assert!(!result.config.telegram_bot_enabled);
    assert!(!result.config.feishu_bot_enabled);
    assert!(!result.config.wecom_bot_enabled);
    assert!(!result.config.dingtalk_bot_enabled);
    assert!(!result.config.mcp_server_enabled);
    assert_eq!(result.config.remote_storage.provider, RemoteStorageProvider::None);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken-primary");
    std::fs::remove_dir_all(dir).unwrap();
}
```

- [ ] **Step 3: 运行恢复测试并确认 RED**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-core 主配置 -- --nocapture'`

Expected: FAIL，现有加载行为没有备份恢复和故障安全状态。

- [ ] **Step 4: 实现备份恢复与安全配置构造**

实现顺序：

1. 主配置读取或解析失败时保存 `primary_error`，读取 `config_backup_path(path)`。
2. 备份可读取、反序列化并 normalize 时返回 `RecoveredFromBackup`，不得调用 `save`。
3. 备份缺失或损坏时记录 `backup_error`，从 `AppConfig::default()` 构造安全配置。
4. 安全配置明确设置：

```rust
config.storage.screenshots_enabled = false;
config.localhost_api_enabled = false;
config.telegram_bot_enabled = false;
config.feishu_bot_enabled = false;
config.wecom_bot_enabled = false;
config.dingtalk_bot_enabled = false;
config.mcp_server_enabled = false;
config.remote_storage.provider = RemoteStorageProvider::None;
```

5. 返回 `Corrupted`，不得写入主配置或备份。

- [ ] **Step 5: 运行恢复测试并确认 GREEN**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-core 主配置 -- --nocapture'`

Expected: 两个恢复测试 PASS，损坏主文件内容保持原样。

- [ ] **Step 6: 运行 Core 配置测试并提交**

Run: `rustfmt --edition 2021 crates/core/src/config.rs`

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-core config::tests -- --nocapture'`

Expected: `config::tests` 全部 PASS。

```bash
git add crates/core/src/config.rs
git commit -m "fix: recover corrupted config safely"
```

---

### Task 3: 将配置保存改为安全临时文件、单代备份和原子替换

**Files:**
- Modify: `crates/core/src/config.rs:1-4,1123-1137,1618-末尾`
- Test: `crates/core/src/config.rs` 内现有 `tests` 模块

**Interfaces:**
- Consumes: `config_backup_path(path: &Path) -> PathBuf`。
- Produces: 维持公开签名 `AppConfig::save(&self, path: &Path) -> Result<()>`。
- Produces: 私有安全写入、父目录同步和平台替换辅助函数；临时文件名必须与目标同目录且包含 UUID。

- [ ] **Step 1: 写出正常保存、权限和备份的失败测试**

新增测试：

```rust
#[test]
fn 覆盖保存应生成可解析备份并写入新配置() {
    let dir = unique_temp_dir("config-atomic-save");
    let path = dir.join("config.json");
    let mut old = AppConfig::default();
    old.locale = "en".to_string();
    old.save(&path).unwrap();
    let mut new = old.clone();
    new.locale = "zh-CN".to_string();

    new.save(&path).unwrap();

    assert_eq!(AppConfig::load(&path).unwrap().locale, "zh-CN");
    assert_eq!(AppConfig::load(&config_backup_path(&path)).unwrap().locale, "en");
    std::fs::remove_dir_all(dir).unwrap();
}
```

Unix 下再增加 `配置与备份权限应为0600`，使用 `std::os::unix::fs::PermissionsExt::mode() & 0o777` 分别断言主文件和备份为 `0o600`。

- [ ] **Step 2: 写出失败保留原文件与清理临时文件的失败测试**

先保存一份合法主配置，再创建名为 `config.json.bak` 的非空目录，使备份替换必然失败：

```rust
#[test]
fn 备份更新失败时应保留原配置并清理临时文件() {
    let dir = unique_temp_dir("config-save-failure");
    let path = dir.join("config.json");
    let original = AppConfig::default();
    original.save(&path).unwrap();
    let backup_path = config_backup_path(&path);
    std::fs::create_dir_all(&backup_path).unwrap();
    std::fs::write(backup_path.join("blocker"), b"x").unwrap();
    let original_bytes = std::fs::read(&path).unwrap();

    let mut changed = original.clone();
    changed.locale = "en".to_string();
    assert!(changed.save(&path).is_err());

    assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
    let names = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(!names.iter().any(|name| name.contains(".tmp-")));
    std::fs::remove_dir_all(dir).unwrap();
}
```

- [ ] **Step 3: 运行保存测试并确认 RED**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-core 保存 -- --nocapture'`

Expected: 至少备份测试失败；现有 `save` 不创建 `.bak`，也不是原子流程。

- [ ] **Step 4: 实现同目录安全临时文件写入**

实现私有辅助函数，要求：

1. `create_dir_all(parent)`。
2. 临时路径格式为 `.<目标文件名>.tmp-<uuid>`。
3. 使用 `OpenOptions::new().write(true).create_new(true)`；Unix 通过 `OpenOptionsExt::mode(0o600)` 在创建时设置权限。
4. `write_all(content)`、`flush()`、`sync_all()`。
5. 任何错误都删除对应临时文件。

- [ ] **Step 5: 实现单代备份更新**

当主文件存在时：

1. 创建独立备份临时文件。
2. 把主文件内容复制到备份临时文件并 `flush/sync_all`。
3. Unix 确保临时备份权限为 `0600`。
4. 用同目录替换操作把备份临时文件替换为 `config.json.bak`。
5. 备份成功前不得替换主配置。

- [ ] **Step 6: 实现平台替换和目录同步**

- Unix 使用同目录 `std::fs::rename(source, target)`，目标存在时由系统原子替换。
- Windows 使用 `MoveFileExW` 的 `MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH` 语义实现同卷替换；通过 `#[cfg(windows)]` 隔离，不引入新依赖。
- 主配置替换成功后在支持目录 `File::open(parent)` 的平台调用 `sync_all()`；目录同步失败记录错误并返回，不能回滚已完成的原子替换。
- 无论成功失败，清理尚未被 rename 的临时文件。

- [ ] **Step 7: 运行保存测试并确认 GREEN**

Run: `rustfmt --edition 2021 crates/core/src/config.rs`

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-core 保存 -- --nocapture'`

Expected: 正常保存、备份、Unix 权限、失败保留原文件和临时文件清理测试全部 PASS。

- [ ] **Step 8: 运行 Core 全部测试并提交**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-core --locked -- --nocapture'`

Expected: Core 全部测试 PASS。

```bash
git add crates/core/src/config.rs
git commit -m "fix: save config atomically with backup"
```

---

### Task 4: 在 Tauri 启动流程接入恢复状态并禁止损坏配置自动覆盖

**Files:**
- Modify: `src-tauri/src/main.rs:413-430,3321-3335,3352-3394,3427-3445,3950-末尾`
- Test: `src-tauri/src/main.rs` 内现有测试模块

**Interfaces:**
- Consumes: `AppConfig::load_with_recovery`、`ConfigLoadStatus`、`ConfigLoadResult`。
- Produces: `fn initial_recording_state(status: ConfigLoadStatus) -> (bool, bool)`。
- Produces: `AppState.config_load_status: ConfigLoadStatus`，为后续 UI 恢复提示保留边界。

- [ ] **Step 1: 写出故障安全录制状态和自动保存策略的失败测试**

在 `main.rs` 测试模块新增：

```rust
#[test]
fn 配置损坏时初始录制必须关闭并暂停() {
    assert_eq!(
        initial_recording_state(ConfigLoadStatus::Corrupted),
        (false, true)
    );
}

#[test]
fn 正常首次启动和备份恢复的录制状态应按恢复策略区分() {
    assert_eq!(initial_recording_state(ConfigLoadStatus::Loaded), (true, false));
    assert_eq!(initial_recording_state(ConfigLoadStatus::Missing), (true, false));
    assert_eq!(
        initial_recording_state(ConfigLoadStatus::RecoveredFromBackup),
        (true, false)
    );
    assert!(!ConfigLoadStatus::RecoveredFromBackup.allows_automatic_save());
    assert!(!ConfigLoadStatus::Corrupted.allows_automatic_save());
}
```

- [ ] **Step 2: 运行定向测试并确认 RED**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review --bin work-review 配置损坏时初始录制 -- --nocapture'`

Expected: FAIL，`initial_recording_state` 和导入的恢复状态尚未接入。

- [ ] **Step 3: 接入加载结果和明确日志**

将启动加载改为：

```rust
let load_result = AppConfig::load_with_recovery(&config_path);
let config_load_status = load_result.status;
let mut config = load_result.config;
```

日志要求：

- `Loaded`：无需额外告警。
- `Missing`：记录首次启动信息。
- `RecoveredFromBackup`：`warn!` 主配置损坏、已从 `config.json.bak` 读取、不会自动覆盖主文件，并记录 `primary_error`。
- `Corrupted`：`error!` 主配置和备份都不可用、已关闭采集/截图/网络服务，并记录两类错误；不得记录配置密钥内容。

- [ ] **Step 4: 阻止恢复/损坏状态下的启动自动保存**

对以下三个启动写回点增加 `config_load_status.allows_automatic_save()` 条件：

1. 迁移旧版 `excluded_apps` 后保存。
2. 更新 `last_app_version` 后保存。
3. macOS 更新录屏权限提示状态后保存。

内存中的字段仍可更新，但 `RecoveredFromBackup` 和 `Corrupted` 状态不得调用 `config.save(&config_path)`。

- [ ] **Step 5: 设置初始录制状态并暴露恢复状态边界**

实现：

```rust
fn initial_recording_state(status: ConfigLoadStatus) -> (bool, bool) {
    if status.requires_fail_safe() {
        (false, true)
    } else {
        (true, false)
    }
}
```

在构造 `AppState` 前计算 `(is_recording, is_paused)`；替换硬编码 `true/false`，并新增 `config_load_status` 字段。由于 `Corrupted` 的配置已经关闭 Localhost API 和 Telegram Bot，现有 `sync_*_runtime` 调用会保持关闭。

- [ ] **Step 6: 运行定向测试和 Tauri 全部测试**

Run: `rustfmt --edition 2021 src-tauri/src/main.rs crates/core/src/config.rs`

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review --bin work-review 配置损坏时初始录制 -- --nocapture'`

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review --bin work-review --locked'`

Expected: 定向测试与 Tauri 测试全部 PASS；编译无新增未使用字段警告。若 `config_load_status` 暂无读取点，通过启动日志或轻量 getter 使用它，而不是添加 `allow(dead_code)`。

- [ ] **Step 7: 检查损坏文件不会被启动流程覆盖**

Run: `rg -n 'config\.save\(&config_path\)' src-tauri/src/main.rs`

Expected: 每个启动期匹配点都受 `allows_automatic_save()` 保护。

Run: `git diff --check`

Expected: 无空白错误。

- [ ] **Step 8: 提交 Tauri 故障安全接入**

```bash
git add src-tauri/src/main.rs
git commit -m "fix: fail closed on corrupted startup config"
```
