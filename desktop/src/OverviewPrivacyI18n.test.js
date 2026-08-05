import test from 'node:test';
import assert from 'node:assert/strict';
import zhCN from './lib/i18n/locales/zh-CN.js';
import zhTW from './lib/i18n/locales/zh-TW.js';
import en from './lib/i18n/locales/en.js';
import ar from './lib/i18n/locales/ar.js';

const locales = { zhCN, zhTW, en, ar };

function assertNonEmptyKey(messages, path, localeName) {
  const value = path.split('.').reduce((current, key) => current?.[key], messages);
  assert.equal(typeof value, 'string', `${localeName} 缺少 ${path}`);
  assert.ok(value.trim().length > 0, `${localeName} 的 ${path} 不能为空`);
}

test('隐私设置四语言应提供本地存储边界和内容过滤数量摘要', () => {
  for (const [localeName, messages] of Object.entries(locales)) {
    assertNonEmptyKey(messages, 'settingsPrivacy.storageAndTransferDescription', localeName);
    assertNonEmptyKey(messages, 'settingsPrivacy.contentFilterSummary', localeName);
    assert.match(
      messages.settingsPrivacy.contentFilterSummary,
      /\{keywordCount\}[\s\S]*\{domainCount\}/,
      `${localeName} 的内容过滤摘要必须保留两个数量占位符`,
    );
  }
});

const overviewKeys = [
  'compositionFilter',
  'compositionDuration',
  'compositionShare',
  'compositionActiveRange',
  'compositionPrimaryApps',
  'compositionNoActiveRange',
  'domainSourcesUnknown',
  'domainListTitle',
  'domainDetailTitle',
  'domainLoadFailed',
];

test('概览四语言应提供分类构成和域名详情文案', () => {
  for (const [localeName, messages] of Object.entries(locales)) {
    for (const key of overviewKeys) {
      assertNonEmptyKey(messages, `overview.${key}`, localeName);
    }
  }
});
