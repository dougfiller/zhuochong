import test from 'node:test';
import assert from 'node:assert/strict';
import {
  extractReportBlockName,
  getVisibleReportSections,
  parseReportSections,
  reportSectionMarkdownForDisplay,
} from './reportSections.js';

test('日报段落解析应把 WR_BLOCK_START 标记保留在紧随其后的标题段落内', () => {
  const sections = parseReportSections(`<!-- WR_BLOCK_START:CATEGORY_TABLE -->
## 一、时间分配

> **短结论：** 主要时间集中在开发。

<!-- WR_BLOCK_END:CATEGORY_TABLE -->
<!-- WR_BLOCK_START:AI_ANALYSIS -->
## 二、AI 分析

### 观察

今天以编码为主。
<!-- WR_BLOCK_END:AI_ANALYSIS -->`);

  assert.equal(sections.length, 2);
  assert.equal(sections[0].title, '## 一、时间分配');
  assert.equal(extractReportBlockName(sections[0]), 'CATEGORY_TABLE');
  assert.equal(sections[1].title, '## 二、AI 分析');
  assert.equal(extractReportBlockName(sections[1]), 'AI_ANALYSIS');
});

test('置顶段落应按 pinned 顺序排到最前，并按展示顺序重新编号', () => {
  const sections = parseReportSections(`<!-- WR_BLOCK_START:CATEGORY_TABLE -->
## 一、时间分配

| 类别 | 时长 |
<!-- WR_BLOCK_END:CATEGORY_TABLE -->
<!-- WR_BLOCK_START:AI_ANALYSIS -->
## 五、AI 分析

### 观察

今天以编码为主。
<!-- WR_BLOCK_END:AI_ANALYSIS -->`);

  const visible = getVisibleReportSections(sections, ['AI_ANALYSIS'], []);

  assert.equal(visible.length, 2);
  assert.equal(extractReportBlockName(visible[0]), 'AI_ANALYSIS');
  assert.equal(visible[0].originalIndex, 1);
  assert.match(
    reportSectionMarkdownForDisplay(visible[0], 0, 'zh-CN'),
    /^## 一、AI 分析/
  );
  assert.match(
    reportSectionMarkdownForDisplay(visible[1], 1, 'zh-CN'),
    /^## 二、时间分配/
  );
});

test('隐藏段落应被过滤且不影响剩余段落重新编号', () => {
  const sections = parseReportSections(`<!-- WR_BLOCK_START:CATEGORY_TABLE -->
## 一、时间分配

body
<!-- WR_BLOCK_END:CATEGORY_TABLE -->
<!-- WR_BLOCK_START:AI_ANALYSIS -->
## 二、AI 分析

body
<!-- WR_BLOCK_END:AI_ANALYSIS -->`);

  const visible = getVisibleReportSections(sections, [], ['CATEGORY_TABLE']);

  assert.equal(visible.length, 1);
  assert.equal(extractReportBlockName(visible[0]), 'AI_ANALYSIS');
  assert.match(
    reportSectionMarkdownForDisplay(visible[0], 0, 'zh-CN'),
    /^## 一、AI 分析/
  );
});

test('日报标题和日期前言不应占用二级段落编号', () => {
  const sections = parseReportSections(`# 工作日报

**日期：2026-06-22**

<!-- WR_BLOCK_START:CATEGORY_TABLE -->
## 一、时间分配

body
<!-- WR_BLOCK_END:CATEGORY_TABLE -->`);

  const visible = getVisibleReportSections(sections, [], []);

  assert.equal(visible.length, 2);
  assert.equal(visible[0].displaySectionIndex, null);
  assert.equal(visible[1].displaySectionIndex, 0);
  assert.match(
    reportSectionMarkdownForDisplay(
      visible[1],
      visible[1].displaySectionIndex,
      'zh-CN'
    ),
    /^## 一、时间分配/
  );
});
