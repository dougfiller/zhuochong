import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('按小时活跃度图表应将所选时段直接显示在柱状图内', async () => {
  const source = await readFile(new URL('./ActivityHourlyChart.svelte', import.meta.url), 'utf8');

  assert.match(source, /let selectedHour = null/);
  assert.match(source, /function selectHour\(hour\)/);
  assert.match(source, /aria-pressed=\{selectedHour === bucket\.hour\}/);
  assert.match(source, /on:click=\{\(\) => selectHour\(bucket\.hour\)\}/);
  // 点击柱状图高亮选中（ring），详情显示在下方信息条而非浮动弹窗
  assert.match(source, /ring-2 ring-sky-300/);
  assert.doesNotMatch(source, /tooltipAlignmentClass/);
  assert.doesNotMatch(source, /hourlyChart\.selectedHour/);
  assert.doesNotMatch(source, /hourlyChart\.selectedHourHint/);
});

test('按小时活跃度图表文案不应继续维护独立所选时段提示', async () => {
  const source = await readFile(new URL('../i18n/locales/zh-CN.js', import.meta.url), 'utf8');

  assert.equal((source.match(/selectedHour:/g) || []).length, 0);
  assert.equal((source.match(/selectedHourHint:/g) || []).length, 0);
});

test('按小时活跃度图表应在图表下方显示当前选中时段信息条', async () => {
  const source = await readFile(new URL('./ActivityHourlyChart.svelte', import.meta.url), 'utf8');

  assert.match(source, /selectedBucket = buckets\[selectedHour\] \|\| null/);
  assert.match(source, /\{#if selectedBucket\}/);
  assert.match(source, /chart\.currentlySelected/);
  assert.match(source, /\{formatHourRangeLabel\(selectedBucket\.hour\)\}/);
  assert.match(source, /\{formatCompact\(selectedBucket\.duration\)\}/);
});

test('按小时活跃度图表无数据时不应把 00:00 显示成峰值', async () => {
  const source = await readFile(new URL('./ActivityHourlyChart.svelte', import.meta.url), 'utf8');

  assert.match(source, /hasActiveData = activeBuckets\.length > 0/);
  assert.match(source, /\{hasActiveData \? formatHourLabel\(peakBucket\.hour\) : '--'\}/);
  assert.match(source, /\{hasActiveData \? formatCompact\(peakBucket\.duration\) : '--'\}/);
});

test('按小时活跃度图表的柱子和时间刻度应共用同一套 24 列网格', async () => {
  const source = await readFile(new URL('./ActivityHourlyChart.svelte', import.meta.url), 'utf8');

  assert.equal((source.match(/grid-cols-\[repeat\(24,minmax\(0,1fr\)\)\]/g) || []).length, 2);
  assert.match(source, /<div class="grid h-44 grid-cols-\[repeat\(24,minmax\(0,1fr\)\)\] items-end gap-1">/);
  assert.match(source, /<div class="mt-3 grid grid-cols-\[repeat\(24,minmax\(0,1fr\)\)\] gap-1">/);
  assert.doesNotMatch(source, /hourAxisLabelAlignmentClass/);
});

test('按小时活跃度图表的紧凑时长应跟随当前语言', async () => {
  const source = await readFile(new URL('./ActivityHourlyChart.svelte', import.meta.url), 'utf8');

  assert.match(source, /formatDurationLocalized\(seconds,\s*\{\s*compact:\s*true\s*\}\)/);
  assert.match(source, /\{formatAxisTickLabel\(tick\)\}/);
  assert.doesNotMatch(source, /return `\$\{minutes \/ 60\}h`/);
  assert.doesNotMatch(source, /return `\$\{minutes\}m`/);
  assert.doesNotMatch(source, /return `\$\{s\}s`/);
});

test('按小时活跃度只保留竖向图并支持所选分类弱化与应用过滤', async () => {
  const source = await readFile(new URL('./ActivityHourlyChart.svelte', import.meta.url), 'utf8');

  assert.match(source, /export let selectedCategory = null/);
  assert.doesNotMatch(source, /export let mode/);
  assert.doesNotMatch(source, /\{#if mode === 'row'\}/);
  assert.match(source, /activity-hourly-category-segment/);
  assert.match(source, /activity-hourly-category-segment-selected/);
  assert.match(source, /activity-hourly-category-segment-muted/);
  assert.match(source, /selectedCategory && seg\.category !== selectedCategory/);
  assert.match(source, /\.filter\(\(app\) => !selectedCategory \|\| app\.category === selectedCategory\)/);
  assert.match(source, /activity-hourly-selected-apps/);
});
