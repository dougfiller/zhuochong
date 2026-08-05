import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const privacySourceUrl = new URL('./components/SettingsPrivacy.svelte', import.meta.url);

async function readPrivacySource() {
  return readFile(privacySourceUrl, 'utf8');
}

test('隐私设置应使用单层 settings-block 分组并移除手动分隔与级别标签云', async () => {
  const source = await readPrivacySource();

  assert.match(source, /class="settings-card settings-privacy"/);
  assert.match(source, /class="settings-section settings-privacy-sections"/);
  assert.match(source, /class="settings-block settings-privacy-app-rules"/);
  assert.match(source, /class="settings-block settings-privacy-content-filter"/);
  assert.doesNotMatch(source, /<hr\b/);
  assert.doesNotMatch(source, /groupRules/);
  assert.doesNotMatch(source, /settings-chip-(?:success|warn|danger)/);
});

test('隐私设置顶部说明应只引用准确的新 i18n 文案键', async () => {
  const source = await readPrivacySource();

  assert.match(source, /t\('settingsPrivacy\.storageAndTransferDescription'\)/);
  assert.doesNotMatch(source, /t\('settingsPrivacy\.description'\)/);
  assert.doesNotMatch(source, /活动记录和截图默认保存在本机/);
});

test('应用规则应按紧凑语义行渲染并始终提供可访问删除操作', async () => {
  const source = await readPrivacySource();

  assert.match(source, /\{#each config\.privacy\.app_rules as rule, i\}/);
  assert.match(source, /class="settings-privacy-rule-list(?:\s|")/);
  assert.match(source, /class="settings-privacy-rule-row(?:\s|")/);
  assert.match(source, /class="settings-privacy-rule-app(?:\s|")/);
  assert.match(source, /class="settings-privacy-status-dot[^"]*" aria-hidden="true"/);
  assert.doesNotMatch(source, /settings-privacy-status-dot[^>]*>●<\/span>/);
  assert.match(source, /class="settings-privacy-rule-policy(?:\s|")/);
  assert.match(source, /class="settings-link-danger settings-privacy-rule-delete(?:\s|")/);
  assert.match(source, /aria-label=\{`\$\{t\('settingsPrivacy\.delete'\)\} \$\{rule\.app_name\}`\}/);
  assert.doesNotMatch(source, /group-hover:opacity-100/);
  assert.doesNotMatch(source, /settings-text-(?:success|warn|danger)/);
});

test('添加规则应保留原能力并使用统一的可访问分段选择', async () => {
  const source = await readPrivacySource();

  assert.match(source, /class="settings-privacy-rule-editor animate-fadeIn(?:\s|")/);
  assert.match(source, /bind:value=\{selectedApp\}/);
  assert.match(source, /bind:value=\{appSearchQuery\}/);
  assert.match(source, /\{#if runningApps\.length > 0\}/);
  assert.match(source, /toggleBatchApp\(app\)/);
  assert.match(source, /batchSelectedApps\.size/);
  assert.match(source, /role="radiogroup"/);
  assert.match(source, /role="radio"/);
  assert.match(source, /aria-checked=\{selectedLevel === level\.value\}/);
  assert.match(source, /settings-segment-active/);
  assert.doesNotMatch(source, /settings-segment-(?:success|warn|danger)/);
  assert.match(source, /rules\.push\(\{ app_name: appName, level: selectedLevel \}\)/);
});

test('内容过滤应默认折叠、显示数量摘要并暴露展开状态', async () => {
  const source = await readPrivacySource();

  assert.match(source, /let showContentFilter = false/);
  assert.match(source, /t\('settingsPrivacy\.contentFilterSummary', \{ keywordCount, domainCount \}\)/);
  assert.match(source, /aria-expanded=\{showContentFilter\}/);
  assert.match(source, /aria-controls="settings-privacy-content-filter-panel"/);
  assert.match(source, /id="settings-privacy-content-filter-panel"/);
  assert.match(source, /config\.privacy\.excluded_keywords/);
  assert.match(source, /config\.privacy\.excluded_domains/);
});

const appCssUrl = new URL('../../app.css', import.meta.url);

test('隐私设置语义 class 应使用共享低对比边界并提供窄屏布局', async () => {
  const css = await readFile(appCssUrl, 'utf8');

  assert.match(css, /\.settings-privacy-rule-editor\s*\{[\s\S]*var\(--surface-border-subtle\)/);
  assert.match(css, /\.settings-privacy-strategy-segments\s*\{/);
  assert.match(css, /\.settings-privacy-rule-row\s*\{/);
  assert.match(css, /\.settings-privacy-content-filter-panel\s*\{[\s\S]*var\(--surface-border-subtle\)/);
  assert.match(css, /@media \(max-width: 640px\)[\s\S]*\.settings-privacy-rule-row/);
  assert.doesNotMatch(css, /\.settings-privacy-(?:rule-editor|content-filter-panel)[^{]*\{[^}]*border[^;}]*rgba\(255,\s*255,\s*255/);
});
