import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';

test('旧的手绘外观调色辅助应移除，桌宠外观改由预设资源和状态元信息驱动', () => {
  assert.equal(existsSync(new URL('./avatarAppearance.js', import.meta.url)), false);

  const canvasSource = readFileSync(new URL('./AvatarCanvas.svelte', import.meta.url), 'utf8');
  const registrySource = readFileSync(new URL('./avatarPresetRegistry.js', import.meta.url), 'utf8');
  const stateMetaSource = readFileSync(new URL('./avatarStateMeta.js', import.meta.url), 'utf8');

  assert.doesNotMatch(canvasSource, /getAvatarAppearance/);
  assert.match(canvasSource, /getAvatarPresetDefinition/);
  assert.match(canvasSource, /getAvatarModeMeta/);
  assert.match(registrySource, /AVATAR_PRESET_REGISTRY/);
  assert.match(stateMetaSource, /getAvatarModeMeta/);
});
