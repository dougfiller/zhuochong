import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('应用壳层应保留左右两张主卡片，并将 stage 退回为纯布局容器', async () => {
  const [appSource, appCssSource] = await Promise.all([
    readFile(new URL('./App.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./app.css', import.meta.url), 'utf8'),
  ]);

  assert.match(appSource, /app-shell/);
  assert.match(appSource, /app-shell-stage/);
  assert.match(appSource, /app-shell-sidebar-frame/);
  assert.match(appSource, /app-shell-main-frame/);
  assert.match(appSource, /app-shell-windowbar/);

  assert.match(appCssSource, /\.app-shell\b/);
  assert.match(appCssSource, /\.app-shell-stage\b/);
  assert.match(appCssSource, /\.app-shell-sidebar-frame\b/);
  assert.match(appCssSource, /\.app-shell-main-frame\b/);
  assert.match(appCssSource, /\.app-shell-windowbar\b/);
  assert.match(appCssSource, /\.app-shell-stage\s*\{[\s\S]*background:\s*transparent;/);
  assert.match(appCssSource, /\.app-shell-stage\s*\{[\s\S]*border:\s*none;/);
  assert.match(appCssSource, /\.app-shell-stage\s*\{[\s\S]*box-shadow:\s*none;/);
});

test('最外层应用窗口应保持方形边界，圆角只保留给内部 frame', async () => {
  const appCssSource = await readFile(new URL('./app.css', import.meta.url), 'utf8');
  const shellRule = appCssSource.match(/\.app-shell\s*\{(?<body>[^}]*)\}/);
  const radiusDeclarations = [...(shellRule?.groups?.body ?? '').matchAll(/border-radius\s*:\s*([^;]+);/g)]
    .map(match => match[1].trim());

  assert.ok(shellRule?.groups?.body);
  assert.match(appCssSource, /\.app-shell\s*\{[\s\S]*border-radius:\s*0;/);
  assert.match(appCssSource, /\.app-shell\s*\{[\s\S]*overflow:\s*hidden;/);
  assert.deepEqual(radiusDeclarations, ['0']);
});

test('主导航字号应高于设置内导航，形成稳定层级', async () => {
  const appCssSource = await readFile(new URL('./app.css', import.meta.url), 'utf8');

  assert.match(appCssSource, /\.sidebar-nav-label\s*\{[\s\S]*font-size:\s*0\.98rem;/);
  assert.match(appCssSource, /\.settings-tab-rail-item\s*\{[\s\S]*font-size:\s*0\.92rem;/);
});

test('统一底板结构下不应继续保留旧的主内容外壳伪元素修补逻辑', async () => {
  const appCssSource = await readFile(new URL('./app.css', import.meta.url), 'utf8');

  assert.doesNotMatch(appCssSource, /\.app-shell-main::before/);
  assert.doesNotMatch(appCssSource, /\.dark\s+\.app-shell-main::before/);
  assert.doesNotMatch(appCssSource, /\.app-shell-windowbar::before/);
});

test('自定义窗口栏应只保留拖拽命中区，不再形成明显顶部留白带', async () => {
  const [appSource, appCssSource] = await Promise.all([
    readFile(new URL('./App.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./app.css', import.meta.url), 'utf8'),
  ]);

  assert.match(
    appSource,
    /app-shell-stage[\s\S]*\{platform !== 'macos' \? 'app-shell-stage--windowbar' : 'app-shell-stage--macos'\}/
  );
  assert.doesNotMatch(appCssSource, /\.app-shell-stage\s*\{[^}]*padding:\s*0\.35rem/);
  assert.match(appCssSource, /\.app-shell-stage\s*\{[^}]*padding-right:\s*0\.35rem/);
  assert.match(appCssSource, /\.app-shell-stage\s*\{[^}]*padding-bottom:\s*0\.35rem/);
  assert.match(appCssSource, /\.app-shell-stage\s*\{[^}]*padding-left:\s*0\.35rem/);
  assert.match(appCssSource, /\.app-shell-stage--windowbar\s*\{[^}]*padding-top:\s*1\.75rem/);
  assert.match(appCssSource, /\.app-shell-stage--macos\s*\{[^}]*padding-top:\s*0\.6rem/);
  assert.match(appCssSource, /\.app-shell-windowbar\s*\{[^}]*background:\s*transparent/);
  assert.match(appCssSource, /\.dark\s+\.app-shell-windowbar\s*\{[^}]*background:\s*transparent/);
  assert.doesNotMatch(
    appSource,
    /app-shell-sidebar-frame[\s\S]*\{platform !== 'macos' \? 'pt-7' : 'pt-2'\}/
  );
  assert.doesNotMatch(
    appSource,
    /app-shell-main-frame[\s\S]*\{platform !== 'macos' \? 'pt-7' : ''\}/
  );
});

test('统一底板结构下，卡片感应集中在 frame 层，内层容器不再重复叠加厚重背景与阴影', async () => {
  const appCssSource = await readFile(new URL('./app.css', import.meta.url), 'utf8');

  assert.match(appCssSource, /\.app-shell-sidebar-frame[\s\S]*background:/);
  assert.match(appCssSource, /\.app-shell-main-frame[\s\S]*background:/);
  assert.match(appCssSource, /\.app-shell-sidebar\s*\{[\s\S]*background:\s*transparent;/);
  assert.match(appCssSource, /\.app-shell-sidebar\s*\{[\s\S]*box-shadow:\s*none;/);
  assert.match(appCssSource, /\.app-shell-main\s*\{[\s\S]*background:\s*transparent;/);
  assert.match(appCssSource, /\.app-shell-main\s*\{[\s\S]*box-shadow:\s*none;/);
  assert.match(appCssSource, /\.sidebar-editorial-shell\s*\{[\s\S]*background:\s*transparent;/);
  assert.doesNotMatch(appCssSource, /\.sidebar-editorial-shell::before/);
});

test('外围主卡片四角应使用同一套圆角，frame 不应硬裁剪滚动条', async () => {
  const appCssSource = await readFile(new URL('./app.css', import.meta.url), 'utf8');

  assert.match(appCssSource, /--app-shell-frame-radius:\s*2rem/);
  assert.match(appCssSource, /--app-shell-inner-radius:\s*1\.78rem/);
  assert.match(appCssSource, /\.app-shell-sidebar-frame,\s*\.app-shell-main-frame\s*\{[^}]*border-radius:\s*var\(--app-shell-frame-radius\)/);
  assert.doesNotMatch(appCssSource, /\.app-shell-sidebar-frame,\s*\.app-shell-main-frame\s*\{[^}]*overflow:\s*hidden/);
  assert.match(appCssSource, /\.app-shell-sidebar\s*\{[\s\S]*border-radius:\s*var\(--app-shell-inner-radius\)/);
  assert.match(appCssSource, /\.app-shell-main\s*\{[\s\S]*border-radius:\s*var\(--app-shell-inner-radius\)/);
});

test('主内容滚动条应从圆角裁剪边界内缩，避免顶部和底部被遮住', async () => {
  const appCssSource = await readFile(new URL('./app.css', import.meta.url), 'utf8');

  assert.match(appCssSource, /\.app-shell-main\s*\{[\s\S]*padding:\s*0\.18rem/);
  assert.match(appCssSource, /\.app-shell-main-scroll\s*\{[\s\S]*scrollbar-gutter:\s*stable/);
  assert.match(appCssSource, /\.app-shell-main-scroll\s*\{[\s\S]*scrollbar-width:\s*thin/);
  assert.match(appCssSource, /\.app-shell-main-scroll\s*\{[\s\S]*scrollbar-color:\s*rgba\(100,\s*116,\s*139,\s*0\.42\)\s*transparent/);
  assert.match(appCssSource, /\.app-shell-main-scroll\s*\{[\s\S]*border-radius:\s*calc\(var\(--app-shell-inner-radius\) - 0\.18rem\)/);
  assert.match(appCssSource, /\.app-shell-main-scroll\s*\{[\s\S]*margin-right:\s*0\.42rem/);
  assert.match(appCssSource, /\.app-shell-main-scroll\s*\{[\s\S]*padding-right:\s*0\.28rem/);
  assert.match(appCssSource, /\.app-shell-main-scroll\s*\{[\s\S]*padding-bottom:\s*0\.12rem/);
  assert.match(appCssSource, /\.app-shell-main-scroll::-webkit-scrollbar-track\s*\{[^}]*margin-block:\s*1rem/);
  assert.match(appCssSource, /\.app-shell-main-scroll::-webkit-scrollbar-thumb\s*\{[^}]*border:\s*2px solid transparent/);
  assert.match(appCssSource, /\.app-shell-main-scroll::-webkit-scrollbar-thumb\s*\{[^}]*border-block-width:\s*0\.85rem/);
  assert.match(appCssSource, /\.app-shell-main-scroll::-webkit-scrollbar-button\s*\{[^}]*display:\s*none/);
  assert.doesNotMatch(appCssSource, /\.app-shell-main-scroll::-webkit-scrollbar-button:vertical:start:decrement,\s*\.app-shell-main-scroll::-webkit-scrollbar-button:vertical:end:increment\s*\{[^}]*display:\s*block/);
});

test('浮层滚动条应使用轻量内嵌样式，避免下拉菜单出现厚重滚动槽', async () => {
  // 载体变更：日报预设下拉已改为胶片集合（无浮层），
  // 轻量浮层滚动样式的在用载体改为助手历史会话抽屉。
  const [appCssSource, askSource] = await Promise.all([
    readFile(new URL('./app.css', import.meta.url), 'utf8'),
    readFile(new URL('./routes/ask/Ask.svelte', import.meta.url), 'utf8'),
  ]);

  assert.match(askSource, /app-floating-scroll/);
  assert.match(appCssSource, /\.app-floating-scroll\s*\{[^}]*scrollbar-width:\s*thin/);
  assert.match(appCssSource, /\.app-floating-scroll::-webkit-scrollbar\s*\{[^}]*width:\s*0\.5rem/);
  assert.match(appCssSource, /\.app-floating-scroll::-webkit-scrollbar-track\s*\{[^}]*margin-block:\s*0\.35rem/);
  assert.match(appCssSource, /\.app-floating-scroll::-webkit-scrollbar-button\s*\{[^}]*display:\s*none/);
  assert.doesNotMatch(appCssSource, /\.app-floating-scroll::-webkit-scrollbar-button:vertical:start:decrement,\s*\.app-floating-scroll::-webkit-scrollbar-button:vertical:end:increment\s*\{[^}]*display:\s*block/);
});
