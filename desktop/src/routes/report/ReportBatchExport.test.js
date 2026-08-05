import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

// 静态校验：批量日报合并导出在源码层保持期望结构
// 这层校验防止重构时误删 import、改错命令名或参数键

test('批量导出应使用 saveDialog 并调用 export_reports_range', async () => {
  const source = await readFile(
    new URL('./Report.svelte', import.meta.url),
    'utf8',
  );

  // 必要 import：saveDialog 用于让用户选导出文件路径
  assert.match(
    source,
    /import \{ open as openDialog, save as saveDialog \} from '@tauri-apps\/plugin-dialog';/,
  );

  // saveDialog 给出默认文件名与 markdown 过滤器
  assert.match(
    source,
    /saveDialog\(\{\s*defaultPath: `reports-\$\{batchStartDate\}_to_\$\{batchEndDate\}\.md`,\s*filters: \[\{ name: 'Markdown', extensions: \['md'\] \}\],/s,
  );

  // invoke 参数遵循 camelCase（Tauri 自动映射 snake_case）
  assert.match(
    source,
    /invoke\('export_reports_range', \{\s*startDate: batchStartDate,\s*endDate: batchEndDate,\s*targetPath,\s*locale: currentLocale,/s,
  );
});

test('批量导出 modal 必须含 4 个范围预设按钮 + 起止日期输入', async () => {
  const source = await readFile(
    new URL('./Report.svelte', import.meta.url),
    'utf8',
  );

  // 4 个预设按钮
  for (const preset of ['thisWeek', 'lastWeek', 'thisMonth', 'lastMonth']) {
    assert.match(
      source,
      new RegExp(`applyBatchPreset\\('${preset}'\\)`),
      `缺少预设按钮 ${preset}`,
    );
  }

  // 起止日期双向绑定
  assert.match(source, /bind:value=\{batchStartDate\}/);
  assert.match(source, /bind:value=\{batchEndDate\}/);
});

test('批量导出按钮应在 report 工具栏中触发 openBatchExportModal', async () => {
  const source = await readFile(
    new URL('./Report.svelte', import.meta.url),
    'utf8',
  );

  // 按钮触发函数
  assert.match(source, /on:click=\{openBatchExportModal\}/);
  // 导出过程中按钮禁用
  assert.match(source, /disabled=\{batchExporting\}/);
});

test('三种语言 locale 的 report 块都应包含批量导出相关文案', async () => {
  const locales = ['zh-CN', 'zh-TW', 'en'];
  const requiredKeys = [
    'batchExport',
    'batchExportTitle',
    'batchExportModalTitle',
    'batchExportHint',
    'batchExportConfirm',
    'batchExporting',
    'batchExportSuccess',
    'batchExportFailed',
    'batchExportInvalidRange',
    'batchStartDate',
    'batchEndDate',
    'batchPresetThisWeek',
    'batchPresetLastWeek',
    'batchPresetThisMonth',
    'batchPresetLastMonth',
  ];

  for (const locale of locales) {
    const mod = await import(`../../lib/i18n/locales/${locale}.js`);
    const reportDict = mod.default?.report;
    assert.ok(reportDict, `locale ${locale} 缺少 report 命名空间`);
    for (const key of requiredKeys) {
      assert.ok(
        typeof reportDict[key] === 'string' && reportDict[key].length > 0,
        `locale ${locale} 缺少 report.${key}`,
      );
    }
  }
});
