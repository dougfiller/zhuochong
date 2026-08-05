# README 最新界面截图刷新实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** 使用当前工作区最新 UI 生成并替换三语言 README 中变化页面的截图和工作流动图。

**Architecture:** 新增统一 Playwright 截图脚本，在浏览器初始化阶段注入确定性的 Tauri 命令模拟，然后按语言和路由生成固定尺寸 PNG；工作流 GIF 由同一浏览器上下文的页面帧编码得到。README 保持原路径引用，仅替换资产并增强重复截图回归检查。

**Tech Stack:** Node.js、Playwright、Playwright Chromium、Vite、Node Test Runner、ffmpeg/ffprobe。

## Global Constraints

- 只更新 `概览.png`、`日报.png`、`设置-通用.png`、`设置-存储.png`、`工作流.gif`。
- 同时更新英文、简体中文、繁体中文三套资产。
- PNG 固定输出 `2982 × 1682`。
- GIF 固定输出 `960 × 541`、`8 FPS`、`60` 帧，总时长约 `7.51` 秒。
- 浏览器上下文固定使用 `timezoneId: 'Asia/Shanghai'`；`CAPTURE_DATE`、`FIXED_TIME`、`created_at` 采用同一时区约定。
- 使用模拟数据，不读取真实用户数据。
- 不覆盖或回退当前工作区中已有的未提交 UI 修改。

## Toolchain Preconditions

干净检出和 CI 环境不得依赖未声明的 extraneous Playwright 包、已有 Chromium 缓存或开发者机器上的 ffmpeg。执行截图前完成：

```bash
# 若 package.json 尚未声明，则先将 Playwright 加入开发依赖
npm install --save-dev --save-exact playwright@1.61.1

# 安装与 Playwright 版本匹配的 Chromium
npx playwright install chromium

# 检查 Node.js 依赖和浏览器入口
node -e "import('playwright').then(() => console.log('playwright ok'))"

# 检查 GIF 编码、探测命令和所需滤镜
command -v ffmpeg
command -v ffprobe
ffmpeg -version
ffmpeg -hide_banner -filters | grep -E 'palettegen|paletteuse'
```

运行记录中应保存 Node.js、Playwright、Chromium、ffmpeg 版本。macOS 可通过 Homebrew 安装 ffmpeg，Linux/CI 应通过对应系统包管理器安装；无论平台如何，`ffmpeg` 和 `ffprobe` 都必须位于 `PATH`。

## Base URL Contract

- `README_CAPTURE_BASE_URL` 默认为 `http://127.0.0.1:5173`，必须是有效的 HTTP(S) URL。
- 截图脚本使用 `new URL(BASE_URL).origin` 动态得到允许访问的 origin。
- 网络拦截只放行配置 URL 的同源页面和资源，不得硬编码 `http://127.0.0.1:5173`。
- 因此 `http://localhost:5173`、其他端口或 CI 动态地址只要与配置值同源，都应可用；其他外部请求继续阻断。

## Reproducibility Contract

- **环境层**：固定或记录 Node.js、Playwright、Chromium、ffmpeg、操作系统和字体版本，并显式使用 `Asia/Shanghai` 时区。
- **结构层**：两次生成的文件集合必须一致；PNG 必须为 `2982 × 1682`；GIF 必须为 `960 × 541`、`8 FPS`、`60` 帧、时长约 `7.51` 秒。
- **视觉层**：同一工具链优先要求 SHA-256 完全一致。若仅因 Chromium SVG/字体抗锯齿造成字节差异，则比较解码像素或感知哈希，并人工检查差异图。
- **容差上限**：PNG 像素差异不得超过总像素的 `0.02%`；GIF 每个对应解码帧采用同一阈值。差异只能位于已知抗锯齿边缘，文本、布局、颜色、数据和页面状态必须完全一致。
- 超出阈值、缺少差异图，或无法证明差异仅来自抗锯齿时，视为不可复现，不得更新 README 资产。

---

### Task 1: 固定截图资产回归约束

**Files:**
- Modify: `README.test.js`

**Interfaces:**
- Consumes: 三语言截图目录和既有 PNG/GIF 元数据读取函数。
- Produces: 精确媒体规格和设置页面截图不能重复的自动化断言。

- [x] **Step 1: 添加重复图片失败断言**

读取每种语言的 `设置-通用.png`、`设置-存储.png`，计算内容哈希并断言不同；同时对简体中文历史重复的 `设置-桌面化身.png`、`设置-隐私.png` 与新版通用设置图增加非重复断言。

- [x] **Step 2: 锁定精确媒体规格**

断言 12 张 PNG 均为 `2982 × 1682`；断言 3 个 GIF 均为 `960 × 541`、`8 FPS`、`60` 帧，时长约 `7.51` 秒，而不是只比较不同语言资产彼此一致。

- [x] **Step 3: 运行测试确认旧资产失败**

Run: `node --test --test-timeout=60000 README.test.js`
Expected: 新增的截图唯一性或精确规格测试在不合格资产上失败，并指出具体文件和实际值。

### Task 2: 建立统一截图脚本

**Files:**
- Create: `scripts/capture-readme-pages.mjs`
- Reuse: `scripts/capture-readme-hourly-summary.mjs`

**Interfaces:**
- Consumes: `README_CAPTURE_BASE_URL`，默认 `http://127.0.0.1:5173`。
- Produces: 三语言目录中的四张 PNG 和一张 GIF。

- [x] **Step 1: 定义固定配置和三语言演示数据**

加入固定日期、应用统计、网站统计、分类统计、日报正文、配置项与录制状态；数据中不得包含真实用户名、绝对路径、令牌或远程地址。`CAPTURE_DATE`、`FIXED_TIME` 和 `created_at` 必须按 `Asia/Shanghai` 对齐。

- [x] **Step 2: 固定浏览器时区并建立动态网络白名单**

浏览器上下文设置 `timezoneId: 'Asia/Shanghai'`。解析 `README_CAPTURE_BASE_URL`，用 `new URL(BASE_URL).origin` 作为唯一允许访问的动态 origin，确保 `localhost`、不同端口和 CI 地址按配置正常工作。

- [x] **Step 3: 注入完整 Tauri 浏览器模拟**

覆盖应用启动、概览、日报、设置页实际调用的命令；未知只读命令返回安全空值，写命令返回成功，避免截图期间出现错误 Toast。

- [x] **Step 4: 实现页面导航和稳定等待**

分别导航到概览、日报、设置通用、设置存储，等待页面的稳定可见元素和字体加载完成，再截取固定视口。存储页应使用稳定的标签 ID、`data-*` 标识或本地化可访问名称定位，并断言存储面板专属元素可见，不能依赖 `.nth(5)` 的标签顺序。

- [x] **Step 5: 实现工作流 GIF**

捕获概览、日报和存储设置的代表帧，使用 ffmpeg 编码为 `960 × 541`、`8 FPS`、`60` 帧、约 `7.51` 秒的 GIF；三种语言输出规格一致。

### Task 3: 生成并目视检查截图

**Files:**
- Modify: `docs/Introduction_en/概览.png`
- Modify: `docs/Introduction_en/日报.png`
- Modify: `docs/Introduction_en/设置-通用.png`
- Modify: `docs/Introduction_en/设置-存储.png`
- Modify: `docs/Introduction_en/工作流.gif`
- Modify: `docs/Introduction_zh/概览.png`
- Modify: `docs/Introduction_zh/日报.png`
- Modify: `docs/Introduction_zh/设置-通用.png`
- Modify: `docs/Introduction_zh/设置-存储.png`
- Modify: `docs/Introduction_zh/工作流.gif`
- Modify: `docs/Introduction_tw/概览.png`
- Modify: `docs/Introduction_tw/日报.png`
- Modify: `docs/Introduction_tw/设置-通用.png`
- Modify: `docs/Introduction_tw/设置-存储.png`
- Modify: `docs/Introduction_tw/工作流.gif`

**Interfaces:**
- Consumes: 正在运行的 Vite 服务与 Task 2 截图脚本。
- Produces: README 直接引用的新资产。

- [x] **Step 1: 启动当前工作区 Vite 服务**

Run: `npm run dev -- --host 127.0.0.1`
Expected: 服务监听 `http://127.0.0.1:5173`。

- [x] **Step 2: 执行前置检查**

Run: Toolchain Preconditions 中的 Playwright、Chromium、ffmpeg/ffprobe 检查命令。
Expected: Playwright 可导入、Chromium 可启动、ffmpeg/ffprobe 可执行且存在 `palettegen`、`paletteuse` 滤镜。

- [x] **Step 3: 执行截图脚本**

Run: `node scripts/capture-readme-pages.mjs`
Expected: 三种语言各生成四张 `2982 × 1682` PNG 和一张 `960 × 541`、`8 FPS`、`60` 帧、约 `7.51` 秒的 GIF。

- [x] **Step 4: 验证可配置 Base URL**

Run: `README_CAPTURE_BASE_URL=http://localhost:4174 node scripts/capture-readme-pages.mjs`
Expected: 脚本放行配置值对应的动态 origin 并成功生成资产；不访问其他外部源。

- [x] **Step 5: 逐类目视检查**

查看三种语言的概览和日报，以及每种设置页；确认语言、布局、数据、加载状态、裁切和隐私均符合设计。

### Task 4: 完整验证

**Files:**
- Verify: `README.md`
- Verify: `README.zh.md`
- Verify: `README.tw.md`
- Verify: `README.test.js`

**Interfaces:**
- Consumes: 新截图资产。
- Produces: 可证明 README 资产完整、媒体规格正确、生成结果可复现且前端可构建的验证结果。

- [x] **Step 1: 验证精确媒体规格和页面唯一性**

检查 12 张 PNG 均为 `2982 × 1682`；检查 3 个 GIF 均为 `960 × 541`、`8 FPS`、`60` 帧、时长约 `7.51` 秒；同语言的通用与存储截图哈希不同；15 个目标资产均在 Git 差异中。

- [x] **Step 2: 连续生成两次并比较可复现性**

在相同工具链、字体和 `Asia/Shanghai` 时区下连续生成两次。优先比较 SHA-256；若不一致，则对 PNG 和 GIF 解码帧生成像素差异图，确认差异比例不超过 `0.02%` 且只发生在已知 SVG/字体抗锯齿边缘。任何语义、布局、颜色、数据或页面状态差异均失败。

- [x] **Step 3: 运行 README 测试**

Run: `node --test --test-timeout=60000 README.test.js`
Expected: 全部测试通过，并锁定精确 PNG/GIF 规格和设置页面唯一性。

- [x] **Step 4: 运行前端构建**

Run: `npm run build`
Expected: 构建退出码为 `0`。

- [x] **Step 5: 检查最终差异**

Run: `git status --short && git diff --stat`
Expected: 只新增截图脚本、计划文档、测试，并修改目标截图；保留任务开始前已有的其他工作区改动。
