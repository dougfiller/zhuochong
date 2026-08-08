# Windows AI 桌宠微信回复助手

本仓库以 Work Review 1.1.0 为桌面产品底座，在保留本地工作记录、时间线、日报、工作助手和桌宠能力的同时，开发 Windows 微信单聊的手动截图、本机 OCR、聊天知识库检索与回复建议流程。

回复功能只生成可审阅、可复制的建议；粘贴、修改和发送始终由用户完成。项目不允许自动监听微信、自动输入、自动粘贴或自动发送。

> 当前状态：正式 M2 发布门禁为 `blocked`。Windows 微信兼容性档案仍为空，正式签名、实机证据、性能证据、素材授权和 updater 信任链尚未齐备。本仓库当前是开发与验收基线，不应作为已可分发的正式版本。

## 目标流程

```text
用户明确触发
  -> 微信窗口截图与聊天区裁剪
  -> 本机 OCR
  -> knowledge_retrieve() 本地检索
  -> 调用用户明确配置的单一回复模型
  -> 桌宠气泡展示建议
  -> 用户点击复制并自行发送
```

从 M2 开始，准备调用回复模型前必须先完成 `knowledge_retrieve()`。检索未就绪、范围不明确或检索失败时必须结束请求，不能降级到无知识上下文的 M1 路径。

## 仓库结构

```text
desktop/                 可运行的 Tauri 2 + Rust + Svelte 桌面应用
  src/                   Svelte 前端、路由、组件和多语言资源
  src-tauri/             Tauri 后端、微信流程、本地 OCR 与知识库
  crates/                core、MCP server、skills engine Rust crates
  docs/                  用户文档、设计、基线、发布和回滚资料
  scripts/               图标、截图、平台安装和发布辅助脚本
  learning/              可选 Python 学习示例，不是产品运行时
kaifa/                   正式需求、实施计划、日志和验收门禁
scripts/                 根级调度与探针工具
参考/                    只读上游参考，不是构建输入
人物角色参考/            只读视觉参考，未经授权不得进入发行物
```

正式产品入口位于 `desktop/`：

- Web：`desktop/index.html` -> `desktop/src/main.js` -> `desktop/src/App.svelte`
- Tauri：`desktop/src-tauri/src/main.rs`
- Rust workspace：`desktop/Cargo.toml`

## 环境要求

- Windows 11 x64 是微信功能的目标开发与验收平台。
- 推荐 Node.js 22；上游最低说明为 Node.js 18+。
- Rust stable；当前 CI/回归基线使用 Rust 1.97.1。
- Windows 桌面构建需要可用的 MSVC Rust 工具链、C++ 构建工具和 WebView2。
- Python 3.10+ 仅用于部分验收脚本和可选学习示例，不是桌面应用运行时依赖。

完整依赖来源和锁定规则见 [DEPENDENCIES.md](DEPENDENCIES.md)。

## 安装与运行

所有 npm 和普通 Cargo 命令都从 `desktop/` 执行：

```powershell
Set-Location desktop
npm ci
npm run tauri:dev
```

常用命令：

```powershell
npm run dev          # 仅启动 Web 前端，不能代替完整 Tauri 功能
npm run build        # 构建前端产物
npm run preview      # 预览前端产物
npm run tauri:build  # 构建桌面安装包
```

普通 `tauri:dev` 和 `tauri:build` 默认不启用 `wechat-m1` 或 `wechat-m2` Cargo feature。涉及微信里程碑的构建和测试必须显式选择与门禁一致的 feature，不能把默认构建当作 M2 验收。

## 环境变量与本地配置

[.env.example](.env.example) 是项目使用到的可选环境变量目录，包含 MCP Server、本地 OCR、文档截图、教学示例和显式本地知识探针的配置。

当前项目没有统一的根级 dotenv 自动加载器；仅复制为 `.env` 不保证 Rust、Node 脚本或 Python 进程会自动读取。请通过当前 shell、IDE、任务运行器或服务配置显式注入所需变量。应用本身的模型提供商和 API Key 应优先在应用设置中配置，不要写入仓库文件。

`WORK_REVIEW_DB_PATH` 和 `WORK_REVIEW_CONFIG_PATH` 仅供独立 MCP Server 使用，不是桌面应用的数据目录配置。

## 测试与质量门禁

在 `desktop/` 执行常规检查：

```powershell
node --test
npm run build
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

微信 M2 Rust 测试示例：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml 'wechat::' `
  --no-default-features `
  --features 'wechat-contract-check,wechat-m2' `
  -- --test-threads=1
```

从仓库根运行发布门禁单元测试：

```powershell
python3 -B -m unittest kaifa/kaifa_test/test_verify_final_m2_release_gate.py
```

证据门禁的退出结果有严格含义：`0 = pass`、`1 = fail`、`2 = blocked`。缺少真实证据时必须保持 `blocked`，不得用 synthetic fixture 冒充正式证据。

## 隐私与安全边界

- 原始微信导出只读保留；只有派生的本地 `knowledge.sqlite` 可以按规则重建。
- 默认不持久化聊天区截图、OCR 正文、知识命中正文或回复正文。
- 不提交数据库、截图、聊天记录、API Key、token、签名材料或真实用户证据。
- 微信链路不得调用 MCP、Bot、Localhost API、远程上传、联网搜索或通用行动工具。
- 视觉参考和第三方素材必须先取得逐文件、可审计的授权，才能进入发行物。

## 当前限制

- `desktop/src-tauri/src/wechat/profiles/windows-wechat-v1.json` 尚无经过实机冻结的兼容性 profile。
- [正式发布门禁](desktop/docs/release/final/final-release-after-gate.md) 当前结论为 `blocked`，release workflow 也保持禁用。
- `desktop/.github/workflows/` 是从桌面子项目保留的工作流目录；在当前仓库布局下，GitHub 不会把嵌套目录识别为根级 Actions 工作流。
- 当前全量 `node --test` 存在一项既有的 i18n 对齐失败：`zh-TW` 缺少 `knowledgeScope.*` 键。不要把当前基线描述为全量测试已通过。
- 发布门禁单测会在子进程中调用名为 `python3` 的可执行文件；仅提供 `python.exe` 或 `py.exe` 的 Windows 环境需要先配置兼容命令。
- 微信兼容性只计划覆盖经实机验证的 Windows 微信 4.0.x 具体小版本、主题、DPI、窗口尺寸和显示器布局，不应泛化宣称支持全部版本。

## 详细文档

- [正式产品需求基线](kaifa/最终需求文档.md)
- [桌宠助手开发方案](kaifa/桌宠助手快速开发方案.md)
- [逐步骤实现计划](kaifa/桌宠助手逐步骤编程实现计划.md)
- [Work Review 简体中文说明](desktop/README.zh.md)
- [Windows OCR 说明](desktop/docs/WINDOWS_OCR.md)
- [M2 发布重建与回滚](desktop/docs/runbooks/m2-release-rebuild-rollback.md)
- [许可证](desktop/LICENSE) 与 [第三方声明](desktop/THIRD_PARTY_NOTICES.md)
