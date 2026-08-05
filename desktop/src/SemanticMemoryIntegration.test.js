import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const read = (path) => readFileSync(new URL(path, import.meta.url), 'utf8');

test('语义记忆不再有独立页面与侧栏入口（查询统一走助手）', () => {
  const app = read('./App.svelte');
  const sidebar = read('./lib/components/Sidebar.svelte');

  assert.doesNotMatch(app, /'\/memory'/, 'App.svelte 不应再注册 /memory 路由');
  assert.doesNotMatch(sidebar, /sidebar\.nav\.memory/, '侧栏不应再有记忆入口');
  assert.ok(
    !existsSync(fileURLToPath(new URL('./routes/memory/Memory.svelte', import.meta.url))),
    '独立记忆页应已删除'
  );
});

test('索引管理收进设置页语义记忆区块', () => {
  const settings = read('./routes/settings/components/SettingsAI.svelte');

  assert.match(settings, /invoke\('index_semantic_memory'\)/);
  assert.match(settings, /invoke\('semantic_memory_status'\)/);
  assert.match(settings, /memory_semantic_enabled/);
  // 组件销毁后索引循环必须停止
  assert.match(settings, /semanticDestroyed/);
  // 引导用户"直接问助手"
  assert.match(settings, /semanticMemory\.askHint/);
});

test('助手侧的语义检索接线完整', () => {
  const tools = readFileSync(
    fileURLToPath(new URL('../src-tauri/src/agent/tools.rs', import.meta.url)),
    'utf8'
  );
  const executor = readFileSync(
    fileURLToPath(new URL('../src-tauri/src/agent/executor.rs', import.meta.url)),
    'utf8'
  );
  const ask = readFileSync(
    fileURLToPath(new URL('../src-tauri/src/commands/ask.rs', import.meta.url)),
    'utf8'
  );

  assert.match(tools, /"semantic_search"/, '工具注册中心应有 semantic_search');
  assert.match(executor, /\[记忆能力\]/, '系统提示应声明记忆能力');
  assert.match(ask, /search_semantic_memory_inner/, 'ask.rs 应桥接语义检索');
});

test('语义记忆 i18n 键在四语言中完整', () => {
  for (const localeFile of ['zh-CN', 'zh-TW', 'en', 'ar']) {
    const source = read(`./lib/i18n/locales/${localeFile}.js`);
    for (const key of ['semanticMemory:', 'askHint:', 'indexStatus:', 'startIndex:', 'providerOllama:']) {
      assert.ok(source.includes(key), `${localeFile}.js 缺少 ${key}`);
    }
  }
});

test('语义记忆默认关闭且嵌入默认走本地 Ollama（隐私立场守护）', () => {
  const config = readFileSync(
    fileURLToPath(new URL('../crates/core/src/config.rs', import.meta.url)),
    'utf8'
  );
  assert.match(config, /memory_semantic_enabled: bool/);
  assert.match(config, /memory_semantic_enabled: false/);
  assert.match(config, /fn default_embedding_provider\(\) -> String \{\s*"ollama"\.to_string\(\)/);
});
