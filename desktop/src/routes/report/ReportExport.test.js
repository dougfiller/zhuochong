import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('日报手动导出在未设置默认目录时应允许临时选择目录', async () => {
  const source = await readFile(
    new URL('./Report.svelte', import.meta.url),
    'utf8'
  );

  assert.match(source, /import \{ open as openDialog.*\} from '@tauri-apps\/plugin-dialog';/);
  assert.match(source, /const selected = await openDialog\(\{\s*directory: true,\s*multiple: false,/s);
  assert.match(source, /exportDir = selected;/);
  assert.match(source, /invoke\('export_report_markdown', \{\s*date: report\.date \|\| selectedDate,\s*content: report\.content,\s*exportDir,/s);
  assert.doesNotMatch(source, /disabled=\{exportInProgress \|\| !config\?\.daily_report_export_dir\}/);
});
