# Ask/Agent Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 Ask/Agent 跨请求串写、历史轮次破坏、工具状态展示错误和控制事件丢失问题，并把验证后的改动直接提交到 `main`。

**Architecture:** 前端使用 assistant 消息 ID 隔离每次请求，把事件归约逻辑提取为可测试纯函数；历史构造只保留完整对话轮次。后端区分可丢 Token 与必须可靠投递的控制事件，不引入新的 IPC 取消协议。

**Tech Stack:** Svelte、JavaScript ES Modules、Node test runner、Rust、Tokio mpsc、Tauri IPC、Cargo test。

## Global Constraints

- 直接在 `main` 工作，不创建功能分支。
- 所有测试命令最大运行 60 秒。
- `plan.md` 不纳入提交。
- 不扩大到附件/卡片历史、callId 或后端取消协议。
- Token 允许丢帧；控制事件必须可靠。

---

### Task 1: 历史轮次与工具三态摘要

**Files:**
- Modify: `src/routes/ask/historyPayload.js`
- Modify: `src/routes/ask/historyPayload.test.js`

**Interfaces:**
- Consumes: `buildHistoryPayload(messages)`、`summarizeStepsForHistory(steps)`。
- Produces: 最近 4 个完整轮次和 `✓ / ↯ / ? / →N条` 摘要。

- [ ] **Step 1: 写失败测试**

新增用例：中断的末轮 user/streaming assistant 整体丢弃；超过 4 轮时按完整轮次截断；
缺少 `ok` 输出 `?`；缺少有效 hits 时不伪造 `→0条`；输入深冻结后保持不变。

- [ ] **Step 2: 运行测试并确认按预期失败**

```bash
python3 /tmp/work_review_run_60.py node --test src/routes/ask/historyPayload.test.js
```

Expected: 新增断言失败，原因是当前实现保留孤立 user、按消息截断并把未知状态当成功。

- [ ] **Step 3: 最小实现**

扫描完整轮次后 `slice(-4)`，再扁平化为 8 条消息；摘要只接受明确的 `ok=true/false`，
未知状态输出 `tool?`。

- [ ] **Step 4: 运行测试并确认通过**

```bash
python3 /tmp/work_review_run_60.py node --test src/routes/ask/historyPayload.test.js
```

Expected: 全部通过。

### Task 2: 请求级消息隔离与流式元数据

**Files:**
- Modify: `src/lib/stores/assistant.js`
- Modify: `src/AssistantStreaming.test.js`
- Create: `src/routes/ask/streamEvent.js`
- Create: `src/routes/ask/streamEvent.test.js`
- Modify: `src/routes/ask/Ask.svelte`

**Interfaces:**
- Produces: `assistantStore.updateMessageById(messageId, updater)`。
- Produces: `reduceStreamEvent(message, event, fallbackError)` 返回 `{ message, terminal }`。

- [ ] **Step 1: 写失败测试**

新增 store 用例：两个 streaming 消息并存时，按 ID 更新只改变目标消息；不存在 ID 时不变。
新增事件归约用例：失败步骤保留 `ok=false`；done 终止并覆盖完整答案；元数据可在 done 后补写。

- [ ] **Step 2: 运行测试并确认按预期失败**

```bash
python3 /tmp/work_review_run_60.py node --test src/AssistantStreaming.test.js src/routes/ask/streamEvent.test.js
```

Expected: 缺少新 API/模块而失败。

- [ ] **Step 3: 最小实现**

Store 按消息 ID 更新。Ask 每次请求显式生成占位消息 ID，Channel 回调捕获该 ID，超时、
终态、销毁和 finally 后拒绝迟到事件；invoke 返回后按 ID 写入 `usedAi/modelName`。
Store 另以 `sendingRequestId` 绑定发送占用，确保卸载可释放旧请求、旧 finally 不会释放新请求。

- [ ] **Step 4: 运行测试并确认通过**

```bash
python3 /tmp/work_review_run_60.py node --test src/AssistantStreaming.test.js src/routes/ask/streamEvent.test.js
```

Expected: 全部通过。

### Task 3: 失败步骤 UI 与多语言文案

**Files:**
- Modify: `src/routes/ask/Ask.svelte`
- Modify: `src/lib/i18n/locales/zh-CN.js`
- Modify: `src/lib/i18n/locales/zh-TW.js`
- Modify: `src/lib/i18n/locales/en.js`
- Modify: `src/lib/i18n/locales/ar.js`
- Modify: `src/AskI18n.test.js`

**Interfaces:**
- Produces: `ask.stepFailed` 四语言文案。

- [ ] **Step 1: 写失败测试**

断言四个 locale 均存在非空 `ask.stepFailed`。

- [ ] **Step 2: 运行测试并确认按预期失败**

```bash
python3 /tmp/work_review_run_60.py node --test src/AskI18n.test.js
```

Expected: 缺少 `stepFailed` 而失败。

- [ ] **Step 3: 最小实现**

加入四语言失败文案；失败步骤显示红色状态点和失败文案，不显示命中数。

- [ ] **Step 4: 运行测试并确认通过**

```bash
python3 /tmp/work_review_run_60.py node --test src/AskI18n.test.js
```

Expected: 全部通过。

### Task 4: Rust 控制事件可靠投递

**Files:**
- Modify: `src-tauri/src/agent/executor.rs`
- Modify: `src-tauri/src/agent/orchestrator.rs`
- Modify: `src-tauri/src/agent/events.rs`

**Interfaces:**
- Produces: 内部事件信封 + oneshot 投递 ACK；`emit_control_event(...).await` 和
  `emit_done(...).await` 等待 Tauri Channel 实际发送结果；Token 继续非阻塞。

- [ ] **Step 1: 写失败测试**

新增异步测试：容量为 1 的通道先填满 Token 后，控制事件等待容量与外部 ACK；Tauri
Channel 投递失败或内部接收端关闭时返回失败；桥接关闭会取消在途 Future。补
`query_activities` 默认标签断言。

- [ ] **Step 2: 运行测试并确认按预期失败**

```bash
CC=/usr/bin/cc CARGO_TARGET_DIR=target python3 /tmp/work_review_run_60.py cargo test -p work-review agent:: --locked
```

Expected: 当前 `try_send` 丢弃控制事件，新增断言失败。

- [ ] **Step 3: 最小实现**

Token 使用 `try_send`；控制事件使用内部 `send().await` + oneshot ACK，桥接层确认 Tauri
Channel 的真实发送结果；模型/工具用 `tokio::select!` 监听桥接关闭。Orchestrator
所有 Done 调用同步改为 await，并避免把接收端关闭误判为模型失败后继续降级。

- [ ] **Step 4: 运行测试并确认通过**

```bash
CC=/usr/bin/cc CARGO_TARGET_DIR=target python3 /tmp/work_review_run_60.py cargo test -p work-review agent:: --locked
```

Expected: 全部通过。

### Task 5: 系统提示词和空问题协议

**Files:**
- Modify: `src-tauri/src/commands/ask.rs`

**Interfaces:**
- Consumes: 固定工具摘要字面格式。
- Produces: 四 locale 一致格式声明；空问题 `tool_labels=[]`。

- [ ] **Step 1: 写失败测试**

断言四个 locale 的 prompt 都包含真实字面前缀 `[工具：` 和未知符号 `?`；抽取并测试空问题
回答构造时工具标签为空。

- [ ] **Step 2: 运行测试并确认按预期失败**

```bash
CC=/usr/bin/cc CARGO_TARGET_DIR=target python3 /tmp/work_review_run_60.py cargo test -p work-review commands::ask::tests --locked
```

Expected: 非简中 prompt 格式和未知符号断言失败。

- [ ] **Step 3: 最小实现**

四语言只翻译解释文本，不翻译 `[工具：...]` 机器格式；空问题返回空标签。

- [ ] **Step 4: 运行测试并确认通过**

```bash
CC=/usr/bin/cc CARGO_TARGET_DIR=target python3 /tmp/work_review_run_60.py cargo test -p work-review commands::ask::tests --locked
```

Expected: 全部通过。

### Task 6: 全量验证、提交与 main 集成

**Files:**
- Modify: `CHANGELOG.md`（仅在现有条目中补充可靠性修复说明）

- [ ] **Step 1: 运行格式与差异检查**

```bash
cargo fmt --all -- --check
git diff --check
```

- [ ] **Step 2: 运行全量测试和构建**

```bash
CC=/usr/bin/cc CARGO_TARGET_DIR=target python3 /tmp/work_review_run_60.py cargo test --workspace --all-targets --locked
python3 /tmp/work_review_run_60.py node --test
python3 /tmp/work_review_run_60.py npm run build
```

Expected: Rust 全通过；Node 仅允许既有 README 图片尺寸基线失败；构建退出码 0。

- [ ] **Step 3: 提交 Ask/Agent（排除根目录 `plan.md`）**

```bash
git add CHANGELOG.md docs/superpowers/specs/2026-07-22-ask-agent-reliability-design.md docs/superpowers/plans/2026-07-22-ask-agent-reliability.md src src-tauri
git commit -m "fix(agent): isolate streaming requests and preserve tool state"
```

- [ ] **Step 4: 把 P0 提交移入 main 并推送**

按顺序 cherry-pick `f78dfe8 b7987b2 ca0bc06 098eb9a 758429b`，fetch 后验证
`origin/main` 是当前 HEAD 的祖先，再执行 `git push origin main`。

- [ ] **Step 5: 清理误建分支和工作树**

删除远端 `codex/p0-reliability-fixes`、`codex/p0-reliability-fixes-impl`，删除隔离工作树，
再删除对应本地分支；最终确认仅 `plan.md` 保持未跟踪。
