import test from 'node:test';
import assert from 'node:assert/strict';

const localeNames = ['zh-CN', 'zh-TW', 'en', 'ar'];

test('时间线摘要抽屉与关于页新结构应覆盖全部语言', async () => {
  for (const localeName of localeNames) {
    const localeModule = await import(`./lib/i18n/locales/${localeName}.js`);
    const messages = localeModule.default;

    assert.ok(messages.timelineSummary.summaryCount, `${localeName} 缺少 timelineSummary.summaryCount`);
    assert.ok(messages.timelineSummary.activeDuration, `${localeName} 缺少 timelineSummary.activeDuration`);
    assert.ok(messages.timelineSummary.activityCount, `${localeName} 缺少 timelineSummary.activityCount`);
    assert.ok(messages.timelineSummary.refreshFailed, `${localeName} 缺少 timelineSummary.refreshFailed`);
    assert.ok(messages.about.pageSubtitle, `${localeName} 缺少 about.pageSubtitle`);
    assert.ok(messages.about.productPrinciplesTitle, `${localeName} 缺少 about.productPrinciplesTitle`);
  }
});
