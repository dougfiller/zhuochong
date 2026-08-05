# Ask/Agent 流式可靠性与历史一致性设计

## 背景

Ask/Agent 当前改动为工具执行结果增加了 `ok` 状态，并把工具轨迹压缩到下一轮
历史中。代码审查确认核心方向正确，但发现旧流式请求在前端超时后仍可能继续写入
新的回答占位消息，同时控制事件可能因通道背压被静默丢弃。

## 目标

1. 每个流式请求只能更新自己的 assistant 消息，超时、销毁或终态后拒绝迟到事件。
2. 正常流式完成后仍能保存 `usedAi` 与 `modelName`。
3. 历史只包含完整的 `user → assistant` 轮次，最多保留最近 4 轮。
4. 工具摘要准确区分成功、失败和旧数据未知状态，并让所有 locale 识别真实线格式。
5. Token 增量可丢帧，但 `StepStart`、`StepResult`、`Done` 控制事件可靠投递。
6. 失败工具在 UI 中显示失败状态，不再伪装成绿色成功。

## 方案选择

### 方案 A：前端消息 ID 隔离 + 可靠控制事件（采用）

- 前端给每次 assistant 占位消息显式生成 ID。
- Store 新增按 ID 更新消息的能力，事件回调捕获本次消息 ID。
- 请求进入超时、错误、终态或页面销毁后关闭本次事件接收开关。
- Store 用 `sendingRequestId` 绑定发送占用；组件销毁可释放旧请求，旧 finally 不能释放新请求。
- 后端 Token 保持 `try_send`；控制事件进入内部队列后，还必须等待 Tauri Channel 实际发送结果的 ACK。

优点：改动集中、可测试、不会引入新的 IPC 协议。缺点：超时后后端任务仍会自行运行到
既有 30 秒 Agent 上限或网络请求结束，但其事件不会再污染界面。

### 方案 B：增加 requestId 与后端取消命令（暂不采用）

优点：可以真正停止后端计算。缺点：需要维护全局取消令牌、IPC 命令和工具调用取消链，
改动明显超出本次 Ask/Agent 提交范围。

### 方案 C：超时后禁止再次发送直到旧请求返回（不采用）

实现简单，但会让超时恢复失去意义，用户可能长期无法继续提问。

## 详细设计

### 请求状态

每次 `submitQuestion` 创建唯一 `assistantMessageId` 和局部 `acceptingEvents`。
Channel 回调仅在组件未销毁且 `acceptingEvents=true` 时处理事件。所有消息更新均调用
`updateMessageById(assistantMessageId, updater)`。终态、异常和 finally 都关闭事件入口。

发送状态同时绑定该 ID：`beginSending(id)` 声明当前占用者，`finishSending(id)` 仅在 ID
匹配时清理。组件销毁会释放自己的旧请求，使重新进入 Ask 后可立即发送；旧请求迟到的
finally 不会把新请求的 `sending` 状态错误清空。

### 历史构造

按顺序扫描 `user/assistant` 消息，只在非 streaming assistant 出现时提交一整个轮次。
孤立 user、孤立 assistant、streaming assistant 对应的未完成轮次全部丢弃。最终只保留
最近 4 个完整轮次（8 条消息）。后端协议仍只接收 `{role, content}`，不扩大到附件、
卡片或 tool role。

### 工具状态

- `ok === true`：成功；`search_memory` 有有限 hits 时输出 `→N条`，其余输出 `✓`。
- `ok === false`：失败，输出 `↯`。
- 缺少明确布尔状态：旧数据状态未知，输出 `?`，不猜测成功或零命中。

所有语言的系统提示词都引用真实字面格式 `[工具：xxx→N条 | yyy✓ | zzz↯ | aaa?]`，
再用各自语言解释含义。

### 后端事件投递

TokenBatcher 继续使用非阻塞发送，因为 Done 携带完整答案可兜底。步骤开始、步骤结果和
终态事件通过内部信封携带 oneshot ACK：只有 commands 桥接层调用 Tauri Channel 成功后，
发送方才继续。桥接失败会关闭内部接收端并向 Agent 传播失败；模型和工具 Future 使用
`tokio::select!` 同时等待业务结果与关闭信号，已确认断开时停止后续处理，Orchestrator
也不会把事件投递失败误当模型失败而进入 FastPath。

## 非目标

- 不新增工具调用 `callId`，当前工具仍串行执行。
- 不持久化完整工具结果、附件或卡片到 LLM 历史。
- 不实现跨 IPC 的强制取消协议；普通路由卸载不保证立即停止后端，只保证前端隔离，
  以及桥接已检测到 Tauri Channel 失败后取消在途模型/工具。
- 不修复与本次改动无关的 README 图片尺寸基线失败。
