import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('应用应提供 A/B/C 三套可持久化界面风格并作用到根壳层', async () => {
  const [
    appSource,
    appCssSource,
    settingsSource,
    appearanceSource,
    configSource,
    zhCNSource,
    enSource,
    zhTWSource,
  ] = await Promise.all([
    readFile(new URL('./App.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./app.css', import.meta.url), 'utf8'),
    readFile(new URL('./routes/settings/Settings.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./routes/settings/components/SettingsAppearance.svelte', import.meta.url), 'utf8'),
    readFile(new URL('../crates/core/src/config.rs', import.meta.url), 'utf8'),
    readFile(new URL('./lib/i18n/locales/zh-CN.js', import.meta.url), 'utf8'),
    readFile(new URL('./lib/i18n/locales/en.js', import.meta.url), 'utf8'),
    readFile(new URL('./lib/i18n/locales/zh-TW.js', import.meta.url), 'utf8'),
  ]);

  assert.match(configSource, /pub ui_visual_style: String/);
  assert.match(configSource, /default_ui_visual_style/);
  assert.match(configSource, /normalize_ui_visual_style/);
  assert.match(configSource, /"c"\.to_string\(\)/);

  assert.match(settingsSource, /config\.ui_visual_style/);
  assert.match(settingsSource, /config\.ui_visual_style = 'c'/);
  assert.match(settingsSource, /import SettingsAppearance/);
  assert.match(settingsSource, /settings\.tabs\.appearance/);
  assert.match(settingsSource, /activeTab === 'appearance'/);
  assert.match(settingsSource, /function handleSettingsChange/);
  assert.match(settingsSource, /event\.detail\?\.autosaved/);
  assert.match(settingsSource, /<SettingsAppearance bind:config mode="background-only" on:change=\{handleSettingsChange\} \/>/);

  assert.match(appearanceSource, /UI_VISUAL_STYLE_OPTIONS/);
  assert.match(appearanceSource, /id:\s*'a'/);
  assert.match(appearanceSource, /id:\s*'b'/);
  assert.match(appearanceSource, /id:\s*'c'/);
  assert.match(appearanceSource, /config\.ui_visual_style/);
  assert.match(appearanceSource, /selectUiVisualStyle/);
  assert.match(appearanceSource, /ui-visual-style-changed/);
  assert.match(appearanceSource, /autosaved:\s*true/);
  assert.match(appearanceSource, /settings-style-preview__sidebar/);
  assert.match(appearanceSource, /settings-style-preview__topbar/);
  assert.match(appearanceSource, /settings-style-preview__metric/);
  assert.match(appearanceSource, /settings-style-preview__chart/);
  assert.match(appearanceSource, /settings-style-current-mark/);
  assert.match(appearanceSource, /settingsAppearance\.uiStyleCurrent/);
  assert.match(appearanceSource, /settingsAppearance\.uiVisualStyleApplyHint/);

  assert.match(appSource, /uiVisualStyle/);
  assert.match(appSource, /let uiVisualStyle = 'c'/);
  assert.match(appSource, /applyUiVisualStyle/);
  assert.match(appSource, /return \['a', 'b', 'c'\]\.includes\(nextStyle\) \? nextStyle : 'c'/);
  assert.match(appSource, /ui-style-\{uiVisualStyle\}/);
  assert.match(appSource, /ui-visual-style-changed/);
  assert.match(appSource, /app-shell-ambient/);

  assert.match(appCssSource, /\.app-shell\.ui-style-a\b/);
  assert.match(appCssSource, /\.app-shell\.ui-style-b\b/);
  assert.match(appCssSource, /\.app-shell\.ui-style-c\b/);
  assert.match(appCssSource, /\.app-shell\.ui-style-a\s+\.app-shell-ambient\s*\{[^}]*display:\s*none/);
  assert.match(appCssSource, /\.app-shell\.ui-style-a\s+\.page-card,\s*[\s\S]*?\.app-shell\.ui-style-a\s+\.page-card-soft\s*\{[\s\S]*?box-shadow:\s*none/);
  assert.match(appCssSource, /\.dark\s+\.app-shell\.ui-style-a\s*\{[^}]*--surface-border-subtle:\s*rgba\(48,\s*54,\s*61,\s*0\.58\)[^}]*--surface-border-default:\s*rgba\(48,\s*54,\s*61,\s*0\.82\)/);
  assert.doesNotMatch(appCssSource, /\.dark\s+\.app-shell\.ui-style-a\s+\.page-card,\s*[\s\S]*?\.dark\s+\.app-shell\.ui-style-a\s+\.page-card-soft\s*\{[^}]*border-color:/);
  assert.match(appCssSource, /\.app-shell\.ui-style-a\s+\.overview-lead-card\s*\{[\s\S]*?display:\s*contents/);
  assert.match(appCssSource, /\.app-shell\.ui-style-c\s+\.app-shell-stage\s*\{[\s\S]*?grid-template-columns:\s*12rem minmax\(0,\s*1fr\)/);
  assert.match(appCssSource, /\.app-shell\.ui-style-c\s+\.overview-summary-grid\s*\{[\s\S]*?gap:\s*0\.5rem/);
  assert.match(appCssSource, /\.app-shell\.ui-style-c\s+\.page-section-title\s*\{[\s\S]*?font-size:\s*0\.92rem/);
  assert.match(appCssSource, /\.page-shell\s*\{[\s\S]*?max-width:\s*108rem/);
  assert.match(appCssSource, /\.app-shell-stage\s*\{[\s\S]*?max-width:\s*136rem/);
  assert.match(appCssSource, /\.settings-style-preview__sidebar\b/);
  assert.match(appCssSource, /\.settings-style-current-mark\b/);
  assert.match(appCssSource, /\.settings-style-option-active\s+\.settings-style-current-mark/);

  for (const source of [zhCNSource, enSource, zhTWSource]) {
    assert.match(source, /uiVisualStyle:/);
    assert.match(source, /uiStyleATitle/);
    assert.match(source, /uiStyleBTitle/);
    assert.match(source, /uiStyleCTitle/);
    assert.match(source, /uiStyleCurrent/);
    assert.match(source, /uiVisualStyleApplyHint/);
  }
});
