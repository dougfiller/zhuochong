import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';

async function readCommandsSource() {
  // commands.rs 已按领域拆分为 commands/*.rs，这里拼接所有子模块以保持断言语义不变。
  const dir = new URL('../src-tauri/src/commands/', import.meta.url);
  const files = (await readdir(dir)).filter((f) => f.endsWith('.rs'));
  const parts = await Promise.all(files.map((f) => readFile(new URL(f, dir), 'utf8')));
  return parts.join('\n');
}

test('后端检查更新应优先验证当前平台存在可安装更新包', async () => {
  const source = await readCommandsSource();

  assert.match(source, /check_installable_update/);
  assert.match(source, /\.updater_builder\(\)/);
  assert.match(source, /match updater\.check\(\)\.await/);
  assert.match(source, /auto_update_ready: true/);
});
