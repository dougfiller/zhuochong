import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('应用使用图表的柱状模式应使用共享坐标系而不是独立卡片', async () => {
  const source = await readFile(new URL('./AppUsageChart.svelte', import.meta.url), 'utf8');

  assert.match(source, /mode === 'column'/);
  assert.match(source, /app-usage-chart__columns/);
  assert.match(source, /app-usage-chart__plot/);
  assert.match(source, /app-usage-chart__bar/);
  assert.doesNotMatch(source, /grid-cols-2 gap-3 md:grid-cols-4/);
});

test('应用使用排行应与常驻网站保持六行、双层信息和总数页脚', async () => {
  const source = await readFile(new URL('./AppUsageChart.svelte', import.meta.url), 'utf8');

  assert.match(source, /const DEFAULT_COUNT = 6/);
  assert.match(source, /app-usage-chart__rows/);
  assert.match(source, /app-usage-chart__row/);
  assert.match(source, /app-usage-chart__heading/);
  assert.match(source, /overview\.appActivityCount/);
  assert.match(source, /overview\.appsFooter/);
});

test('应用使用图表的时长标签应保持单行显示', async () => {
  const source = await readFile(new URL('./AppUsageChart.svelte', import.meta.url), 'utf8');

  assert.match(source, /whitespace-nowrap/);
  assert.match(source, /tabular-nums/);
});
