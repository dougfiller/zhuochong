import test from 'node:test';
import assert from 'node:assert/strict';

async function loadPresentation() {
  try {
    return await import('./overviewDomainPresentation.js');
  } catch {
    return {};
  }
}

test('域名来源应从前六域名对应的浏览器统计中聚合并稳定排序', async () => {
  const { collectDomainBrowserSources } = await loadPresentation();

  assert.equal(typeof collectDomainBrowserSources, 'function', '应提供来源聚合纯函数');
  const browserUsage = [
    {
      browser_name: 'Safari',
      domains: [
        { domain: 'example.com', duration: 120 },
        { domain: 'other.test', duration: 999 },
      ],
    },
    {
      browser_name: 'Chrome',
      domains: [{ domain: 'example.com', duration: 300 }],
    },
    {
      browser_name: 'Safari',
      domains: [{ domain: 'example.com', duration: 80 }],
    },
  ];

  assert.deepEqual(collectDomainBrowserSources('example.com', browserUsage), [
    { browser_name: 'Chrome', duration: 300, percentage: 60 },
    { browser_name: 'Safari', duration: 200, percentage: 40 },
  ]);
  assert.deepEqual(collectDomainBrowserSources('missing.test', browserUsage), []);
});

test('后端显式 browser_sources 应优先用于全量摘要和单域名详情', async () => {
  const { collectDomainBrowserSources } = await loadPresentation();
  const browserUsage = [{ browser_name: 'Chrome', domains: [{ domain: 'example.com', duration: 999 }] }];
  const explicitSources = [
    { browser_name: 'Firefox', duration: 90, percentage: 75 },
    { browser_name: 'Safari', duration: 30, percentage: 25 },
  ];

  assert.deepEqual(
    collectDomainBrowserSources('example.com', browserUsage, explicitSources),
    explicitSources,
  );
});

test('来源轨道应清洗非法值、限制宽度并保留可读来源文案', async () => {
  const { buildDomainSourceTrack, buildDomainPresentation } = await loadPresentation();

  assert.equal(typeof buildDomainSourceTrack, 'function', '应提供来源轨道纯函数');
  assert.equal(typeof buildDomainPresentation, 'function', '应提供域名展示纯函数');

  assert.deepEqual(buildDomainSourceTrack([
    { browser_name: 'Chrome', duration: 70, percentage: 140 },
    { browser_name: 'Safari', duration: -5, percentage: -20 },
    { browser_name: '', duration: 30 },
  ], 100), [
    { browser_name: 'Chrome', duration: 70, percentage: 100, widthPct: 100 },
    { browser_name: 'Safari', duration: 0, percentage: 0, widthPct: 0 },
  ]);

  assert.deepEqual(
    buildDomainPresentation({
      domain: 'example.com',
      duration: 500,
      page_count: 4,
      browser_sources: [
        { browser_name: 'Chrome', duration: 300, percentage: 60 },
        { browser_name: 'Safari', duration: 200, percentage: 40 },
      ],
    }),
    {
      browserSources: [
        { browser_name: 'Chrome', duration: 300, percentage: 60 },
        { browser_name: 'Safari', duration: 200, percentage: 40 },
      ],
      sourceTrack: [
        { browser_name: 'Chrome', duration: 300, percentage: 60, widthPct: 60 },
        { browser_name: 'Safari', duration: 200, percentage: 40, widthPct: 40 },
      ],
      sourceLabel: 'Chrome 60% · Safari 40%',
      pageCount: 4,
    },
  );
});

test('域名展示层不再提供缩写印章或浏览器图标相关逻辑', async () => {
  const presentation = await loadPresentation();

  assert.equal(presentation.getDomainInitials, undefined);
  assert.equal(presentation.getDomainStampClass, undefined);
});

test('语义分类色点应为内置分类提供稳定颜色并为自定义分类生成兜底色', async () => {
  const { getSemanticCategoryColor } = await loadPresentation();

  assert.equal(typeof getSemanticCategoryColor, 'function', '应提供语义分类色点函数');
  assert.equal(getSemanticCategoryColor('编码开发'), '#3B82F6');
  assert.equal(getSemanticCategoryColor('设计创作'), '#F59E0B');
  assert.equal(getSemanticCategoryColor(''), '#94A3B8');
  assert.equal(getSemanticCategoryColor('自定义专注'), getSemanticCategoryColor('自定义专注'));
  assert.match(getSemanticCategoryColor('自定义专注'), /^#[0-9A-F]{6}$/);
});
