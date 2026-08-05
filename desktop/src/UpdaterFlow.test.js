import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

async function readCommandsSource() {
  return readFile(new URL('../src-tauri/src/commands/updater.rs', import.meta.url), 'utf8');
}

test('未配置本产品更新信任链时后端必须禁用更新且不得保留上游地址', async () => {
  const source = await readCommandsSource();

  assert.match(source, /AUTO_UPDATE_DISABLED_MESSAGE/);
  assert.match(source, /disabled: true/);
  assert.match(source, /pub async fn should_check_updates[\s\S]*?Ok\(false\)/);
  assert.doesNotMatch(source, /wm94i\/Work-Review|gh-proxy|api\.github\.com|\.updater_builder\(\)|reqwest::Client/);
});

test('未配置本产品更新信任链时不得注册原生 updater 或授予其权限', async () => {
  const [cargoToml, mainSource, capability] = await Promise.all([
    readFile(new URL('../src-tauri/Cargo.toml', import.meta.url), 'utf8'),
    readFile(new URL('../src-tauri/src/main.rs', import.meta.url), 'utf8'),
    readFile(new URL('../src-tauri/capabilities/migrated.json', import.meta.url), 'utf8'),
  ]);

  assert.doesNotMatch(cargoToml, /tauri-plugin-updater/);
  assert.doesNotMatch(mainSource, /tauri_plugin_updater/);
  assert.doesNotMatch(capability, /updater:/);
});
