import test from 'node:test';
import assert from 'node:assert/strict';

async function loadPresentation() {
  try {
    return await import('./overviewCategoryPresentation.js');
  } catch {
    return {};
  }
}

test('分类摘要应计算总时长、占比与跨小时聚合后的主要应用', async () => {
  const { buildCategoryCompositionSummary } = await loadPresentation();
  assert.equal(typeof buildCategoryCompositionSummary, 'function', '应提供分类摘要纯函数');

  const summary = buildCategoryCompositionSummary({
    category: 'work',
    compositionTotals: { work: 900, entertainment: 600 },
    hourlyBreakdown: {
      9: [{ category: 'work', duration: 300 }],
      10: [{ category: 'work', duration: 400 }],
      12: [{ category: 'work', duration: 200 }],
    },
    appBreakdown: [
      { hour: 9, apps: [{ app_name: 'Code', category: 'work', duration: 180 }, { app_name: 'Music', category: 'entertainment', duration: 120 }] },
      { hour: 10, apps: [{ app_name: 'Code', category: 'work', duration: 220 }, { app_name: 'Docs', category: 'work', duration: 180 }] },
      { hour: 12, apps: [{ app_name: 'Docs', category: 'work', duration: 200 }] },
    ],
  });

  assert.deepEqual(summary, {
    category: 'work',
    duration: 900,
    percentage: 60,
    activeRange: { startHour: 9, endHour: 10, duration: 700 },
    primaryApps: [
      { appName: 'Code', duration: 400 },
      { appName: 'Docs', duration: 380 },
    ],
  });
});

test('连续活跃时段应选择累计时长最大的连续小时组，相同时优先较早时段', async () => {
  const { buildCategoryCompositionSummary } = await loadPresentation();

  const summary = buildCategoryCompositionSummary({
    category: 'focus',
    compositionTotals: { focus: 600 },
    hourlyBreakdown: {
      7: [{ category: 'focus', duration: 150 }],
      8: [{ category: 'focus', duration: 150 }],
      12: [{ category: 'focus', duration: 200 }],
      13: [{ category: 'focus', duration: 100 }],
    },
    appBreakdown: [],
  });

  assert.deepEqual(summary.activeRange, { startHour: 7, endHour: 8, duration: 300 });
});

test('空分类摘要应返回可安全渲染的零值且不修改输入', async () => {
  const { buildCategoryCompositionSummary } = await loadPresentation();
  const compositionTotals = Object.freeze({ work: 120 });
  const hourlyBreakdown = Object.freeze({ 9: Object.freeze([{ category: 'work', duration: 120 }]) });
  const appBreakdown = Object.freeze([{ hour: 9, apps: Object.freeze([{ app_name: 'Code', category: 'work', duration: 120 }]) }]);

  const summary = buildCategoryCompositionSummary({
    category: 'missing',
    compositionTotals,
    hourlyBreakdown,
    appBreakdown,
  });

  assert.deepEqual(summary, {
    category: 'missing',
    duration: 0,
    percentage: 0,
    activeRange: null,
    primaryApps: [],
  });
  assert.deepEqual(compositionTotals, { work: 120 });
  assert.equal(hourlyBreakdown[9][0].duration, 120);
  assert.equal(appBreakdown[0].apps[0].app_name, 'Code');
});
