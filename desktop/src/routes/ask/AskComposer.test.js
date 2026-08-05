import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const source = await readFile(new URL('./Ask.svelte', import.meta.url), 'utf8');

test('中文输入法组合期间按 Enter 不应误发送', () => {
  const handler = source.match(/function handleComposerKeydown\(event\) \{[\s\S]*?\n  \}/)?.[0] ?? '';

  assert.ok(handler, '应存在输入器键盘处理函数');
  assert.match(source, /<textarea[\s\S]*on:keydown=\{handleComposerKeydown\}/);
  assert.match(handler, /event\.isComposing/);
  assert.match(handler, /event\.keyCode\s*===\s*229/);
  assert.match(handler, /if \(event\.isComposing \|\| event\.keyCode === 229\) return;/);
});

test('普通 Enter 发送且 Shift Enter 保持换行', () => {
  const handler = source.match(/function handleComposerKeydown\(event\) \{[\s\S]*?\n  \}/)?.[0] ?? '';

  assert.match(handler, /event\.key === 'Enter' && !event\.shiftKey/);
  assert.match(handler, /event\.preventDefault\(\)/);
  assert.match(handler, /submitQuestion\(\)/);
});
