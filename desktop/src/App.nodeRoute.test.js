import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('应用壳层不应继续注册独立节点路由，节点能力应收回设置页', async () => {
  const source = await readFile(new URL('./App.svelte', import.meta.url), 'utf8');

  assert.doesNotMatch(source, /import NodeGateway from '\.\/routes\/node\/NodeGateway\.svelte'/);
  assert.doesNotMatch(source, /'\/node':\s*NodeGateway/);
});
