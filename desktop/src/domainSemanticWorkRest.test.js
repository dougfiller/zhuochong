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

test('网站语义规则命中后应同步基础分类，保障工休统计可用', async () => {
  const commandsSource = await readCommandsSource();
  const mainSource = await readFile(new URL('../src-tauri/src/main.rs', import.meta.url), 'utf8');

  assert.match(commandsSource, /semantic_category_to_base_category/);
  assert.match(commandsSource, /update_activity_classification\([\s\S]*next_base_category/);
  assert.match(mainSource, /semantic_category_to_base_category/);
  assert.match(mainSource, /classification\.base_category\s*=\s*monitor::semantic_category_to_base_category/);
});
