# 依赖声明

本项目包含 Node.js、Rust 和可选 Python 三个依赖域，无法用一个语言生态的清单完整表达。下列机器可读清单和锁文件共同构成项目的依赖声明；本文件负责说明它们的作用，不复制一份容易漂移的包列表。

| 依赖域 | 声明文件 | 锁文件 | 用途 |
| --- | --- | --- | --- |
| Svelte/Vite/Tauri CLI | `desktop/package.json` | `desktop/package-lock.json` | 前端、构建、测试和 Tauri CLI |
| Rust workspace | `desktop/Cargo.toml` 与各成员的 `Cargo.toml` | `desktop/Cargo.lock` | 桌面后端、共享核心、MCP Server、skills engine |
| Python（可选） | `desktop/learning/requirements.txt` | 无 | 教学原型；正式产品和 `kaifa/kaifa_test` 不依赖这些包 |

## 工具链基线

- Node.js：22 推荐，项目说明最低为 18+。
- npm：使用 Node.js 配套版本，并以 `package-lock.json` 为准执行 `npm ci`。
- Rust：stable；当前自动化与回归基线使用 1.97.1。
- Python：3.10+，只用于根级验收工具和可选教学原型；当前发布门禁单测还要求可执行命令名 `python3`。

操作系统级依赖（MSVC/C++ 构建工具、WebView2、Linux/macOS 的 Tauri 原生库、可选 OCR 运行时）不由 npm、Cargo 或 pip 清单安装，必须按目标平台单独准备。

Windows OCR 优先使用系统 API。仅在启用本地 PaddleOCR fallback 时，外部 Python 环境还需要 `paddlepaddle` 和兼容的 `paddleocr` 3.x；这组平台相关依赖当前没有仓库级锁文件，安装与验证方式见 `desktop/docs/WINDOWS_OCR.md`，不应把它误当作默认桌面运行时依赖。

基础模板模式不要求外部 AI 服务。Ollama、LM Studio、云端模型、Bot、对象存储和其他集成都属于可选运行时服务，其端点与凭据由应用 UI/本地 `config.json` 管理，不属于 `.env.example` 中的主应用配置，也不得提交到仓库。

## 安装

主应用依赖：

```powershell
Set-Location desktop
npm ci
cargo fetch --locked
```

仅在运行 `desktop/learning/` 教学示例时安装 Python 包：

```powershell
python -m pip install -r desktop/learning/requirements.txt
```

`kaifa/kaifa_test/` 的验收器只使用 Python 标准库，不需要执行上述 pip 安装。

## 更新规则

- Node 包变更必须同时提交 `desktop/package.json` 和 `desktop/package-lock.json`。
- Rust 包变更必须修改对应 `Cargo.toml`，并只提交由该变更产生的 `desktop/Cargo.lock` 更新。
- Python 教学依赖只写入 `desktop/learning/requirements.txt`，不要把它们提升为桌面应用运行时依赖。
- 不手工编辑锁文件，不把 `node_modules/`、`.venv/`、`dist/` 或 `target/` 纳入版本控制。
- 引入第三方代码、模型或素材时，除版本和完整性外，还必须登记许可证与发行授权；现有声明见 `desktop/THIRD_PARTY_NOTICES.md`。
