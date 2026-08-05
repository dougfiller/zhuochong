import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('存储设置应提供独立的启用截图开关，而不只是截图间隔', async () => {
  const source = await readFile(
    new URL('./components/SettingsStorage.svelte', import.meta.url),
    'utf8'
  );

  assert.match(source, /settingsStorage\.screenshotsEnabled/);
  assert.match(source, /config\.storage\.screenshots_enabled/);
  assert.match(source, /settingsStorage\.screenshotsEnabledHint/);
  assert.match(source, /role="switch"/);
  assert.match(source, /aria-label=\{t\('settingsStorage\.screenshotsEnabled'\)\}/);
  assert.match(source, /aria-checked=\{config\.storage\.screenshots_enabled\}/);
});

test('存储设置数值输入和自动导出开关应提供本地化可访问名称', async () => {
  const source = await readFile(
    new URL('./components/SettingsStorage.svelte', import.meta.url),
    'utf8'
  );

  for (const key of ['pollingInterval', 'retentionDays', 'storageLimitLabel', 'autoExport']) {
    assert.match(source, new RegExp(`aria-label=\\{t\\('settingsStorage\\.${key}'\\)\\}`));
  }
  assert.match(source, /aria-checked=\{config\.daily_report_auto_export\}/);
  assert.equal((source.match(/settingsStorage\.(?:showSecret|hideSecret)/g) || []).length, 6);
  assert.match(source, /aria-label=\{`\$\{t\(s3AccessKeyVisible/);
  assert.match(source, /aria-label=\{`\$\{t\(webdavPasswordVisible/);
});
