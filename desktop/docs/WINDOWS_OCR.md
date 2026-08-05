# Windows 平台说明

本文档说明 Work Review 在 Windows 下的 OCR、URL 获取和打包情况。

## 当前支持

### 活动追踪

- 支持前台窗口应用识别
- 支持锁屏自动暂停
- 支持空闲检测，避免应用挂着不动时继续累计时长

### 浏览器 URL 获取

当前通过 Windows UI Automation 获取地址栏信息，优先覆盖：

- Chrome
- Edge
- Brave
- Firefox

同时也兼容常见 Chromium 系浏览器场景。

### OCR

Windows 当前默认使用**系统原生 OCR**。

特点：

- 无需额外安装 Python
- 开箱即用
- 识别效果以系统能力为准

仓库中同时保留了 PaddleOCR 相关脚本和安装指引，主要用于后续增强和调试能力；当前正式记录链路默认仍以 Windows 原生 OCR 为主。

## 可选的 PaddleOCR 安装命令

如果你要手动准备 PaddleOCR 环境，可参考：

```bash
pip install paddlepaddle paddleocr -i https://mirror.baidu.com/pypi/simple
```

说明：

- 这不是当前 Windows 默认必需依赖
- 正常使用 Work Review 不需要先安装它

## 构建 Windows 安装包

```bash
npm install
npm run tauri:build
```

当前 Windows 打包目标来自 `src-tauri/tauri.conf.json`，为 `nsis`。

常见产物位置：

```text
src-tauri/target/release/bundle/nsis/*.exe
```

## 补充说明

- 如果浏览器 URL 获取不稳定，通常和浏览器版本、窗口状态或 UI Automation 可访问性有关
- 如果 OCR 识别效果一般，这是 Windows 原生 OCR 的能力边界，不代表活动记录本身失效
- 实际工作时长统计主要依赖活动窗口与时间累计，OCR 主要用于辅助日报理解和关键词提取
