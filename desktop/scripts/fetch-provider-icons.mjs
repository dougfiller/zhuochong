// 拉取 AI 服务商品牌图标到 public/icons/providers/{id}.svg
//
// 用法:  node scripts/fetch-provider-icons.mjs
//
// 图标源: @lobehub/icons-static-svg (MIT, https://github.com/lobehub/lobe-icons)
// 各品牌 Logo 商标归各自厂商所有,仅用于在配置界面标识对应服务。
// UI 侧对缺失图标有字母块回退,本脚本失败不影响功能。

import { mkdirSync, writeFileSync } from 'node:fs';
import { get } from 'node:https';
import { fileURLToPath } from 'node:url';

const OUT_DIR = fileURLToPath(new URL('../public/icons/providers/', import.meta.url));
const CDN = 'https://cdn.jsdelivr.net/npm/@lobehub/icons-static-svg@latest/icons/';

// 本项目 provider id → lobehub 图标名候选(优先彩色版,依次尝试)
const CANDIDATES = {
  ollama: ['ollama'],
  openai: ['openai'],
  siliconflow: ['siliconcloud-color', 'siliconcloud'],
  deepseek: ['deepseek-color', 'deepseek'],
  qwen: ['qwen-color', 'qwen'],
  zhipu: ['zhipu-color', 'zhipu', 'chatglm-color', 'chatglm'],
  moonshot: ['moonshot', 'kimi-color', 'kimi'],
  doubao: ['doubao-color', 'doubao'],
  minimax: ['minimax-color', 'minimax'],
  gemini: ['gemini-color', 'gemini'],
  claude: ['claude-color', 'claude', 'anthropic'],
  openrouter: ['openrouter'],
  groq: ['groq'],
  xai: ['xai', 'grok'],
  mistral: ['mistral-color', 'mistral'],
  lmstudio: ['lmstudio'],
  // custom 无品牌,保留字母块
};

function fetchText(url) {
  return new Promise((resolve, reject) => {
    get(url, (res) => {
      if (res.statusCode !== 200) {
        res.resume();
        reject(new Error(`HTTP ${res.statusCode}`));
        return;
      }
      let data = '';
      res.on('data', (chunk) => (data += chunk));
      res.on('end', () => resolve(data));
    }).on('error', reject);
  });
}

mkdirSync(OUT_DIR, { recursive: true });

let ok = 0;
let miss = 0;
for (const [id, names] of Object.entries(CANDIDATES)) {
  let saved = false;
  for (const name of names) {
    try {
      const svg = await fetchText(`${CDN}${name}.svg`);
      if (!svg.includes('<svg')) throw new Error('非 SVG 内容');
      writeFileSync(`${OUT_DIR}${id}.svg`, svg);
      console.log(`✓ ${id}  (${name})`);
      ok += 1;
      saved = true;
      break;
    } catch {
      // 尝试下一个候选名
    }
  }
  if (!saved) {
    console.warn(`✗ ${id}  未找到图标,界面将回退为字母块`);
    miss += 1;
  }
}

console.log(`\n完成: ${ok} 个成功${miss ? `, ${miss} 个缺失(有字母块回退)` : ''}`);
