import test from 'node:test';
import assert from 'node:assert/strict';

import {
  formatHourRange,
  getMainApps,
  getSummaryDisplayParts,
  getPrimarySummary,
  getSecondarySummary,
  getSummaryRhythmTone,
  orderHourlySummariesForDisplay,
} from './summaryPresentation.js';

test('时段摘要应按最近小时优先展示且不修改原数组', () => {
  const summaries = [
    { hour: 9, summary: '上午' },
    { hour: '16', summary: '下午' },
    { hour: 11, summary: '中午' },
  ];
  const originalOrder = summaries.map((summary) => summary.hour);

  const result = orderHourlySummariesForDisplay(summaries);

  assert.deepEqual(result.map((summary) => Number(summary.hour)), [16, 11, 9]);
  assert.deepEqual(summaries.map((summary) => summary.hour), originalOrder);
  assert.notEqual(result, summaries);
});

test('时段摘要排序应兼容空值、单条记录和小时边界', () => {
  assert.deepEqual(orderHourlySummariesForDisplay(), []);
  assert.deepEqual(orderHourlySummariesForDisplay(null), []);
  assert.deepEqual(orderHourlySummariesForDisplay([]), []);
  assert.deepEqual(orderHourlySummariesForDisplay([{ hour: 11 }]), [{ hour: 11 }]);
  assert.deepEqual(
    orderHourlySummariesForDisplay([{ hour: 0 }, { hour: 23 }, { hour: 12 }])
      .map((summary) => summary.hour),
    [23, 12, 0]
  );
});

test('时段摘要应拆出主摘要和副摘要', () => {
  const source = '上午主要处理日报生成逻辑，补齐了状态提示，并完成了回归验证。下午继续优化样式细节。';

  assert.equal(getPrimarySummary(source), '上午主要处理日报生成逻辑，补齐了状态提示');
  assert.equal(getSecondarySummary(source), '并完成了回归验证。');
});

test('展开时段摘要时应只展示完整正文，不重复已经展示的主摘要', () => {
  const source = '上午主要处理日报生成逻辑，补齐了状态提示，并完成了回归验证。';

  assert.deepEqual(getSummaryDisplayParts(source, false), {
    primary: '上午主要处理日报生成逻辑，补齐了状态提示',
    secondary: '并完成了回归验证。',
  });
  assert.deepEqual(getSummaryDisplayParts(source, true), {
    primary: source,
    secondary: '',
  });
});

test('自然小时应格式化为完整小时范围', () => {
  assert.equal(formatHourRange(9), '09:00–10:00');
  assert.equal(formatHourRange(23), '23:00–24:00');
  assert.equal(formatHourRange(-1), '00:00–01:00');
  assert.equal(formatHourRange(99), '23:00–24:00');
});

test('时段摘要应按时长边界生成节奏类型', () => {
  assert.equal(getSummaryRhythmTone(19 * 60 + 59), 'light');
  assert.equal(getSummaryRhythmTone(20 * 60), 'steady');
  assert.equal(getSummaryRhythmTone(44 * 60 + 59), 'steady');
  assert.equal(getSummaryRhythmTone(45 * 60), 'deep');
});

test('主应用列表应限制数量、过滤空项并兼容中英文逗号', () => {
  assert.deepEqual(
    getMainApps('Cursor, Google Chrome，Slack, Terminal, Notes'),
    ['Cursor', 'Google Chrome', 'Slack', 'Terminal']
  );
  assert.deepEqual(getMainApps(''), []);
});
