# Work Review 修改前继承矩阵

基线 ID：`work-review-v1.1.0-before-wechat-rag-20260805`。此矩阵只描述未改造的 `desktop/`；`before` 是不可覆盖的修改前证据，所有 `after` 均留给后续步骤。`blocked`、`fail` 与 `not-run` 都保持发布门禁关闭，不能因文档已建立而视为能力通过。

## BASE-01

- **ac_base**: AC-BASE-01
- **capability**: 启动、单实例、托盘、自启动与退出
- **entry_and_core_files**: `src-tauri/src/main.rs`; `src-tauri/src/autostart.rs`; `src/routes/settings/components/SettingsGeneral.svelte`
- **data_dependencies**: 配置文件与 Windows 托盘/自启动状态；不收集真实用户数据
- **before_method**: Windows 11 x64 冷启动、二次启动、托盘显示/恢复、自启动开关与退出
- **before_evidence**: 无；目标 Windows 实机未提供
- **before_status**: blocked
- **before_reason**: 当前执行机是 macOS，不能以源码或 macOS 结果替代 Windows 实机行为
- **after_method**: 同一 Windows 场景与脱敏事件顺序
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: none
- **wechat_rag_risk**: 不得新增第二实例、托盘、自启动或窗口生命周期
- **release_blocker**: yes
- **known_issue**: none

## BASE-02

- **ac_base**: AC-BASE-02
- **capability**: 录制暂停/恢复、前台窗口、标题、URL、时长、分类与空闲
- **entry_and_core_files**: `src-tauri/src/monitor.rs`; `idle_detector.rs`; `commands/recording.rs`; `commands/category.rs`
- **data_dependencies**: Work Review 数据库与无敏感本地测试应用
- **before_method**: Windows 两个本地应用切换、暂停/恢复、空闲等待并核对时间线
- **before_evidence**: 无；目标 Windows 实机未提供
- **before_status**: blocked
- **before_reason**: 需要 Windows 前台窗口与输入空闲观测
- **after_method**: 相同场景并比较记录字段
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: none
- **wechat_rag_risk**: 微信只能隔离读取前台窗口，不能破坏既有采样状态
- **release_blocker**: yes
- **known_issue**: none

## BASE-03

- **ac_base**: AC-BASE-03
- **capability**: 普通截图、OCR、隐私忽略与脱敏
- **entry_and_core_files**: `src-tauri/src/screenshot.rs`; `ocr.rs`; `storage.rs`
- **data_dependencies**: 无敏感 fixture、截图目录与隐私规则
- **before_method**: Windows 测试应用验证截图/OCR/排除规则，仅记录计数与路径类型
- **before_evidence**: 无；目标 Windows 与安全 fixture 未提供
- **before_status**: blocked
- **before_reason**: 不收集真实屏幕或 OCR 正文，需受控 Windows fixture
- **after_method**: 相同受控 fixture 与脱敏摘要
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: none
- **wechat_rag_risk**: 微信裁剪与隐藏恢复不得扩大普通截图数据面
- **release_blocker**: yes
- **known_issue**: none

## BASE-04

- **ac_base**: AC-BASE-04
- **capability**: 概览、时间线、详情、小时汇总与分类读取
- **entry_and_core_files**: `src/routes/Overview.svelte`; `src/routes/timeline/`; `commands/timeline.rs`; `commands/stats.rs`
- **data_dependencies**: 固定脱敏记录或本地产生的脱敏数据
- **before_method**: Windows 中核对列表、详情、汇总、分类与刷新
- **before_evidence**: 无；受控 Windows 数据集未执行
- **before_status**: blocked
- **before_reason**: 正式目标环境和脱敏 fixture 验收尚未提供
- **after_method**: 同一数据集与页面操作
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: none
- **wechat_rag_risk**: 新增微信记录不得污染 Work Review 活动或统计模型
- **release_blocker**: yes
- **known_issue**: none

## BASE-05

- **ac_base**: AC-BASE-05
- **capability**: 日报、周报、历史与 Markdown 导出
- **entry_and_core_files**: `src/routes/report/`; `commands/report.rs`; `crates/core/src/analysis/`
- **data_dependencies**: 脱敏 fixture 与临时导出目录
- **before_method**: Windows 生成日/周报、历史浏览并检查导出结构
- **before_evidence**: 无；受控 Windows fixture 未执行
- **before_status**: blocked
- **before_reason**: 不以 macOS 构建替代 Windows 导出与历史验收
- **after_method**: 使用同一脱敏 fixture
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: none
- **wechat_rag_risk**: 微信建议不得进入既有报告、导出或上传内容
- **release_blocker**: yes
- **known_issue**: none

## BASE-06

- **ac_base**: AC-BASE-06
- **capability**: 工作助手、模型设置与已有屏幕语义记忆
- **entry_and_core_files**: `src/routes/ask/`; `SettingsAI.svelte`; `commands/ai.rs`; `commands/semantic_memory.rs`
- **data_dependencies**: 配置、既有语义记忆表与 mock 错误路径
- **before_method**: 无真实模型时验证保存、mock/error 与既有读写边界
- **before_evidence**: 无；无凭据契约尚未执行
- **before_status**: not-run
- **before_reason**: 未授权使用真实模型，mock/错误路径仍待专门受控执行
- **after_method**: 同一 mock/错误路径
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: mock-contract
- **wechat_rag_risk**: `knowledge.sqlite` 必须独立，禁止污染已有 memory 表或工具型 Agent
- **release_blocker**: yes
- **known_issue**: none

## BASE-07

- **ac_base**: AC-BASE-07
- **capability**: 设置保存、多语言、数据目录、存储统计与清理
- **entry_and_core_files**: `src/routes/settings/`; `commands/config.rs`; `storage.rs`; `crates/core/src/config.rs`
- **data_dependencies**: 临时数据目录、配置与脱敏统计
- **before_method**: Windows 修改、重启、恢复设置，切换语言并验证统计/清理
- **before_evidence**: 无；目标 Windows 临时目录验收未执行
- **before_status**: blocked
- **before_reason**: Windows 重启与数据目录行为尚未在目标环境验证
- **after_method**: 相同临时目录与恢复步骤
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: none
- **wechat_rag_risk**: 微信隐私、留存与知识库配置不得改变默认工作记录配置
- **release_blocker**: yes
- **known_issue**: none

## BASE-08

- **ac_base**: AC-BASE-08
- **capability**: 桌宠、普通气泡、提醒、位置、拖动、穿透、负坐标与多显示器
- **entry_and_core_files**: `avatar_engine.rs`; `avatar_input.rs`; `src/routes/avatar/AvatarWindow.svelte`; `src/lib/components/Avatar/`
- **data_dependencies**: 窗口位置配置与 Windows 单/多显示器拓扑
- **before_method**: Windows 单屏与多屏验证显示、拖动、穿透、持久化、提醒和负坐标
- **before_evidence**: 无；目标 Windows 多显示器环境未提供
- **before_status**: blocked
- **before_reason**: 需要 Windows 显示器拓扑与 WebView2 实机观测
- **after_method**: 复用同一拓扑与脱敏截图说明
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: none
- **wechat_rag_risk**: 微信只能复用同一桌宠/气泡路径，不得覆盖原提醒
- **release_blocker**: yes
- **known_issue**: none

## BASE-09

- **ac_base**: AC-BASE-09
- **capability**: S3/WebDAV、远程上传、MCP、Bot、Localhost API 与联网搜索入口默认隔离
- **entry_and_core_files**: `remote_upload.rs`; `localhost_api.rs`; `commands/integration.rs`; `bot_common.rs`; `agent/`; `crates/mcp-server/`
- **data_dependencies**: 配置 schema、队列状态与脱敏错误码
- **before_method**: 无凭据验证默认关闭、配置 schema、不入队与明确错误
- **before_evidence**: 无；无凭据隔离契约未执行
- **before_status**: not-run
- **before_reason**: 不发真实请求；待在受控环境执行 schema/mock/错误路径
- **after_method**: 同一默认隔离和错误路径
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: mock-contract
- **wechat_rag_risk**: 微信回复链路必须零调用这些外发能力
- **release_blocker**: conditional
- **known_issue**: none

## BASE-10

- **ac_base**: AC-BASE-10
- **capability**: 无凭据网络能力的 mock/契约、队列与错误路径
- **entry_and_core_files**: `remote_upload.rs`; `localhost_api.rs`; `bot_common.rs`; 相应 Node/Rust 测试
- **data_dependencies**: mock、队列状态、脱敏错误摘要
- **before_method**: 列出每项 mock/契约命令、失败码、队列状态和脱敏规则
- **before_evidence**: 无；无凭据契约清单尚未实测
- **before_status**: not-run
- **before_reason**: 未授权外发，且受控 mock/队列验收待后续环境准备
- **after_method**: 同一 mock/契约清单
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: mock-contract
- **wechat_rag_risk**: 后续必须证明微信/RAG 未隐式启用或复用外发链路
- **release_blocker**: conditional
- **known_issue**: none

## BASE-AUTO-FE

- **ac_base**: supporting
- **capability**: 原生前端 Node 测试
- **entry_and_core_files**: `*.test.js`; `package.json`
- **data_dependencies**: 锁文件与 npm 依赖
- **before_method**: `node --test`
- **before_evidence**: `evidence/BASE-AUTO-FE-summary.txt`
- **before_status**: pass
- **before_reason**: 479/479 通过
- **after_method**: 同一命令
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: none
- **wechat_rag_risk**: 后续前端改动必须复跑
- **release_blocker**: yes
- **known_issue**: none

## BASE-AUTO-BUILD

- **ac_base**: supporting
- **capability**: 原生前端生产构建
- **entry_and_core_files**: `package.json`; `vite.config.*`; `src/`
- **data_dependencies**: 锁文件与 npm 依赖
- **before_method**: `npm run build`
- **before_evidence**: `evidence/BASE-AUTO-BUILD-summary.txt`
- **before_status**: pass
- **before_reason**: Vite 5.4.21 已完成 240 modules 转换
- **after_method**: 同一命令
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: none
- **wechat_rag_risk**: 后续前端改动必须复跑
- **release_blocker**: yes
- **known_issue**: none

## BASE-AUTO-RUST-CHECK

- **ac_base**: supporting
- **capability**: Rust workspace 编译检查
- **entry_and_core_files**: `Cargo.toml`; `src-tauri/`; `crates/`
- **data_dependencies**: `Cargo.lock` 与 Cargo 缓存
- **before_method**: `cargo check --workspace --all-targets --quiet`
- **before_evidence**: `evidence/BASE-AUTO-RUST-CHECK-summary.txt`
- **before_status**: pass
- **before_reason**: 命令退出码为 0
- **after_method**: 同一命令
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: none
- **wechat_rag_risk**: 后续 Rust 改动必须复跑
- **release_blocker**: yes
- **known_issue**: none

## BASE-AUTO-RUST-CLIPPY

- **ac_base**: supporting
- **capability**: Rust workspace clippy 门禁
- **entry_and_core_files**: `Cargo.toml`; `src-tauri/`; `crates/`
- **data_dependencies**: `Cargo.lock` 与 Cargo 缓存
- **before_method**: `cargo clippy --workspace --all-targets -- -D warnings`
- **before_evidence**: `evidence/BASE-AUTO-RUST-CLIPPY-summary.txt`
- **before_status**: pass
- **before_reason**: 命令退出码为 0；保留上游 block v0.1.6 future-incompat 警告
- **after_method**: 同一命令
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: none
- **wechat_rag_risk**: 后续 Rust 改动必须复跑
- **release_blocker**: yes
- **known_issue**: none

## BASE-AUTO-RUST-TEST

- **ac_base**: supporting
- **capability**: Rust workspace 测试
- **entry_and_core_files**: `src-tauri/src/commands/system.rs`; `Cargo.toml`; `crates/`
- **data_dependencies**: `Cargo.lock` 与测试 fixture
- **before_method**: `cargo test --workspace --quiet`
- **before_evidence**: `evidence/BASE-AUTO-RUST-TEST-summary.txt`
- **before_status**: fail
- **before_reason**: 372 通过、1 失败；失败 ID 见 `UPSTREAM-RUST-001`
- **after_method**: 同一命令；不得跳过失败测试
- **after_evidence**: 无
- **after_status**: not-run
- **credential_mode**: none
- **wechat_rag_risk**: 后续不得把该上游失败归因于微信/RAG 代码
- **release_blocker**: yes
- **known_issue**: UPSTREAM-RUST-001
