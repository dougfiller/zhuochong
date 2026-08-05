# MCP Localhost API Bearer 鉴权修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 MCP Server 委托主应用 Localhost API 时读取同数据目录 Token，并通过 Bearer Header 完成鉴权且保持失败回退。

**Architecture:** MCP 启动时根据实际 `WORK_REVIEW_CONFIG_PATH` 计算 `localhost_api_token.txt` 路径并保存到进程状态；HTTP 请求通过可单测构造函数同时校验 API 开关、Token 与 URL。Token 缺失、为空、读取失败或请求失败时返回 `None`，现有日报模板和历史上下文回退保持不变。

**Tech Stack:** Rust 2021、ureq 2.12、serde_json、std::path/std::fs、Cargo 单元测试。

## Global Constraints

- 不修改 Ask/Agent 当前未提交代码。
- Token 文件名固定为 `localhost_api_token.txt`，目录取实际配置文件 `config.json` 的父目录。
- 请求必须使用 `Authorization: Bearer <token>`，Token 不得进入 URL、普通日志或错误消息。
- Token 文件不存在、为空或读取失败时不得发起未鉴权委托，并保持现有本地回退。
- 不改变 MCP 工具权限策略、工具数量或工具返回结构。
- 严格执行 RED → GREEN → REFACTOR；单次后台测试最长 60 秒。
- 不新增依赖，不进行无关重构。

---

### Task 1: 将 Token 路径绑定到 MCP 的实际配置路径

**Files:**
- Modify: `crates/mcp-server/src/main.rs:1-20,35-40,100-105,974-末尾`
- Test: `crates/mcp-server/src/main.rs` 内现有 `tests` 模块

**Interfaces:**
- Consumes: 实际 `config_path: &Path`。
- Produces: `fn localhost_api_token_path(config_path: &Path) -> PathBuf`。
- Produces: `fn read_localhost_api_token(token_path: &Path) -> Option<String>`。
- Produces: `AppState.localhost_api_token_path: PathBuf`。

- [ ] **Step 1: 写出 Token 路径和空 Token 行为的失败测试**

在 MCP `tests` 模块增加唯一临时目录辅助函数，并新增：

```rust
#[test]
fn token路径应跟随自定义配置文件目录() {
    let config_path = Path::new("/tmp/work-review-custom/config.json");
    assert_eq!(
        localhost_api_token_path(config_path),
        PathBuf::from("/tmp/work-review-custom/localhost_api_token.txt")
    );
}

#[test]
fn token读取应去除空白并拒绝空内容() {
    let dir = unique_temp_dir("mcp-token");
    std::fs::create_dir_all(&dir).expect("应创建临时目录");
    let token_path = dir.join("localhost_api_token.txt");

    std::fs::write(&token_path, "  wr-local-secret\n").expect("应写入 Token");
    assert_eq!(read_localhost_api_token(&token_path).as_deref(), Some("wr-local-secret"));

    std::fs::write(&token_path, " \n ").expect("应覆盖 Token");
    assert_eq!(read_localhost_api_token(&token_path), None);
    std::fs::remove_dir_all(dir).expect("应清理临时目录");
}
```

`unique_temp_dir` 使用 `std::process::id()` 与 `SystemTime::now().duration_since(UNIX_EPOCH).as_nanos()`，不新增测试依赖。

- [ ] **Step 2: 运行定向测试并确认 RED**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-mcp-server token -- --nocapture'`

Expected: FAIL，原因是 Token 路径和读取函数尚不存在。

- [ ] **Step 3: 实现 Token 路径与读取函数**

在 Localhost API 委托区域加入：

```rust
fn localhost_api_token_path(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("localhost_api_token.txt")
}

fn read_localhost_api_token(token_path: &Path) -> Option<String> {
    std::fs::read_to_string(token_path)
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}
```

在 `main` 中先把 `config_path` 转成 `PathBuf`，该路径同时传给 `AppConfig::load` 和 `localhost_api_token_path`；向 `AppState` 增加 `localhost_api_token_path` 字段。

- [ ] **Step 4: 运行 Token 测试并确认 GREEN**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-mcp-server token -- --nocapture'`

Expected: 新增测试全部 PASS；缺失文件自然返回 `None`。

- [ ] **Step 5: 格式化并提交路径绑定**

Run: `rustfmt --edition 2021 crates/mcp-server/src/main.rs`

```bash
git add crates/mcp-server/src/main.rs
git commit -m "fix: bind mcp api token to config path"
```

---

### Task 2: 构造携带 Bearer Header 且不泄露 Token 的委托请求

**Files:**
- Modify: `crates/mcp-server/src/main.rs:241-274,630-649,677-695,974-末尾`
- Test: `crates/mcp-server/src/main.rs` 内现有 `tests` 模块

**Interfaces:**
- Consumes: `localhost_api_base(&AppConfig) -> Option<String>`、`read_localhost_api_token(&Path) -> Option<String>`。
- Produces: `fn build_localhost_get_request(config: &AppConfig, token_path: &Path, path: &str) -> Option<ureq::Request>`。
- Produces: `fn try_localhost_get(config: &AppConfig, token_path: &Path, path: &str) -> Option<Value>`。

- [ ] **Step 1: 写出 Bearer Header、URL 无 Token 和缺失 Token 的失败测试**

新增两个测试：

```rust
#[test]
fn localhost委托请求应携带bearer且url不含token() {
    let dir = unique_temp_dir("mcp-request");
    std::fs::create_dir_all(&dir).expect("应创建临时目录");
    let token_path = dir.join("localhost_api_token.txt");
    std::fs::write(&token_path, "wr-local-secret").expect("应写入 Token");
    let mut config = AppConfig::default();
    config.localhost_api_enabled = true;
    config.localhost_api_host = Some("127.0.0.1".to_string());
    config.localhost_api_port = 47_831;

    let request = build_localhost_get_request(&config, &token_path, "/v1/context")
        .expect("有效配置和 Token 应构造请求");
    assert_eq!(request.method(), "GET");
    assert_eq!(request.header("Authorization"), Some("Bearer wr-local-secret"));
    assert_eq!(request.url(), "http://127.0.0.1:47831/v1/context");
    assert!(!request.url().contains("wr-local-secret"));
    std::fs::remove_dir_all(dir).expect("应清理临时目录");
}

#[test]
fn localhost委托缺少token时不应构造请求() {
    let mut config = AppConfig::default();
    config.localhost_api_enabled = true;
    assert!(build_localhost_get_request(
        &config,
        Path::new("/definitely/missing/localhost_api_token.txt"),
        "/v1/context"
    ).is_none());
}
```

- [ ] **Step 2: 运行定向测试并确认 RED**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-mcp-server localhost委托 -- --nocapture'`

Expected: FAIL，原因是 `build_localhost_get_request` 尚不存在或请求未设置 Header。

- [ ] **Step 3: 实现请求构造函数并收窄失败日志**

实现：

```rust
fn build_localhost_get_request(
    config: &AppConfig,
    token_path: &Path,
    path: &str,
) -> Option<ureq::Request> {
    let base = localhost_api_base(config)?;
    let token = read_localhost_api_token(token_path)?;
    let url = format!("{base}{path}");
    Some(
        ureq::get(&url)
            .set("Authorization", &format!("Bearer {token}"))
            .timeout(std::time::Duration::from_secs(2)),
    )
}
```

将 `try_localhost_get` 改为调用构造函数后 `.call()`；失败日志只记录 URL 和 ureq 错误，不记录 Header、Token 或 Token 文件内容。

- [ ] **Step 4: 运行请求构造测试并确认 GREEN**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-mcp-server localhost委托 -- --nocapture'`

Expected: 两个测试 PASS；Token 只存在于 Header。

- [ ] **Step 5: 更新两个委托调用点并保持回退行为**

将 AI 日报和当前上下文调用改为：

```rust
try_localhost_get(&s.config, &s.localhost_api_token_path, &path)
try_localhost_get(&s.config, &s.localhost_api_token_path, "/v1/context")
```

不得修改后续的 `if let Some(...)` 与本地模板/历史数据回退分支。

- [ ] **Step 6: 运行 MCP 全部测试与格式检查**

Run: `rustfmt --edition 2021 crates/mcp-server/src/main.rs`

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review-mcp-server --locked -- --nocapture'`

Run: `git diff --check`

Expected: MCP 测试全部 PASS；`git diff --check` 无输出。

- [ ] **Step 7: 提交 MCP 鉴权修复**

```bash
git add crates/mcp-server/src/main.rs
git commit -m "fix: authenticate mcp localhost delegation"
```
