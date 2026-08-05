import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('日报生成应在网站访问部分体现域名语义分类', async () => {
  const [summarySource, localSource] = await Promise.all([
    readFile(new URL('../crates/core/src/analysis/summary.rs', import.meta.url), 'utf8'),
    readFile(new URL('../crates/core/src/analysis/local.rs', import.meta.url), 'utf8'),
  ]);

  assert.match(summarySource, /domain\.semantic_category/);
  assert.match(localSource, /domain\.semantic_category/);
  assert.match(summarySource, /translate_semantic_category_name\(semantic_category,\s*locale/);
  assert.match(localSource, /translate_semantic_category_name\(semantic_category,\s*locale/);
});

test('日报生成应体现按小时活跃度分布', async () => {
  const [summarySource, localSource, analysisModSource, reportBlocksSource] = await Promise.all([
    readFile(new URL('../crates/core/src/analysis/summary.rs', import.meta.url), 'utf8'),
    readFile(new URL('../crates/core/src/analysis/local.rs', import.meta.url), 'utf8'),
    readFile(new URL('../crates/core/src/analysis/mod.rs', import.meta.url), 'utf8'),
    readFile(new URL('../crates/core/src/analysis/report_blocks.rs', import.meta.url), 'utf8'),
  ]);

  // hourly 渲染逻辑现集中在 report_blocks.rs（段落积木化后 summary/local 调 assemble）
  assert.match(reportBlocksSource, /hourly_activity_distribution|高峰时段|按小时活跃度/);
  assert.match(reportBlocksSource, /HourlySummary/);
  assert.match(analysisModSource, /hourly_activity_distribution|高峰时段|按小时活跃度/);
});
