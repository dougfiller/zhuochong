# 时间线详情与常驻网站列表优化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 优化时间线活动详情的信息层级，并将概览常驻网站改成与应用使用一致的轻量无边框排行。

**Architecture:** 保留现有右侧抽屉、分类弹层、隐私规则、多浏览器聚合与网站详情交互，仅调整 Svelte 模板语义顺序和局部样式。两个页面互不共享新增状态，分别通过源码结构测试锁定布局与交互契约。

**Tech Stack:** Svelte、JavaScript、Tailwind/CSS、Node.js test runner、Vite。

## Global Constraints

- 不新增依赖，不修改数据模型和后端接口。
- 不覆盖或回退工作区已有改动，不修改 `CHANGELOG.md`。
- 不创建分支、不暂存、不提交、不推送。
- 深色模式使用低对比边界；移动端 640px 及以下保留全屏详情。
- 单次测试最长运行 60 秒。

---

### Task 1: 时间线活动详情重排

**Files:**
- Modify: `src/routes/timeline/Timeline.svelte`
- Test: `src/routes/timeline/TimelineLayout.test.js`
- Test: `src/routes/timeline/Timeline.category.test.js`

**Interfaces:**
- Consumes: 现有活动详情数据、分类弹层、记录策略和删除交互。
- Produces: `timeline-detail-hero`、`timeline-detail-preview`、`timeline-detail-meta`、`timeline-detail-settings` 等语义区域。

- [ ] **Step 1: 写失败测试**
  - 断言详情顺序为身份与时间、截图、活动信息、记录设置。
  - 断言记录设置不使用多层卡片，深色边界为低对比。
  - 断言分类弹层和 640px 全屏规则仍保留。

- [ ] **Step 2: 运行定向测试确认失败**
  - Run: `timeout 60s node --test src/routes/timeline/TimelineLayout.test.js src/routes/timeline/Timeline.category.test.js`
  - Expected: 新增结构断言失败。

- [ ] **Step 3: 实现最小布局和样式调整**
  - 保留右侧抽屉。
  - 顶部集中应用身份、分类、时间与时长。
  - 截图置于第一视觉区域；标题和网址使用轻量信息行。
  - 分类和记录策略收拢到底部记录设置区域。

- [ ] **Step 4: 运行定向测试确认通过**
  - Run: `timeout 60s node --test src/routes/timeline/TimelineLayout.test.js src/routes/timeline/Timeline.category.test.js`
  - Expected: PASS。

### Task 2: 常驻网站轻量排行

**Files:**
- Modify: `src/routes/Overview.svelte`
- Test: `src/routes/OverviewEditorial.test.js`
- Test: `src/routes/Overview.test.js`
- Test: `src/routes/overviewDomainPresentation.test.js`

**Interfaces:**
- Consumes: `buildDomainPresentation()` 返回的浏览器来源与 `sourceTrack` 分段数据。
- Produces: 与应用使用一致的无边框网站排行行，继续调用 `openDomainDetail(domain)`。

- [ ] **Step 1: 写失败测试**
  - 断言网站行默认透明且无独立边框。
  - 断言浏览器分段轨道无边框、保留浅色底轨和分段。
  - 断言域名、分类、来源、时长及点击详情仍存在。

- [ ] **Step 2: 运行定向测试确认失败**
  - Run: `timeout 60s node --test src/routes/OverviewEditorial.test.js src/routes/Overview.test.js src/routes/overviewDomainPresentation.test.js`
  - Expected: 新增无边框结构断言失败。

- [ ] **Step 3: 实现最小布局和样式调整**
  - 删除网站行外围边框与实色卡片背景。
  - 将分段轨道作为主进度条，并与应用使用的高度和间距统一。
  - 页面数量降为次要信息；hover/focus 仅显示轻背景。
  - 保留多浏览器聚合、点击详情和查看全部。

- [ ] **Step 4: 运行定向测试确认通过**
  - Run: `timeout 60s node --test src/routes/OverviewEditorial.test.js src/routes/Overview.test.js src/routes/overviewDomainPresentation.test.js`
  - Expected: PASS。

### Task 3: 集成与视觉验收

**Files:**
- Verify: `src/routes/timeline/Timeline.svelte`
- Verify: `src/routes/Overview.svelte`

- [ ] **Step 1: 运行全部前端 JavaScript 测试**
  - Run: `timeout 60s sh -c 'find src scripts -type f \( -name "*.test.js" -o -name "*.test.mjs" \) -print0 | xargs -0 node --test'`
  - Expected: PASS。

- [ ] **Step 2: 执行生产构建**
  - Run: `timeout 60s npm run build`
  - Expected: exit 0；允许记录既有未使用属性警告。

- [ ] **Step 3: 浏览器视觉验收**
  - 检查概览网站排行和时间线详情的桌面、窄屏、深色模式。
  - 检查分类弹层、网站详情、删除和关闭交互没有回归。

- [ ] **Step 4: 检查变更质量**
  - Run: `git diff --check -- src/routes/timeline/Timeline.svelte src/routes/timeline/TimelineLayout.test.js src/routes/timeline/Timeline.category.test.js src/routes/Overview.svelte src/routes/OverviewEditorial.test.js src/routes/Overview.test.js src/routes/overviewDomainPresentation.test.js`
  - Expected: 无空白错误。
