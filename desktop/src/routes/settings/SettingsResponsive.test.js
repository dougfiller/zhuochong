import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8');

test('AI 能力入口应在小屏使用单列并在中屏恢复三列', async () => {
  const source = await read('./components/SettingsAI.svelte');

  assert.match(source, /class="mb-3 grid grid-cols-1 gap-2 md:grid-cols-3"/);
  assert.doesNotMatch(source, /class="mb-3 grid grid-cols-3 gap-2"/);
});

test('存储设置固定网格应渐进适配小屏', async () => {
  const source = await read('./components/SettingsStorage.svelte');
  const responsiveFieldGrids = source.match(
    /class="grid grid-cols-1 gap-2 md:grid-cols-2"/g,
  );

  assert.equal(responsiveFieldGrids?.length, 4);
  assert.doesNotMatch(source, /class="grid gap-2 grid-cols-2"/);
  assert.match(source, /class="grid grid-cols-1 gap-3 sm:grid-cols-3"/);
  assert.doesNotMatch(source, /class="grid grid-cols-3 gap-3"/);
});

test('节点设置字段应在小屏使用单列', async () => {
  const [localApi, botCredentials] = await Promise.all([
    read('./components/nodeGateway/LocalApiPanel.svelte'),
    read('./components/nodeGateway/BotCredentialsPanel.svelte'),
  ]);

  assert.match(localApi, /class="grid grid-cols-1 gap-2 md:grid-cols-2"/);
  assert.doesNotMatch(localApi, /class="grid gap-2 grid-cols-2"/);

  assert.match(
    botCredentials,
    /class="settings-responsive-field-grid grid gap-2"/,
  );
  assert.doesNotMatch(
    botCredentials,
    /class:grid-cols-2=\{fields\.length > 2\}/,
  );
});
