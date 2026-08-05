# 助手统一页头与随机问题 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 统一助手页头、压缩欢迎态空白，并实现四条两行两列的随机高质量快捷问题。

**Architecture:** 将随机抽样逻辑提取为纯函数，Svelte 页面只负责在进入、新对话和模型切换时刷新。四语言维护完整候选池，AI 生成结果与本地池合并后统一抽样。

**Tech Stack:** Svelte 4、JavaScript、Node.js test runner、CSS。

## Global Constraints

- 不新增依赖。
- 不修改助手问答后端协议。
- 快捷问题一次固定显示 4 条，布局固定为 2 列 × 2 行。
- 所有新增界面文案覆盖简体中文、繁体中文、英文和阿拉伯语。

---

### Task 1: 随机抽样纯函数

**Files:**
- Create: `src/routes/ask/starterPromptPresentation.js`
- Create: `src/routes/ask/starterPromptPresentation.test.js`

- [ ] 编写失败测试，覆盖四条、去重、可注入随机数和避免连续同组。
- [ ] 运行专项测试并确认失败。
- [ ] 实现最小随机抽样逻辑。
- [ ] 运行专项测试并确认通过。

### Task 2: 页面和文案接入

**Files:**
- Modify: `src/routes/ask/Ask.svelte`
- Modify: `src/lib/i18n/locales/zh-CN.js`
- Modify: `src/lib/i18n/locales/zh-TW.js`
- Modify: `src/lib/i18n/locales/en.js`
- Modify: `src/lib/i18n/locales/ar.js`
- Modify: `src/routes/ask/AskEditorial.test.js`
- Modify: `src/AskI18n.test.js`

- [ ] 编写失败测试，覆盖统一页头、精简欢迎文案、随机池和“参考记录”。
- [ ] 运行专项测试并确认失败。
- [ ] 接入统一页头与随机刷新时机。
- [ ] 更新四语言问题池和参考记录文案。
- [ ] 运行专项测试并确认通过。

### Task 3: 欢迎态视觉收紧

**Files:**
- Modify: `src/app.css`
- Test: `src/routes/ask/AskEditorial.test.js`

- [ ] 编写失败测试，覆盖两列两行、字体和留白约束。
- [ ] 运行专项测试并确认失败。
- [ ] 收紧欢迎态间距并保持四卡片两列布局。
- [ ] 检查浅色、深色、720px 和 520px。

### Task 4: 完整验证

- [ ] 运行全部 Node 测试，最长 60 秒。
- [ ] 运行 `npm run build`。
- [ ] 浏览器检查助手欢迎态、随机刷新、参考记录和响应式布局。
