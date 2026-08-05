import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

// 防止 issue #111 回归：英文模式下不得有中文残留。
// 思路：纯源码静态扫描（不渲染组件），沿用 I18nLayout.test.js 的 readFile + assert 模式。

const SRC = new URL('../src/', import.meta.url);

// 仅覆盖 CJK 基本区，与 SettingsAI.svelte 中的判定范围保持一致。
const CJK = /[一-鿿]/;

// 去掉注释，避免注释里的中文样例误伤扫描。
function stripComments(src) {
  return src
    .replace(/<!--[\s\S]*?-->/g, '')
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/(^|[^:])\/\/[^\n]*/g, '$1');
}

async function collectSvelte(dir, out = []) {
  const dirPath = typeof dir === 'string' ? dir : fileURLToPath(dir);
  let entries;
  try {
    entries = await readdir(dirPath, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const entry of entries) {
    if (['node_modules', '.git', 'dist', 'target'].includes(entry.name)) continue;
    const full = join(dirPath, entry.name);
    if (entry.isDirectory()) {
      await collectSvelte(full, out);
    } else if (entry.name.endsWith('.svelte')) {
      out.push(full);
    }
  }
  return out;
}

function relativize(file) {
  return file.replace(/.*\/src\//, 'src/');
}

// 1) 所有 Svelte 模板里，title / aria-label / placeholder 的「字面量」属性值不得含中文。
//    字面量即 attr="..."；表达式 attr={t(...)} 不会被命中，因此强制 UI 文本走 i18n。
test('Svelte 字面量属性 title/aria-label/placeholder 不得含中文（英文模式会泄漏）', async () => {
  const files = await collectSvelte(SRC);
  assert.ok(files.length > 10, '应能扫描到 Svelte 文件');

  const attrRe = /\b(?:title|aria-label|placeholder)="[^"]*[一-鿿][^"]*"/g;
  const offenders = [];
  for (const file of files) {
    const src = stripComments(await readFile(file, 'utf8'));
    for (const match of src.matchAll(attrRe)) {
      offenders.push(`${relativize(file)}: ${match[0]}`);
    }
  }
  assert.deepEqual(offenders, [], `英文模式会泄漏中文的字面量属性：\n${offenders.join('\n')}`);
});

// 2) SettingsAI 测试连接的错误提示必须走 i18n，不得硬编码中文。
test('SettingsAI 测试连接错误提示应通过 i18n 输出而非硬编码中文', async () => {
  const src = await readFile(
    new URL('../src/routes/settings/components/SettingsAI.svelte', import.meta.url),
    'utf8',
  );
  const hardcoded = src.match(/return\s+'[^']*[一-鿿][^']*'/g);
  assert.equal(hardcoded, null, `SettingsAI 仍有硬编码中文错误提示：${hardcoded && hardcoded.join(' | ')}`);
  assert.match(src, /settingsAI\.testError\./, 'SettingsAI 应引用 settingsAI.testError.* 命名空间');
});

// 3) Ask 的 starter prompt 必须按 locale 取自 i18n，否则英文模式下 AI 会吐中文提问。
test('Ask starter prompt 应按 locale 取自 i18n 而非硬编码中文模板', async () => {
  const src = await readFile(new URL('../src/routes/ask/Ask.svelte', import.meta.url), 'utf8');
  assert.match(src, /t\(['"]ask\.starterSystemPrompt['"]\)/);
  assert.match(src, /t\(['"]ask\.starterUserPrompt['"]/);
  assert.match(src, /translateCategoryLabel/, 'topCategory 应通过 translateCategoryLabel 按当前 locale 翻译');
  assert.doesNotMatch(src, /\.join\('、'\)/, 'recentApps 分隔符应 locale 化，不得硬编码中文顿号');
  assert.doesNotMatch(src, /你是工作助手的 starter prompt 生成器/, '不得残留中文 prompt 模板');
});

// 4) 三语 locale 的 key 集合必须完全对齐，否则缺失 key 会 fallback 回 zh-CN 造成泄漏。
test('三语 locale 文件的 key 集合必须完全对齐（防止 fallback 泄漏）', async () => {
  const [zhCN, en, zhTW] = await Promise.all([
    import(new URL('../src/lib/i18n/locales/zh-CN.js', import.meta.url)),
    import(new URL('../src/lib/i18n/locales/en.js', import.meta.url)),
    import(new URL('../src/lib/i18n/locales/zh-TW.js', import.meta.url)),
  ]);

  const flatKeys = (obj, prefix = '', out = new Set()) => {
    for (const key of Object.keys(obj)) {
      const value = obj[key];
      const path = prefix ? `${prefix}.${key}` : key;
      if (value && typeof value === 'object' && !Array.isArray(value)) {
        flatKeys(value, path, out);
      } else {
        out.add(path);
      }
    }
    return out;
  };

  const zh = flatKeys(zhCN.default);
  const enKeys = flatKeys(en.default);
  const tw = flatKeys(zhTW.default);

  const enMissing = [...zh].filter((k) => !enKeys.has(k));
  const enExtra = [...enKeys].filter((k) => !zh.has(k));
  const twMissing = [...zh].filter((k) => !tw.has(k));

  assert.deepEqual(enMissing, [], `en 缺失 key（会 fallback 到中文）：${enMissing.join(', ')}`);
  assert.deepEqual(enExtra, [], `en 多余 key（zh-CN 没有）：${enExtra.join(', ')}`);
  assert.deepEqual(twMissing, [], `zh-TW 缺失 key：${twMissing.join(', ')}`);
});
