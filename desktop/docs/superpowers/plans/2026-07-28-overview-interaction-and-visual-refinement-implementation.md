# Work Review 概览交互与全应用细节优化实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修正常驻网站跨浏览器聚合与详情语义，让今日节奏分类可点击，并统一概览、助手、关于、macOS 标题和隐私设置的视觉与交互。

**Architecture:** 后端保留完整域名聚合，概览接口只返回前 6 与真实总数；新增全域名摘要和单域名详情命令按需加载。前端把域名来源、分类摘要和模型名称计算提取为纯函数，通过 Svelte 状态驱动轻量内联交互；共享样式和多语言最后统一整合。

**Tech Stack:** Svelte 4、JavaScript、Node `node:test`、Tauri 2、Rust、SQLite、Tailwind/共享 CSS。

## Global Constraints

- 全程使用简体中文注释和文档。
- 不删除、还原或覆盖工作区现有未提交改动。
- 不修改 `CHANGELOG.md`。
- 不暂存、不提交、不推送，不创建分支或 worktree。
- 不新增第三方依赖。
- 单次测试或构建命令最长 60 秒。
- 深色边界使用共享中性 token，不新增亮白描边或多层内高光。
- 页面保持干净、清爽、简单；不新增抽屉和复杂筛选器。
- 共享 `src/app.css` 和四个 locale 文件仅由主流程统一修改，子任务不得并行写入。

---

### Task 1: 后端域名全量语义与小时隐私口径

**Files:**
- Modify: `crates/core/src/database.rs`
- Modify: `src-tauri/src/commands/stats.rs`
- Modify: `src-tauri/src/commands/mod.rs`（仅在需要显式导出时）
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/tauri.conf.json`

**Interfaces:**
- Produces: `DailyStats.domain_total_count: usize`，序列化键保持 snake_case。
- Produces: Tauri 命令 `get_overview_domains(mode, date, date_from, date_to) -> OverviewDomainCollection`。
- Produces: Tauri 命令 `get_overview_domain_detail(domain, mode, date, date_from, date_to) -> Option<OverviewDomainDetail>`。
- Produces: `OverviewDomainSummary { domain, duration, semantic_category, page_count, browser_sources }`。
- Produces: `OverviewDomainDetail { domain, duration, semantic_category, urls, browser_sources }`。
- Produces: `DomainBrowserSource { browser_name, duration, percentage, urls }`；摘要响应中的 `urls` 为空，详情响应携带当前域名 URL。
- Preserves: 现有 `get_overview_stats` 参数契约。

- [ ] **Step 1: 为多浏览器同域名、真实总数和跨日非截断写失败测试**

在 Rust 单元测试中构造两个浏览器访问同一域名、每日排名低但跨日累计高的活动数据，断言：

- 域名只出现一次。
- 总时长合并。
- `domain_total_count` 是隐私过滤后的唯一域名数。
- 跨日汇总不基于每日前 10 截断。
- 来源按时长倒序并计算稳定百分比。

- [ ] **Step 2: 运行专项 Rust 测试并确认 RED**

Run: `timeout 60 cargo test -p work-review-core domain_usage -- --nocapture`

Expected: FAIL，原因是当前 `domain_usage.truncate(10)`、缺少总数或缺少来源响应。

- [ ] **Step 3: 移除数据库层域名前 10 截断并设置真实总数**

- 在完整 URL 聚合之后保留全部 `domain_usage`。
- 为 `DailyStats` 增加带 `#[serde(default)]` 的 `domain_total_count`。
- 单日统计在隐私过滤完成后的命令层重新计算总数，避免被排除域名计数。
- `get_overview_stats_inner` 对首页响应只保留前 6 个域名，并把 `browser_usage[*].domains` 限制到这 6 个域名。

- [ ] **Step 4: 新增全域名摘要和单域名详情命令**

- 抽取 `load_overview_stats_full(...)`，统一 today/date/week 日期范围和隐私过滤。
- `get_overview_domains` 返回完整域名摘要，按时长降序，不携带 URL 明细。
- `get_overview_domain_detail` 只返回目标域名，聚合所有浏览器来源和 URL。
- 无法归属到浏览器的剩余时长作为“其他来源”，占比总和稳定在 100% 附近。
- 在 `generate_handler!` 注册两个新命令。

- [ ] **Step 5: 为小时应用明细补齐排除域名过滤并写回归测试**

- 把 `ignored_apps` 与 `excluded_domains` 一起传入数据库小时范围聚合。
- 在读取活动行、聚合应用之前过滤被排除的浏览器域名。
- 断言概览分类总量与小时应用明细不会重新包含黑名单域名。

- [ ] **Step 6: 隐藏 macOS 原生标题**

在 `src-tauri/tauri.conf.json` 主窗口配置中增加 `"hiddenTitle": true`；不清空 `title`，不修改装饰和侧边栏品牌。

- [ ] **Step 7: 运行后端专项验证**

Run: `timeout 60 cargo test -p work-review-core domain_usage -- --nocapture`

Run: `timeout 60 cargo test -p work-review-core hourly_app_breakdown -- --nocapture`

Run: `timeout 60 cargo test --manifest-path src-tauri/Cargo.toml commands::stats -- --nocapture`

Expected: PASS。

---

### Task 2: 概览域名展示与今日节奏分类交互

**Files:**
- Modify: `src/routes/overviewDomainPresentation.js`
- Modify: `src/routes/overviewDomainPresentation.test.js`
- Create: `src/routes/overviewCategoryPresentation.js`
- Create: `src/routes/overviewCategoryPresentation.test.js`
- Modify: `src/routes/Overview.svelte`
- Modify: `src/routes/Overview.test.js`
- Modify: `src/routes/OverviewEditorial.test.js`
- Modify: `src/lib/components/ActivityHourlyChart.svelte`
- Modify: `src/lib/components/ActivityHourlyChart.test.js`
- Modify: `src/lib/components/ActivityHourlyChartOverflow.test.js`

**Interfaces:**
- Consumes: `stats.domain_total_count`、前 6 `stats.domain_usage`、对应 `stats.browser_usage`。
- Consumes: `get_overview_domains` 与 `get_overview_domain_detail`。
- Produces: `buildDomainBrowserSources(domain, browserUsage)` 纯函数。
- Produces: `buildCategorySelectionSummary(category, categoryUsage, hourlyBreakdown)` 纯函数。
- Produces: `ActivityHourlyChart.selectedCategory` 属性，空值表示不筛选。
- Produces: `ActivityHourlyChart` 事件 `hourselect`，detail 为选中小时或 `null`。

- [ ] **Step 1: 重写域名展示纯函数测试并确认 RED**

断言：

- 不再导出或使用域名缩写、印章 class 和浏览器图标回退逻辑。
- 多浏览器来源按时长排序并计算占比。
- 单浏览器、多浏览器和未归属剩余时长均有稳定输出。
- URL 页面数取真实 URL 数量。

Run: `timeout 60 node --test src/routes/overviewDomainPresentation.test.js`

Expected: FAIL。

- [ ] **Step 2: 实现无图标域名展示模型并通过测试**

列表行只输出：分类细色标、域名、时长、页面数、分类、来源文字和比例轨道数据。

Run: `timeout 60 node --test src/routes/overviewDomainPresentation.test.js`

Expected: PASS。

- [ ] **Step 3: 为分类摘要纯函数写失败测试并确认 RED**

覆盖：分类总时长、占比、活跃小时合并为连续时段、主要应用排序、空数据和隐私过滤后零时长。

Run: `timeout 60 node --test src/routes/overviewCategoryPresentation.test.js`

Expected: FAIL。

- [ ] **Step 4: 实现分类摘要纯函数并通过测试**

Run: `timeout 60 node --test src/routes/overviewCategoryPresentation.test.js`

Expected: PASS。

- [ ] **Step 5: 修改 ActivityHourlyChart 为单一竖向图并先更新失败测试**

- 删除 `mode` 属性和横向 24 行模板。
- 新增 `selectedCategory`，选中后仅强调该分类分段。
- 小时详情在同时选中分类时只展示该分类应用。
- 保留小时点击、再次点击取消、键盘访问和 24 小时横轴。

Run: `timeout 60 node --test src/lib/components/ActivityHourlyChart.test.js src/lib/components/ActivityHourlyChartOverflow.test.js`

Expected: 旧横向模式断言失败，新行为测试在实现前失败。

- [ ] **Step 6: 修改 Overview 域名与分类交互**

- 删除 `hourlyActivityViewMode`、本地存储键、切换按钮和相关文案依赖。
- 分类构成分段改成按钮，维护 `selectedCompositionCategory`。
- 图表下方渲染内联分类摘要；再次点击或切换日期清除选择。
- 网站行不再调用 `getPrimaryDomainBrowser`，点击后请求域名详情。
- “查看全部”请求全域名摘要并显示现有浮层容器中的域名列表；选择域名再加载详情。
- 域名分类修改成功后刷新当前域名详情和概览统计。
- 页脚使用 `stats.domain_total_count`。
- KPI、今日节奏摘要和空状态居中；域名、应用和图表标签保持逻辑方向起点对齐。

- [ ] **Step 7: 更新概览结构测试并运行专项验证**

Run: `timeout 60 node --test src/routes/Overview.test.js src/routes/OverviewEditorial.test.js src/routes/overviewDomainPresentation.test.js src/routes/overviewCategoryPresentation.test.js src/lib/components/ActivityHourlyChart.test.js src/lib/components/ActivityHourlyChartOverflow.test.js`

Expected: PASS。

---

### Task 3: 助手模型标签、助手页头与关于页头

**Files:**
- Create: `src/routes/ask/modelPresentation.js`
- Create: `src/routes/ask/modelPresentation.test.js`
- Modify: `src/routes/ask/Ask.svelte`
- Modify: `src/routes/ask/AskEditorial.test.js`
- Modify: `src/routes/about/About.svelte`
- Modify: `src/routes/about/AboutEditorial.test.js`
- Modify: `src/ApplicationVisualPolish.test.js`

**Interfaces:**
- Produces: `resolveModelOptionLabel(selectedModelId, modelProfiles, locale, translate) -> string`。
- Preserves: 实际请求继续读取当前 `selectedModelId` 对应配置。

- [ ] **Step 1: 为模型名称解析写失败测试**

覆盖基础模型、已配置模型、异步配置从空变为有、无效选择回退和语言变化。

Run: `timeout 60 node --test src/routes/ask/modelPresentation.test.js`

Expected: FAIL，因为模块尚不存在。

- [ ] **Step 2: 实现纯函数并通过测试**

Run: `timeout 60 node --test src/routes/ask/modelPresentation.test.js`

Expected: PASS。

- [ ] **Step 3: 修复 Ask 响应式标签并统一页头轴**

- 用显式 `$:` 派生 `currentModelLabel`，依赖 `selectedModelId`、`modelProfiles` 和 locale。
- 顶部上下文与底部选择器宽度统一使用派生值。
- 页头使用操作轴；上下文、欢迎态、对话和输入器保留阅读轴。
- 页头结构对齐统一标题组，不叠加重复间距。

- [ ] **Step 4: 统一 About 页头轴**

仅把页头切到操作轴，主体继续使用阅读轴，不增加常用入口或新卡片。

- [ ] **Step 5: 更新并运行助手/关于专项测试**

Run: `timeout 60 node --test src/routes/ask/modelPresentation.test.js src/routes/ask/AskEditorial.test.js src/routes/about/AboutEditorial.test.js src/ApplicationVisualPolish.test.js`

Expected: PASS。

---

### Task 4: 隐私设置扁平分组与渐进展开

**Files:**
- Modify: `src/routes/settings/components/SettingsPrivacy.svelte`
- Create: `src/routes/settings/SettingsPrivacy.test.js`
- Modify: `src/routes/settings/SettingsEditorial.test.js`

**Interfaces:**
- Preserves: `config.privacy.app_rules` 数据结构与现有保存回调。
- Preserves: 敏感词、域名黑名单、运行中应用和批量添加行为。

- [ ] **Step 1: 写隐私设置结构失败测试**

断言：

- 使用单层 `.settings-block` 分组。
- 不再使用手动 `<hr>` 和按级别标签云。
- 规则按紧凑列表行渲染，删除按钮始终存在。
- 内容过滤默认折叠并显示关键词/域名数量摘要。
- 添加规则仍在当前区域内联展开。
- 顶部说明不再绝对声称任何数据都不会离开本机。

Run: `timeout 60 node --test src/routes/settings/SettingsPrivacy.test.js src/routes/settings/SettingsEditorial.test.js`

Expected: FAIL。

- [ ] **Step 2: 重构 SettingsPrivacy 模板但保留业务函数**

- 外层只保留一个设置容器。
- 应用规则改成应用名、策略、删除操作三列语义行。
- 添加规则使用搜索/运行中应用/记录方式/操作按钮的内联区域。
- 三种策略使用统一分段选择语义，不再铺大面积警示色。
- 内容过滤标题显示数量摘要，展开后沿用现有输入和保存逻辑。

- [ ] **Step 3: 运行隐私设置专项测试**

Run: `timeout 60 node --test src/routes/settings/SettingsPrivacy.test.js src/routes/settings/SettingsEditorial.test.js`

Expected: PASS。

---

### Task 5: 共享样式、多语言与集成验证

**Files:**
- Modify: `src/app.css`
- Modify: `src/lib/i18n/locales/zh-CN.js`
- Modify: `src/lib/i18n/locales/zh-TW.js`
- Modify: `src/lib/i18n/locales/en.js`
- Modify: `src/lib/i18n/locales/ar.js`
- Modify: 仅因新增文案而受影响的 i18n/视觉一致性测试

**Interfaces:**
- Consumes: Tasks 1–4 的最终 DOM class 和文案 key。
- Produces: 四语言一致的域名来源、分类摘要、全量网站和隐私数量摘要文案。

- [ ] **Step 1: 写或更新 i18n 与视觉 token 失败测试**

覆盖四种语言新增 key 完整、删除的横纵切换不再被模板引用、深色边界没有硬编码亮白外围线。

- [ ] **Step 2: 添加共享样式**

- 域名列表：无图标、分类细色标、来源比例轨道、紧凑详情。
- 分类选择：明确焦点态、选中态、弱化态和内联摘要。
- KPI/摘要居中但结构化列表不居中。
- 助手/关于页头操作轴。
- 隐私规则行、内联编辑、折叠摘要和窄屏单列。
- 深色模式统一使用现有边界 token。

- [ ] **Step 3: 补齐四语言文案**

避免新增重复 key；删除不再使用的横纵切换 key 仅在确认全仓无引用后进行。

- [ ] **Step 4: 运行所有受影响前端测试**

Run: `timeout 60 node --test src/routes/Overview.test.js src/routes/OverviewEditorial.test.js src/routes/overviewDomainPresentation.test.js src/routes/overviewCategoryPresentation.test.js src/lib/components/ActivityHourlyChart.test.js src/lib/components/ActivityHourlyChartOverflow.test.js src/routes/ask/modelPresentation.test.js src/routes/ask/AskEditorial.test.js src/routes/about/AboutEditorial.test.js src/routes/settings/SettingsPrivacy.test.js src/routes/settings/SettingsEditorial.test.js src/ApplicationVisualPolish.test.js src/AskI18n.test.js src/I18nLocaleFlow.test.js src/SurfaceConsistency.test.js src/UiVisualStyle.test.js`

Expected: PASS。

- [ ] **Step 5: 运行完整前端测试与构建**

Run: `timeout 60 node --test`

Run: `timeout 60 npm run build`

Expected: PASS，且无新增警告。

- [ ] **Step 6: 运行最终 Rust 验证**

Run: `timeout 60 cargo test -p work-review-core`

Run: `timeout 60 cargo test --manifest-path src-tauri/Cargo.toml commands::stats -- --nocapture`

Expected: PASS。

- [ ] **Step 7: 检查变更边界**

- `git diff --check` 无空白错误。
- `git status --short` 中不出现计划范围外的新改动。
- 不触碰 `CHANGELOG.md` 的现有用户改动。
- 不暂存、不提交。
