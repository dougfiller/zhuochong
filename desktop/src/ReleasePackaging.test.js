import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('macOS 打包配置应显式注入 Info.plist 权限说明', async () => {
  const tauriConfig = JSON.parse(
    await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')
  );
  const infoPlist = await readFile(
    new URL('../src-tauri/Info.plist', import.meta.url),
    'utf8'
  );

  assert.equal(tauriConfig.bundle.macOS.infoPlist, 'Info.plist');
  assert.match(infoPlist, /NSScreenCaptureUsageDescription/);
  assert.match(infoPlist, /NSAppleEventsUsageDescription/);
});

test('Linux deb 依赖应覆盖 gdbus 与 Wayland 截图工具', async () => {
  const tauriConfig = JSON.parse(
    await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')
  );
  const depends = tauriConfig.bundle.linux.deb.depends;

  assert.ok(depends.includes('libglib2.0-bin'));
  assert.ok(depends.includes('gnome-screenshot | grim | scrot | maim | imagemagick'));
});

test('Linux 发行包应同时生成 RPM 并在 README 中说明', async () => {
  const tauriConfig = JSON.parse(
    await readFile(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')
  );
  const zh = await readFile(new URL('../README.zh.md', import.meta.url), 'utf8');
  const en = await readFile(new URL('../README.md', import.meta.url), 'utf8');
  const tw = await readFile(new URL('../README.tw.md', import.meta.url), 'utf8');

  assert.ok(tauriConfig.bundle.targets.includes('rpm'));
  assert.ok(tauriConfig.bundle.linux.rpm.depends.includes('xdotool'));
  assert.ok(tauriConfig.bundle.linux.rpm.depends.includes('xprintidle'));
  assert.ok(tauriConfig.bundle.linux.rpm.depends.includes('tesseract'));
  assert.ok(tauriConfig.bundle.linux.rpm.depends.includes('procps-ng'));
  assert.match(zh, /Linux x86_64[\s\S]*\.rpm/);
  assert.match(en, /Linux x86_64[\s\S]*\.rpm/);
  assert.match(tw, /Linux x86_64[\s\S]*\.rpm/);
  assert.match(zh, /sudo dnf install/);
  assert.match(en, /sudo dnf install/);
  assert.match(tw, /sudo dnf install/);
});

test('README 应说明 mac 输入监控权限与三端桌宠联动条件', async () => {
  const zh = await readFile(new URL('../README.zh.md', import.meta.url), 'utf8');
  const en = await readFile(new URL('../README.md', import.meta.url), 'utf8');
  const tw = await readFile(new URL('../README.tw.md', import.meta.url), 'utf8');

  assert.match(zh, /输入监控/);
  assert.match(zh, /Windows/);
  assert.match(zh, /Linux/);
  assert.match(en, /Input Monitoring/);
  assert.match(en, /Windows/);
  assert.match(en, /Linux/);
  assert.match(tw, /輸入監控/);
  assert.match(tw, /Windows/);
  assert.match(tw, /Linux/);
});

test('README 应将 Wayland 启动崩溃的首选绕过方案写为关闭 WebKit DMA-BUF 渲染器', async () => {
  const zh = await readFile(new URL('../README.zh.md', import.meta.url), 'utf8');
  const en = await readFile(new URL('../README.md', import.meta.url), 'utf8');
  const tw = await readFile(new URL('../README.tw.md', import.meta.url), 'utf8');

  for (const source of [zh, en, tw]) {
    assert.match(source, /Gdk-Message: Error 71/);
    assert.match(source, /WEBKIT_DISABLE_DMABUF_RENDERER=1 \.\/Work_Review/);
    assert.match(source, /GDK_BACKEND=x11 \.\/Work_Review/);
    assert.ok(
      source.indexOf('WEBKIT_DISABLE_DMABUF_RENDERER=1 ./Work_Review') <
        source.indexOf('GDK_BACKEND=x11 ./Work_Review'),
      'README 应先推荐关闭 WebKit DMA-BUF 渲染器，再把 X11 作为最后兜底'
    );
  }
});

test('本地 mac 打包脚本应关闭 updater 产物签名，避免缺少私钥时报错', async () => {
  const packageJson = JSON.parse(
    await readFile(new URL('../package.json', import.meta.url), 'utf8')
  );
  const localTauriConfig = JSON.parse(
    await readFile(new URL('../src-tauri/tauri.local.conf.json', import.meta.url), 'utf8')
  );

  assert.equal(
    packageJson.scripts['tauri:build:local-mac'],
    'tauri build --target aarch64-apple-darwin --bundles app --config src-tauri/tauri.local.conf.json'
  );
  assert.equal(localTauriConfig.bundle.createUpdaterArtifacts, false);
});
