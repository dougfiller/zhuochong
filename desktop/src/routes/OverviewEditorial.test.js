import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('概览页应渲染总编台式分区布局', async () => {
  const source = await readFile(new URL('./Overview.svelte', import.meta.url), 'utf8');

  assert.match(source, /overview-editorial-shell/);
  assert.match(source, /overview-lead-card/);
  assert.match(source, /overview-summary-grid/);
  assert.match(source, /overview-command-deck/);
  assert.match(source, /overview-section-grid/);
  assert.match(source, /overview-section-card/);
  assert.match(source, /overview-browser-gallery/);
  assert.doesNotMatch(source, /overview-single-card/);
  assert.doesNotMatch(source, /<StatsCard[^>]*embedded/);
  assert.match(source, /<AppUsageChart[\s\S]*embedded/);
  assert.match(source, /<ActivityHourlyChart[\s\S]*embedded/);
});

test('概览页常驻网站与分类选择应采用统一的轻量视觉结构', async () => {
  const source = await readFile(new URL('./Overview.svelte', import.meta.url), 'utf8');

  assert.match(source, /overview-domain-row/);
  assert.match(source, /overview-domain-heading/);
  assert.match(source, /overview-domain-meta/);
  assert.match(source, /overview-domain-source-list/);
  assert.match(source, /overview-domain-source-track/);
  assert.match(source, /overview-domain-category-meta/);
  assert.match(source, /overview-semantic-popover/);
  assert.match(source, /overview-semantic-option/);
  assert.match(source, /overview-semantic-check/);
  assert.doesNotMatch(source, /overview-domain-stamp/);
  assert.doesNotMatch(source, /overview-domain-source-badge/);
});

test('常驻网站应呈现与应用使用一致的轻分隔排行', async () => {
  const source = await readFile(new URL('./Overview.svelte', import.meta.url), 'utf8');

  const rowClass = source.match(/<button[^>]*class="([^"]*overview-domain-row[^"]*)"/);
  assert.ok(rowClass, '常驻网站应保留可点击的网站排行行');
  assert.doesNotMatch(rowClass[1], /!border-0/, '网站行边框应由共享 CSS 统一管理');
  assert.match(rowClass[1], /!bg-transparent/, '网站行默认应保持透明');
  assert.match(rowClass[1], /hover:!bg-slate-100\/70/, '网站行悬停时应仅显示轻背景');
  assert.match(rowClass[1], /focus-visible:!bg-slate-100\/70/, '网站行键盘聚焦时应显示轻背景');
  assert.doesNotMatch(rowClass[1], /focus-visible:ring/, '网站行焦点只应保留共享 CSS 的 outline');

  const trackClass = source.match(/class="([^"]*overview-domain-source-track[^"]*)"/);
  assert.ok(trackClass, '常驻网站应保留多浏览器分段轨道');
  assert.doesNotMatch(trackClass[1], /!border-0/, '浏览器分段轨道边框应由共享 CSS 统一管理');
  assert.match(trackClass[1], /!bg-slate-100/, '浏览器分段轨道应保留浅色底轨');

  const categoryMetaClass = source.match(/class="([^"]*overview-domain-category-meta[^"]*)"/);
  assert.ok(categoryMetaClass, '常驻网站应保留页面数量与分类辅助信息');
  assert.match(categoryMetaClass[1], /text-slate-500/, '浅色模式辅助信息应提升对比度');
  assert.match(categoryMetaClass[1], /dark:text-\[#7d8590\]/, '深色模式辅助信息应提升对比度');
  assert.doesNotMatch(categoryMetaClass[1], /text-slate-400|#636c76/, '辅助信息不应继续使用低对比颜色');

  assert.match(source, /overview-domain-heading[\s\S]*overview-domain-category-meta/);
  assert.match(source, /overview-domain-source-list[\s\S]*overview-domain-source-segment/);
  assert.match(source, /overview-domain-duration/);
  assert.match(source, /on:click=\{\(\) => openDomainDetail\(domain\)\}/);
  assert.match(source, /on:click=\{toggleDomainUsageExpanded\}/);
  assert.match(source, /domainUsageLoading[\s\S]*t\('common\.loading'\)[\s\S]*domainUsageExpanded[\s\S]*t\('common\.collapse'\)[\s\S]*t\('overview\.viewAll'\)/);
});

test('常驻网站加载骨架应复用正式网站行的三列信息结构', async () => {
  const source = await readFile(new URL('./Overview.svelte', import.meta.url), 'utf8');

  const skeletonBlock = source.match(/overview-domain-skeleton-list[\s\S]*?\{:else if topDomainPresentations/);
  assert.ok(skeletonBlock, '常驻网站加载态应有独立骨架列表');
  assert.match(skeletonBlock[0], /overview-domain-skeleton-row/);
  assert.match(skeletonBlock[0], /overview-domain-skeleton-heading/);
  assert.match(skeletonBlock[0], /overview-domain-skeleton-source[\s\S]*overview-domain-skeleton-source-label[\s\S]*overview-domain-skeleton-source-track/);
  assert.match(skeletonBlock[0], /overview-domain-skeleton-duration/);
  assert.doesNotMatch(skeletonBlock[0], /h-9 w-9/, '骨架不应保留已移除的网站图标占位');
});

test('概览共享样式应使用轻量边界并提供克制的分类选中反馈', async () => {
  const css = await readFile(new URL('../app.css', import.meta.url), 'utf8');
  const requiredClasses = [
    'overview-composition-segment',
    'overview-composition-summary',
    'overview-composition-kpi',
    'overview-composition-primary-apps',
    'overview-domain-row',
    'overview-domain-heading',
    'overview-domain-meta',
    'overview-domain-source-list',
    'overview-domain-source-track',
    'overview-domain-source-segment',
    'overview-domain-dialog',
    'overview-domain-summary-list',
    'overview-domain-summary-row',
    'overview-domain-detail',
    'overview-domain-detail-source',
    'activity-hourly-category-segment',
    'activity-hourly-category-segment-selected',
    'activity-hourly-category-segment-muted',
    'activity-hourly-selected-apps',
  ];

  for (const className of requiredClasses) {
    assert.match(css, new RegExp(`\\.${className}(?=[\\s,{:\\[])`), `${className} 应有共享 CSS 契约`);
  }

  assert.match(css, /\.overview-domain-row\s*\{[^}]*border:\s*0(?:;|\s)/, '网站行本身不应形成独立卡片边框');
  assert.match(css, /\.overview-domain-source-track\s*\{[^}]*border:\s*0(?:;|\s)/, '来源轨道共享契约应直接无边框');
  assert.match(css, /\.app-usage-chart__rows,\s*\.overview-browser-gallery\s*\{[^}]*gap:\s*0/, '两组排行应使用连续列表节奏');
  assert.match(
    css,
    /\.app-usage-chart__row \+ \.app-usage-chart__row::before,\s*\.overview-domain-row \+ \.overview-domain-row::before\s*\{[^}]*inset-inline:\s*0\.5rem;[^}]*height:\s*1px;[^}]*background:\s*color-mix\(in srgb, var\(--surface-border-subtle\) 78%, transparent\)/,
    '应用和网站相邻行应共享低对比内缩分隔线',
  );
  assert.match(css, /\.overview-domain-row:focus-visible[\s\S]*?outline:\s*2px solid rgb\(125 211 252 \/ 0\.72\)/, '网站行应保留单一 outline 焦点反馈');

  assert.match(css, /\.overview-composition-summary\s*\{[^}]*border:\s*1px solid var\(--surface-border-subtle\)/);
  assert.match(css, /\.overview-composition-kpi\s*\{[^}]*border:\s*1px solid var\(--surface-border-subtle\)/);
  assert.match(css, /\.overview-domain-summary-row\s*\{[^}]*border-color:\s*var\(--surface-border-subtle\)/);
  assert.match(css, /\.overview-domain-dialog\s*\{[^}]*border-color:\s*var\(--surface-border-subtle\)/);
  assert.match(css, /\.overview-domain-detail(?:-source)?\s*\{[^}]*border:\s*1px solid var\(--surface-border-subtle\)/);
  assert.match(css, /\.activity-hourly-selected-apps\s*\{[^}]*border:\s*1px solid var\(--surface-border-subtle\)/);

  assert.match(css, /\.overview-composition-segment\[aria-pressed='true'\]\s*\{[^}]*transform:\s*scaleY\(1\.08\)[^}]*outline:\s*1px solid rgb\(125 211 252 \/ 0\.72\)/);
  assert.match(css, /\.overview-composition-segment\.opacity-30\s*\{[^}]*opacity:\s*0\.34/);
  assert.match(css, /\.activity-hourly-category-segment-selected\s*\{[^}]*transform:\s*scaleX\(1\.04\)[^}]*outline:\s*1px solid rgb\(125 211 252 \/ 0\.72\)/);
  assert.match(css, /\.activity-hourly-category-segment-muted\s*\{[^}]*opacity:\s*0\.28\s*!important/);
  assert.match(css, /\.dark :is\([\s\S]*?\.overview-composition-segment,[\s\S]*?\.activity-hourly-category-segment,[\s\S]*?\.app-usage-chart__bar[\s\S]*?filter:\s*saturate\(0\.62\) brightness\(0\.88\)/);
  assert.match(css, /\.overview-composition-segment:focus-visible\s*\{[^}]*outline:\s*2px solid rgb\(125 211 252 \/ 0\.72\)[^}]*outline-offset:\s*2px/);

  const darkRule = css.match(/\.dark :is\(\.overview-composition-summary,[\s\S]*?\.activity-hourly-selected-apps\)\s*\{([^}]*)\}/);
  assert.ok(darkRule, '深色概览表面应有统一覆盖');
  assert.match(darkRule[1], /border-color:\s*var\(--surface-border-subtle\)/);
  assert.match(darkRule[1], /box-shadow:\s*none/);
  assert.doesNotMatch(darkRule[1], /inset|255\s+255\s+255|255\s*,\s*255\s*,\s*255/);
});

test('概览网站行在窄屏应让来源轨道跨越整行', async () => {
  const css = await readFile(new URL('../app.css', import.meta.url), 'utf8');

  assert.match(css, /\.overview-domain-row\s*>\s*\.overview-domain-heading\s*\{[^}]*grid-area:\s*heading/);
  assert.match(css, /\.overview-domain-row\s*>\s*:nth-child\(2\)\s*\{[^}]*grid-area:\s*source/);
  assert.match(css, /\.overview-domain-row\s*>\s*:last-child\s*\{[^}]*grid-area:\s*duration/);
  assert.match(
    css,
    /@media\s*\(max-width:\s*640px\)\s*\{[\s\S]*?\.overview-domain-row\s*\{[^}]*grid-template-columns:\s*minmax\(0,\s*1fr\) auto;[^}]*grid-template-areas:\s*"heading duration"\s*"source source"/,
  );
  assert.match(css, /@media\s*\(max-width:\s*640px\)\s*\{[\s\S]*?\.overview-domain-dialog\s*\{[^}]*max-height:\s*calc\(100dvh - 1rem\)/);
});
