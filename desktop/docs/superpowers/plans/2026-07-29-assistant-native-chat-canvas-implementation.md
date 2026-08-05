# Assistant Native Chat Canvas Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将助手页实现为已确认的 Work Review 原生聊天画布，并保持现有模型、会话、流式回复和操作确认逻辑不变。

**Architecture:** 继续以 `Ask.svelte` 作为行为与页面结构入口，通过语义化的会话栏、工作脉络菜单、无卡片消息流和依据线重组 DOM；视觉规则集中在 `app.css` 的 ask 命名空间中；所有新文案进入四语言资源。不开新接口、不加依赖。

**Tech Stack:** Svelte、JavaScript、CSS、Node.js `node:test`、Vite

## Global Constraints

- 不新增依赖。
- 不创建分支或 worktree。
- 不暂存、不提交、不推送。
- 不修改 `CHANGELOG.md`。
- 不使用 `git reset`、`git restore`、`git checkout --`、`git stash`。
- 保持中文输入法、Enter/Shift+Enter、流式响应、停止生成、历史会话和操作确认逻辑不变。
- 深色模式边界必须与概览的低对比线条一致。

---

### Task 1: 固化助手页面结构契约

**Files:**
- Modify: `src/routes/ask/AskEditorial.test.js`
- Modify: `src/AskI18n.test.js`

**Interfaces:**
- Consumes: `Ask.svelte` 的 CSS 类和四语言 `ask` 文案键。
- Produces: 新会话栏、工作脉络、依据线和精简文案的静态回归契约。

- [ ] **Step 1: 写失败测试**

断言页面存在 `ask-conversation-bar`、`ask-context-menu`、`ask-reference-trail`，且不存在 `page-title-badge`、`ask-context-strip`、`ask-kicker`、`ask-starter-index`、`ask-answer-card`。

- [ ] **Step 2: 运行测试并确认 RED**

```bash
node --test src/routes/ask/AskEditorial.test.js src/AskI18n.test.js
```

预期：因生产结构和新文案尚未实现而失败。

### Task 2: 更新四语言助手文案

**Files:**
- Modify: `src/lib/i18n/locales/zh-CN.js`
- Modify: `src/lib/i18n/locales/zh-TW.js`
- Modify: `src/lib/i18n/locales/en.js`
- Modify: `src/lib/i18n/locales/ar.js`

**Interfaces:**
- Produces: `ask.welcomeBrief` 的短文案，以及 `ask.workContext`、`ask.contextDate`、`ask.contextSources`、`ask.referenceTrail` 等页面键。

- [ ] **Step 1: 用简短欢迎说明替换长句**

中文使用“结合工作记录，为你提炼重点。”，其他语言保持同等简洁。

- [ ] **Step 2: 添加工作脉络和依据线文案**

四语言键名保持完全一致，避免英文或阿拉伯语回退到中文。

- [ ] **Step 3: 运行 i18n 测试并确认通过**

```bash
node --test src/AskI18n.test.js src/I18nLocaleFlow.test.js
```

### Task 3: 重构助手页面语义结构

**Files:**
- Modify: `src/routes/ask/Ask.svelte`

**Interfaces:**
- Consumes: Task 2 的 i18n 键、现有 `currentConversationTitle`、`currentContextDate`、`currentModelLabel` 和 `message.references`。
- Produces: `ask-conversation-bar`、`ask-assistant-mark`、`ask-context-menu`、`ask-reference-trail` DOM。

- [ ] **Step 1: 把传统页头替换成紧凑会话栏**

保留历史与新对话事件处理，去掉大标题徽章、工作区副标题和页头数量角标。

- [ ] **Step 2: 删除常驻上下文条并迁移到 composer**

使用可访问的 `details/summary` 展示当前日期和数据来源；真实模型只在相邻选择器中显示一次。

- [ ] **Step 3: 精简空状态**

删除 kicker、编号和箭头；保留动态 starter prompt 点击发送能力。

- [ ] **Step 4: 取消助手回答卡片**

助手正文使用 `ask-assistant-response`；用户气泡继续使用现有角色判断。

- [ ] **Step 5: 把引用卡片改成依据线**

默认显示汇总入口，展开后单列展示现有引用元数据与摘要。

- [ ] **Step 6: 把免责声明移出模型组**

保持模型选择唯一且真实，输入工具栏只含工作脉络、模型和发送/停止。

### Task 4: 实现聊天画布视觉系统

**Files:**
- Modify: `src/app.css`

**Interfaces:**
- Consumes: Task 3 的 ask 类名。
- Produces: 居中阅读轴、轻量会话栏、无卡片回答、浮动输入区、深色与响应式样式。

- [ ] **Step 1: 定义助手页中性视觉变量**

复用全局 surface token，浅色使用低对比中性表面，深色边界使用 `--surface-border-default`。

- [ ] **Step 2: 实现会话栏和统一身份标识**

高度 48–56px；身份标识为对话气泡加闪光；操作按钮为轻量图标按钮。

- [ ] **Step 3: 实现空状态和轻量 starter**

空状态略高于垂直中心，说明短句居中，第一项提示可横跨两列，移动端改单列。

- [ ] **Step 4: 实现消息与工具过程排版**

助手正文无外框；用户气泡中性；工具过程为紧凑状态行；确认区域保持明确。

- [ ] **Step 5: 实现 composer、工作脉络菜单和依据线**

输入框宽度约 800px，工具行无分隔线；菜单和来源展开层使用低对比浮层。

- [ ] **Step 6: 完成深色和响应式规则**

移除旧 `ask-context-strip`、`ask-answer-card`、`ask-kicker` 相关规则，避免死样式。

### Task 5: 验证与回归

**Files:**
- Verify only

- [ ] **Step 1: 运行助手专项测试**

```bash
node --test src/routes/ask/*.test.js src/AskI18n.test.js
```

- [ ] **Step 2: 运行前端完整测试**

```bash
python3 - <<'PY'
# 收集 src 与 scripts 下的 *.test.js / *.test.mjs，使用 node --test 运行，最长 60 秒。
PY
```

- [ ] **Step 3: 运行前端构建**

```bash
npm run build
```

- [ ] **Step 4: 检查工作区差异**

```bash
git diff -- src/routes/ask/Ask.svelte src/app.css src/lib/i18n/locales/zh-CN.js src/lib/i18n/locales/zh-TW.js src/lib/i18n/locales/en.js src/lib/i18n/locales/ar.js src/routes/ask/AskEditorial.test.js src/AskI18n.test.js docs/superpowers/specs/2026-07-29-assistant-native-chat-canvas-design.md docs/superpowers/plans/2026-07-29-assistant-native-chat-canvas-implementation.md
```

确认没有触碰 `CHANGELOG.md`，没有新增依赖，没有改变助手的数据与请求逻辑。
