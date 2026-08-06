# 代码改动说明：建立 Windows OCR 专用后端和条件本地 OCR fallback

## 1. 基本信息

- 日期：2026-08-06 11:20
- 对应方案：`kaifa/kaifa_plan/2026年08月06日11点15分-建立Windows OCR专用后端和条件本地OCR fallback.md`
- LOOF run：`20260806-1110-步骤-8建立-windows-ocr-专用后端和条件本地-ocr-fallbackdispatc`

## 2. 改动原因

既有 Work Review Windows OCR 通过图片路径、临时 PowerShell 脚本与 `StorageFile` 工作，不能用于微信私有内存聊天区。此次新增仅后端可达的 WinRT 内存路径，并保持未取得实机 probe 时的 fail-closed 行为。

## 3. 改动范围

### 3.1 涉及范围

- 新增 `wechat/ocr.rs`：私有 dispatcher、输入限制、结果规范化、脱敏事件和单次 fallback 门禁。
- 修改 `ocr.rs`：新增 Windows `SoftwareBitmap`/`OcrEngine` 内存入口；不改变 `extract_text(path)`。
- 修改微信类型、状态机、profile 和 runtime：未规范化文本不能构造 reply；OCR 非 Text 转 `Failed`；fallback audit 必须精确绑定冻结 probe。
- 新增静态检测脚本及测试说明。

### 3.2 不涉及范围

- 不新增 Tauri command、前端 IPC、网络/远程 OCR、数据库、文件落盘、命令启动、UI Automation、微信注入或自动发送。
- 不修改普通 Work Review 路径式 OCR、production catalog 或真实 Windows profile。

## 4. 详细改动清单

| 路径 | 类型 | 说明 |
| --- | --- | --- |
| `desktop/src-tauri/src/ocr.rs` | 修改 | 新增 WinRT RGBA 内存 OCR 方法及受限结果。 |
| `desktop/src-tauri/src/wechat/ocr.rs` | 新增 | 仅消费聊天 ROI；primary/fallback 最多各一次。 |
| `desktop/src-tauri/src/wechat/{types,profiles,window_identity,runtime,state_machine}.rs` | 修改 | 文本 token、审计门、失败终态接线。 |
| `desktop/src-tauri/Cargo.toml`、`desktop/Cargo.lock` | 修改 | Windows-only `windows` projection feature。 |
| `kaifa/kaifa_test/verify_wechat_windows_ocr.py` | 新增 | 静态边界检测。 |

## 5. 行为变化

`WechatCaptureSlices.chat_rgba` -> primary WinRT memory OCR -> 规范化/长度检查 -> 仅 Text 构造 `OcrReadyReply`。Empty 不 fallback；Unavailable/Failed 仅在精确冻结 audit 存在时调用一次本地 fallback；所有非 Text 都写脱敏事件并将状态机终止为 `Failed`。

## 6. 验收与测试结果

| 测试项 | 实际结果 | 结果 |
| --- | --- | --- |
| 静态内存边界 | `verify_wechat_windows_ocr.py` 通过 | 通过 |
| 微信 Rust 定向测试 | 38/38 通过 | 通过 |
| macOS `cargo check` | 通过（含既有 dead-code 警告） | 通过 |
| diff 空白检查 | 通过 | 通过 |
| Windows target / 真机 UAT | 当前 host 无 Windows target，未运行 | blocked |
| `cargo fmt --check` | 未安装 rustfmt | 未运行 |

## 7. 当前边界与风险

- 当前 catalog 无 enabled profile，production fallback 仍 Disabled；本实现不宣称 Windows 微信上已可用。
- Windows target 编译、WinRT API 实机行为与 Text/Empty/Unavailable/Failed probe 必须在受控 Windows 11 x64 环境补充。

## 8. 回滚方案

回滚本次涉及的 Rust、manifest/lock 和静态检测脚本即可恢复原有 Work Review OCR；不需要清理数据，因为本次未新增数据、文件或外部服务。
