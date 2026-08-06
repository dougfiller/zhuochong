## 冻结 Work Review 来源并建立正式产品源码副本（2026-08-05）

检测脚本：`kaifa/kaifa_test/verify_work_review_baseline.py`

该脚本不启动产品、不安装依赖；它验证步骤 1 的复制工件。`create` 会先以官方固定 commit 的 `git archive` 对本地参考进行内容级比对，只有比对一致才在临时同级目录完成复制与检测后原子创建 `desktop/`；`verify` 可以在后续重复执行。

| 验收项 | 命令/方法 | 实际结果 |
| --- | --- | --- |
| 脚本接口 | `python3 -B kaifa/kaifa_test/verify_work_review_baseline.py --help` | 通过；提供 `create`、`verify`、来源和目标参数。 |
| 固定上游身份 | 以 `git clone --depth 1 --branch v1.1.0` 检出官方仓库，验证 tag、commit 和 tree | 通过；`v1.1.0` 解析为 `500f9d2cb3027392cfcc32ad18395dfe348fb4a1`，tree 为 `ae807b665fe55e609dd7b81f25d4214ef9e9eae9`。 |
| 本地参考与官方归档 | 以官方 `git archive` 比较 `参考/Work-Review-main/` | 通过；已仅删除三个 README 中偏离固定提交的额外说明，580 文件、44,961,912 字节逐项一致。 |
| 正向来源证明 | 用当前参考执行 `create` 与 `verify` | 通过；创建和两次在线复核均报告固定 commit `500f9d2cb3027392cfcc32ad18395dfe348fb4a1` 与 tree `ae807b665fe55e609dd7b81f25d4214ef9e9eae9`。 |
| 前端门禁 | `npm ci && node --test && npm run build` | 通过；479/479 测试通过，Vite 生产构建成功。 |
| Rust 编译与 lint | `cargo check --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings` | 通过；仅报告上游依赖 `block v0.1.6` 的 future-incompat 警告。 |
| Rust workspace 测试 | `cargo test --workspace` | 失败；372 通过、1 失败：`commands::system::tests::图标解析应忽略与app_name矛盾的executable_path` 断言实际首位图标为 `None`。该失败位于冻结上游业务代码，不在本次来源修复范围。 |
| 当前 `desktop/` 状态 | 复核 manifest 与官方归档 | 已验证为可信上游冻结基线；测试生成的 `node_modules/` 和 `dist/` 已移除，基线恢复为 580 个受控文件。 |

本次来源门禁已解除；但 Rust workspace 测试尚非全绿，后续如要解除完整发布门禁，应以独立任务诊断并修复上述上游测试失败，不能把它掩盖为来源冻结问题。

## Work Review 修改前回归基线与继承矩阵（2026-08-05）

检测脚本：`kaifa/kaifa_test/verify_work_review_regression_baseline.py`。脚本只读取 `desktop/` 的冻结 manifest、矩阵、结果 JSON 和脱敏摘要；不启动应用、不安装依赖、不联网，也不会读取聊天导出或真实截图。

| 验收项 | 命令/方法 | 实际结果 |
| --- | --- | --- |
| 自动化前端基线 | `cd desktop && node --test` | 通过；479/479。摘要见 `BASE-AUTO-FE-summary.txt`。 |
| 自动化前端构建 | `cd desktop && npm run build` | 通过；Vite 5.4.21，240 个模块转换完成。 |
| Rust 编译 | `cd desktop && cargo check --workspace --all-targets --quiet` | 通过。 |
| Rust lint | `cd desktop && cargo clippy --workspace --all-targets -- -D warnings` | 通过；保留上游依赖 `block v0.1.6` 的 future-incompat 警告，不误报零警告。 |
| Rust workspace 测试 | `cd desktop && cargo test --workspace --quiet` | 失败；372 通过、1 失败，固定登记为 `UPSTREAM-RUST-001`，未修改或跳过该测试。 |
| 基线工件一致性 | `python3 -B kaifa/kaifa_test/verify_work_review_regression_baseline.py --project-root .` | 应通过；校验 10 条 AC-BASE、5 条自动化支撑行、冻结 580 文件、摘要 SHA-256 和既有失败归因。 |
| 冻结来源复核 | `python3 -B kaifa/kaifa_test/verify_work_review_baseline.py verify --source '参考/Work-Review-main' --destination desktop` | 应通过本地快照；只有网络可用时才会重新验证官方归档。 |

Windows 11 x64 + WebView2 的 BASE-01--05、07--08 记录为 `blocked`，而无凭据模型/网络契约的 BASE-06、09--10 记录为 `not-run`；这些不是通过结果。后续在受控 Windows 机器上只能补同一行的 `before` 证据，任何微信/RAG 改动后只能补 `after` 证据。

## 品牌、应用标识和发行安全下限适配（2026-08-06）

本步骤没有新增独立检测脚本；在 `desktop/` 既有 Node 测试中扩展了 updater、打包配置和 Release workflow 的静态门禁。命令只读取源码/配置或编译 updater 单元测试，不访问更新 endpoint，也不创建发行物。

| 验收项 | 命令/方法 | 实际结果 |
| --- | --- | --- |
| 更新与发行安全门禁 | `cd desktop && node --test src/UpdaterFlow.test.js src/lib/utils/updater.test.js src/ReleasePackaging.test.js releaseWorkflow.test.js` | 13/13 通过；确认 updater 明确 disabled、原生插件依赖/注册/capability 均不存在、Tauri 不生成 updater artifacts、CSP 不含上游域名、Release workflow 不会发布。 |
| Rust updater 单元测试 | `cd desktop && npm ci && npm run build && cargo test --manifest-path src-tauri/Cargo.toml commands::updater::tests --quiet` | 前端构建通过；Rust 测试 1 通过、0 失败、370 过滤。禁用响应不提供 release URL，且不可用作自动安装。 |
| 配置与残留地址扫描 | `node -e "JSON.parse(require('node:fs').readFileSync('desktop/src-tauri/tauri.conf.json'))"`；针对 updater 源码、Tauri 配置和 Release workflow 搜索上游 endpoint/代理 manifest | 通过；JSON 合法，目标范围未发现上游 Release API、代理或 `updater.json`。 |
| 全量前端测试（环境限制） | `cd desktop && node --test` | 未通过，非本次代码失败：445 通过、5 失败；当前未安装依赖导致缺少 `playwright`、`svelte` 与 `vite`。未执行安装以免改变工作区依赖状态。 |
| Rust 格式检查（环境限制） | `rustfmt --check desktop/src-tauri/src/commands/updater.rs` | 未运行；当前 `stable-aarch64-apple-darwin` toolchain 未安装 `rustfmt` component。 |

## 冻结微信知识库状态机和验收契约（2026-08-06）

本步骤未新增独立检测脚本，检测资产是 `desktop/src-tauri/src/wechat/` 与 `knowledge/` 的 Rust 单元测试，以及 `tests/fixtures/wechat_contract/` 下的手写脱敏 JSON fixture。测试不会读取、复制、哈希或提交真实聊天数据，也不会创建/打开知识库数据库、发送模型请求或启动应用。

| 验收项 | 命令/方法 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| 微信类型、OCR 边界、fixture 与状态机 | `cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat:: --no-default-features` | OCR 仅 `Text` 且单聊可形成 M1 输入；群聊拒绝，非法迁移、序号倒退、旧 request/binding/observation 均拒绝 | 11/11 通过 | 通过 |
| 知识 scope、检索结果与 token budget | `cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge:: --no-default-features` | 三种 scope wire 形状固定、空多选范围拒绝、`no_hit` 为成功封装、命中摘录不超过检索结果携带的 token budget | 3/3 通过 | 通过 |
| 检索失败零调用与 `no_hit` 放行 | 同上 `wechat::` 定向测试 | `retrieval_failed.json` 驱动 `KB_RETRIEVAL_FAILED` 进入 `Failed` 且 fake transport 为 0；只有 `no_hit` 空命中可进入生成 | 已包含在 11/11 通过 | 通过 |
| M1 编译门禁 | `cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m1'` | 仅 M1 入口可编译 | 编译成功 | 通过 |
| M2 编译门禁 | `cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'` | 仅 RAG 入口可编译 | 编译成功 | 通过 |
| 无 release feature（预期失败） | `cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features wechat-contract-check` | `compile_error!` 指明必须二选一 | 退出码 101，命中预期信息 | 通过 |
| 双 release feature（预期失败） | `cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m1,wechat-m2'` | `compile_error!` 拒绝双启用 | 退出码 101，命中预期信息且无其他编译错误 | 通过 |
| 私有构造（预期失败） | `cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2,wechat-contract-probe-private-constructors'` | 非受信任 sibling module 不能构造 `RetrievedReply` 或 `ModelKnowledgeContext` | 退出码 101，两个类型均因私有字段构造失败 | 通过 |
| M1 引用 RAG 入口（预期失败） | `cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m1,wechat-contract-probe-m1-rag'` | M1 target 无 RAG 入口 | 退出码 101，`generate_rag_reply` 因 `wechat-m2` 未启用而不可解析 | 通过 |
| M2 引用 M1 入口（预期失败） | `cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2,wechat-contract-probe-m2-m1'` | M2 target 无 M1 入口 | 退出码 101，`generate_m1_reply` 因 `wechat-m1` 未启用而不可解析 | 通过 |
| Rust 格式检查 | `cargo fmt --manifest-path desktop/src-tauri/Cargo.toml --check` | 仅检查格式 | 未运行：当前 `stable-aarch64-apple-darwin` 未安装 `cargo-fmt`/`rustfmt` component | 环境限制 |

`normal_m1.json`、`empty_ocr.json`、`group_chat.json`、`duplicate_message.json`、`unsupported_schema.json`、`ambiguous_conversations.json`、`no_hit.json` 与 `retrieval_failed.json` 仅为契约样例。后续实现必须保持它们脱敏；群聊 fixture 只可表达用户明确选择的知识范围，不扩展为实时群聊回复承诺。

## 建立最小微信/知识库模块骨架并接入现有配置（2026-08-06）

检测脚本：`kaifa/kaifa_test/verify_wechat_knowledge_skeleton.py`。该脚本为纯静态检查，不会启动应用、访问网络、读取真实微信/聊天/知识库内容，或创建 `knowledge.sqlite`。

| 验收项 | 命令/方法 | 实际结果 |
| --- | --- | --- |
| 空 runtime 与 command 接线 | `python3 -B kaifa/kaifa_test/verify_wechat_knowledge_skeleton.py` | 通过；三项独立 managed state、三个安全 DTO command、空实现错误码、loopback validator 和 config normalize 均存在。 |
| 未知 profile fail-closed | `cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat::commands::tests::unknown_profile_fails_closed --no-default-features` | 通过；`WX_PROFILE_UNSUPPORTED` 且 auto trigger 为 false。 |
| 非 loopback embedding 拒绝 | `cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge::config::tests::only_exact_loopback_http_endpoints_are_accepted --no-default-features` | 通过；DNS、云、LAN、`0.0.0.0` 和 https 均拒绝，无网络调用。 |
| 旧 config/default/normalize | `cargo test --manifest-path desktop/Cargo.toml -p work-review-core config --quiet` | 通过；45 项通过，覆盖旧字段缺失、空 recent dir、留存和安全范围归一化。 |
| 设置页生产构建 | `cd desktop && npm run build` | 通过；Vite 5.4.21 转换 242 个模块并完成生产构建。 |
| Rust 格式检查 | `cargo fmt --manifest-path desktop/Cargo.toml --check` | 环境限制；当前 `stable-aarch64-apple-darwin` 未安装 `cargo-fmt`/`rustfmt` component，未将此检查伪报为通过。 |

## 前台微信识别和版本化布局兼容性档案（2026-08-06）

检测脚本：`kaifa/kaifa_test/verify_windows_wechat_profile.py`。脚本只读取受版本控制的 catalog 与 Rust 源码；不会启动应用、访问网络、读取真实微信、聊天正文、数据库或截图。

| 验收项 | 命令/方法 | 实际结果 |
| --- | --- | --- |
| 空 production catalog | `python3 -B kaifa/kaifa_test/verify_windows_wechat_profile.py --project-root .` | 通过；schema 1 的 `windows-wechat-v1.json` 无 enabled profile，静态检查确认严格 schema、ROI/SHA-256、稳定错误码和无采集/OCR/模型边界。 |
| profile/identity 契约 | `cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat:: --no-default-features` | 通过；23/23。新增用例覆盖精确匹配、标题不能放行、非法/disabled profile 拒绝、最小化/几何失败和二次读取 stale。 |
| 非 Windows 编译 | `cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features` | 通过；仅既存 dead-code 警告与 `block v0.1.6` future-incompat 提示。 |
| diff 空白检查 | `git diff --check -- desktop/src-tauri/src/monitor.rs desktop/src-tauri/src/wechat desktop/src-tauri/Cargo.toml kaifa/kaifa_test` | 通过。 |
| Windows 编译与实机 probe/UAT | 目标 Windows 11 x64 + 精确微信版本、主题、DPI 和拓扑 | blocked；当前主机只有 `aarch64-apple-darwin` target，未取得真实路径/哈希/ProductVersion/ROI/probe 报告。production catalog 故意为空。 |
| Rust 格式检查 | `cargo fmt --manifest-path desktop/src-tauri/Cargo.toml --check` | 环境限制；未安装 `cargo-fmt`/`rustfmt` component。 |

## 临时微信截图裁剪与隐藏恢复（2026-08-06）

检测脚本：`kaifa/kaifa_test/verify_wechat_ephemeral_capture.py`。脚本只检查源码；单元测试使用合成 RGBA 帧，不启动 Tauri、不访问网络、不读取微信、聊天、数据库或真实截图。

| 验收项 | 命令/方法 | 实际结果 | 结果 |
| --- | --- | --- | --- |
| 私有无落盘边界 | `python3 -B kaifa/kaifa_test/verify_wechat_ephemeral_capture.py` | 检查内存 frame、原点平移、guard restore、worker join、coordinator 接线；拒绝 focus/unminimize/reveal/PNG API | 通过 |
| 负坐标与越界裁剪 | `cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat::capture::tests --no-default-features` | 两项合成测试：副屏负原点先平移；帧外窗口与非法 ROI fail-closed | 通过 |
| 当前平台编译 | `cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features` | macOS 编译通过；保留既有 dead-code 与 `block v0.1.6` future-incompat 提示 | 通过 |
| Windows 实机路径 | Windows 11 x64 + 受支持微信 profile 的成功/失败/超时/取消测试 | 用户决定本 run 不执行 Windows 实机测试；此项不提供运行时通过证据 | not-run（用户豁免） |

## 建立 Windows OCR 专用后端和条件本地 OCR fallback（2026-08-06）

检测脚本：`kaifa/kaifa_test/verify_wechat_windows_ocr.py`。该脚本只读取源码与版本控制 catalog；Rust 单测只使用合成 RGBA/虚构 profile，不启动 Tauri、不访问网络、真实微信、模型、检索、文件 OCR 或外部进程。

| 验收项 | 命令/方法 | 实际结果 | 结果 |
| --- | --- | --- | --- |
| 私有内存 OCR 边界 | `python3 -B kaifa/kaifa_test/verify_wechat_windows_ocr.py --project-root .` | 通过；检查仅 `chat_rgba`、WinRT `SoftwareBitmap`/`OcrEngine` 内存入口、结果规范化、空 production catalog 和禁止路径/PowerShell/Paddle/StorageFile/HTTP/文件 API。 | 通过 |
| dispatcher、规范化和 fallback 门禁 | `cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat:: --no-default-features` | 38/38 通过；覆盖控制字符/空白/长度拒绝、Empty/Unavailable/Failed 终止、零 fallback、准确审计才一次 fallback，以及 OCR 失败不能进入 retrieval/generation。 | 通过 |
| 当前平台编译 | `cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features` | 通过；仅现有/隔离模块的 dead-code 警告与 `block v0.1.6` future-incompat 提示。 | 通过 |
| diff 空白检查 | `git diff --check -- desktop/src-tauri/src/ocr.rs desktop/src-tauri/src/wechat desktop/src-tauri/Cargo.toml kaifa/kaifa_test` | 通过。 | 通过 |
| Windows target 编译与 WinRT/UAT | `cargo check --target x86_64-pc-windows-msvc ...`；Windows 11 x64 受控 probe | blocked；本机仅安装 `aarch64-apple-darwin`，目标检查因缺 `x86_64-pc-windows-msvc` target 失败；未执行 Windows 真机或真实微信。 | blocked |
| Rust 格式检查 | `cargo fmt --manifest-path desktop/src-tauri/Cargo.toml --check` | 环境限制；`stable-aarch64-apple-darwin` 未安装 `cargo-fmt`/`rustfmt` component。 | 未运行 |
