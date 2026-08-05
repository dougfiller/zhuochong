# Work Review 全应用外观轻量统一 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在不改变业务结构与后端契约的前提下，实现时段摘要最近时间优先、全应用内容轴与深色边界统一、设置页外壳收敛及窄屏响应式优化。

**Architecture:** 所有数据行为变化限定在前端展示层；共享视觉规则集中在 `src/app.css` 的设计 token 和页面轴原语中。页面组件只增加必要的语义类名或响应式类，设置页继续保留原导航与内容组件结构。

**Tech Stack:** Svelte 4、JavaScript ES Modules、Tailwind CSS 3、Node.js `node:test`、Vite 5。

## Global Constraints

- 只使用简体中文沟通、文档和新增代码注释。
- 保留现有左侧主导航、应用壳、页面信息架构和业务功能。
- 容器能居中的合理居中；设置项、时间线、日报正文、助手回答和长文本按语义起点对齐。
- 深色模式大容器外围线与概览一致，不使用过亮边框或重复内高光。
- 不修改数据库结构、Rust 小时摘要排序、本地 HTTP API 契约或主时间线排序。
- 不引入新依赖或新 UI 框架。
- 不修改 `CHANGELOG.md`，不清理或还原用户已有工作区改动。
- 不暂存、不提交、不推送；每个任务以测试和定向差异检查作为检查点。
- 所有测试和构建命令最大运行时间为 60 秒。

---

### Task 1: 时段摘要按最近小时优先展示

**Files:**
- Modify: `src/routes/timeline/summaryPresentation.js`
- Modify: `src/routes/timeline/summaryPresentation.test.js`
- Modify: `src/routes/timeline/HourlySummaryDrawer.svelte`
- Modify: `src/routes/timeline/HourlySummaryDrawer.test.js`

**Interfaces:**
- Consumes: `summaries: Array<{ hour: number|string }>`，来自现有时间线小时摘要状态。
- Produces: `orderHourlySummariesForDisplay(summaries): Array`，返回新数组并按 `hour` 数值倒序排列。

- [ ] **Step 1: 为展示排序纯函数写失败测试**

在 `summaryPresentation.test.js` 的导入列表加入 `orderHourlySummariesForDisplay`，并增加：

```js
test('时段摘要应按最近小时优先展示且不修改原数组', () => {
  const summaries = [
    { hour: 9, summary: '上午' },
    { hour: '16', summary: '下午' },
    { hour: 11, summary: '中午' },
  ];
  const originalOrder = summaries.map((summary) => summary.hour);

  const result = orderHourlySummariesForDisplay(summaries);

  assert.deepEqual(result.map((summary) => Number(summary.hour)), [16, 11, 9]);
  assert.deepEqual(summaries.map((summary) => summary.hour), originalOrder);
  assert.notEqual(result, summaries);
});

test('时段摘要排序应兼容空值、单条记录和小时边界', () => {
  assert.deepEqual(orderHourlySummariesForDisplay(), []);
  assert.deepEqual(orderHourlySummariesForDisplay([]), []);
  assert.deepEqual(orderHourlySummariesForDisplay([{ hour: 11 }]), [{ hour: 11 }]);
  assert.deepEqual(
    orderHourlySummariesForDisplay([{ hour: 0 }, { hour: 23 }, { hour: 12 }])
      .map((summary) => summary.hour),
    [23, 12, 0]
  );
});
```

- [ ] **Step 2: 运行纯函数测试并确认失败**

Run:

```bash
timeout 60s node --test src/routes/timeline/summaryPresentation.test.js
```

Expected: FAIL，提示 `orderHourlySummariesForDisplay` 未导出。

- [ ] **Step 3: 实现不修改原数组的展示排序函数**

在 `summaryPresentation.js` 中增加：

```js
export function orderHourlySummariesForDisplay(summaries = []) {
  return [...summaries].sort(
    (left, right) => Number(right.hour) - Number(left.hour)
  );
}
```

- [ ] **Step 4: 运行纯函数测试并确认通过**

Run:

```bash
timeout 60s node --test src/routes/timeline/summaryPresentation.test.js
```

Expected: 全部 PASS。

- [ ] **Step 5: 为抽屉接线写失败测试**

在 `HourlySummaryDrawer.test.js` 增加静态接线约束：

```js
test('时段摘要抽屉应遍历最近时间优先的展示数组', async () => {
  const source = await readFile(
    new URL('./HourlySummaryDrawer.svelte', import.meta.url),
    'utf8'
  );

  assert.match(source, /orderHourlySummariesForDisplay/);
  assert.match(source, /\$:\s*displaySummaries\s*=\s*orderHourlySummariesForDisplay\(summaries\)/);
  assert.match(source, /\{#each\s+displaySummaries\s+as\s+summary\s+\(summary\.hour\)\}/);
  assert.match(source, /summaryCount[\s\S]*summaries\.length/);
  assert.match(source, /peakDuration\s*=\s*summaries\.reduce/);
});
```

若文件尚未导入 `readFile`，加入：

```js
import { readFile } from 'node:fs/promises';
```

- [ ] **Step 6: 运行抽屉测试并确认失败**

Run:

```bash
timeout 60s node --test src/routes/timeline/HourlySummaryDrawer.test.js
```

Expected: FAIL，抽屉仍直接遍历 `summaries`。

- [ ] **Step 7: 将展示排序接入抽屉**

在 `HourlySummaryDrawer.svelte` 的导入列表增加：

```js
orderHourlySummariesForDisplay,
```

在现有响应式状态附近增加：

```js
$: displaySummaries = orderHourlySummariesForDisplay(summaries);
```

将列表遍历从：

```svelte
{#each summaries as summary (summary.hour)}
```

改为：

```svelte
{#each displaySummaries as summary (summary.hour)}
```

摘要数量、峰值计算和展开状态继续使用原始 `summaries`。

- [ ] **Step 8: 运行时段摘要相关测试与定向差异检查**

Run:

```bash
timeout 60s node --test \
  src/routes/timeline/summaryPresentation.test.js \
  src/routes/timeline/HourlySummaryDrawer.test.js \
  src/routes/timeline/TimelineSummaryRoute.test.js
git diff --check -- \
  src/routes/timeline/summaryPresentation.js \
  src/routes/timeline/summaryPresentation.test.js \
  src/routes/timeline/HourlySummaryDrawer.svelte \
  src/routes/timeline/HourlySummaryDrawer.test.js
```

Expected: 测试全部 PASS，`git diff --check` 无输出。

---

### Task 2: 建立全应用内容宽度与边界视觉原语

**Files:**
- Create: `src/ApplicationVisualPolish.test.js`
- Modify: `src/app.css`

**Interfaces:**
- Consumes: 现有 `:root`、`.dark`、`.page-shell`、`.page-header`、`.page-card` 和 A/B/C 风格覆盖。
- Produces: `--content-width-operation`、`--content-width-reading`、`--surface-border-subtle`、`--surface-border-default`、`--surface-border-emphasis` 与 `.page-axis-operation`、`.page-axis-reading`。

- [ ] **Step 1: 为共享视觉原语写失败测试**

创建 `src/ApplicationVisualPolish.test.js`：

```js
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const cssUrl = new URL('./app.css', import.meta.url);

test('全应用应提供操作与阅读内容轴以及三级边界 token', async () => {
  const css = await readFile(cssUrl, 'utf8');

  assert.match(css, /--content-width-operation:\s*78rem/);
  assert.match(css, /--content-width-reading:\s*64rem/);
  assert.match(css, /--surface-border-subtle:/);
  assert.match(css, /--surface-border-default:/);
  assert.match(css, /--surface-border-emphasis:/);
  assert.match(css, /\.page-axis-operation\s*\{[\s\S]*max-width:\s*var\(--content-width-operation\)/);
  assert.match(css, /\.page-axis-reading\s*\{[\s\S]*max-width:\s*var\(--content-width-reading\)/);
  assert.match(css, /\.page-axis-operation,[\s\S]*\.page-axis-reading\s*\{[\s\S]*margin-inline:\s*auto/);
});

test('深色模式边界 token 应使用低对比中性层级', async () => {
  const css = await readFile(cssUrl, 'utf8');

  assert.match(css, /\.dark\s*\{[\s\S]*--surface-border-subtle:\s*rgba\(48,\s*54,\s*61,\s*0\.58\)/);
  assert.match(css, /\.dark\s*\{[\s\S]*--surface-border-default:\s*rgba\(48,\s*54,\s*61,\s*0\.82\)/);
  assert.doesNotMatch(css, /--surface-border-default:\s*rgba\(255,\s*255,\s*255/);
});
```

- [ ] **Step 2: 运行视觉原语测试并确认失败**

Run:

```bash
timeout 60s node --test src/ApplicationVisualPolish.test.js
```

Expected: FAIL，新的 token 和页面轴尚不存在。

- [ ] **Step 3: 在 `:root` 和 `.dark` 中增加共享 token**

在 `src/app.css` 的现有设计 token 附近增加：

```css
:root {
  --content-width-operation: 78rem;
  --content-width-reading: 64rem;
  --surface-border-subtle: rgba(203, 213, 225, 0.62);
  --surface-border-default: rgba(148, 163, 184, 0.46);
  --surface-border-emphasis: rgba(79, 70, 229, 0.42);
}

.dark {
  --surface-border-subtle: rgba(48, 54, 61, 0.58);
  --surface-border-default: rgba(48, 54, 61, 0.82);
  --surface-border-emphasis: rgba(99, 102, 241, 0.58);
}
```

这些声明合并进现有 `:root` 与 `.dark`，不新建重复选择器块。

- [ ] **Step 4: 增加共享页面轴原语**

在 `.page-shell` 附近增加：

```css
.page-axis-operation,
.page-axis-reading {
  width: 100%;
  margin-inline: auto;
}

.page-axis-operation {
  max-width: var(--content-width-operation);
}

.page-axis-reading {
  max-width: var(--content-width-reading);
}
```

- [ ] **Step 5: 让共享表面优先消费边界 token**

将 `.page-card`、`.page-card-soft` 和需要统一的普通共享表面边界改为：

```css
.page-card {
  border-color: var(--surface-border-default);
}

.page-card-soft {
  border-color: var(--surface-border-subtle);
}
```

保留现有背景、圆角、内边距与阴影。A/B/C 风格可以继续调整圆角与密度，但不再覆盖为更亮的深色白边。

- [ ] **Step 6: 运行共享视觉测试和既有壳层测试**

Run:

```bash
timeout 60s node --test \
  src/ApplicationVisualPolish.test.js \
  src/AppShellVisual.test.js \
  src/SurfaceConsistency.test.js \
  src/UiVisualStyle.test.js
git diff --check -- src/app.css src/ApplicationVisualPolish.test.js
```

Expected: 全部 PASS；若既有测试仍断言旧硬编码边界，应将断言改为验证共享 token，而不是恢复旧亮边。

---

### Task 3: 统一助手与关于页的阅读内容轴

**Files:**
- Modify: `src/ApplicationVisualPolish.test.js`
- Modify: `src/routes/ask/Ask.svelte`
- Modify: `src/routes/about/About.svelte`
- Modify: `src/app.css`
- Modify: `src/routes/ask/AskEditorial.test.js`
- Modify: `src/routes/about/AboutStyles.test.js`

**Interfaces:**
- Consumes: Task 2 的 `.page-axis-reading` 与 `--surface-border-default`。
- Produces: 助手页头、上下文条、欢迎区、对话区、输入区共用阅读轴；关于页页头与主体共用阅读轴。

- [ ] **Step 1: 为页面内容轴接线写失败测试**

在 `ApplicationVisualPolish.test.js` 增加：

```js
test('助手与关于页应将页头和主体接入阅读内容轴', async () => {
  const [askSource, aboutSource] = await Promise.all([
    readFile(new URL('./routes/ask/Ask.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./routes/about/About.svelte', import.meta.url), 'utf8'),
  ]);

  assert.match(askSource, /ask-editorial-header page-axis-reading/);
  assert.match(askSource, /ask-context-strip page-axis-reading/);
  assert.match(askSource, /ask-thread-shell[^\"]*page-axis-reading/);
  assert.match(askSource, /ask-composer-shell[^\"]*page-axis-reading/);
  assert.match(aboutSource, /page-header page-axis-reading/);
  assert.match(aboutSource, /about-minimal-shell[^\"]*page-axis-reading/);
});
```

- [ ] **Step 2: 运行测试并确认失败**

Run:

```bash
timeout 60s node --test src/ApplicationVisualPolish.test.js
```

Expected: FAIL，页面尚未接入共享阅读轴。

- [ ] **Step 3: 为助手页面增加阅读轴类名**

在 `Ask.svelte` 中调整类名：

```svelte
<header class="ask-editorial-header page-axis-reading">
```

```svelte
<div class="ask-context-strip page-axis-reading">
```

```svelte
<section class="ask-welcome-panel page-axis-reading" aria-labelledby="ask-welcome-title">
```

```svelte
<div class="ask-thread-shell page-axis-reading flex min-h-full flex-col">
```

```svelte
<div class="ask-composer-shell page-axis-reading">
```

移除这些节点上重复的 `mx-auto max-w-4xl`，避免 Tailwind 最大宽度和共享内容轴互相竞争。

- [ ] **Step 4: 为关于页面增加阅读轴类名**

在 `About.svelte` 中调整：

```svelte
<div class="page-header page-axis-reading">
```

```svelte
<div class="w-full about-minimal-shell page-axis-reading">
```

移除 `about-minimal-shell` 上重复的 `mx-auto max-w-4xl`。

- [ ] **Step 5: 收敛助手和关于页深色普通边界**

在 `app.css` 中让助手上下文条、输入区外围和关于页普通卡片使用共享边界：

```css
.ask-context-strip,
.ask-composer-panel,
.about-principles-card,
.about-tech-stack {
  border-color: var(--surface-border-default);
}

.dark .ask-context-strip,
.dark .ask-composer-panel,
.dark .about-principles-card,
.dark .about-tech-stack {
  border-color: var(--surface-border-default);
  box-shadow: none;
}
```

只改外围线和重复内高光，不改变消息气泡、危险色、时间线语义色或按钮选中状态。

- [ ] **Step 6: 更新页面编辑约束测试**

在 `AskEditorial.test.js` 和 `AboutStyles.test.js` 中增加对 `.page-axis-reading` 的断言，并将旧的硬编码 `rgba(71, 85, 105, 0.5)` 断言替换为：

```js
assert.match(css, /\.dark \.about-principles-card[\s\S]*?border-color:\s*var\(--surface-border-default\);/);
```

助手测试同样验证共享 token，不断言旧的独立亮边颜色。

- [ ] **Step 7: 运行助手、关于和全局视觉测试**

Run:

```bash
timeout 60s node --test \
  src/ApplicationVisualPolish.test.js \
  src/routes/ask/AskEditorial.test.js \
  src/routes/ask/AskComposer.test.js \
  src/routes/about/About.test.js \
  src/routes/about/AboutEditorial.test.js \
  src/routes/about/AboutStyles.test.js
git diff --check -- \
  src/routes/ask/Ask.svelte \
  src/routes/about/About.svelte \
  src/app.css \
  src/ApplicationVisualPolish.test.js \
  src/routes/ask/AskEditorial.test.js \
  src/routes/about/AboutStyles.test.js
```

Expected: 全部 PASS，页面业务结构与交互测试不变。

---

### Task 4: 收敛设置页外壳并居中操作工作台

**Files:**
- Modify: `src/routes/settings/Settings.svelte`
- Modify: `src/app.css`
- Modify: `src/routes/settings/SettingsEditorial.test.js`
- Modify: `src/ApplicationVisualPolish.test.js`

**Interfaces:**
- Consumes: Task 2 的 `.page-axis-operation`、边界 token 和现有设置标签状态。
- Produces: 居中的 `78rem` 设置页头/工作台、轻量吸顶页头、纯布局 `.settings-stage-shell`、单层分类导航与内容边界。

- [ ] **Step 1: 为设置页结构写失败测试**

在 `SettingsEditorial.test.js` 增加或更新：

```js
test('设置页应使用居中的操作工作台并避免外壳套外壳', async () => {
  const [source, css] = await Promise.all([
    readFile(new URL('./Settings.svelte', import.meta.url), 'utf8'),
    readFile(new URL('../../app.css', import.meta.url), 'utf8'),
  ]);

  assert.match(source, /page-header page-axis-operation/);
  assert.match(source, /settings-editorial-board page-axis-operation/);
  assert.match(css, /\.settings-stage-shell\s*\{[^}]*min-width:\s*0/);
  assert.doesNotMatch(css, /\.settings-stage-shell\s*\{[^}]*(?:background|border|box-shadow):/);
  assert.match(css, /\.settings-editorial-shell \.page-header\s*\{[\s\S]*border-bottom:\s*1px solid var\(--surface-border-subtle\)/);
  assert.match(css, /\.settings-tab-rail\s*\{[\s\S]*border:\s*1px solid var\(--surface-border-subtle\)/);
});
```

- [ ] **Step 2: 运行设置编辑约束测试并确认失败**

Run:

```bash
timeout 60s node --test src/routes/settings/SettingsEditorial.test.js
```

Expected: FAIL，设置页尚未接入操作轴，Stage 仍有背景/边框/阴影。

- [ ] **Step 3: 将设置页头和工作台接入操作轴**

在 `Settings.svelte` 中调整：

```svelte
<div class="page-header page-axis-operation">
```

```svelte
<div class="w-full settings-editorial-board page-axis-operation">
```

页面标题、说明、状态和保存按钮结构保持不变。

- [ ] **Step 4: 将吸顶页头改为轻量分隔结构**

调整 `app.css`：

```css
.settings-editorial-shell .page-header {
  position: sticky;
  top: 0;
  z-index: 30;
  padding-block: 0.65rem;
  border: 0;
  border-bottom: 1px solid var(--surface-border-subtle);
  border-radius: 0;
  background: color-mix(in srgb, var(--editorial-surface-subtle) 92%, transparent);
  box-shadow: none;
  backdrop-filter: blur(14px);
}
```

保留现有吸顶位置和保存区布局；若项目目标浏览器不接受 `color-mix`，使用现有浅色/深色显式背景声明，不引入运行时脚本。

- [ ] **Step 5: 将 `.settings-stage-shell` 收敛为纯布局容器**

替换为：

```css
.settings-stage-shell {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 0.85rem;
}
```

删除该选择器及其深色覆盖中的背景、渐变、边框、圆角、内高光、阴影和额外内边距。内部 `.settings-card` 保留为唯一主要内容边界。

- [ ] **Step 6: 简化设置分类导航**

将分类导航收敛为：

```css
.settings-tab-rail {
  min-width: 0;
  padding: 0.45rem;
  border: 1px solid var(--surface-border-subtle);
  border-radius: var(--radius-md);
  background: var(--editorial-surface-subtle);
  box-shadow: none;
}

.settings-tab-rail-item {
  transform: none;
  text-align: start;
}

.settings-tab-rail-item:hover {
  transform: none;
}

.settings-tab-rail-item-active {
  border-color: var(--surface-border-default);
  background: var(--editorial-surface-featured);
  box-shadow: none;
}
```

深色覆盖只修改文字和背景，不再重新加入渐变或内高光。

- [ ] **Step 7: 运行设置页、壳层和风格回归**

Run:

```bash
timeout 60s node --test \
  src/routes/settings/SettingsEditorial.test.js \
  src/AppShellVisual.test.js \
  src/UiVisualStyle.test.js \
  src/ApplicationVisualPolish.test.js
git diff --check -- \
  src/routes/settings/Settings.svelte \
  src/routes/settings/SettingsEditorial.test.js \
  src/app.css \
  src/ApplicationVisualPolish.test.js
```

Expected: 全部 PASS；A/B/C 风格不出现重复边界或间距塌陷。

---

### Task 5: 完成设置分类横向导航与表单网格响应式

**Files:**
- Modify: `src/app.css`
- Create: `src/routes/settings/SettingsResponsive.test.js`
- Modify: `src/routes/settings/components/SettingsAI.svelte`
- Modify: `src/routes/settings/components/SettingsStorage.svelte`
- Modify: `src/routes/settings/components/nodeGateway/LocalApiPanel.svelte`
- Modify: `src/routes/settings/components/nodeGateway/BotCredentialsPanel.svelte`

**Interfaces:**
- Consumes: 现有 `settings-stage-layout`、`settings-tab-rail`、设置组件网格。
- Produces: `920px` 以下横向可滚动分类导航和小屏单列/中屏多列的表单网格。

- [ ] **Step 1: 写设置响应式失败测试**

创建 `SettingsResponsive.test.js`：

```js
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8');

test('设置分类在中小窗口应改为横向滚动导航', async () => {
  const css = await read('../../app.css');

  assert.match(css, /@media \(max-width:\s*920px\)[\s\S]*\.settings-tab-rail\s*\{[\s\S]*display:\s*flex/);
  assert.match(css, /@media \(max-width:\s*920px\)[\s\S]*\.settings-tab-rail\s*\{[\s\S]*overflow-x:\s*auto/);
  assert.match(css, /@media \(max-width:\s*920px\)[\s\S]*\.settings-tab-rail-item\s*\{[\s\S]*flex:\s*0 0 auto/);
});

test('设置页固定网格应在小屏使用单列', async () => {
  const [ai, storage, localApi, botCredentials] = await Promise.all([
    read('./components/SettingsAI.svelte'),
    read('./components/SettingsStorage.svelte'),
    read('./components/nodeGateway/LocalApiPanel.svelte'),
    read('./components/nodeGateway/BotCredentialsPanel.svelte'),
  ]);

  assert.match(ai, /grid-cols-1[^\"]*md:grid-cols-3/);
  assert.doesNotMatch(storage, /class="[^"]*grid-cols-2[^"]*"/);
  assert.match(storage, /grid-cols-1[^\"]*md:grid-cols-2/);
  assert.match(storage, /grid-cols-1[^\"]*sm:grid-cols-3/);
  assert.match(localApi, /grid-cols-1[^\"]*md:grid-cols-2/);
  assert.match(botCredentials, /class:grid-cols-2=\{false\}|settings-responsive-field-grid/);
});
```

- [ ] **Step 2: 运行响应式测试并确认失败**

Run:

```bash
timeout 60s node --test src/routes/settings/SettingsResponsive.test.js
```

Expected: FAIL，导航和固定网格尚未调整。

- [ ] **Step 3: 实现 `920px` 以下横向设置导航**

在现有 `@media (max-width: 920px)` 中增加：

```css
.settings-tab-rail {
  display: flex;
  gap: 0.4rem;
  overflow-x: auto;
  overscroll-behavior-inline: contain;
  scrollbar-width: thin;
  padding: 0.4rem;
}

.settings-tab-rail-item {
  flex: 0 0 auto;
  min-width: max-content;
}

.settings-tab-rail-item:focus-visible {
  outline-offset: -2px;
}
```

保留 `.settings-stage-layout { grid-template-columns: 1fr; }`。不使用下拉菜单，不隐藏文字标签。

- [ ] **Step 4: 将 AI 能力入口改为渐进式网格**

在 `SettingsAI.svelte` 将：

```svelte
<div class="mb-3 grid grid-cols-3 gap-2">
```

改为：

```svelte
<div class="mb-3 grid grid-cols-1 gap-2 md:grid-cols-3">
```

已有 `grid-cols-2 sm:grid-cols-3` 的紧凑选项保持不变，因为它已具备小屏降级。

- [ ] **Step 5: 将存储相关固定网格改为小屏单列**

在 `SettingsStorage.svelte` 中，把四处：

```svelte
<div class="grid gap-2 grid-cols-2">
```

统一改为：

```svelte
<div class="grid grid-cols-1 gap-2 md:grid-cols-2">
```

将存储统计：

```svelte
<div class="grid grid-cols-3 gap-3">
```

改为：

```svelte
<div class="grid grid-cols-1 gap-3 sm:grid-cols-3">
```

保留现有 `md:grid-cols-2` 区域，不重复修改。

- [ ] **Step 6: 调整本地 API 和 Bot 凭据网格**

在 `LocalApiPanel.svelte` 将：

```svelte
<div class="grid gap-2 grid-cols-2">
```

改为：

```svelte
<div class="grid grid-cols-1 gap-2 md:grid-cols-2">
```

在 `BotCredentialsPanel.svelte` 将动态固定两列：

```svelte
<div class="grid gap-2" class:grid-cols-2={fields.length > 2}>
```

改为语义类：

```svelte
<div class="settings-responsive-field-grid grid gap-2">
```

并在 `app.css` 增加：

```css
.settings-responsive-field-grid {
  grid-template-columns: 1fr;
}

@media (min-width: 768px) {
  .settings-responsive-field-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
```

- [ ] **Step 7: 运行设置响应式与功能测试**

Run:

```bash
timeout 60s node --test \
  src/routes/settings/SettingsResponsive.test.js \
  src/routes/settings/SettingsAiLayout.test.js \
  src/routes/settings/SettingsStorageCapture.test.js \
  src/routes/settings/SettingsNodeGateway.test.js \
  src/routes/settings/SettingsEditorial.test.js
git diff --check -- \
  src/app.css \
  src/routes/settings/SettingsResponsive.test.js \
  src/routes/settings/components/SettingsAI.svelte \
  src/routes/settings/components/SettingsStorage.svelte \
  src/routes/settings/components/nodeGateway/LocalApiPanel.svelte \
  src/routes/settings/components/nodeGateway/BotCredentialsPanel.svelte
```

Expected: 全部 PASS，网格调整不改变设置字段和事件绑定。

---

### Task 6: 全量回归、构建与工作区保护检查

**Files:**
- Verify only: 本计划涉及的全部文件。

**Interfaces:**
- Consumes: Tasks 1–5 的实现结果。
- Produces: 可验证的测试、构建和差异检查证据；不创建提交。

- [ ] **Step 1: 运行本次相关测试全集**

Run:

```bash
timeout 60s node --test \
  src/ApplicationVisualPolish.test.js \
  src/AppShellVisual.test.js \
  src/SurfaceConsistency.test.js \
  src/UiVisualStyle.test.js \
  src/routes/timeline/summaryPresentation.test.js \
  src/routes/timeline/HourlySummaryDrawer.test.js \
  src/routes/timeline/TimelineSummaryRoute.test.js \
  src/routes/timeline/TimelineLayout.test.js \
  src/routes/ask/AskEditorial.test.js \
  src/routes/ask/AskComposer.test.js \
  src/routes/about/About.test.js \
  src/routes/about/AboutEditorial.test.js \
  src/routes/about/AboutStyles.test.js \
  src/routes/settings/SettingsEditorial.test.js \
  src/routes/settings/SettingsResponsive.test.js \
  src/routes/settings/SettingsAiLayout.test.js \
  src/routes/settings/SettingsStorageCapture.test.js \
  src/routes/settings/SettingsNodeGateway.test.js
```

Expected: 全部 PASS。若失败，先确认是否为本次差异导致；不得通过恢复用户改动解决。

- [ ] **Step 2: 运行生产构建**

Run:

```bash
timeout 60s npm run build
```

Expected: Vite 构建成功。记录既有警告与新增警告的区别；本次不得新增 Svelte 编译警告。

- [ ] **Step 3: 运行定向差异格式检查**

Run:

```bash
git diff --check -- \
  src/app.css \
  src/ApplicationVisualPolish.test.js \
  src/routes/timeline/summaryPresentation.js \
  src/routes/timeline/summaryPresentation.test.js \
  src/routes/timeline/HourlySummaryDrawer.svelte \
  src/routes/timeline/HourlySummaryDrawer.test.js \
  src/routes/ask/Ask.svelte \
  src/routes/ask/AskEditorial.test.js \
  src/routes/about/About.svelte \
  src/routes/about/AboutStyles.test.js \
  src/routes/settings/Settings.svelte \
  src/routes/settings/SettingsEditorial.test.js \
  src/routes/settings/SettingsResponsive.test.js \
  src/routes/settings/components/SettingsAI.svelte \
  src/routes/settings/components/SettingsStorage.svelte \
  src/routes/settings/components/nodeGateway/LocalApiPanel.svelte \
  src/routes/settings/components/nodeGateway/BotCredentialsPanel.svelte
```

Expected: 无输出。

- [ ] **Step 4: 核对仅包含计划内差异且不触碰用户其他改动**

Run:

```bash
git status --short
git diff --stat -- \
  src/app.css \
  src/ApplicationVisualPolish.test.js \
  src/routes/timeline/summaryPresentation.js \
  src/routes/timeline/summaryPresentation.test.js \
  src/routes/timeline/HourlySummaryDrawer.svelte \
  src/routes/timeline/HourlySummaryDrawer.test.js \
  src/routes/ask/Ask.svelte \
  src/routes/ask/AskEditorial.test.js \
  src/routes/about/About.svelte \
  src/routes/about/AboutStyles.test.js \
  src/routes/settings/Settings.svelte \
  src/routes/settings/SettingsEditorial.test.js \
  src/routes/settings/SettingsResponsive.test.js \
  src/routes/settings/components/SettingsAI.svelte \
  src/routes/settings/components/SettingsStorage.svelte \
  src/routes/settings/components/nodeGateway/LocalApiPanel.svelte \
  src/routes/settings/components/nodeGateway/BotCredentialsPanel.svelte
```

Expected: 工作区仍可能存在用户原有修改，但本次只报告计划内新增差异；不暂存、不提交、不推送。

