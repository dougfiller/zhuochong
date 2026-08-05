# 助手页 Editorial 视觉与交互优化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将助手页改造成与概览、日报一致的干净 editorial 工作台，同时保持现有流式请求、工具调用、引用、模型选择和历史会话行为不变。

**Architecture:** 保留 `Ask.svelte` 的请求状态机与数据链路，只重组派生展示状态和模板结构；新增样式限定在 `ask-*` 命名域，避免覆盖正在修改的概览、日报样式。使用现有 Node 源码契约测试保护结构与关键交互，并补充中文输入法 Enter 防误发送回归测试。

**Tech Stack:** Svelte 4、Tailwind CSS、全局 `app.css`、Node.js `node:test`、Vite 5、Tauri 2 API。

## Global Constraints

- 仅优化助手页前端视觉与交互呈现。
- 不修改 Rust/Tauri 后端接口、数据库结构和工具能力。
- 不重写 `streamEvent.js`、`historyPayload.js`、`requestEventGate.js`。
- 保留现有流式输出、停止生成、请求隔离、行动确认、引用和历史会话行为。
- 不实现没有真实参数支撑的“今天 / 本周 / 自定义”切换器。
- 全局样式已有用户未提交改动；所有新样式必须限定在助手页命名域。
- 新增代码注释和文档使用简体中文。
- 单次自动化测试命令最长运行 60 秒。

---

### Task 1: 固化助手页新结构与输入器回归测试

**Files:**
- Modify: `src/routes/ask/AskEditorial.test.js`
- Create: `src/routes/ask/AskComposer.test.js`
- Read only: `src/routes/ask/Ask.svelte`
- Read only: `src/app.css`

**Interfaces:**
- Consumes: 设计规格中的页头、上下文条、回答卡、工具摘要、引用区和输入器结构。
- Produces: 可证明旧页面结构尚未满足要求的失败测试。

- [ ] **Step 1: 扩展结构测试**

在 `AskEditorial.test.js` 中断言：

- 存在 `ask-editorial-header`、`ask-context-strip`、`ask-answer-card`、`ask-tool-summary`、`ask-reference-grid`。
- 历史会话与新对话入口位于页头区域。
- 不再渲染独立的 `ask-sending-bubble`。
- 助手回答仍保留 `assistant-markdown`、行动确认按钮和引用元数据。
- 样式包含 22px 一级圆角、轻量边框和响应式页头规则。
- 欢迎态不再依赖大面积 radial gradient 伪元素。

- [ ] **Step 2: 新增输入法回归测试**

创建 `AskComposer.test.js`，断言 `handleComposerKeydown` 在 `event.isComposing` 或 `event.keyCode === 229` 时直接返回；普通 Enter 仍提交，Shift+Enter 不提交。

- [ ] **Step 3: 运行测试并确认失败**

Run:

```bash
timeout 60 node --test src/routes/ask/AskEditorial.test.js src/routes/ask/AskComposer.test.js
```

Expected: FAIL，缺少新的 editorial 结构标记和 IME 保护。

- [ ] **Step 4: 提交测试基线**

```bash
git add src/routes/ask/AskEditorial.test.js src/routes/ask/AskComposer.test.js
git commit -m "test: define assistant editorial workspace"
```

---

### Task 2: 重组页头、欢迎态和消息层级

**Files:**
- Modify: `src/routes/ask/Ask.svelte:1-815`
- Modify: `src/routes/ask/Ask.svelte:814-995`
- Modify: `src/lib/i18n/locales/zh-CN.js`
- Modify: `src/lib/i18n/locales/zh-TW.js`
- Modify: `src/lib/i18n/locales/en.js`
- Modify: `src/lib/i18n/locales/ar.js`
- Test: `src/routes/ask/AskEditorial.test.js`
- Test: `src/routes/ask/AskComposer.test.js`

**Interfaces:**
- Consumes: `messages`、`conversations`、`conversationId`、`selectedModelId`、`currentModelOptionLabel()`、`starterPrompts`、`sending`。
- Produces: `currentConversationTitle`、`completedStepCount(message)`、`activeStepLabel(message)` 等只读派生展示；新的 editorial 模板结构。

- [ ] **Step 1: 添加只读展示派生状态**

在 `Ask.svelte` 中添加：

- 当前会话标题：从 `conversations` 与 `conversationId` 派生，空会话使用 i18n 能力说明。
- 当前模型标签：复用 `currentModelOptionLabel()`。
- 工具步骤统计：区分运行中、完成、失败、待确认，不改变步骤原始数据。
- 引用显示条件和数量文案，不改变引用载荷。

- [ ] **Step 2: 增加 IME Enter 防误发送**

在 `handleComposerKeydown(event)` 开头处理：

```js
if (event.isComposing || event.keyCode === 229) return;
```

保留 Enter 发送、Shift+Enter 换行逻辑。

- [ ] **Step 3: 实现稳定页头与上下文条**

页头包含：

- “助手”标题。
- 当前会话标题或能力说明。
- 历史会话按钮及数量。
- 新对话按钮。

上下文条包含：

- 当前本地日期。
- 当前模型标签。
- 可参考活动、小时总结、日报和记忆的说明。

不得添加真实不可用的范围切换控件。

- [ ] **Step 4: 重做欢迎态**

- 保留 starter 按钮和 `submitQuestion(prompt)`。
- 删除大面积渐变背景和居中英雄式占位。
- 使用轻量标题、说明与 2×2 问题卡。

- [ ] **Step 5: 重做消息层级**

- 用户消息改为浅蓝色细边框气泡。
- 助手消息改为白色 `ask-answer-card`。
- 工具步骤收拢到 `ask-tool-summary`，仅有详情的步骤可展开。
- 待确认步骤仍强制展示批准/拒绝按钮。
- 删除独立三点 `ask-sending-bubble`，只保留当前助手消息中的思考/流式状态。
- 引用改为回答卡内 `ask-reference-grid`；少量引用直接展示，避免消息级和步骤级双重折叠。

- [ ] **Step 6: 补齐四语言文案**

新增并同步：

- 页面能力说明。
- 上下文条日期/模型/可参考数据说明。
- 工具步骤汇总文案。
- 历史数量文案。
- 输入器快捷键提示。

- [ ] **Step 7: 运行专项测试**

Run:

```bash
timeout 60 node --test \
  src/routes/ask/AskEditorial.test.js \
  src/routes/ask/AskComposer.test.js \
  src/routes/ask/AskModelI18n.test.js \
  src/routes/ask/AskStreamIsolation.test.js \
  src/AssistantStreaming.test.js \
  src/AskI18n.test.js
```

Expected: PASS。

- [ ] **Step 8: 提交结构改动**

```bash
git add src/routes/ask/Ask.svelte src/lib/i18n/locales/*.js src/routes/ask/*.test.js src/AssistantStreaming.test.js src/AskI18n.test.js
git commit -m "feat: redesign assistant workspace structure"
```

---

### Task 3: 对齐 editorial 样式、输入区和历史抽屉

**Files:**
- Modify: `src/app.css`
- Modify: `src/routes/ask/Ask.svelte:995-1143`
- Test: `src/routes/ask/AskEditorial.test.js`
- Test: `src/AppShellVisual.test.js`
- Test: `src/SurfaceConsistency.test.js`
- Test: `src/UiVisualStyle.test.js`

**Interfaces:**
- Consumes: Task 2 的 `ask-editorial-*`、`ask-answer-card`、`ask-tool-summary`、`ask-reference-grid` 结构。
- Produces: 亮色/暗色/窄屏可用的助手页视觉样式与精简输入区。

- [ ] **Step 1: 实现页面布局样式**

- 单一消息滚动容器。
- 页头与上下文条不制造第二滚动条。
- 内容最大宽度对齐概览、日报阅读尺度。
- 一级卡片使用 22px 圆角，内嵌区使用 14px 圆角。
- 去除欢迎区和回答卡的大面积 radial/linear gradient。

- [ ] **Step 2: 精简底部输入区**

- 历史会话和新对话只保留在页头。
- 模型选择、快捷键提示、发送/停止保持同一操作层。
- 发送中主按钮直接表现为停止按钮，并调用现有 `stopCurrentRequest()`。
- 保持 textarea 自适应高度与最大高度。

- [ ] **Step 3: 优化历史会话抽屉**

- 使用 editorial 白色面、轻边框和蓝色选中态。
- 删除按钮改用垃圾桶图标，但继续调用现有 `deleteConversation()`。
- 保留 Esc、focus trap、切换和新对话逻辑。
- 不新增删除确认或数据库逻辑。

- [ ] **Step 4: 添加暗色与响应式样式**

- 窄屏页头操作换行。
- 阿拉伯语 RTL 下保持文字方向和按钮布局正确。
- 暗色使用现有 GitHub dark 色阶。
- 输入区和回答卡不得横向溢出。

- [ ] **Step 5: 运行视觉契约测试**

Run:

```bash
timeout 60 node --test \
  src/routes/ask/AskEditorial.test.js \
  src/AppShellVisual.test.js \
  src/SurfaceConsistency.test.js \
  src/UiVisualStyle.test.js
```

Expected: PASS。

- [ ] **Step 6: 提交样式改动**

```bash
git add src/app.css src/routes/ask/Ask.svelte src/routes/ask/AskEditorial.test.js
git commit -m "style: align assistant with editorial pages"
```

---

### Task 4: 全量验证与视觉检查

**Files:**
- Verify only: all modified files
- Optional test adjustment: assistant visual/source contract tests only when they reveal a real mismatch

**Interfaces:**
- Consumes: Tasks 1-3 完整实现。
- Produces: 可审计的测试、构建和视觉验证证据。

- [ ] **Step 1: 运行助手相关完整测试**

Run:

```bash
timeout 60 node --test \
  src/routes/ask/*.test.js \
  src/AssistantStreaming.test.js \
  src/AskI18n.test.js \
  src/I18nLocaleFlow.test.js
```

Expected: 所有测试通过，0 failure。

- [ ] **Step 2: 运行前端完整 Node 测试**

Run:

```bash
timeout 60 node --test src/*.test.js src/routes/**/*.test.js
```

Expected: 所有测试通过，0 failure。

- [ ] **Step 3: 运行构建**

Run:

```bash
timeout 60 npm run build
```

Expected: Vite 构建成功，exit code 0。

- [ ] **Step 4: 启动本地页面并视觉检查**

检查：

- 亮色欢迎态。
- 有会话回答态。
- 流式思考与工具步骤态。
- 引用区域。
- 历史会话抽屉。
- 暗色主题。
- 1024px 和窄屏布局。

- [ ] **Step 5: 检查差异与用户改动隔离**

Run:

```bash
git diff --check
git status --short
git diff -- src/routes/ask/Ask.svelte src/routes/ask/*.test.js src/app.css src/lib/i18n/locales/*.js
```

确认没有覆盖概览、日报及其他用户未提交改动。

- [ ] **Step 6: 请求代码审查并修复重要问题**

提供设计规格、计划、修改文件和验证输出；Critical/Important 问题必须在完成前解决。

- [ ] **Step 7: 最终复验**

修复审查问题后重新运行 Task 4 Steps 1-3，记录最新输出。
