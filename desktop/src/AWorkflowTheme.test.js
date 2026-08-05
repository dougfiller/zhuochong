import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('A 包公共样式在全局 CSS 中不应继续使用 :global(.dark) 写法', async () => {
  const source = await readFile(new URL('./app.css', import.meta.url), 'utf8');

  assert.doesNotMatch(source, /:global\(\.dark\)\s+\.sidebar-editorial-shell/);
  assert.doesNotMatch(source, /:global\(\.dark\)\s+\.overview-command-deck/);
  assert.doesNotMatch(source, /:global\(\.dark\)\s+\.report-article-card/);
  assert.doesNotMatch(source, /:global\(\.dark\)\s+\.ask-workbench-shell/);

  assert.match(source, /\.dark\s+\.sidebar-editorial-shell/);
  assert.match(source, /\.dark\s+\.overview-command-deck/);
  assert.match(source, /\.dark\s+\.report-article-card/);
  assert.match(source, /\.dark\s+\.ask-workbench-shell/);
});
