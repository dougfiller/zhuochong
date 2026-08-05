import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

async function readDrawerSource() {
  return readFile(new URL('./HourlySummaryDrawer.svelte', import.meta.url), 'utf8');
}

test('时段摘要抽屉应遍历最近时间优先的展示数组', async () => {
  const source = await readDrawerSource();

  assert.match(source, /orderHourlySummariesForDisplay/);
  assert.match(source, /\$:\s*displaySummaries\s*=\s*orderHourlySummariesForDisplay\(summaries\)/);
  assert.match(source, /\{#each\s+displaySummaries\s+as\s+summary\s+\(summary\.hour\)\}/);
  assert.match(source, /summaryCount[\s\S]*summaries\.length/);
  assert.match(source, /peakDuration\s*=\s*summaries\.reduce/);
});

test('小时摘要应以右侧对话框抽屉展示并支持遮罩、关闭按钮与 Escape', async () => {
  const source = await readDrawerSource();

  assert.match(source, /export let open = false/);
  assert.match(source, /hourly-summary-overlay/);
  assert.match(source, /hourly-summary-drawer/);
  assert.match(source, /import \{ trapFocus \} from '\$lib\/utils\/focusTrap\.js'/);
  assert.match(source, /<aside[\s\S]*use:trapFocus[\s\S]*role="dialog"/);
  assert.match(source, /aria-modal="true"/);
  assert.match(source, /aria-labelledby="hourly-summary-title"/);
  assert.match(source, /on:click\|self=\{requestClose\}/);
  assert.match(source, /event\.key === 'Escape'/);
  assert.match(source, /bind:this=\{closeButton\}/);
  assert.match(source, /aria-label=\{t\('window\.close'\)\}/);
  assert.doesNotMatch(source, /t\('common\.close'\)/);
});

test('小时摘要抽屉应显示真实自然小时字段、活跃时长、节奏、高峰与最多四个应用', async () => {
  const source = await readDrawerSource();

  assert.match(source, /formatHourRange\(summary\.hour\)/);
  assert.match(source, /formatDurationLocalized\(summary\.total_duration/);
  assert.match(source, /getPrimarySummary\(summary\.summary\)/);
  assert.match(source, /getSecondarySummary\(summary\.summary\)/);
  assert.match(source, /getSummaryRhythmTone\(summary\.total_duration\)/);
  assert.doesNotMatch(source, /getSummaryRhythmTone\(summary\.total_duration\)\}\}/);
  assert.match(source, /isPeakSummary\(summary\)/);
  assert.match(source, /getMainApps\(summary\.main_apps\)/);
  assert.match(source, /timelineSummary\.summaryCount/);
  assert.match(source, /timelineSummary\.activeDuration/);
  assert.match(source, /timelineSummary\.activityCount/);
  assert.match(source, /summary\.activity_count/);
});

test('小时摘要抽屉应提供加载、刷新失败、空状态和长摘要展开收起', async () => {
  const source = await readDrawerSource();

  assert.match(source, /export let loading = false/);
  assert.match(source, /export let refreshing = false/);
  assert.match(source, /export let error = null/);
  assert.match(source, /\{#if \(loading \|\| refreshing\) && summaries\.length === 0\}/);
  assert.match(source, /timelineSummary\.noData/);
  assert.match(source, /timelineSummary\.expandFull/);
  assert.match(source, /timelineSummary\.collapse/);
  assert.match(source, /expandedHours/);
  assert.match(source, /summarySignature/);
  assert.match(source, /previousSummarySignature/);
  assert.match(source, /toggleExpand/);
  assert.doesNotMatch(source, /LocalizedDatePicker/);
});

test('小时摘要抽屉深色外围线应消费共享 token 并支持窄屏全宽', async () => {
  const source = await readDrawerSource();
  const drawerRule = source.match(
    /:global\(\.dark\) \.hourly-summary-drawer\s*\{[^}]*\}/
  )?.[0] ?? '';
  const headerRule = source.match(
    /:global\(\.dark\) \.hourly-summary-header\s*\{[^}]*\}/
  )?.[0] ?? '';
  const itemRule = source.match(
    /:global\(\.dark\) \.hourly-summary-item\s*\{[^}]*\}/
  )?.[0] ?? '';

  assert.match(drawerRule, /border-color:\s*var\(--surface-border-default\)/);
  assert.match(headerRule, /border-bottom-color:\s*var\(--surface-border-subtle\)/);
  assert.match(itemRule, /border-color:\s*var\(--surface-border-subtle\)/);
  assert.match(source, /@media \(max-width: 640px\)[\s\S]*\.hourly-summary-drawer[\s\S]*width:\s*100%/);
  assert.doesNotMatch(drawerRule, /rgba\(255, 255, 255, 0\.[4-9]/);
});
