## 冻结 Work Review 来源并建立正式产品源码副本（2026-08-05）

### 基线复制与复核

```bash
python3 -B kaifa/kaifa_test/verify_work_review_baseline.py create \
  --source '参考/Work-Review-main' \
  --destination desktop

python3 -B kaifa/kaifa_test/verify_work_review_baseline.py verify \
  --source '参考/Work-Review-main' \
  --destination desktop
```

- `create` 仅在 `desktop/` 不存在时执行：先从官方 Git 仓库检出固定 tag，并以 `git archive` 获取固定 commit 的文件集。只有该文件集与本地参考逐文件 SHA-256 一致时，才会在同级临时目录复制、写入三份基线元数据并原子形成 `desktop/`；任何不一致或上游不可获取都会失败且不创建目标目录。
- `verify` 先复核来源清单、目标路径/字节数/SHA-256、必需工程文件、许可证、第三方声明和禁止路径；可联网时还会重新取得官方归档并复核 tree 与文件清单。网络不可用时它只报告“本地快照已复核”，不将上游身份标记为已验证。
- 固定来源身份为 `https://github.com/wm94i/Work-Review` 的 `v1.1.0`，解析提交 `500f9d2cb3027392cfcc32ad18395dfe348fb4a1`。`-B` 禁止 Python 在本机缓存目录写入 `.pyc`，保持本步骤仅修改项目内产物。

## 建立 Work Review 修改前回归基线与继承矩阵（2026-08-05）

```bash
# 仅校验结构、冻结源码、脱敏证据哈希和已知失败；不启动应用、不安装依赖、不联网。
python3 -B kaifa/kaifa_test/verify_work_review_regression_baseline.py --project-root .

# 复核步骤 1 的冻结来源；网络不可用时会只确认本地快照，不会伪报上游已重新验证。
python3 -B kaifa/kaifa_test/verify_work_review_baseline.py verify \
  --source '参考/Work-Review-main' \
  --destination desktop
```

- 基线记录在 `desktop/docs/baselines/work-review-inheritance-matrix.md` 与 `work-review-regression-baseline.json`；自动化证据只提交脱敏摘要和 SHA-256，严禁放入截图正文、活动/OCR 文本、凭据、Cookie、模型 payload 或用户绝对路径。
- 状态语义固定为：`pass` 为已有可复核证据的真实通过；`conditional-pass` 只表示 schema/mock/错误路径通过；`fail`、`blocked`、`not-run` 均关闭相应发布门禁。后续只追加 `after` 证据，不能改写 `before`。

## 冻结微信知识库状态机和验收契约（2026-08-06）

以下命令只编译或执行步骤 4 的纯领域契约；不启动 Tauri、不访问网络、真实微信、聊天导出、`knowledge.sqlite` 或模型 transport。

```bash
cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat:: --no-default-features
cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge:: --no-default-features
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m1'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'
```

下面两条是预期失败检查：前者验证发布目标不能不选 M1/M2，后者验证不能同时选择两者。两者都必须因 `model_contract.rs` 的 `compile_error!` 失败；不要把它们当作常规成功命令。

```bash
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features wechat-contract-check
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m1,wechat-m2'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2,wechat-contract-probe-private-constructors'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m1,wechat-contract-probe-m1-rag'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2,wechat-contract-probe-m2-m1'
```

后三条同样是预期失败：分别必须因私有字段、M1 无 RAG 入口、M2 无 M1 入口失败。`wechat-contract-check` 仅用于隔离发布契约检查，因此既有未选择微信 release feature 的默认 Work Review 构建保持兼容。新增 fixture 位于 `desktop/src-tauri/tests/fixtures/wechat_contract/`，只可使用手写虚构 ID、通用文字和固定值；不得填入聊天正文、联系人、截图、真实来源路径、源目录哈希或凭据。
- `verify_work_review_baseline.py` 现在将本步骤明确列出的矩阵、结果 JSON 与五份摘要视为可审计的 `docs/baselines/` 元数据；仍拒绝该目录中的任何其他文件或目录。

## 建立最小微信/知识库模块骨架并接入现有配置（2026-08-06）

以下命令只检查步骤 5 的安全空骨架；不会启动 Tauri、访问网络、真实微信、聊天正文、模型或知识库数据库。

```bash
python3 -B kaifa/kaifa_test/verify_wechat_knowledge_skeleton.py
cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat::commands::tests::unknown_profile_fails_closed --no-default-features
cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge::config::tests::only_exact_loopback_http_endpoints_are_accepted --no-default-features
cargo test --manifest-path desktop/Cargo.toml -p work-review-core config --quiet
```

- `verify_wechat_knowledge_skeleton.py` 是静态接线门禁：核对三项独立 managed state、三个安全 command、空实现错误码、loopback validator 与 `AppConfig` 的默认/normalize 接线；不读取用户配置或内容。
- 两条 Tauri 定向单测分别证明未知微信 profile fail-closed、非 loopback embedding endpoint 被拒绝且没有网络调用。
- core config 定向测试覆盖旧配置缺字段的默认值，以及留存、最近目录、topK、token budget 和 token counter 的安全归一化。

## 前台微信识别和版本化布局兼容性档案（2026-08-06）

以下命令只验证本步骤的静态 fail-closed 门禁和 Rust 纯校验；不会启动 Tauri、访问网络、读取微信数据库、UI Automation、截图、OCR、检索或模型。

```bash
python3 -B kaifa/kaifa_test/verify_windows_wechat_profile.py --project-root .
cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat:: --no-default-features
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features
git diff --check -- desktop/src-tauri/src/monitor.rs desktop/src-tauri/src/wechat desktop/src-tauri/Cargo.toml kaifa/kaifa_test
```

- `verify_windows_wechat_profile.py` 确认嵌入式 `windows-wechat-v1.json` 的 profile 集合为空；探针证据未取得时不能意外启用 profile，并静态检查 schema、ROI/SHA-256、错误码和无采集/OCR/模型边界。
- `cargo test ... wechat::` 当前通过 23/23，覆盖空 production catalog、合成冻结 profile 精确匹配、标题不能补救 exe/DPI 不符、ROI 非法拒绝和 HWND/边界变化的 `WX_REQUEST_STALE`。
- macOS 的 `cargo check ... --no-default-features` 通过。当前仅安装 `aarch64-apple-darwin` target，未取得 Windows 微信、冻结 exe 证据或目标 Windows UAT；production profile 因此保持空，Windows 发布验收仍为 blocked。
- `cargo fmt --manifest-path desktop/src-tauri/Cargo.toml --check` 未成功运行：`stable-aarch64-apple-darwin` 未安装 `cargo-fmt`/`rustfmt` component；未安装工具链以避免改变开发环境。

## 临时微信截图裁剪与隐藏恢复（2026-08-06）

```bash
python3 -B kaifa/kaifa_test/verify_wechat_ephemeral_capture.py
cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat::capture::tests --no-default-features
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features
```

- 静态门禁只读取源码，检查内存帧、物理原点裁剪、无焦点恢复、worker join 和协调器接线；不启动应用、不访问网络或读取任何微信内容。
- Windows 真机的 GDI/WGC 成功、失败、超时、取消与窗口恢复仍待受控 Windows 11 环境验证；当前 macOS 结果不替代该验证。

## 建立 Windows OCR 专用后端和条件本地 OCR fallback（2026-08-06）

```bash
# 只读源码与空 production catalog 的静态边界检查；不启动应用、不访问网络或真实微信。
python3 -B kaifa/kaifa_test/verify_wechat_windows_ocr.py --project-root .

# 合成 RGBA 与契约单测；不调用 WinRT、模型、检索、文件 OCR 或外部进程。
cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat:: --no-default-features

# 当前主机的非 Windows 编译检查。
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features
```

- `verify_wechat_windows_ocr.py` 检查 `wechat/ocr.rs` 只消费 `chat_rgba`，并拒绝路径、进程、PowerShell、Paddle、StorageFile、HTTP、Tauri command 与文件 API；它还确认 `OcrService` 的新内存入口直接使用 WinRT `SoftwareBitmap`/`OcrEngine`，以及 production catalog 仍为空。
- Windows target 编译和真机 UAT 必须在受控 Windows 11 x64 上补做。当前 host 未安装 `x86_64-pc-windows-msvc` target，不能用 macOS 合成测试替代该证据；未取得冻结 probe 前，fallback 保持 Disabled。

## 单请求运行时阶段追踪和内容留存（2026-08-06）

```bash
# 只读源码与虚构 fixture 的静态隐私/隔离门禁；不启动应用、访问网络、读取微信或创建内容文件。
python3 -B kaifa/kaifa_test/verify_wechat_reply_runtime.py --project-root .

# 定向 Rust 单测和本机编译检查；单测仅在系统临时目录创建虚构 metadata/content。
cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat:: --no-default-features
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features
```

- `verify_wechat_reply_runtime.py` 检查 single-flight runtime、metadata-only JSONL trace、独立 content root 和两个只读/删除 command 的源码边界；它拒绝 trace/content 依赖上传、数据库、知识库、localhost API 或截图服务。
- `list_wechat_reply_traces` 仅查询脱敏 trace DTO；`delete_wechat_reply_content` 仅删除受管理的 UUID content 请求目录。二者不是生成、OCR、检索、模型或上传入口。

## 无工具单轮模型 transport 与微信专用客户端（2026-08-06）

```bash
# 只读源码的静态边界检查；不启动 Tauri、不访问网络、真实微信、模型或知识库。
python3 -B kaifa/kaifa_test/verify_wechat_model_transport.py

# 纯 Rust 定向测试与 feature 编译；测试使用虚构模型配置和 fake transport，不发 HTTP 请求。
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'agent::model::tests' --no-default-features
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'wechat::config::tests' --no-default-features
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'wechat::model_client::tests' --no-default-features
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'wechat::runtime::reply_runtime_tests' --no-default-features
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m1'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'
```

- `verify_wechat_model_transport.py` 检查共享 transport、普通 command 委托、微信私有 client、精确 profile resolver 与 runtime 提交 helper；并拒绝 client 中出现 Tauri command 或单轮请求体出现工具字段。脚本只读取源码。
- Rust 测试验证四 provider 的单轮 body 只有 system + user 且没有工具字段，拒绝工具/截断/空白结果；微信 profile 必须精确、已测试成功且模型非空；fake transport 只收到一条固定 system + user 请求；运行时未计数时不能提交回复。

## M1 手动微信回复后端编排闭环（2026-08-06）

以下命令只验证步骤 11 的私有 M1 编排和虚构 fixture；不启动 Tauri、不访问真实微信、模型、知识库、数据库或网络。

```bash
python3 -B kaifa/kaifa_test/verify_wechat_reply_flow.py --project-root .
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'wechat::reply_flow::tests' --no-default-features --features 'wechat-contract-check,wechat-m1'
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'wechat::runtime::reply_runtime_tests' --no-default-features --features 'wechat-contract-check,wechat-m1'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m1'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'
rustfmt --edition 2021 --check desktop/src-tauri/src/wechat/runtime.rs desktop/src-tauri/src/wechat/reply_flow.rs
git diff --check -- desktop/src-tauri/src/wechat/mod.rs desktop/src-tauri/src/wechat/runtime.rs desktop/src-tauri/src/wechat/reply_flow.rs kaifa/kaifa_test/verify_wechat_reply_flow.py
```

- `verify_wechat_reply_flow.py` 仅读取本步骤源码，确认 M1-only module gate、lease request id 贯穿 capture、capture-version 原子写入 OCR trace、OCR-only M1 input、stale-capture gate 和 runtime-owned model call；同时拒绝 command、M2/RAG、气泡、剪贴板、输入控制和 HTTP 依赖。
- Rust 定向测试只使用系统临时目录 trace 与 fake transport；不创建真实截图或请求。两个 feature 编译分别确认 M1 含私有 reply flow，而 M2 不引用它。
