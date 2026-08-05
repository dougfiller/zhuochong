import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('Cargo.toml 应声明 cargo-clippy 兼容 feature 以避免 objc 宏触发 check-cfg 误报', () => {
  const source = readFileSync(new URL('./src-tauri/Cargo.toml', import.meta.url), 'utf8');

  assert.match(
    source,
    /\[features\][\s\S]*\bcargo-clippy\s*=\s*\[\s*\]/,
    '需要显式声明 cargo-clippy feature，兼容旧 objc 宏里的 cfg(feature = "cargo-clippy")'
  );
});

test('package.json / src-tauri Cargo.toml / tauri.conf.json 三处版本号必须一致', () => {
  const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'));
  const tauriConf = JSON.parse(
    readFileSync(new URL('./src-tauri/tauri.conf.json', import.meta.url), 'utf8')
  );
  const cargo = readFileSync(new URL('./src-tauri/Cargo.toml', import.meta.url), 'utf8');
  const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];

  assert.equal(
    pkg.version,
    tauriConf.version,
    `package.json(${pkg.version}) 与 tauri.conf.json(${tauriConf.version}) 版本不一致`
  );
  assert.equal(
    pkg.version,
    cargoVersion,
    `package.json(${pkg.version}) 与 src-tauri/Cargo.toml(${cargoVersion}) 版本不一致`
  );
});
