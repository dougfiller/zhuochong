import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('本地化日期选择器应支持 single 与 range 两种模式，并基于传入 locale 渲染', async () => {
  const source = await readFile(new URL('./LocalizedDatePicker.svelte', import.meta.url), 'utf8');

  assert.match(source, /export let mode = 'single';/);
  assert.match(source, /export let localeCode = 'zh-CN';/);
  assert.match(source, /Intl\.DateTimeFormat\(localeCode/);
  assert.match(source, /mode === 'range'/);
  assert.match(source, /startDate/);
  assert.match(source, /endDate/);
  assert.match(source, /localized-date-picker__selection/);
  assert.match(source, /localized-date-picker__selection-chip/);
  assert.match(source, /startDate && endDate && startDate === endDate/);
});

test('本地化日期选择器应通过自定义面板展示日期，而不再依赖原生 type=date', async () => {
  const source = await readFile(new URL('./LocalizedDatePicker.svelte', import.meta.url), 'utf8');
  const appCss = await readFile(new URL('../../app.css', import.meta.url), 'utf8');

  assert.doesNotMatch(source, /type="date"/);
  assert.match(source, /popover/);
  assert.match(source, /inlinePanel/);
  assert.match(source, /localized-date-picker__popover--inline/);
  assert.match(source, /function portalToBody/);
  assert.match(source, /document\.body\.appendChild\(node\)/);
  assert.match(source, /updateFloatingPanelPosition/);
  assert.match(source, /localized-date-picker__popover--floating/);
  assert.match(appCss, /\.localized-date-picker__popover--floating/);
  assert.match(source, /document\.addEventListener\('mousedown'/);
  assert.match(source, /document\.addEventListener\('scroll'/);
  assert.match(source, /window\.addEventListener\('resize'/);
  assert.match(source, /keydown/);
  assert.match(source, /selectStart/);
  assert.match(source, /selectEnd/);
  assert.match(source, /localized-date-picker__day--start/);
  assert.match(source, /localized-date-picker__day--end/);
});
