# Bot 日报 API 一致性修复 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Bot 使用不泄露 Token 的 POST JSON 请求生成日报，并让 Localhost API 同时兼容现有 GET 与新的 POST 调用。

**Architecture:** 在 `bot_common.rs` 提取可单测的请求构造函数，统一设置 Bearer Header、JSON Body 与超时；在 `localhost_api.rs` 增加日报生成请求体和纯参数解析函数，GET/POST 进入同一个异步生成处理函数。保留 Query Token 鉴权和 GET 路由兼容性，但新的内部调用不再把 Token 放进 URL。

**Tech Stack:** Rust 2021、reqwest 0.11、serde/serde_json、Tauri 2、Cargo 单元测试。

## Global Constraints

- 不修改 Ask/Agent 当前未提交代码。
- 保留 `GET /v1/reports/generate?date=...` 兼容行为。
- 新增 `POST /v1/reports/generate`，JSON 字段为 `date: String`、`force: Option<bool>`、`locale: Option<String>`。
- Bot 的生成日报请求必须使用 `Authorization: Bearer <token>`，Token 不得进入 URL、普通日志或错误消息。
- GET 与 POST 必须复用同一份日报生成业务调用。
- 严格执行 RED → GREEN → REFACTOR；单次后台测试最长 60 秒。
- 不新增依赖，不进行文件拆分或无关重构。

---

### Task 1: 为 Bot 生成日报请求建立安全构造边界

**Files:**
- Modify: `src-tauri/src/bot_common.rs:33-39,373-427,433-末尾`
- Test: `src-tauri/src/bot_common.rs` 内现有 `tests` 模块

**Interfaces:**
- Consumes: `reqwest::Client`、`DeviceEndpoint { url: String, token: String, .. }`、日期字符串。
- Produces: `fn build_generate_report_request(client: &Client, device: &DeviceEndpoint, date: &str) -> reqwest::RequestBuilder`。
- Produces: `/generate` 分支只负责调用构造函数、发送请求和格式化响应。

- [ ] **Step 1: 写出 Bot 请求方法、鉴权和负载的失败测试**

在 `bot_common.rs` 的 `tests` 模块新增 `bot生成日报请求应使用post_json和bearer鉴权`：

```rust
#[test]
fn bot生成日报请求应使用post_json和bearer鉴权() {
    let client = Client::new();
    let device = DeviceEndpoint {
        name: "本机".to_string(),
        url: "http://127.0.0.1:47831/".to_string(),
        token: "wr-local-secret".to_string(),
        is_local: true,
    };

    let request = build_generate_report_request(&client, &device, "2026-07-21")
        .build()
        .expect("请求应可构造");

    assert_eq!(request.method(), reqwest::Method::POST);
    assert_eq!(request.url().as_str(), "http://127.0.0.1:47831/v1/reports/generate");
    assert!(!request.url().as_str().contains("wr-local-secret"));
    assert_eq!(
        request.headers().get(reqwest::header::AUTHORIZATION).and_then(|v| v.to_str().ok()),
        Some("Bearer wr-local-secret")
    );
    let body = request.body().and_then(|body| body.as_bytes()).expect("JSON Body 应存在");
    let payload: serde_json::Value = serde_json::from_slice(body).expect("JSON Body 应合法");
    assert_eq!(payload, serde_json::json!({ "date": "2026-07-21" }));
}
```

- [ ] **Step 2: 运行定向测试并确认 RED**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review --bin work-review bot_common::tests::bot生成日报请求应使用post_json和bearer鉴权 -- --exact --nocapture'`

Expected: FAIL，错误明确指向 `build_generate_report_request` 尚不存在。

- [ ] **Step 3: 实现最小请求构造函数并替换 `/generate` 分支中的内联构造**

在命令处理函数之前加入：

```rust
fn build_generate_report_request(
    client: &Client,
    device: &DeviceEndpoint,
    date: &str,
) -> reqwest::RequestBuilder {
    let url = format!(
        "{}/v1/reports/generate",
        device.url.trim_end_matches('/')
    );
    client
        .post(url)
        .bearer_auth(&device.token)
        .json(&serde_json::json!({ "date": date }))
        .timeout(Duration::from_secs(120))
}
```

将 `/generate` 分支改为 `build_generate_report_request(client, device, &date).send().await`，删除 `?token=` URL 拼接和重复的 `.post/.json/.timeout`。

- [ ] **Step 4: 运行定向测试并确认 GREEN**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review --bin work-review bot_common::tests::bot生成日报请求应使用post_json和bearer鉴权 -- --exact --nocapture'`

Expected: PASS，且 URL 不含 Token。

- [ ] **Step 5: 运行 `bot_common` 现有回归测试并格式化改动文件**

Run: `rustfmt --edition 2021 src-tauri/src/bot_common.rs`

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review --bin work-review bot_common::tests -- --nocapture'`

Expected: 所有 `bot_common::tests` PASS；无新增编译警告。

- [ ] **Step 6: 提交 Bot 请求修复**

```bash
git add src-tauri/src/bot_common.rs
git commit -m "fix: secure bot report generation request"
```

---

### Task 2: 为 Localhost API 增加兼容的 POST 日报生成路由

**Files:**
- Modify: `src-tauri/src/localhost_api.rs:61-68,627-658,956-959,1014-末尾`
- Test: `src-tauri/src/localhost_api.rs` 内现有 `tests` 模块

**Interfaces:**
- Consumes: `ParsedRequest { method, query, body, .. }`。
- Produces: `GenerateReportRequest { date: String, force: Option<bool>, locale: Option<String> }`。
- Produces: `fn parse_generate_report_params(request: &ParsedRequest) -> Result<(String, Option<bool>, Option<String>)>`。
- Produces: `async fn handle_generate_report_request(request: &ParsedRequest, app: &AppHandle, state: &Arc<Mutex<AppState>>) -> Result<HttpResponse>`，GET/POST 共用。

- [ ] **Step 1: 写出 POST JSON、GET 兼容和缺少日期的失败测试**

在 `localhost_api.rs` 的 `tests` 模块导入 `parse_generate_report_params` 与 `ParsedRequest`，增加测试构造辅助函数和三个独立测试：

```rust
fn parsed_request(method: &str, query: &[(&str, &str)], body: serde_json::Value) -> ParsedRequest {
    ParsedRequest {
        method: method.to_string(),
        path: "/v1/reports/generate".to_string(),
        query: query.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect(),
        headers: HashMap::new(),
        body: serde_json::to_vec(&body).expect("请求体应可序列化"),
    }
}
```

分别断言：

```rust
#[test]
fn post日报生成参数应从json请求体解析() {
    let request = parsed_request(
        "POST",
        &[],
        serde_json::json!({ "date": "2026-07-21", "force": true, "locale": "zh-CN" }),
    );
    assert_eq!(
        parse_generate_report_params(&request).expect("POST 参数应合法"),
        ("2026-07-21".to_string(), Some(true), Some("zh-CN".to_string()))
    );
}

#[test]
fn get日报生成参数应继续从query解析() {
    let request = parsed_request(
        "GET",
        &[("date", "2026-07-21"), ("force", "false"), ("locale", "en")],
        serde_json::Value::Null,
    );
    assert_eq!(
        parse_generate_report_params(&request).expect("GET 参数应保持兼容"),
        ("2026-07-21".to_string(), Some(false), Some("en".to_string()))
    );
}

#[test]
fn 日报生成参数缺少日期时应返回配置错误() {
    let request = parsed_request("POST", &[], serde_json::json!({ "force": true }));
    let error = parse_generate_report_params(&request).expect_err("缺少日期必须失败");
    assert!(error.to_string().contains("date 参数不能为空"));
}
```

- [ ] **Step 2: 运行三个定向测试并确认 RED**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review --bin work-review 日报生成参数 -- --nocapture'`

Expected: FAIL，原因是 `parse_generate_report_params` 和 `GenerateReportRequest` 尚不存在。

- [ ] **Step 3: 增加请求体类型和统一参数解析函数**

在 `ExportReportRequest` 附近新增：

```rust
#[derive(Debug, Deserialize)]
struct GenerateReportRequest {
    date: String,
    #[serde(default)]
    force: Option<bool>,
    #[serde(default)]
    locale: Option<String>,
}
```

实现 `parse_generate_report_params`：

- `GET` 从 Query 读取 `date/force/locale`。
- `POST` 使用现有 `parse_json_body::<GenerateReportRequest>`。
- `date.trim().is_empty()` 时统一返回 `AppError::Config("date 参数不能为空".to_string())`。
- 返回日期时保留调用方输入内容，不改变既有日期解析语义。
- 其他方法返回 `AppError::Config("日报生成接口仅支持 GET 或 POST".to_string())`。

- [ ] **Step 4: 运行参数解析测试并确认 GREEN**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review --bin work-review 日报生成参数 -- --nocapture'`

Expected: 三个测试全部 PASS。

- [ ] **Step 5: 提取共用异步处理函数并接通 GET/POST 路由**

实现：

```rust
async fn handle_generate_report_request(
    request: &ParsedRequest,
    app: &AppHandle,
    state: &Arc<Mutex<AppState>>,
) -> Result<HttpResponse> {
    let (date, force, locale) = parse_generate_report_params(request)?;
    commands::generate_report_inner(date, force, locale, app, state)
        .await
        .map(|content| HttpResponse::json(200, &serde_json::json!({ "content": content })))
}
```

将路由匹配改成：

```rust
("GET" | "POST", "/v1/reports/generate") => {
    handle_generate_report_request(&request, app, state).await
}
```

删除原 GET 分支内重复的参数解析和业务调用。

- [ ] **Step 6: 运行 Localhost API 测试、Bot 测试和编译检查**

Run: `rustfmt --edition 2021 src-tauri/src/localhost_api.rs src-tauri/src/bot_common.rs`

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review --bin work-review localhost_api::tests -- --nocapture'`

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review --bin work-review bot_common::tests -- --nocapture'`

Expected: 两组测试全部 PASS，POST 路由受现有 `RequestAuthMode::LocalApiToken` 保护。

- [ ] **Step 7: 提交 Localhost API 路由修复**

```bash
git add src-tauri/src/localhost_api.rs
git commit -m "fix: support post report generation api"
```

---

### Task 3: 验证 Bot 与 Localhost API 的集成契约

**Files:**
- Verify only: `src-tauri/src/bot_common.rs`
- Verify only: `src-tauri/src/localhost_api.rs`

**Interfaces:**
- Consumes: Task 1 的安全 POST 请求与 Task 2 的兼容路由。
- Produces: 可被后续全仓验证复用的测试证据，不新增生产接口。

- [ ] **Step 1: 检查 Token 不再出现在 Bot 生成日报 URL**

Run: `rg -n 'reports/generate\?token|generate\?[^" ]*token' src-tauri/src/bot_common.rs src-tauri/src/localhost_api.rs`

Expected: `bot_common.rs` 的生成日报分支无匹配；Localhost API 仍可在统一鉴权函数中兼容 Query Token，但不新增长 URL Token 调用。

- [ ] **Step 2: 运行 Tauri crate 全部测试**

Run: `timeout 60s sh -c 'CC=/usr/bin/cc cargo test -p work-review --bin work-review --locked'`

Expected: PASS，且无由本计划引入的新失败。

- [ ] **Step 3: 检查格式和补丁完整性**

Run: `git diff --check`

Run: `git status --short`

Expected: `git diff --check` 无输出；工作树只包含本计划预期文件或其他并行任务的已知文件。
