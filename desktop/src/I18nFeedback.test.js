import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('交互反馈文案不应继续直接透出后端 message 或硬编码中文 fallback', async () => {
  const [storageSource, aiSource, updaterSource, aboutSource, appUsageSource] = await Promise.all([
    readFile(new URL('./routes/settings/components/SettingsStorage.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./routes/settings/components/SettingsAI.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./lib/utils/updater.js', import.meta.url), 'utf8'),
    readFile(new URL('./routes/about/About.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./lib/components/AppUsageChart.svelte', import.meta.url), 'utf8'),
  ]);

  assert.doesNotMatch(storageSource, /result\?\.message\s*\|\|/);
  assert.doesNotMatch(aiSource, /aiStore\.setError\("请先配置并测试 AI 模型连接"\)/);
  assert.doesNotMatch(aiSource, /aiStore\.setError\("必须先完成 API 连接测试才能保存"\)/);
  assert.match(updaterSource, /t\('updater\.upToDate'\)/);
  assert.match(updaterSource, /localizeRuntimeStatusMessage/);
  assert.doesNotMatch(aboutSource, /读取中\.\.\./);
  assert.match(appUsageSource, /t\('overview\.appsFooter'/);
});

test('日报模式标签在简中和繁中下不应退化为未翻译的 key', async () => {
  const [zhCN, zhTW] = await Promise.all([
    readFile(new URL('./lib/i18n/locales/zh-CN.js', import.meta.url), 'utf8'),
    readFile(new URL('./lib/i18n/locales/zh-TW.js', import.meta.url), 'utf8'),
  ]);

  assert.match(zhCN, /modeNames:\s*\{[\s\S]*local:\s*'基础模板'/);
  assert.match(zhTW, /modeNames:\s*\{[\s\S]*local:\s*'基礎模板'/);
});
