import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('主窗口应默认隐藏并由启动流程决定是否显示', async () => {
  const tauriConfig = JSON.parse(
    await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')
  );
  const mainSource = await readFile(
    new URL('../src-tauri/src/main.rs', import.meta.url),
    'utf8'
  );

  assert.equal(tauriConfig.app.windows[0].visible, false);
  assert.match(
    mainSource,
    /if should_hide_main_window \{\s*let _ = window\.hide\(\);\s*let _ = app\.emit\("main-window-visibility", false\);\s*\} else \{\s*let _ = window\.show\(\);\s*let _ = app\.emit\("main-window-visibility", true\);\s*\}/
  );
});

test('主窗口可见性应在前端挂载时补偿同步并在重新显示时恢复动画', async () => {
  const [appSource, mainSource] = await Promise.all([
    readFile(new URL('./App.svelte', import.meta.url), 'utf8'),
    readFile(new URL('../src-tauri/src/main.rs', import.meta.url), 'utf8'),
  ]);

  assert.match(appSource, /await appWindow\.isVisible\(\)/);
  assert.match(
    appSource,
    /document\.body\.classList\.toggle\('app-window-hidden',\s*visible === false\)/
  );
  assert.match(
    appSource,
    /safeListen\('main-window-visibility',\s*\(event\) => \{\s*syncMainWindowVisibility\(event\.payload\);\s*\}/
  );
  assert.match(
    mainSource,
    /let _ = window\.show\(\);\s*let _ = app\.emit\("main-window-visibility", true\);/
  );
});

test('非 Windows 自启插件应只固定注入 autostart 参数，静默与否交给配置判断', async () => {
  const mainSource = await readFile(
    new URL('../src-tauri/src/main.rs', import.meta.url),
    'utf8'
  );

  assert.match(
    mainSource,
    /tauri_plugin_autostart::init\(\s*tauri_plugin_autostart::MacosLauncher::LaunchAgent,\s*Some\(vec!\[AUTOSTART_LAUNCH_ARG\]\)/
  );
  assert.doesNotMatch(
    mainSource,
    /tauri_plugin_autostart::init\([\s\S]*Some\(vec!\[AUTOSTART_LAUNCH_ARG,\s*"--hidden"\]\)/
  );
});

test('浏览器预览环境缺少 Tauri window metadata 时应用仍应可挂载', async () => {
  const appSource = await readFile(new URL('./App.svelte', import.meta.url), 'utf8');

  assert.match(appSource, /function getSafeCurrentWebviewWindow\(\)/);
  assert.match(appSource, /try \{\s*return getCurrentWebviewWindow\(\);/);
  assert.match(appSource, /catch \(e\) \{[\s\S]*return createBrowserPreviewWindow\(\);/);
  assert.match(appSource, /const appWindow = getSafeCurrentWebviewWindow\(\);/);
  assert.doesNotMatch(appSource, /const appWindow = getCurrentWebviewWindow\(\);/);
});

test('浏览器预览环境缺少 Tauri event metadata 时主应用事件监听不应中断挂载', async () => {
  const appSource = await readFile(new URL('./App.svelte', import.meta.url), 'utf8');

  assert.match(appSource, /async function safeListen\(eventName, handler\)/);
  assert.match(appSource, /return await listen\(eventName, handler\);/);
  assert.match(appSource, /return \(\) => \{\};/);
  assert.match(appSource, /safeListen\('main-window-visibility'/);
  assert.match(appSource, /await safeListen\('recording-state-changed'/);
  assert.match(appSource, /await safeListen\('config-changed'/);
  assert.match(appSource, /await safeListen\('avatar-open-timeline'/);
  assert.match(appSource, /await safeListen\('screenshot-taken'/);
});
