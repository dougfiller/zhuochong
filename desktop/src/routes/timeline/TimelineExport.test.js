import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

// 静态校验：时间线 JSON 导出在源码层保持期望结构
// 这层校验防止重构时误删 import、改错 invoke 命令名或参数键
test('时间线导出应使用 plugin-dialog 的 save 与 ask，并调用 export_timeline_json', async () => {
  const source = await readFile(
    new URL('./Timeline.svelte', import.meta.url),
    'utf8'
  );

  // 必要 import：save 用于选导出路径，ask 用于让用户决定是否包含 OCR
  assert.match(
    source,
    /import \{ ask, save as saveDialog \} from '@tauri-apps\/plugin-dialog';/
  );

  // 调用 saveDialog 时给定默认文件名与 JSON 过滤器，体验上更清晰
  assert.match(
    source,
    /saveDialog\(\{\s*defaultPath: `timeline-\$\{selectedDate\}\.json`,\s*filters: \[\{ name: 'JSON', extensions: \['json'\] \}\],/s
  );

  // ask 调用应使用 i18n key，确保三语一致
  assert.match(source, /t\('timeline\.exportIncludeOcrMessage'\)/);
  assert.match(source, /t\('timeline\.exportIncludeOcrTitle'\)/);

  // invoke 参数命名遵循 camelCase（Tauri 自动 → snake_case）
  assert.match(
    source,
    /invoke\('export_timeline_json', \{\s*date: selectedDate,\s*targetPath,\s*includeOcr,/s
  );
});

test('时间线工具栏应包含导出按钮，并在加载/无数据时禁用', async () => {
  const source = await readFile(
    new URL('./Timeline.svelte', import.meta.url),
    'utf8'
  );

  // 按钮触发函数
  assert.match(source, /on:click=\{exportTimelineJson\}/);

  // 加载中或当前日期无活动时按钮禁用，避免无意义触发
  assert.match(
    source,
    /disabled=\{exportingTimeline \|\| !activities\.length\}/
  );
});

test('三种语言 locale 的 timeline 块都应包含导出相关文案', async () => {
  const locales = ['zh-CN', 'zh-TW', 'en'];
  const requiredKeys = [
    'exportTitle',
    'exportNothing',
    'exportIncludeOcrTitle',
    'exportIncludeOcrMessage',
    'exportIncludeOcrYes',
    'exportIncludeOcrNo',
    'exportSuccess',
    'exportFailed',
  ];

  for (const locale of locales) {
    const mod = await import(`../../lib/i18n/locales/${locale}.js`);
    const timelineDict = mod.default?.timeline;
    assert.ok(timelineDict, `locale ${locale} 缺少 timeline 命名空间`);
    for (const key of requiredKeys) {
      assert.ok(
        typeof timelineDict[key] === 'string' && timelineDict[key].length > 0,
        `locale ${locale} 缺少 timeline.${key}`,
      );
    }
  }
});
