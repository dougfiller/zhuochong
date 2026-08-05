# README 最新界面截图刷新设计

## 目标

基于 2026-07-28 当前工作区中的最新应用界面，刷新 README 中已经发生明显变化的页面截图，并保证英文、简体中文、繁体中文三套资产一致、可复现、可验证。

## 更新范围

本轮只替换以下 README 既有资产，不改变 README 图片路径和展示尺寸：

- `概览.png`
- `日报.png`
- `设置-通用.png`
- `设置-存储.png`
- `工作流.gif`

输出目录：

- `docs/Introduction_en`
- `docs/Introduction_zh`
- `docs/Introduction_tw`

## 工具链前置条件

截图脚本依赖 Playwright、Playwright Chromium 和系统 `ffmpeg`。干净检出或 CI 环境必须先完成以下安装与检查，不能依赖本机 `node_modules` 中未声明的 extraneous 包或已缓存浏览器：

```bash
# Playwright 应作为项目开发依赖声明；尚未声明时执行一次
npm install --save-dev --save-exact playwright@1.61.1

# 安装与当前 Playwright 版本匹配的 Chromium
npx playwright install chromium

# 验证 Node.js 可以解析 Playwright
node -e "import('playwright').then(() => console.log('playwright ok'))"

# 验证 GIF 编码和媒体检查工具可用
command -v ffmpeg
command -v ffprobe
ffmpeg -version
ffmpeg -hide_banner -filters | grep -E 'palettegen|paletteuse'
```

`ffmpeg` 的具体安装方式由运行平台负责，例如 macOS 可使用 Homebrew，Linux/CI 使用系统包管理器。截图任务开始前必须确认 `ffmpeg`、`ffprobe` 位于 `PATH`，并记录 Node.js、Playwright、Chromium 和 ffmpeg 版本，以便定位跨环境渲染差异。

## 视觉与数据规范

- 使用当前工作区的 Vite 前端，而不是历史构建产物。
- 使用浅色主题、视觉样式 `b`。
- 浏览器视口固定为 `1491 × 841`，设备倍率为 `2`。
- PNG 输出尺寸固定为 `2982 × 1682`。
- GIF 输出尺寸固定为 `960 × 541`，固定为 `8 FPS`、`60` 帧，总时长约 `7.51` 秒。
- 三种语言分别使用 `en`、`zh-CN`、`zh-TW`。
- 浏览器上下文显式设置 `timezoneId: 'Asia/Shanghai'`。
- `CAPTURE_DATE`、`FIXED_TIME`、模拟数据中的 `created_at` 统一按 `Asia/Shanghai` 解释，避免运行机器时区导致日期跨天。
- 使用固定日期和固定模拟数据，避免真实用户数据、机器路径、密钥或截图内容进入仓库。
- 概览页展示新版洞察、KPI、今日节奏、应用和网站统计。
- 日报页展示新版 TL;DR、KPI、目录、正文和数据对照区域。
- 设置页分别停留在“通用”和“存储”面板，确保两张截图内容不同。
- 工作流 GIF 展示概览、日报和存储设置三个最新页面，使用最新全局样式和侧边栏。

## 服务地址与网络边界

截图脚本通过 `README_CAPTURE_BASE_URL` 连接 Vite 服务，默认值为 `http://127.0.0.1:5173`。脚本必须使用 `new URL(BASE_URL).origin` 动态计算允许访问的源，而不是硬编码默认地址。

- `README_CAPTURE_BASE_URL` 必须是可解析的 HTTP(S) URL。
- 页面导航和资源请求只允许访问该 URL 对应的动态 origin。
- `localhost`、不同端口或 CI 分配的地址只要与配置值同源，都应正常工作。
- 其他网络请求继续拦截，避免截图流程访问外部服务或泄露数据。

## 实现方案

新增一个统一的 Playwright 截图脚本，复用现有 Tauri 浏览器模拟方式：

1. 检查 Playwright 可加载、Chromium 可启动，并确认 `ffmpeg`、`ffprobe` 可执行。
2. 解析 `README_CAPTURE_BASE_URL`，以其动态 origin 建立网络白名单。
3. 在页面加载前注入 `window.__TAURI_INTERNALS__`。
4. 对截图涉及的 Tauri 命令返回确定性模拟数据。
5. 对每种语言创建独立浏览器上下文，并固定 `Asia/Shanghai` 时区。
6. 依次导航并截取四张 PNG。
7. 捕获一组工作流帧并编码为 GIF。
8. 覆盖对应语言目录中的既有同名文件。

Playwright 必须作为项目开发依赖显式声明；Chromium 和系统 ffmpeg 属于截图工具链前置条件，不得隐式依赖某位开发者机器上的缓存或 extraneous 包。

## 可复现性验证标准

可复现性分为“环境一致性”“结构一致性”和“视觉一致性”三层：

1. **环境一致性**：记录并尽量固定 Node.js、Playwright、Chromium、ffmpeg 版本、操作系统、字体和 `Asia/Shanghai` 时区。跨版本或跨平台结果不承诺字节级一致。
2. **结构一致性**：两次运行必须生成相同的文件集合；PNG 必须为 `2982 × 1682`；GIF 必须为 `960 × 541`、`8 FPS`、`60` 帧、时长约 `7.51` 秒。
3. **视觉一致性**：同一工具链下优先要求 SHA-256 完全一致。若 Chromium 对 SVG 或字体产生微小抗锯齿差异，不把文件哈希作为唯一门槛，而应比较解码后的像素或感知哈希；差异只能局限于已知抗锯齿边缘，不能出现文本、布局、颜色、数据或页面状态变化。
4. **差异处置**：出现非字节一致时必须保存并检查差异图；PNG 可接受的像素差异比例上限为总像素的 `0.02%`，GIF 按对应解码帧采用同一阈值。超过阈值或无法证明仅为抗锯齿差异时，视为不可复现并阻止更新。
5. **业务约束**：三种语言下的通用设置、存储设置必须各自内容不同；所有页面必须使用固定模拟数据，且不得出现加载态、错误提示、真实路径、令牌或远程地址。

## 验证标准

- 三个语言目录中的五个目标资产均已更新。
- 12 张 PNG 均精确为 `2982 × 1682`。
- 3 个 GIF 均精确为 `960 × 541`、`8 FPS`、`60` 帧，时长约 `7.51` 秒。
- 三种语言下 `设置-通用.png` 与 `设置-存储.png` 的内容哈希不同。
- README 现有图片引用仍全部有效。
- 按“可复现性验证标准”连续生成两次并完成字节级或容差级比较。
- `node --test --test-timeout=60000 README.test.js` 通过。
- `npm run build` 通过。
- 人工查看三种语言代表截图，确认无加载态、空白、弹窗遮挡、敏感信息或明显裁切。

## 非目标

- 不刷新未发生明显变化的时间线详情、助手、AI 模型、外观、桌面化身、隐私、接入管理和关于截图。
- 不修改应用业务逻辑、路由结构或产品文案。
- 不读取或展示本机真实 Work Review 数据。
