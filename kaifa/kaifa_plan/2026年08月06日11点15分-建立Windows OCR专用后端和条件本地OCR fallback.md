# 步骤 8：建立 Windows OCR 专用后端和条件本地 OCR fallback

> 文档状态：实施前技术方案（仅方案设计）  
> dispatch：`aich8-zhuochong-desktop-pet-rag-27-steps-20260805-task-08`  
> LOOF run：`20260806-1110-步骤-8建立-windows-ocr-专用后端和条件本地-ocr-fallbackdispatc`  
> 前置基线：步骤 6 的 production catalog 仍为空，Windows 实机 profile/probe 未完成；步骤 7 已提供私有 `WechatCaptureSlices`（`chat_rgba` 与 `header_identity_rgba`）及无落盘 capture transaction。故本步骤的运行时入口必须保持 fail-closed，不能把合成测试、Umi GUI/服务或 `参考/` 目录当作可用 OCR 引擎。

## 0. 方案设计原则

- **唯一首选且复用来源明确**：微信 OCR 只经 `desktop/src-tauri/src/ocr.rs` 的 Work Review `OcrService` 新增 Windows 内存入口执行 WindowsOCR；普通 Work Review 仍保留既有路径式 `new(data_dir).extract_text(path)` 行为。微信模块不得自建第二套 OCR 应用、服务或命令包装器。
- **内存边界不可绕过**：输入只接受步骤 7 已裁好的 `WechatCaptureSlices.chat_rgba`；不接受路径、字节串、前端 payload、整屏帧或 header ROI。不得调用现有会创建 `paddle_ocr.py`、临时 `.ps1`、`StorageFile` 或 `Command` 的路径式实现。
- **默认拒绝**：只有有限、规范化的非空 `Text` 才能构造现有 `OcrReadyReply`。`Empty`、`Unavailable`、`Failed` 都写脱敏事件、令请求进入失败终态，并保证模型与检索 spy 均为零。
- **fallback 不是猜测性能力**：当前没有冻结的 Windows 真机失败 probe 或经审计的本地无界面引擎，故 fallback 编排为 `Disabled`。仅当同一受支持 profile 的冻结实机 probe 明确为 `Unavailable` 或 `Failed`，且人工审计记录了一个单一、本地、无界面、无网络/无落盘/无命令的实现，才可启用该一个 fallback。
- **不扩权**：不新增远程 OCR、网络、数据库、内容留存、任意命令、UI Automation、键鼠、剪贴板、微信注入/协议/数据库读取、粘贴/发送、第二 Tauri/WPF/GUI 应用壳或前端 IPC。

## 1. 现状、目标与完成条件

### 1.1 已核实的约束

1. `OcrService::new(data_dir)` 会检查/写入 `paddle_ocr.py`；`extract_text(&Path)` 的 Windows `WindowsOCR` 实现会生成临时 PowerShell 脚本、启动 `powershell.exe`，并通过图片文件路径创建 `StorageFile`。该路径适用于既有 Work Review 归档 OCR，**不满足**微信内存、无落盘、无命令边界。
2. `WechatCaptureSlices` 已是私有 Rust 类型，且同时含聊天区与仅供绑定复核的 header。步骤 8 只能读取 `chat_rgba`，不得把 header 送入 OCR、日志、模型或 fallback。
3. `OcrReadyReply::from_backend` 已把 `Text` 与 `Empty`/`Unavailable`/`Failed` 分开，并拒绝群聊；该不变量必须保留并加强规范化/长度限制。
4. `windows-wechat-v1.json` 没有 enabled profile，步骤 6 的 Windows UAT 被标为 blocked。因此本步骤只能交付可编译的私有实现与合成测试；不能写出“Windows OCR 已在微信上可用”的结论。

### 1.2 可验收目标

1. 受控微信路径的唯一 OCR 输入为一张内存 `RgbaImage` 聊天区图，且所有代码路径均不接收或生成图片/脚本文件、命令行、URL 或前端 OCR 参数。
2. 新的 `OcrService` Windows 专用内存入口使用 Windows 系统 OCR WinRT API 直接从内存 `SoftwareBitmap` 识别；它不触发 Paddle、PowerShell、Python 或现有路径式 retry/persist 行为。
3. 结果被规范化并设上限后才转为 `OcrBackendResult::Text`；超限、非法像素、API 不可用和内部异常均 fail-closed，不能形成 `OcrReadyReply`。
4. fallback 的启用条件在版本化、随源码冻结的审计记录中可重放；每次请求最多执行一次 primary 和一次获批 fallback，绝不在 `Empty` 时 fallback，绝不扫描 PATH、目录或服务端口寻找引擎。
5. 对 `Empty`、`Unavailable`、`Failed` 的单元测试均证明：稳定 `WX_OCR_*` 错误、脱敏事件、状态机失败、模型 spy=0、检索 spy=0。

## 2. 最小文件范围与职责

| 路径 | 动作 | 最小职责 |
| --- | --- | --- |
| `desktop/src-tauri/src/ocr.rs` | 修改 | 在既有 `OcrService` 下增加仅 Windows 编译的、仅内存的 `WindowsOCR` 适配；路径式 `extract_text` 与普通 Work Review 的 fallback 行为不改。 |
| `desktop/src-tauri/src/wechat/ocr.rs` | 新增 | 私有 dispatcher：限制聊天图输入、调用 primary、严格判定审计授权的一个 fallback、规范化结果、写脱敏事件；不含 Tauri command。 |
| `desktop/src-tauri/src/wechat/mod.rs` | 最小修改 | 声明私有 `ocr` 模块；不 re-export 给前端。 |
| `desktop/src-tauri/src/wechat/types.rs` | 最小修改 | 让 `OcrReadyReply` 仅接受已规范化、受限的内部 text token；保留所有既有 wire error 值。 |
| `desktop/src-tauri/src/wechat/profiles.rs`、`profiles/windows-wechat-v1.json` | 最小修改 | 为 profile 增加可选的、解析即严格验证的 OCR fallback 审计结构；当前嵌入 catalog 继续无 enabled profile、无 fallback。 |
| `desktop/src-tauri/Cargo.toml`、`desktop/Cargo.lock` | 条件修改 | 仅当现有传递依赖不能直接调用 WinRT projection 时，在 `cfg(windows)` 下声明已锁定的 `windows` crate 与最小 Media/OCR/Imaging/Streams feature；不引入 OCR 服务、Python 或 GUI 依赖。 |
| `kaifa/kaifa_test/verify_wechat_windows_ocr.py`、微信 Rust 单测 | 新增/修改 | 静态禁止项、dispatcher/规范化/fallback 门禁/spy 测试；不伪造 Windows UAT 成功。 |

不修改数据库 schema、配置页、Svelte、Tauri command 清单、普通截图归档和现有 Work Review OCR 路径；不把 fallback ID 放入用户可编辑配置。

## 3. 私有契约与数据流

```mermaid
sequenceDiagram
  participant C as "WechatCaptureSlices"
  participant D as "WechatOcrDispatcher"
  participant P as "OcrService WindowsOCR memory"
  participant A as "Frozen fallback audit"
  participant R as "OcrReadyReply"
  participant M as "Model/Retrieval"

  C->>D: "chat_rgba only"
  D->>D: "size/pixel limits + request validity"
  D->>P: "in-memory image"
  alt "Text after normalization"
    P-->>D: "Text"
    D->>R: "validated text token"
    R-->>M: "later authorized pipeline only"
  else "Unavailable or Failed"
    D->>A: "exact profile + frozen failed probe?"
    alt "one audited local fallback"
      A-->>D: "approved engine"
      D->>D: "run once, in memory"
    else "not approved"
      D-->>D: "redacted event; terminate"
    end
  else "Empty"
    D-->>D: "redacted event; terminate; no fallback"
  end
```

### 3.1 输入限制

`WechatOcrDispatcher::recognize(capture: &WechatCaptureSlices, identity: &WechatWindowIdentity, ...)` 只在后端私有流程中调用。它在读取像素前校验：request/capture version 仍当前、`chat_rgba` 宽高非零、像素 byte length 恰为 `width * height * 4`、总像素不超过一个编译期常量，以及所有乘法均用 checked arithmetic。任何不符都映射 `Failed`，不裁剪、不重采、不重试。

调用方不得传入 `header_identity_rgba`、`Path`、`Vec<u8>`、base64、URL、语言参数、引擎名、环境变量或超时值。语言使用 Windows 用户 profile 的系统选择；不新增“任意语言/模型/命令”配置面。

### 3.2 Windows primary

在 `OcrService` 中新增 crate-private Windows-only memory method（名称以实际实现为准，例如 `extract_windows_ocr_rgba`），它直接把 `RgbaImage` 转为 WinRT `SoftwareBitmap`，使用 `Windows.Media.Ocr.OcrEngine` 的用户语言引擎识别，再仅返回内部原始文字和必要置信/行数信息。

- 该 method 不调用 `OcrService::new`、`extract_text`、`execute_ocr_pipeline_with`、Paddle worker、`Command`、PowerShell、`StorageFile`、`temp_dir` 或任何文件 API。
- 不能创建 Windows OCR engine 时返回 `Unavailable`；解码/WinRT/识别错误及不可信返回返回 `Failed`。只有引擎正常完成而无有效文字时为 `Empty`。
- Rust WinRT 绑定的具体 `windows` feature 与异步完成 API 必须由 Windows target 实际编译确认；若当前锁定版本无法安全从内存构造 bitmap，本实现返回 `Unavailable`，不得回退到脚本或文件接口。
- 现有 `OcrService` 的公开/路径式 API 仅供普通 Work Review 使用；微信 dispatcher 不得因此改变其输出、落盘、重试或日志语义。

### 3.3 结果规范化与 reply 闸门

新建不可伪造的内部 `NormalizedOcrText`：将 CRLF/CR 规范为 LF、去除 NUL 与非换行控制字符、逐行 trim、折叠连续空行、再 trim 整体。设置编译期最大 Unicode scalar 数和最大 UTF-8 byte 数；超过任一上限一律为 `Failed`，不静默截断或把部分聊天正文交给模型。

`OcrReadyReply` 的构造器改为只接收 `NormalizedOcrText`，或让 dispatcher 在同一模块内持有唯一受信任构造通路。`Text` 且单聊才得到 `OcrReadyReply`；群聊仍是 `WX_GROUP_CHAT_UNSUPPORTED`。`Empty`、`Unavailable`、`Failed` 对应既有 `WX_OCR_EMPTY`、`WX_OCR_UNAVAILABLE`、`WX_OCR_FAILED`，并令 state machine 从 `Ocr` 转为 `Failed`，而非 `Generating`/`Retrieving`。

## 4. 条件 fallback 的冻结审计门

### 4.1 审计记录（非用户配置）

在受版本控制的 compatibility profile 内使用可选 `ocr_fallback_audit`。它至少绑定：`profile_id`、`profile_version`、primary `WindowsOCR`、probe 记录 SHA-256、probe Windows build/微信版本/主题/DPI/拓扑、probe outcome（仅 `unavailable` 或 `failed`）、单一 `fallback_id`、fallback 二进制/库 SHA-256、离线/无界面/无落盘/无命令审计结论和审核时间。解析时任一缺失、hash 格式错误、profile 不相等、outcome 为 `empty`/`text`、多个 fallback 或未知 ID 都使 fallback 为 `Disabled`，不会影响 primary 的 fail-closed 结果。

当前 catalog 维持空 profile，不能预填虚构 SHA、Umi、RapidOCR、PATH 命令或参考目录。真实 probe 到位前，代码中只存在 `Disabled` 与 mock 实现；不得把 mock、测试 fixture 或静态文件当作审计证据。

### 4.2 执行规则

1. primary 永远先跑且只跑一次。
2. primary `Text` 或 `Empty` 立即结束；`Empty` **不允许** fallback。
3. primary `Unavailable`/`Failed` 时，dispatcher 仅查询与当前 identity 精确匹配的冻结审计。无记录即以原错误结束。
4. 有记录时只调用其唯一编译进程内、无界面、本地 memory fallback 一次；该 fallback 不使用进程启动、socket/HTTP、文件、`参考/`、Umi GUI/服务或自动下载。fallback 结果按同一规范化规则处理，失败不再重试/切换。
5. fallback 的代码、版本或审计 hash 变化即视为未审计，自动 `Disabled`；回滚只删除/禁用 audit，立即恢复“primary 失败即终止”。

这不是把 Umi-OCR/RapidOCR 目录当引擎的授权。若未来无法提供满足该契约的、经审计的本地无界面实现，就永久不启用 fallback。

## 5. 脱敏事件、失败分流与边界

仅新增后端 `WechatOcrAuditEvent { request_id_hash, capture_version, stage, outcome, provider }`。`request_id_hash` 使用仅进程生命周期的 keyed/截断表示；事件不得含原文、图像、OCR boxes、标题、联系人、ROI、路径、PID、HWND、错误详情、Windows 用户信息或 fallback 参数。事件仅进入现有安全日志/测试 sink，不入数据库、不送前端。

| 分支 | event outcome | 后续行为 |
| --- | --- | --- |
| 输入越界/WinRT 异常/primary 或 fallback 失败 | `failed` | `WX_OCR_FAILED`，state=`Failed`，model spy=0，retrieval spy=0。 |
| 引擎/API 不可用，且无获批 fallback 或 fallback 不可用 | `unavailable` | `WX_OCR_UNAVAILABLE`，同上。 |
| primary 或 fallback 成功但规范化后为空 | `empty` | `WX_OCR_EMPTY`，同上，绝不 fallback。 |
| 合法 Text | `text`（不写字符数/内容） | 仅构造 `OcrReadyReply`；模型/RAG 仍由后续获授权步骤接管。 |

本步骤不调 `generate_m1_reply`、`generate_rag_reply`、`knowledge_retrieve()` 或 transport；测试中的 spy 是为了证明失败出口没有越权地调用这些下游接口，不是新增模型链路。

## 6. 测试、实机 probe 与发布门禁

| 场景 | 方法 | 通过条件 |
| --- | --- | --- |
| 静态私有边界 | 新增 `verify_wechat_windows_ocr.py` | 微信 OCR 模块无 `Path`/`Command`/PowerShell/Paddle/HTTP/Tauri command/StorageFile/文件写入；只消费 `chat_rgba`。 |
| 规范化与上限 | Rust 纯函数测试 | CRLF、控制字符、空行、NUL、空结果、超 byte/scalar、非法尺寸都得到预期结果；超限不截断。 |
| reply 只能来自 Text | `types`/dispatcher tests | `Text` 可构造；三种非 Text、群聊及未规范化 token 均不可构造。 |
| fallback 门禁 | fake primary/fallback + fake audit | 仅匹配的 `Unavailable`/`Failed` 且 audit 完整时调用一次；`Text`/`Empty`/audit 缺失/篡改/双 fallback 均为零次。 |
| 下游零调用 | fake model/retrieval spies | Empty/Unavailable/Failed 均断言 error、event、state=`Failed`、两个 spy=0。 |
| 普通 Work Review 回归 | 既有 OCR 单测与路径调用审计 | `extract_text(path)`、Paddle/PowerShell 旧路径不被微信改动；只按既有范围验证。 |
| Windows 编译与 primary UAT | 受控 Windows 11 x64 | 编译 Windows 分支；用非敏感测试聊天完成 `Text`、`Empty`、不可用、失败四类 probe，原始截图/文本不入仓。 |
| fallback UAT | 仅在先前 probe 为 Unavailable/Failed 后 | 审计文件 hash/profile 精确匹配、一次 fallback、无网络/进程/文件证据；否则该项 blocked。 |

实施记录必须如实区分 macOS 合成测试、Windows target 编译和 Windows 真机 UAT。当前 host 不能证明 Windows primary/fallback；production catalog 为空、步骤 6 的主题取证阻断未解决前，发布门禁继续 blocked。

## 7. 回滚与代码实施修改顺序

1. 在 `wechat/ocr.rs` 先写输入校验、规范化、结果/事件/spy 的纯测试及 `Disabled` fallback；确认非 Text 无法到达下游。
2. 以最小 `OcrService` patch 新增 Windows native memory method；明确把所有路径式、脚本式和外部进程逻辑排除在该方法外，并增加 Windows-gated mock/编译测试。
3. 让 dispatcher 只连接 `WechatCaptureSlices.chat_rgba` 与 `OcrReadyReply`，在现有私有 state-machine 编排点接入 `Ocr -> Failed`；不添加 command 或 UI。
4. 增加严格 audit schema/parser、篡改/多引擎/Empty probe 拒绝测试；当前实际 catalog 不填 fallback。
5. 完成静态边界检查、Rust 定向测试和普通 OCR 回归审计；仅在受控 Windows UAT 结束后，才由人工提交脱敏 probe 与（如确有需要）一个 fallback 审计。

回滚删除微信 memory OCR dispatcher、native memory adapter、audit schema 和其测试即可；普通 Work Review `OcrService::extract_text(path)` 不受影响。出现任何不明的 WinRT、fallback 或边界问题时，保留 `Disabled` fallback 并使微信 OCR 返回稳定 `WX_OCR_UNAVAILABLE`/`WX_OCR_FAILED`，绝不以临时文件、PowerShell、Umi GUI/服务、远程 API 或自动化操作恢复功能。
