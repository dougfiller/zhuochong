import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('桌面化身开关应暴露本地化 switch 语义', async () => {
  const source = await readFile(new URL('./components/SettingsAppearance.svelte', import.meta.url), 'utf8');

  assert.equal((source.match(/role="switch"/g) || []).length, 5);
  assert.equal((source.match(/aria-checked=\{/g) || []).length, 5);
  assert.match(source, /aria-label=\{t\('settingsAppearance\.avatarClickThrough'\)\}/);
  assert.match(source, /aria-label=\{t\('settingsAppearance\.avatarBodyHidden'\)\}/);
  assert.match(source, /aria-label=\{t\('settingsAppearance\.breakReminder'\)\}/);
});

test('桌面化身 Beta 应显示在外层标签栏而不是内容卡内部', async () => {
  const [settingsSource, appearanceSource] = await Promise.all([
    readFile(new URL('./Settings.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./components/SettingsAppearance.svelte', import.meta.url), 'utf8'),
  ]);

  assert.match(settingsSource, /id:\s*'avatar'[^\n]*beta:\s*true/);
  // 桌面化身区用 avatarBetaHint 文案、不应硬编码 Beta 徽标；
  // 界面风格区的 Beta 徽标是独立功能，不受此约束。
  const avatarBlock =
    appearanceSource.match(/\{#if showAvatarControls\}[\s\S]*?\{\/if\}/)?.[0] ?? appearanceSource;
  assert.doesNotMatch(avatarBlock, />\s*Beta\s*</);
});
