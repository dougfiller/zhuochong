# 时间线、常驻网站与关于页优化实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将已确认的时间线详情/小时摘要抽屉、常驻网站统一域名印章与分类 Popover、关于页居中简洁布局落地到正式项目，并统一深色模式的低对比边界。

**Architecture:** 时间线由 `Timeline.svelte` 继续负责数据加载和互斥抽屉状态，新增展示型 `HourlySummaryDrawer.svelte`，活动详情在原页面内改成右侧抽屉；小时摘要继续复用 `summaryPresentation.js` 的纯函数。常驻网站仅增加独立的域名展示纯函数并重排现有语义分类编辑，不改变数据模型；关于页只重排已有能力。旧摘要路由保留为兼容跳转，避免历史链接失效。

**Tech Stack:** Svelte、JavaScript、Tailwind/CSS、Node.js `node:test`、Vite、Tauri API。

## Global Constraints

- 全程使用简体中文沟通，新增代码注释优先使用简体中文。
- 测试驱动：每个功能先写失败测试并确认失败，再写最小实现并确认通过。
- 所有测试和构建命令最长运行 60 秒。
- 不删除、重置、暂存或覆盖工作区已有未提交改动；禁止 `git restore`、`git checkout --`、`git reset`、`git stash`。
- 不修改时间线、常驻网站和小时摘要的后端数据模型。
- 小时摘要只能表达自然小时范围，例如 `09:00–10:00`，不能伪造连续活动起止时间。
- 小时摘要不得新增分类、截图或“定位到时间线”等当前不存在的能力。
- 时间线详情与小时摘要只能同时打开一个；打开一个时必须关闭另一个。
- 深色模式外围边界参考概览/日报低对比体系：主卡约 `rgba(71, 85, 105, 0.5)`，内部细线使用 `#30363d` 或等价透明度；禁止亮白外围线和白色内高光。
- 页面标题、列表文本继续左对齐；品牌主体、独立状态、空状态、操作组、产品原则和技术栈优先居中；数值和时长继续右对齐。
- 保留关于页 GitHub、数据目录、赞助、二维码放大、Escape 关闭、更新检查、自动检查更新与 Wayland/Linux 现有测试约束。
- 旧路由 `#/timeline/summary/:date` 与 `#/timeline/summary` 暂时保留，但转到时间线抽屉。
- 当前工作区已有 `app.css`、`Overview.svelte` 和四个语言文件的未提交改动，只允许精确修改本次相关区块。
- 本轮不自动创建提交，避免把用户现有未提交改动混入提交；完成后仅汇报差异和验证结果。

---

## 文件边界

- 新建 `src/routes/timeline/HourlySummaryDrawer.svelte`：小时摘要抽屉的纯展示与交互。
- 新建 `src/routes/timeline/HourlySummaryDrawer.test.js`：小时摘要抽屉结构、状态和深色边界测试。
- 修改 `src/routes/timeline/summaryPresentation.js`：增加自然小时范围格式化纯函数。
- 修改 `src/routes/timeline/summaryPresentation.test.js`：补充小时范围和节奏边界测试。
- 修改 `src/routes/timeline/Timeline.svelte`：摘要入口、互斥抽屉状态、摘要刷新、活动详情抽屉和分类 Popover。
- 修改 `src/routes/timeline/TimelineLayout.test.js`、`Timeline.category.test.js`：抽屉入口和分类 Popover 的失败测试。
- 修改 `src/routes/timeline/Summary.svelte`：旧摘要页面收敛为兼容跳转。
- 修改/新增 `src/routes/timeline/TimelineSummaryRoute.test.js`，并调整 `SummaryLayout.test.js`、`src/I18nLocaleFlow.test.js`：验证旧路由兼容与新抽屉国际化流。
- 新建 `src/routes/overviewDomainPresentation.js`：生成统一域名印章缩写。
- 新建 `src/routes/overviewDomainPresentation.test.js`：域名缩写边界测试。
- 修改 `src/routes/Overview.svelte`、`src/routes/Overview.test.js`、`src/routes/OverviewEditorial.test.js`：统一印章和语义分类 Popover。
- 修改 `src/routes/about/About.svelte`、`About.test.js`、`AboutEditorial.test.js`：居中品牌卡、更新三列、原则卡和技术栈。
- 修改 `src/lib/i18n/locales/zh-CN.js`、`zh-TW.js`、`en.js`、`ar.js`：补充摘要与关于页文案。
- 精确修改 `src/app.css` 中关于页专属样式；不改概览和日报现有样式区。

---

### Task 1: 小时摘要右侧抽屉

**Files:**
- Create: `src/routes/timeline/HourlySummaryDrawer.svelte`
- Create: `src/routes/timeline/HourlySummaryDrawer.test.js`
- Modify: `src/routes/timeline/summaryPresentation.js`
- Modify: `src/routes/timeline/summaryPresentation.test.js`
- Modify: `src/routes/timeline/Timeline.svelte`
- Modify: `src/routes/timeline/TimelineLayout.test.js`
- Modify: `src/routes/timeline/Summary.svelte`
- Modify: `src/routes/timeline/SummaryLayout.test.js`
- Create: `src/routes/timeline/TimelineSummaryRoute.test.js`
- Modify: `src/I18nLocaleFlow.test.js`
- Modify: `src/lib/i18n/locales/zh-CN.js`
- Modify: `src/lib/i18n/locales/zh-TW.js`
- Modify: `src/lib/i18n/locales/en.js`
- Modify: `src/lib/i18n/locales/ar.js`

**Interfaces:**
- Consumes: `hourlySummaries`、`selectedDate`、`loading`、`error` 和现有 `getPrimarySummary()`、`getSecondarySummary()`、`getMainApps()`、`getSummaryRhythmMeta()`。
- Produces: `HourlySummaryDrawer`，属性为 `open:boolean`、`date:string`、`summaries:array`、`loading:boolean`、`error:string|null`、`onClose:function`；`formatHourRange(hour):string`。

- [x] **Step 1: 为自然小时和节奏边界写失败测试**

在 `summaryPresentation.test.js` 中断言：

```js
assert.equal(formatHourRange(9), '09:00–10:00');
assert.equal(formatHourRange(23), '23:00–24:00');
assert.equal(getSummaryRhythmMeta({ total_duration: 1199 }).key, 'light');
assert.equal(getSummaryRhythmMeta({ total_duration: 1200 }).key, 'steady');
assert.equal(getSummaryRhythmMeta({ total_duration: 2699 }).key, 'steady');
assert.equal(getSummaryRhythmMeta({ total_duration: 2700 }).key, 'deep');
```

- [x] **Step 2: 运行纯函数测试并确认失败**

Run:

```bash
timeout 60s node --test src/routes/timeline/summaryPresentation.test.js
```

Expected: FAIL，至少提示 `formatHourRange` 未导出。

- [x] **Step 3: 实现最小自然小时格式化函数**

在 `summaryPresentation.js` 中增加并导出：

```js
export function formatHourRange(hour) {
  const start = Math.max(0, Math.min(23, Number(hour) || 0));
  return `${String(start).padStart(2, '0')}:00–${String(start + 1).padStart(2, '0')}:00`;
}
```

保持现有节奏阈值分别为 20 分钟和 45 分钟。

- [x] **Step 4: 运行纯函数测试并确认通过**

Run:

```bash
timeout 60s node --test src/routes/timeline/summaryPresentation.test.js
```

Expected: PASS。

- [x] **Step 5: 为小时摘要抽屉写失败结构测试**

`HourlySummaryDrawer.test.js` 至少断言：

- `role="dialog"`、`aria-modal="true"`、`aria-labelledby`。
- 存在遮罩、右侧抽屉、关闭按钮。
- 使用 `formatHourRange` 与 `formatDurationLocalized`。
- 使用主摘要、副摘要、节奏、高峰、最多四个应用。
- 存在展开全文/收起。
- 存在 loading、error、empty 三种状态。
- 不导入 `LocalizedDatePicker`，不包含旧 `summary-editorial-shell`。
- 深色抽屉边框使用 `#30363d`/低透明度，且不含 `rgba(255, 255, 255, 0.8)` 类亮边。

- [x] **Step 6: 运行抽屉测试并确认失败**

Run:

```bash
timeout 60s node --test src/routes/timeline/HourlySummaryDrawer.test.js
```

Expected: FAIL，文件或新结构不存在。

- [x] **Step 7: 实现展示型小时摘要抽屉**

实现要求：

- 抽屉固定在右侧，桌面最大宽约 `42rem`，小屏全宽。
- 顶部显示日期与摘要数量。
- 列表按自然小时展示，中文正文左对齐，独立状态居中。
- 主要应用最多四个。
- 每条摘要独立保存展开状态，`date` 或 `summaries` 变化时清空展开状态。
- 打开后聚焦关闭按钮，关闭由父组件回调处理。
- `Escape` 关闭；点击抽屉内部不冒泡到遮罩。
- 深色外围边框收敛到 `rgba(48, 54, 61, 0.88)` 或等价低对比边界，阴影不产生光晕。

- [x] **Step 8: 运行抽屉测试并确认通过**

Run:

```bash
timeout 60s node --test src/routes/timeline/HourlySummaryDrawer.test.js
```

Expected: PASS。

- [x] **Step 9: 为时间线摘要入口和旧路由写失败测试**

更新 `TimelineLayout.test.js`，断言：

- 导入并挂载 `HourlySummaryDrawer`。
- 摘要入口使用 `<button>`，调用 `openSummaryDrawer`。
- 入口具有 `aria-haspopup="dialog"` 和 `aria-expanded`。
- 抽屉接收 `selectedDate`、`hourlySummaries`、加载和错误状态。
- 不再出现 `href="#/timeline/summary/{selectedDate}"`。

新增 `TimelineSummaryRoute.test.js`，断言：

- `App.svelte` 仍保留两个旧路由。
- `Summary.svelte` 使用 `window.location.replace` 或等价替换语义。
- 目标为 `#/timeline?date=<date>&summary=1`。
- 不再渲染旧独立摘要轨道。

同步调整 `SummaryLayout.test.js` 和 `I18nLocaleFlow.test.js`，不再要求独立摘要页的日期选择器。

- [x] **Step 10: 运行时间线/路由测试并确认失败**

Run:

```bash
timeout 60s node --test \
  src/routes/timeline/TimelineLayout.test.js \
  src/routes/timeline/SummaryLayout.test.js \
  src/routes/timeline/TimelineSummaryRoute.test.js \
  src/I18nLocaleFlow.test.js
```

Expected: FAIL，入口仍是链接、旧页面仍存在。

- [x] **Step 11: 在时间线接入摘要抽屉并实现兼容跳转**

`Timeline.svelte`：

- 增加 `showSummaryDrawer`、`summaryRefreshing`、`summaryRefreshError`、请求序号和入口按钮引用。
- 解析 `summary=1`，加载目标日期后自动打开抽屉。
- `openSummaryDrawer()` 先关闭活动详情，再打开摘要；使用当前缓存立即展示，并静默调用一次 `get_hourly_summaries`。
- 静默刷新使用日期快照和请求序号，防止旧请求覆盖新日期。
- 刷新失败保留旧摘要，仅向抽屉传入非阻断错误。
- 关闭后将焦点返回入口按钮。

`Summary.svelte`：

- 根据旧路由参数提取合法日期。
- 使用替换导航到 `#/timeline?date=YYYY-MM-DD&summary=1`；无日期时使用当前本地日期。
- 只保留极简跳转状态，不保留旧摘要视图。

- [x] **Step 12: 补充四种语言摘要文案并运行测试**

增加真实语义文案：

- `summaryCount`
- `activeDuration`
- 如现有键无法复用，再最小补充 `summaryRefreshFailed`

Run:

```bash
timeout 60s node --test \
  src/routes/timeline/TimelineLayout.test.js \
  src/routes/timeline/SummaryLayout.test.js \
  src/routes/timeline/TimelineSummaryRoute.test.js \
  src/routes/timeline/HourlySummaryDrawer.test.js \
  src/routes/timeline/summaryPresentation.test.js \
  src/I18nLocaleFlow.test.js
```

Expected: PASS。

---

### Task 2: 活动详情右侧抽屉与分类 Popover

**Files:**
- Modify: `src/routes/timeline/Timeline.svelte`
- Modify: `src/routes/timeline/TimelineLayout.test.js`
- Modify: `src/routes/timeline/Timeline.category.test.js`

**Interfaces:**
- Consumes: 现有 `selectedActivity`、`closeDetail()`、分类 Store、保存/新增/重命名/删除分类逻辑。
- Produces: 活动详情右侧抽屉；`showActivityCategoryPopover:boolean`；紧凑分类选择 Popover。

- [x] **Step 1: 为详情抽屉和分类 Popover 写失败测试**

在布局/分类测试中断言：

- 详情容器使用 `timeline-detail-drawer`，遮罩容器右对齐，不再使用 `max-w-3xl` 居中对话框。
- 详情具有 `role="dialog"`、`aria-modal="true"`、关闭按钮和 Escape 关闭。
- 分类入口使用 `aria-haspopup="listbox"` 或菜单语义。
- 分类浮层包含色点、分类名称、当前项勾选。
- 保存操作继续调用现有分类更新函数。
- 打开活动详情会关闭摘要抽屉。

- [x] **Step 2: 运行测试并确认失败**

Run:

```bash
timeout 60s node --test \
  src/routes/timeline/TimelineLayout.test.js \
  src/routes/timeline/Timeline.category.test.js
```

Expected: FAIL，新抽屉类和 Popover 结构不存在。

- [x] **Step 3: 将活动详情改为右侧抽屉**

最小重排现有详情内容：

- 遮罩保留全屏，内部改为 `justify-end`。
- 抽屉顶底留白、右侧贴边或小间距，桌面最大宽约 `42rem`，移动端全宽。
- 保留截图、应用、时间、窗口标题、URL 等现有真实字段。
- 详情正文继续左对齐；时长和状态值保持右对齐。
- 打开时关闭小时摘要抽屉，关闭后恢复触发活动卡焦点。
- 深色边框与小时摘要抽屉一致，不使用白色 inset。

- [x] **Step 4: 将分类按钮组收敛为紧凑 Popover**

- 默认仅显示当前分类的色点、名称和下拉箭头。
- 点击后展示现有分类列表。
- 每项含色点、名称，当前分类显示勾选。
- 选择后调用现有更新逻辑并关闭 Popover。
- 新增/重命名/删除分类入口仍可进入原有轻量编辑流程，不删除能力。
- 点击抽屉外或 Escape 时优先关闭 Popover，再关闭抽屉。

- [x] **Step 5: 运行测试并确认通过**

Run:

```bash
timeout 60s node --test \
  src/routes/timeline/TimelineLayout.test.js \
  src/routes/timeline/Timeline.category.test.js \
  src/routes/timeline/TimelineScreenshotMode.test.js
```

Expected: PASS。

---

### Task 3: 常驻网站统一域名印章与分类 Popover

**Files:**
- Create: `src/routes/overviewDomainPresentation.js`
- Create: `src/routes/overviewDomainPresentation.test.js`
- Modify: `src/routes/Overview.svelte`
- Modify: `src/routes/Overview.test.js`
- Modify: `src/routes/OverviewEditorial.test.js`

**Interfaces:**
- Consumes: `topDomains`、`faviconFailedDomains`、`getBrowserDomainDisplayLabel()`、现有语义分类 Store 和保存/编辑逻辑。
- Produces: `getDomainInitials(domain):string`；统一 `domain-stamp`；语义分类 `domain-category-popover`。

- [x] **Step 1: 为域名印章缩写写失败测试**

`overviewDomainPresentation.test.js` 至少断言：

```js
assert.equal(getDomainInitials('github.com'), 'GH');
assert.equal(getDomainInitials('feishu.cn'), 'FE');
assert.equal(getDomainInitials('www.figma.com'), 'FI');
assert.equal(getDomainInitials(''), '—');
```

还要覆盖大小写、协议、子域名和非拉丁字符的稳定兜底。

- [x] **Step 2: 运行纯函数测试并确认失败**

Run:

```bash
timeout 60s node --test src/routes/overviewDomainPresentation.test.js
```

Expected: FAIL，模块或函数不存在。

- [x] **Step 3: 实现最小域名缩写纯函数**

规则：

- 清理协议、路径、端口和 `www.`。
- 取主域名标签前两个字符并大写；英文站点保持 2 个字母。
- 无法解析或未识别页面返回稳定占位 `—`。
- 不依赖网络，不在纯函数内请求 favicon。

- [x] **Step 4: 运行纯函数测试并确认通过**

Run:

```bash
timeout 60s node --test src/routes/overviewDomainPresentation.test.js
```

Expected: PASS。

- [x] **Step 5: 为统一印章和分类 Popover 写失败测试**

更新 Overview 测试断言：

- 顶部常驻网站始终渲染 `domain-stamp` 和 `domain-stamp-initials`。
- favicon 只渲染为 `domain-stamp-favicon` 右下角小角标，不再替换主印章。
- favicon 失败后只隐藏角标，印章缩写保持不变。
- 浏览器详情中的分类编辑使用 `domain-category-popover`。
- Popover 选项包含色点、名称和当前项勾选。
- 现有 `set_domain_semantic_rule`、自定义语义分类增删改调用仍存在。

- [x] **Step 6: 运行 Overview 测试并确认失败**

Run:

```bash
timeout 60s node --test \
  src/routes/Overview.test.js \
  src/routes/OverviewEditorial.test.js
```

Expected: FAIL，现有实现仍为 favicon/单字母混排和展开式编辑区。

- [x] **Step 7: 落地统一域名印章**

在常驻网站列表中始终显示固定圆角底板：

- 中央显示 `getDomainInitials()` 的 1～2 个字符。
- favicon 仅作为右下角小角标；失败时不改变底板尺寸和字母布局。
- 未解析页面使用中性印章，不请求 favicon。
- 统计盒内部居中；域名、页面数继续左对齐；时长继续右对齐。
- 深色边框使用低对比 `#30363d`，角标无白色光圈。

- [x] **Step 8: 将域名语义分类编辑收敛为 Popover**

- 当前分类按钮只显示色点、名称和箭头。
- Popover 列表显示全部语义分类，当前项勾选。
- 选择后继续调用现有保存逻辑。
- 自定义分类的创建、重命名和删除能力保留在 Popover 的次级区域。
- 不合并时间线分类与网站语义分类数据模型。

- [x] **Step 9: 运行 Overview 测试并确认通过**

Run:

```bash
timeout 60s node --test \
  src/routes/overviewDomainPresentation.test.js \
  src/routes/Overview.test.js \
  src/routes/OverviewEditorial.test.js
```

Expected: PASS。

---

### Task 4: 关于页居中简洁重排

**Files:**
- Modify: `src/routes/about/About.svelte`
- Modify: `src/routes/about/About.test.js`
- Modify: `src/routes/about/AboutEditorial.test.js`
- Modify: `src/app.css`
- Modify: `src/lib/i18n/locales/zh-CN.js`
- Modify: `src/lib/i18n/locales/zh-TW.js`
- Modify: `src/lib/i18n/locales/en.js`
- Modify: `src/lib/i18n/locales/ar.js`

**Interfaces:**
- Consumes: 关于页现有更新、系统打开、赞助弹层和二维码交互函数。
- Produces: `about-brand-identity`、`about-update-grid`、`about-principles-card`、`about-tech-item` 居中结构。

- [x] **Step 1: 更新关于页布局断言并确认形成失败测试**

测试至少断言：

- 页面仍有左对齐标准页头：`about.pageTitle`、`about.pageSubtitle`。
- 品牌卡包含居中的 `about-brand-identity` 和 `about-brand-title-row`。
- GitHub、数据目录、赞助三个操作位于更新区域之前。
- `about-update-grid` 准确包含 3 个 `about-update-unit`。
- 更新反馈位于品牌卡内。
- 存在统一 `about-principles-card` 和 `about.productPrinciplesTitle`。
- 技术栈使用 `about-tech-item`，不再使用 `about-tech-pill`。
- 四种语言均包含 `pageSubtitle` 和 `productPrinciplesTitle`。

- [x] **Step 2: 运行关于页测试并确认失败**

Run:

```bash
timeout 60s node --test \
  src/routes/about/About.test.js \
  src/routes/about/AboutEditorial.test.js
```

Expected: FAIL，新结构和新文案不存在。

- [x] **Step 3: 重排品牌卡和更新区域**

页面结构：

```text
标准页头（左对齐）
居中内容容器
├── 品牌卡
│   ├── Logo
│   ├── Work Review + 版本号
│   ├── 产品说明
│   ├── GitHub / 打开数据目录 / 赞助支持
│   ├── 分隔线
│   └── 当前版本 / 自动检查更新 / 更新状态
├── 产品原则卡
│   └── 本地优先 / 清晰回看 / 轻量整理
└── 技术栈行
    └── Tauri 2 / Svelte / Rust / SQLite
```

注意：

- 默认不能静态宣称“已是最新”，只显示真实版本和“检查更新”操作。
- `updateStatus` 仅在检查产生真实结果后显示。
- 不恢复“常用入口”。
- 不修改更新行为和后端命令。

- [x] **Step 4: 精确调整关于页 CSS 与响应式居中**

只修改关于页专属选择器：

- 三列更新区桌面横排并居中，窄屏纵向排列。
- 分隔线使用 `border-inline-start`，兼容 RTL。
- 产品原则三列居中，正文可保持局部左对齐以便扫读。
- 技术栈在所有断点保持 `justify-content: center`。
- 深色边界统一到 `rgba(71, 85, 105, 0.5)` 或更低，不产生白色高光。

- [x] **Step 5: 补充四种语言关于页文案**

新增：

```text
about.pageSubtitle
about.productPrinciplesTitle
```

保留现有键名和现有功能文案；可把现有原则标题内容调整为“清晰回看”“轻量整理”，但不重命名键。

- [x] **Step 6: 运行关于页及国际化测试并确认通过**

Run:

```bash
timeout 60s node --test \
  src/routes/about/About.test.js \
  src/routes/about/AboutEditorial.test.js \
  src/I18nFeedback.test.js \
  src/I18nEnglishLeak.test.js
```

Expected: PASS。

---

### Task 5: 深色边界与全局对齐收敛

**Files:**
- Modify: `src/routes/timeline/Timeline.svelte`
- Modify: `src/routes/timeline/HourlySummaryDrawer.svelte`
- Modify: `src/routes/timeline/TimelineLayout.test.js`
- Modify: `src/routes/timeline/HourlySummaryDrawer.test.js`
- Modify only if necessary: `src/app.css`

**Interfaces:**
- Consumes: 概览/日报当前深色主卡边界基准 `rgba(71, 85, 105, 0.5)`。
- Produces: 时间线主卡、活动抽屉、摘要抽屉、Popover 的一致低对比边界。

- [x] **Step 1: 增加深色边界失败断言**

静态测试断言：

- 时间线主卡深色边框不高于 `.5` 基准。
- 深色时间轨道不包含 `rgba(248, 250, 252, 0.84)`。
- 普通节点不使用 `#e2e8f0`。
- 主卡、活动抽屉和摘要抽屉不含 `inset 0 1px 0 rgba(255, 255, 255, ...)` 白色内高光。
- Popover 不使用高透明白色描边。

- [x] **Step 2: 运行边界测试并确认失败**

Run:

```bash
timeout 60s node --test \
  src/routes/timeline/TimelineLayout.test.js \
  src/routes/timeline/HourlySummaryDrawer.test.js
```

Expected: FAIL，当前时间线仍有偏亮轨道、节点或 inset。

- [x] **Step 3: 最小收敛时间线局部深色样式**

- 主卡外围从 `.58` 收敛到 `.5`。
- 删除深色模式白色顶部 inset 和浅色 `::before` 高光。
- 轨道改用低对比灰蓝渐变或纯色细线。
- 普通节点改为中性灰蓝，选中/高峰节点保留强调色。
- 抽屉与 Popover 使用相同边界层级。
- 不修改概览和日报现有选择器。

- [x] **Step 4: 检查对齐规则**

逐项确认：

- 页面标题、时间线记录、摘要正文、域名信息仍左对齐。
- 活跃时长、网站时长、页面右上工具仍右对齐。
- 摘要空状态/加载状态、关于品牌/操作/原则/技术栈、域名印章内部居中。
- 两行以上中文正文不做强制居中。

- [x] **Step 5: 运行相关静态测试并确认通过**

Run:

```bash
timeout 60s node --test \
  src/routes/timeline/TimelineLayout.test.js \
  src/routes/timeline/HourlySummaryDrawer.test.js \
  src/routes/OverviewEditorial.test.js \
  src/routes/about/AboutEditorial.test.js
```

Expected: PASS。

---

### Task 6: 回归、构建、视觉验证与代码审查

**Files:**
- Verify all changed files.
- Do not modify unrelated files unless a failing test identifies a real regression.

**Interfaces:**
- Consumes: Tasks 1–5 的全部实现。
- Produces: 可重复的测试、构建、视觉截图和代码审查证据。

- [x] **Step 1: 运行相关前端测试全集**

Run:

```bash
timeout 60s node --test \
  src/routes/timeline/Timeline.category.test.js \
  src/routes/timeline/TimelineExport.test.js \
  src/routes/timeline/TimelineLayout.test.js \
  src/routes/timeline/TimelineScreenshotMode.test.js \
  src/routes/timeline/SummaryLayout.test.js \
  src/routes/timeline/TimelineSummaryRoute.test.js \
  src/routes/timeline/HourlySummaryDrawer.test.js \
  src/routes/timeline/summaryPresentation.test.js \
  src/routes/timeline/timelineData.test.js \
  src/routes/overviewDomainPresentation.test.js \
  src/routes/Overview.test.js \
  src/routes/OverviewEditorial.test.js \
  src/routes/about/About.test.js \
  src/routes/about/AboutEditorial.test.js \
  src/I18nLocaleFlow.test.js \
  src/I18nFeedback.test.js \
  src/I18nEnglishLeak.test.js
```

Expected: 0 failures。

- [x] **Step 2: 检查补丁格式与意外覆盖**

Run:

```bash
git diff --check
git diff --stat
git diff -- src/routes/timeline src/routes/about src/routes/Overview.svelte src/routes/Overview.test.js src/routes/OverviewEditorial.test.js src/routes/overviewDomainPresentation.js src/app.css src/lib/i18n/locales
```

Expected: `git diff --check` 退出码 0；差异只包含计划内精确改动，不覆盖概览/日报已有工作。

- [x] **Step 3: 运行生产构建**

Run:

```bash
timeout 60s npm run build
```

Expected: Vite build 退出码 0。

- [x] **Step 4: 启动本地页面并进行视觉验证**

- 启动开发服务器。
- 使用浏览器分别检查亮色和深色：
  - 时间线主页面。
  - 活动详情抽屉。
  - 小时摘要抽屉。
  - 常驻网站印章与分类 Popover。
  - 关于页。
- 至少检查桌面宽屏、约 1024px、约 640px。
- 检查默认 UI 风格和 `ui-style-c`。
- 截图保存到临时验证目录，不覆盖正式文档图片。

- [x] **Step 5: 执行代码审查**

审查重点：

- 是否存在抽屉状态竞态、双遮罩或焦点无法恢复。
- 是否保留旧摘要路由兼容。
- 是否错误伪造小时数据或更新状态。
- 是否破坏分类新增/重命名/删除能力。
- 是否在 `app.css` 或 `Overview.svelte` 带入无关改动。
- 是否存在深色白边、RTL 分隔线错误、移动端溢出。

- [x] **Step 6: 修复审查问题并重新运行完整验证**

任何代码修复后重新执行：

```bash
timeout 60s node --test <上面的相关前端测试全集>
timeout 60s npm run build
git diff --check
```

Expected: 全部退出码 0。

- [x] **Step 7: 最终汇报**

最终回答包含：

- 已修改/新增的文件与功能。
- 红绿测试和最终测试的通过数量。
- 生产构建结果。
- 视觉验证范围和仍需用户确认的截图。
- 明确说明没有重置或覆盖工作区已有改动。
