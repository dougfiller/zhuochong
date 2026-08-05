import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('按小时活跃度图表时间刻度应与柱子同列对齐，避免首尾刻度溢出', async () => {
  const source = await readFile(new URL('./ActivityHourlyChart.svelte', import.meta.url), 'utf8');

  assert.equal((source.match(/grid-cols-\[repeat\(24,minmax\(0,1fr\)\)\]/g) || []).length, 2);
  assert.match(source, /<div class="min-w-0 text-center">/);
  assert.doesNotMatch(source, /hourAxisLabelAlignmentClass/);
});

test('按小时活跃度图表不应保留横向 24 行模式', async () => {
  const source = await readFile(new URL('./ActivityHourlyChart.svelte', import.meta.url), 'utf8');

  assert.doesNotMatch(source, /grid-cols-\[3\.25rem_minmax\(0,1fr\)_4\.75rem\]/);
  assert.doesNotMatch(source, /mode === 'row'/);
  assert.doesNotMatch(source, /const width = bucket\.duration/);
});
