# 2026-08-08：步骤 27 全量性能、业务 UAT、签名发布与阶段感知回滚

```bash
# 正式门禁：当前外部 freeze 与 Windows 同批证据缺失，预期 exit 2/blocked。
python3 -B kaifa/kaifa_test/verify_final_m2_release_gate.py \
  --project-root . \
  --freeze desktop/docs/release/final/release-freeze-v1.json \
  --manifest desktop/docs/release/final/final-release-after-gate.json

# verifier 正反行为测试：raw sample 重算、晚批准阈值、M1/无 feature/双 feature、
# hash/path/symlink、题集缺项、签名、素材、runtime 与 rollback 目标。
python3 -B -m unittest kaifa/kaifa_test/test_verify_final_m2_release_gate.py

# M2 编排与知识回归。knowledge 测试会在系统分配的 127.0.0.1 临时端口启动
# 手写 fake embedding server；若沙箱报 Operation not permitted，须在获准的本机
# loopback 环境按原命令重跑，不能把沙箱失败写成产品失败。
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'wechat::' \
  --no-default-features --features 'wechat-contract-check,wechat-m2' -- --test-threads=1
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'knowledge::' \
  --no-default-features --features 'wechat-contract-check,wechat-m2' -- --test-threads=1

# recovery bundle 所复用的 core config/database 与正式 M2 build feature 检查。
cargo test --manifest-path desktop/Cargo.toml -p work-review-core config
cargo test --manifest-path desktop/Cargo.toml -p work-review-core database
cargo check --manifest-path desktop/src-tauri/Cargo.toml \
  --no-default-features --features 'custom-protocol,wechat-m2'

# 步骤 25/26 兼容门禁、前端回归、生产构建、语法、格式和范围空白。
python3 -B kaifa/kaifa_test/verify_m2_observability_gate.py \
  --project-root . --evidence desktop/docs/baselines/work-review-m2-after-gate.json
python3 -B kaifa/kaifa_test/verify_task25_m2_rag.py --project-root .
(cd desktop && node --test)
(cd desktop && npm run build)
PYTHONPYCACHEPREFIX=/tmp/aich8-task27-pycache python3 -m py_compile \
  kaifa/kaifa_test/verify_final_m2_release_gate.py \
  kaifa/kaifa_test/test_verify_final_m2_release_gate.py
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml -- --check
git diff --check -- desktop/src-tauri/src/commands/config.rs \
  desktop/src-tauri/src/knowledge/commands.rs desktop/src-tauri/src/knowledge/store.rs \
  desktop/src-tauri/src/wechat/types.rs desktop/docs/release/final desktop/docs/uat \
  desktop/docs/runbooks desktop/scripts/release kaifa/kaifa_test/verify_final_m2_release_gate.py \
  kaifa/kaifa_test/test_verify_final_m2_release_gate.py \
  kaifa/kaifa_test/fixtures/final_release kaifa/kaifa_personnel/mingling.md \
  kaifa/kaifa_test/test.md kaifa/kaifa_log
```

Windows 采集脚本仅在冻结环境由发行负责人手工运行：

```powershell
desktop\scripts\release\collect-final-evidence.ps1 `
  -ReleaseBatchId '<已冻结批次>' `
  -CandidatePath '<同批签名候选绝对路径>' `
  -MetadataInput '<无正文 metadata JSON>'
```

- PowerShell collector 会自行打开 Windows 原生目录选择器，并只接受隔离的非 junction 证据目录；它只读取明确文件、计算 candidate hash、读取 Authenticode 公有状态并封装递归校验后的 metadata，不做微信 UIA 输入、键鼠模拟、粘贴/发送、数据库/协议读取、网络上传或自动发布。
- 正式 gate 退出码固定为 `0=pass`、`1=fail`、`2=blocked`。当前必须是 2；只有用户/发行负责人冻结全部输入并完成同批 Windows 证据后才可能变为 0。
- `createUpdaterArtifacts=false`、空 production 微信 profile 和 fixed-fail release workflow 本步骤保持不变；上述命令不启用 updater 或发布。

## 大型 messages[] 流式导入、规范化和媒体引用（2026-08-07）

```bash
# 只读取本 run 的源码与 migration；不读取聊天导出或媒体正文。
python3 -B kaifa/kaifa_test/verify_streaming_message_import.py

# 在 Tauri crate 中运行知识模块的定向单元测试。
(cd desktop/src-tauri && cargo test knowledge --bin work-review)
```

- `verify_streaming_message_import.py` 是静态边界门禁：确认 `messages[]` visitor、v3 normalization/media schema、generation input key 和媒体 `exists_state='unknown'` 均存在，且不引入媒体打开、读取或元数据探测路径。
- Rust 测试仅使用手写 fixture 和系统临时目录的 SQLite；不启动 Tauri、不访问网络，也不会对真实微信或导出包做任何操作。

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

## 桌宠微信建议复制/关闭与代际校验（2026-08-06）

- `request_wechat_suggestion_copy(input)`：仅在 runtime 的 `requestId + suggestionGeneration + bindingGeneration` 与当前展示建议完全一致时返回正文；该命令不写剪贴板。
- `confirm_wechat_suggestion_copy(input)` 与 `dismiss_wechat_suggestion(input)`：仅在同一三元组仍有效时清除建议，并向既有 `avatar` 窗口发送受限的微信建议失效事件。没有新增窗口、快捷键、自动化、Rust clipboard 或输入/发送能力。
- 本步骤没有新增可执行脚本。前端针对性测试使用仓库既有命令 `cd desktop && node --test src/lib/components/Avatar/avatarWindow.test.js src/lib/components/Avatar/avatarOutline.test.js`；该项目没有 `npm test` script。

## 显式触发微信回复、设置隐私与安全错误反馈（2026-08-06）

```bash
# 只读源码边界检查；不启动应用、不访问网络、微信、模型或用户内容。
python3 -B kaifa/kaifa_test/verify_wechat_explicit_trigger.py --project-root .

# 前端定向测试与生产构建。
cd desktop && node --test src/lib/utils/errorDisplay.test.js src/routes/avatar/WechatExplicitTrigger.test.js src/lib/components/Avatar/avatarWindow.test.js src/routes/settings/SettingsWechatKnowledge.test.js
cd desktop && npm run build

# 仅运行本步骤相关的 Rust 单测与 M1/M2 feature 编译门禁。
cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat::commands --no-default-features --features 'wechat-contract-check,wechat-m1'
cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat::reply_flow --no-default-features --features 'wechat-contract-check,wechat-m1'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m1'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'
```

- `verify_wechat_explicit_trigger.py` 是新增的纯静态门禁：确认唯一无输入 command、私有 publish/emit/finish 顺序、3 秒可取消倒计时、白名单错误格式化，且拒绝前端输入 DTO 与 clipboard Rust API；不会读取用户内容或创建文件。
- Rust command 只在 `wechat-m1` 时调用私有流程；M2 或未启用 M1 时固定返回 `WX_WINDOW_UNSUPPORTED`，不会降级为 M2 或通用 Agent。
- 这些检查不替代受控 Windows 11 的真实微信/profile/模型纵向验收；当前 macOS 不产生 Windows 真机成功证据。

## M1 自动化、Windows 实机和 Work Review 回归门禁（2026-08-06）

```bash
# 只读取脱敏 after-gate JSON；不启动产品、微信、模型或网络。
python3 -B kaifa/kaifa_test/verify_m1_release_gate.py --project-root .

# 使用纯虚构 fixture 验证 pass、缺证据/哈希不一致 blocked、禁止能力 fail。
python3 -B kaifa/kaifa_test/verify_m1_release_gate.py --input kaifa/kaifa_test/fixtures/m1_gate/pass.json
python3 -B kaifa/kaifa_test/verify_m1_release_gate.py --input kaifa/kaifa_test/fixtures/m1_gate/blocked-missing-evidence.json
python3 -B kaifa/kaifa_test/verify_m1_release_gate.py --input kaifa/kaifa_test/fixtures/m1_gate/blocked-hash-mismatch.json
python3 -B kaifa/kaifa_test/verify_m1_release_gate.py --input kaifa/kaifa_test/fixtures/m1_gate/fail-capability.json
```

- `verify_m1_release_gate.py` 的退出码为：`0=pass`、`1=fail`、`2=blocked`。它只接受同一 candidate commit、NSIS SHA-256 和 batch ID 关联的命令、Windows、能力计数和素材台账证据。
- 当前正式 after-gate 工件故意返回 `blocked`：没有受控 Windows 11 x64/冻结 profile/NSIS candidate 同批证据，且素材台账仍有 `pending-verification`。不得把已有 macOS 静态或 fake 测试写成 Windows 通过。
- 如用户明确授权，after-gate JSON 可在 `default_pass_requirements` 中声明 `candidate_nsis`、`windows`、`assets`、`automated`、`after_matrix`、`capability_counters` 为默认通过。该例外只对显式声明它的 after-gate 文档生效；未声明的 fixture 仍按严格规则校验。

## 微信 JSON 导出包只读导入契约（2026-08-06）

```bash
# 仅读取本步骤源码、配置和完全虚构 fixture；不启动 Tauri，不访问网络、微信或用户聊天数据。
python3 -B kaifa/kaifa_test/verify_wechat_json_archive.py --project-root .

# 定向 Rust 单测与 M2 feature 编译，不读取真实导出包。
cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge::archive --no-default-features --features 'wechat-contract-check,wechat-m2'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'
```

- `verify_wechat_json_archive.py` 是新增的只读静态门禁：检查精确 v1 schema、手写脱敏 fixture 的消息类型覆盖、导入 guard 的流式入口、派生 dataDir SQLite、私有源 ignore 和空 Tauri resources；不会创建文件或读取真实导出目录。
- Rust 定向测试只使用系统临时 `data_dir` 和 `acct_fixture_01` 等虚构 fixture 标识，验证 unknown schema/path traversal/media 请求 fail-closed、selected 不升级 full、派生库只写 dataDir、完全相同成员 metadata fast verify 不重开 messages 流。

## 独立 knowledge.sqlite、版本化 migration 和单一 Store（2026-08-06）

```bash
# 仅读取本步骤源码与 migration 资源，不创建或打开用户数据库。
python3 -B kaifa/kaifa_test/verify_knowledge_store.py --project-root .

# 只运行独立知识库、archive DTO 与 fail-closed 的 Rust 单测。
cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge:: --no-default-features

# 检查 M2 feature 编译；不会启动 Tauri、访问网络、微信、模型或用户数据。
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'

# 仅检查本步骤文件的 Rust 格式和空白错误。
rustfmt --edition 2021 --check desktop/src-tauri/src/knowledge/{mod,runtime,migrations,store,archive_store,archive_importer}.rs desktop/src-tauri/src/main.rs
git diff --check -- desktop/src-tauri/src/knowledge desktop/src-tauri/src/main.rs kaifa/kaifa_test/verify_knowledge_store.py
```

- `verify_knowledge_store.py` 是纯静态门禁：检查独立 migration 资源、Store 的唯一连接入口、读写 API、WAL/外键/FTS5/版本验证、active/candidate/denial 边界、导入器没有 `Connection`，以及启动时以现有 `data_dir` 管理 Store。它不读取或创建任何真实聊天数据。
- Rust 测试只在系统临时目录创建虚构 SQLite 文件，覆盖新库、candidate 不可见直到原子 activate、未来版本 fail-closed、archive 导入只读与脱敏；不会访问 Work Review `workreview.db`。
# 2026-08-06：步骤 17 源 lineage 与不可变消息版本

- `python3 -B kaifa/kaifa_test/verify_knowledge_lineage_generations.py --project-root .`：只读静态门禁，检查 knowledge schema v2、流式 staging、不可变版本与原子 activation 的必要源码边界；不读取用户导出或数据库。

# 2026-08-07：步骤 19 知识源管理与安全重建 UI

- `python3 -B kaifa/kaifa_test/verify_knowledge_source_management.py`：只读静态门禁，核对知识源 Store 门面、维护状态、Tauri command、单次目录选择与截短 opaque ID 渲染；不读取用户 dataDir、真实导出或网络。
- `node --test desktop/src/routes/settings/SettingsWechatKnowledge.test.js`：检查知识设置使用 camelCase command payload，目录选择与来源清单不回写旧 `config.knowledge.knowledgeSources`。
- `cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'`：编译知识源 command/Store 与现有 archive 边界，不启动应用或访问真实微信数据。

# 2026-08-07：步骤 20 冻结候选索引代际、会话内分块与隔离 FTS5

```bash
# 仅读取本步骤源码和 migration 的静态边界门禁；不打开用户数据库、导出包或网络。
python3 -B kaifa/kaifa_test/verify_knowledge_candidate_chunks_fts.py --project-root .

# 使用系统临时目录和完全虚构消息运行 bundled rusqlite 行为回归。
cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge:: --no-default-features --features 'wechat-contract-check,wechat-m2'

# 编译 M2 feature 组合，不启动 Tauri、不连接模型或读取真实微信数据。
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'

# 复验步骤 16、17、19 的静态知识库边界。
python3 -B kaifa/kaifa_test/verify_knowledge_store.py --project-root .
python3 -B kaifa/kaifa_test/verify_knowledge_lineage_generations.py --project-root .
python3 -B kaifa/kaifa_test/verify_knowledge_source_management.py --project-root .

# 仅检查本步骤 Rust 文件格式和范围内空白。
rustfmt --edition 2021 --check desktop/src-tauri/src/knowledge/mod.rs desktop/src-tauri/src/knowledge/migrations.rs desktop/src-tauri/src/knowledge/store.rs desktop/src-tauri/src/knowledge/chunk.rs
git diff --check -- desktop/src-tauri/src/knowledge/mod.rs desktop/src-tauri/src/knowledge/migrations.rs desktop/src-tauri/src/knowledge/store.rs desktop/src-tauri/src/knowledge/chunk.rs desktop/src-tauri/src/knowledge/migrations/knowledge/0004_candidate_index_chunks_fts.sql kaifa/kaifa_test/verify_knowledge_candidate_chunks_fts.py kaifa/kaifa_test/verify_knowledge_lineage_generations.py kaifa/kaifa_personnel/mingling.md kaifa/kaifa_test/test.md kaifa/kaifa_log/2026年08月07日16时12分-冻结候选索引代际并实现会话内分块与隔离FTS5.md
```

- 新静态脚本确认 v4 migration、纯分块模块、building-only 生产 builder、catalog/ready/stable-scope/time/topK SQL 过滤和无 `LIKE` 回退；脚本不会执行数据库或读取用户内容。
- Rust 行为测试使用 `rusqlite = 0.30` 的 bundled SQLite/FTS5，固定版本为 `chunk-v1`、`token-counter-v1`、`fts-pretoken-v1`；macOS 结果不替代 Windows 真机验收。

# 2026-08-07：步骤 21 复用嵌入向量 RRF 并强制本地 Loopback

```bash
# 纯静态边界门禁；仅读源码，不打开数据库、不读取正文、不访问网络。
python3 -B kaifa/kaifa_test/verify_knowledge_embedding_loopback.py --project-root .

# 共享 f32-LE、严格解码、归一化、流式 Top-K 与 RRF 纯算法。
cargo test --manifest-path desktop/crates/core/Cargo.toml semantic::

# endpoint、DNS pin、redirect、timeout/unavailable 与稳定 chunk_key RRF。
# HTTP 行为测试只绑定系统分配的本机 loopback 临时端口，不访问外网。
cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge::embedding::tests:: --no-default-features --features 'wechat-contract-check,wechat-m2'

# frozen building/active、NULL 续作与严格 BLOB 行为。
cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge::store::tests::embedding_store_is_frozen_resumable_and_exact_blob_fail_closed --no-default-features --features 'wechat-contract-check,wechat-m2'

# 知识库回归、M2 编译和屏幕语义记忆兼容回归。
cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge:: --no-default-features --features 'wechat-contract-check,wechat-m2'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'
cargo test --manifest-path desktop/src-tauri/Cargo.toml commands::semantic_memory::tests:: --no-default-features --features 'wechat-contract-check,wechat-m2'
node --test desktop/src/SemanticMemoryIntegration.test.js

# 真实中文质量探针：必须由用户显式提供本地 endpoint/model；没有参数时输出 not-run。
AICH8_KNOWLEDGE_PROBE_ENDPOINT='<显式本地http-loopback地址>' AICH8_KNOWLEDGE_PROBE_MODEL='<本地模型精确名称>' cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge::embedding::tests::real_chinese_quality_probe --no-default-features --features 'wechat-contract-check,wechat-m2' -- --ignored --nocapture
```

- 质量探针复用生产 validator/client/parser；只输出短 digest、dimension、batch sizes、p50/p95、Recall@5 和 verdict，不输出正文、向量或完整 endpoint。
- 新静态门禁不替代 Rust HTTP/SQLite 行为测试。

# 2026-08-07：步骤 22 校验候选索引、原子切换 Catalog 并冻结性能门禁

```bash
# 只读源码静态边界：检查 schema v5、候选校验、BEGIN IMMEDIATE、catalog CAS、完整 active 组合与 denial 不清空 catalog。
python3 -B kaifa/kaifa_test/verify_knowledge_candidate_activation.py --project-root .

# 正式性能门禁验证器。用户已明确授权全部 Windows 相关性能要求默认通过；当前工件输出 authorized-defaults 且退出 0。
# 该政策通过不等于 Windows 实测：evidenceStatus 保持 not_run_user_waived，不能写成真实测量证据。
python3 -B kaifa/kaifa_test/verify_knowledge_performance_gate.py --project-root . --gate desktop/docs/performance/knowledge-performance-gate-v1.json

# migration、候选校验/事务激活、denial 查询和 loopback 编排的完整知识模块回归。
# 嵌入测试只绑定系统分配的 127.0.0.1 临时端口，不访问外网。
cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge:: --no-default-features --features 'wechat-contract-check,wechat-m2'

# M2 feature 编译及既有步骤 16/17/19/20/21 静态边界回归。
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'
python3 -B kaifa/kaifa_test/verify_knowledge_store.py --project-root .
python3 -B kaifa/kaifa_test/verify_knowledge_lineage_generations.py --project-root .
python3 -B kaifa/kaifa_test/verify_knowledge_source_management.py --project-root .
python3 -B kaifa/kaifa_test/verify_knowledge_candidate_chunks_fts.py --project-root .
python3 -B kaifa/kaifa_test/verify_knowledge_embedding_loopback.py --project-root .

# 仅检查本步骤 Rust 文件格式和范围内空白。
rustfmt --edition 2021 --check desktop/src-tauri/src/knowledge/migrations.rs desktop/src-tauri/src/knowledge/store.rs desktop/src-tauri/src/knowledge/embedding.rs
git diff --check -- desktop/src-tauri/src/knowledge desktop/docs/performance kaifa/kaifa_test kaifa/kaifa_personnel/mingling.md kaifa/kaifa_log
```

- 两个新增 Python 脚本都不会打开用户 `knowledge.sqlite`、微信导出或网络；activation gate 仅读源码，performance gate 仅读指定 JSON 与其中显式列出的仓库内脱敏 evidence 文件。
- 当前 `knowledge-performance-gate-v1.json` 仅在完整列出五项 Windows 性能要求、保存用户原话/授权时间/授权范围，且 `evidenceStatus=not_run_user_waived` 时允许政策默认通过；验证器输出 `pass mode=authorized-defaults factualEvidence=not-run-user-waived` 并退出 0。
- 删除授权、只声明部分默认项、扩大范围、伪造 evidence 状态或保留 blockers 都会 fail-closed。真实 Windows/冻结样本/阈值/观测/原始证据仍未运行，不能把该政策例外表述为实测通过。

# 2026-08-07：步骤 23 唯一 knowledge_retrieve 混合检索门面

```bash
# 步骤 23 纯静态边界门禁；仅读取 knowledge Rust 源码，不打开数据库、导出包或网络。
python3 -B kaifa/kaifa_test/verify_knowledge_retrieval_facade.py

# 门面行为测试：使用系统临时目录中的完全虚构 knowledge.sqlite。
# hybrid/fallback 用例只绑定系统分配的 127.0.0.1 临时端口，不访问外网。
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'knowledge::retrieve::tests' --no-default-features --features 'wechat-contract-check,wechat-m2'

# 完整 knowledge 回归；真实中文质量 probe 仍需显式本地模型，因此保持 ignored/not-run。
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'knowledge::' --no-default-features --features 'wechat-contract-check,wechat-m2'

# M2 feature 编译，不启动应用、微信或模型。
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'

# 预期失败的隐私编译探针；必须因 RetrievedReply/ModelKnowledgeContext 私有字段而 exit 101。
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2,wechat-contract-probe-private-constructors'

# 范围内格式与空白。
rustfmt --edition 2021 --check desktop/src-tauri/src/knowledge/{mod,types,retrieve,store,embedding}.rs
git diff --check -- desktop/src-tauri/src/knowledge kaifa/kaifa_test/verify_knowledge_retrieval_facade.py kaifa/kaifa_personnel/mingling.md kaifa/kaifa_test/test.md kaifa/kaifa_log
```

- 静态脚本输出 `KNOWLEDGE_RETRIEVAL_FACADE_GATE status=passed scope=static-boundary`，只确认唯一业务定义、Store 授权 SQL/token、版本化 canonical hash、三种 scope/error wire、私有 success/no-hit 构造器和 IDs/scores-only trace 存在；脚本不替代 Rust 行为测试，也不再输出易被误解为行为覆盖数的 `checks=37`。
- Rust loopback 用例在默认沙箱内会因临时监听端口受限而报 `Operation not permitted`；应在获准环境原命令重跑，不能把权限失败记成产品失败。
- retire/deny/delete 必须在同一写事务中递增冻结授权 epoch；payload 正文、message range、source paths 和末次授权校验必须在同一 SQLite read transaction，返回前还要用新 reader 重验 catalog/authorization token。
- `frozen_result_hash` 的 `knowledge-retrieval-result-v1` schema 覆盖请求策略、status/mode、命中顺序与全部确定性 hit 字段；`elapsed_ms` 明确排除，以保持同结果重试稳定。
- `MIN_VECTOR_SCORE_V1=0.20` 是版本化保守默认，仅有合成行为证据；真实中文 Recall@5 与 Windows 性能仍为 not-run，不能写成质量通过。

# 2026-08-07：步骤 24 会话范围绑定与代际失效

```bash
# 步骤 24 纯静态安全门禁；仅读取本次 Rust/Svelte 源码，不打开用户数据库、微信导出、截图或网络。
python3 -B kaifa/kaifa_test/verify_knowledge_scope_binding.py --project-root .

# 进程内 binding 的 nonce、代际、窗口/header 变化、溢出与一次性确认行为。
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'wechat::binding::tests' --no-default-features --features 'wechat-contract-check,wechat-m2'

# 微信 runtime/profile/header/capture 与知识 Store/retrieval 回归。
cargo test --manifest-path desktop/src-tauri/Cargo.toml wechat:: --no-default-features --features 'wechat-contract-check,wechat-m2'
cargo test --manifest-path desktop/src-tauri/Cargo.toml knowledge:: --no-default-features --features 'wechat-contract-check,wechat-m2' -- --test-threads=1

# knowledge 测试中的虚构 embedding fixture 只绑定系统分配的 127.0.0.1 临时端口，不访问外网；
# 当前仓库的若干 fault-injection hook 是进程全局状态，因此完整 knowledge 回归使用单线程隔离。
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'knowledge::retrieve::tests' --no-default-features --features 'wechat-contract-check,wechat-m2' -- --test-threads=1

# M2 feature 编译，不启动 Tauri、微信或模型。
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'

# 两个既有 UI 表面的范围选择器、显式触发与安全错误展示。
(cd desktop && node --test src/routes/settings/SettingsWechatKnowledge.test.js src/routes/avatar/WechatExplicitTrigger.test.js src/lib/components/Avatar/avatarWindow.test.js src/lib/utils/errorDisplay.test.js)
(cd desktop && npm run build)

# Rust 格式与本步骤范围空白。
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml -- --check
git diff --check -- desktop kaifa/kaifa_test kaifa/kaifa_personnel/mingling.md
```

- `verify_knowledge_scope_binding.py` 只验证唯一进程内 binding、opaque scope key、header-only 内存路径、single-chat profile 门禁、hint-only 配置、双 UI 三操作和禁止自动化能力等稳定源码边界；它不替代行为测试。
- 默认沙箱不允许 loopback bind；knowledge HTTP fixture 需在获准环境运行。真实 Windows 前台微信、冻结 production profile、header OCR 与窗口切换 UAT 仍为 `not-run`。

# 2026-08-07：步骤 25 最小 ModelKnowledgeContext 与强制 RAG M2

```bash
# 步骤 25 静态 fail-closed 门禁；仅读取本次 Rust/Svelte 源码，不打开用户数据库、微信导出或网络。
python3 -B kaifa/kaifa_test/verify_task25_m2_rag.py

# M2 微信合约、运行时、上下文裁减、transport spy 和 trace/source receipt 行为回归。
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'wechat::' --no-default-features --features 'wechat-contract-check,wechat-m2' --no-fail-fast

# 检索门面回归；仅使用完全虚构数据和系统分配的 127.0.0.1 临时端口，不访问外网。
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'knowledge::retrieve::tests' --no-default-features --features 'wechat-contract-check,wechat-m2'

# M2/M1 分别编译；不启动 Tauri、微信或模型。
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m1'

# 以下四个是预期 exit 101 的编译负向探针：M2 不可触达 M1、私有构造不可绕过、release 必须且只能选一个 feature。
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2,wechat-contract-probe-m2-m1'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2,wechat-contract-probe-private-constructors'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m1,wechat-m2'

# 显式加载历史和点击查看来源的设置页定向回归及生产构建。
(cd desktop && node --test src/routes/settings/SettingsWechatKnowledge.test.js)
(cd desktop && npm run build)

# Rust 格式与本步骤范围空白。
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml -- --check
git diff --check -- desktop/src-tauri/src/knowledge desktop/src-tauri/src/wechat desktop/src-tauri/src/main.rs desktop/src/routes/settings desktop/src/lib/i18n/locales kaifa/kaifa_test/verify_task25_m2_rag.py kaifa/kaifa_personnel/mingling.md kaifa/kaifa_test/test.md kaifa/kaifa_log
```

- `verify_task25_m2_rag.py` 检查唯一受信任 `build_model_context`、实际 canonical payload 预算、尾部整 hit 裁减、M2 强制检索顺序、私有 permit、冻结重试、禁止 Agent/tools 和显式来源查看；静态通过不替代 Rust 行为测试。
- 真实 Windows 前台微信/OCR/窗口切换、真实模型网络请求、真实聊天数据、性能和发布均未运行；macOS/fake transport/SQLite 结果不替代这些证据。

# 2026-08-08：步骤 26 M2 可观测性、能力隔离与严格综合故障门禁

```bash
# 默认读取正式 after-gate；当前必须 exit 2/blocked，不能当测试失败或发布 pass。
python3 -B kaifa/kaifa_test/verify_m2_observability_gate.py --project-root . --evidence desktop/docs/baselines/work-review-m2-after-gate.json

# 完全虚构正向 fixture 必须 exit 0。
python3 -B kaifa/kaifa_test/verify_m2_observability_gate.py --project-root . --evidence kaifa/kaifa_test/fixtures/m2_observability/pass.json

# 以下负向 fixture 的预期退出码依次为 2、1、1、1、2。
python3 -B kaifa/kaifa_test/verify_m2_observability_gate.py --project-root . --evidence kaifa/kaifa_test/fixtures/m2_observability/blocked-missing-evidence.json
python3 -B kaifa/kaifa_test/verify_m2_observability_gate.py --project-root . --evidence kaifa/kaifa_test/fixtures/m2_observability/fail-capability.json
python3 -B kaifa/kaifa_test/verify_m2_observability_gate.py --project-root . --evidence kaifa/kaifa_test/fixtures/m2_observability/fail-default-pass.json
python3 -B kaifa/kaifa_test/verify_m2_observability_gate.py --project-root . --evidence kaifa/kaifa_test/fixtures/m2_observability/fail-hash-mismatch.json
python3 -B kaifa/kaifa_test/verify_m2_observability_gate.py --project-root . --evidence kaifa/kaifa_test/fixtures/m2_observability/blocked-missing-ac.json

# metadata schema、physical attempt、重试冻结、audit 写失败零 transport 与 M2 编排回归。
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'wechat::' --no-default-features --features 'wechat-contract-check,wechat-m2' -- --test-threads=1

# knowledge/embedding/fault/atomic 全回归。测试 HTTP 只绑定系统分配的本机 loopback 临时端口；
# 默认沙箱若报 Operation not permitted，须在获准的本机 loopback 环境原命令重跑。
cargo test --manifest-path desktop/src-tauri/Cargo.toml 'knowledge::' --no-default-features --features 'wechat-contract-check,wechat-m2' -- --test-threads=1

# M2/M1 feature 正向编译和步骤 25 静态边界兼容回归。
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m2'
cargo check --manifest-path desktop/src-tauri/Cargo.toml --no-default-features --features 'wechat-contract-check,wechat-m1'
python3 -B kaifa/kaifa_test/verify_task25_m2_rag.py

# 前端全量、生产构建、Python 语法、Rust 格式和本 run 范围空白。
(cd desktop && node --test)
(cd desktop && npm run build)
PYTHONPYCACHEPREFIX=/tmp/aich8-task26-pycache python3 -m py_compile kaifa/kaifa_test/verify_m2_observability_gate.py
cargo fmt --manifest-path desktop/src-tauri/Cargo.toml -- --check
git diff --check -- desktop/.github/workflows/ci.yml desktop/src-tauri/src/knowledge/embedding.rs desktop/src-tauri/src/knowledge/retrieve.rs desktop/src-tauri/src/wechat/mod.rs desktop/src-tauri/src/wechat/model_client.rs desktop/src-tauri/src/wechat/observability.rs desktop/src-tauri/src/wechat/reply_flow.rs desktop/src-tauri/src/wechat/runtime.rs desktop/src-tauri/src/wechat/types.rs desktop/docs/baselines/work-review-m2-after-gate.json desktop/docs/baselines/work-review-m2-after-gate.md kaifa/kaifa_test/verify_m2_observability_gate.py kaifa/kaifa_test/verify_task25_m2_rag.py kaifa/kaifa_test/fixtures/m2_observability kaifa/kaifa_personnel/mingling.md kaifa/kaifa_test/test.md kaifa/kaifa_log/2026年08月08日00时50分-完善M2可观测性能力隔离和严格故障门禁.md
```

- `verify_m2_observability_gate.py` 仅读指定脱敏 JSON 与固定源码边界，不启动产品、微信、模型、数据库、安装包或网络。退出码固定为 `0=pass`、`1=fail`、`2=blocked`。
- `request_evidence` 必须用同一 opaque UUID 串联 stageSeq=5 retrieval permit 和真实 physical attempts；attempt 只能 `[1]` 或 `[1,2]`，重试 bytes/context/model/binding 必须冻结一致。
- 正式 after-gate 当前保持 blocked：没有 Windows/profile、真实模型/embedding、NSIS/batch/package scan、完整 AC/fault/sentinel 和素材商业授权证据。禁止用 M1 历史 `default_pass_requirements` 绕过。
