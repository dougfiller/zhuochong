import test from 'node:test';
import assert from 'node:assert/strict';

import {
  AVATAR_OPACITY_DEFAULT,
  AVATAR_OPACITY_MAX,
  AVATAR_OPACITY_MIN,
  AVATAR_SCALE_DEFAULT,
  AVATAR_SCALE_MAX,
  AVATAR_SCALE_MIN,
  clampAvatarOpacity,
  clampAvatarScale,
  formatAvatarOpacityLabel,
  formatAvatarScaleLabel,
  getAvatarToggleUiState,
  getAvatarToggleToast,
  toggleAvatarSetting,
  updateAvatarOpacitySetting,
  updateAvatarScaleSetting,
} from './avatarToggle.js';

test('开启桌宠时应立即以开启后的配置保存', async () => {
  const config = { avatar_enabled: false };
  let savedConfig = null;

  const enabled = await toggleAvatarSetting(config, async (nextConfig) => {
    savedConfig = { ...nextConfig };
  });

  assert.equal(enabled, true);
  assert.equal(config.avatar_enabled, true);
  assert.deepEqual(savedConfig, { avatar_enabled: true });
});

test('保存失败时应回滚桌宠开关状态', async () => {
  const config = { avatar_enabled: true };

  await assert.rejects(
    toggleAvatarSetting(config, async () => {
      throw new Error('save failed');
    }),
    /save failed/
  );

  assert.equal(config.avatar_enabled, true);
});

test('关闭桌宠时应同时关闭休息提醒，避免保留无效依赖配置', async () => {
  const config = { avatar_enabled: true, break_reminder_enabled: true };
  let savedConfig = null;

  const enabled = await toggleAvatarSetting(config, async (nextConfig) => {
    savedConfig = { ...nextConfig };
  });

  assert.equal(enabled, false);
  assert.equal(config.avatar_enabled, false);
  assert.equal(config.break_reminder_enabled, false);
  assert.deepEqual(savedConfig, {
    avatar_enabled: false,
    break_reminder_enabled: false,
  });
});

test('桌宠开关提示文案应与状态匹配', () => {
  assert.equal(
    getAvatarToggleToast(true),
    '桌宠已显示，可在屏幕右下角附近查看'
  );
  assert.equal(getAvatarToggleToast(false), '桌宠已隐藏');
});

test('桌宠开关 UI 状态应区分开启和关闭', () => {
  const enabledState = getAvatarToggleUiState(true, false);
  const disabledState = getAvatarToggleUiState(false, true);

  assert.match(enabledState.trackClass, /bg-primary-500/);
  assert.equal(enabledState.thumbClass, 'translate-x-5');
  assert.equal(enabledState.ariaLabel, '关闭桌面化身');

  assert.match(disabledState.trackClass, /bg-slate-300/);
  assert.equal(disabledState.thumbClass, 'translate-x-0');
  assert.match(disabledState.buttonClass, /cursor-wait/);
  assert.equal(disabledState.ariaLabel, '开启桌面化身');
});

test('桌宠缩放应限制在允许范围内', () => {
  assert.equal(AVATAR_SCALE_MIN, 0.4);
  assert.equal(AVATAR_SCALE_MAX, 1.3);
  assert.equal(AVATAR_SCALE_DEFAULT, 0.9);
  assert.equal(clampAvatarScale(0.2), 0.4);
  assert.equal(clampAvatarScale(2), 1.3);
  assert.equal(clampAvatarScale('bad'), 0.9);
});

test('桌宠缩放文案应格式化为百分比', () => {
  assert.equal(formatAvatarScaleLabel(0.9), '90%');
  assert.equal(formatAvatarScaleLabel(1.26), '126%');
});

test('桌宠缩放保存失败时应回滚到之前的值', async () => {
  const config = { avatar_scale: 0.9 };

  await assert.rejects(
    updateAvatarScaleSetting(config, 1.2, async () => {
      throw new Error('scale save failed');
    }),
    /scale save failed/
  );

  assert.equal(config.avatar_scale, 0.9);
});

test('桌宠透明度应限制在允许范围内', () => {
  assert.equal(AVATAR_OPACITY_MIN, 0.45);
  assert.equal(AVATAR_OPACITY_MAX, 1);
  assert.equal(AVATAR_OPACITY_DEFAULT, 0.82);
  assert.equal(clampAvatarOpacity(0.1), 0.45);
  assert.equal(clampAvatarOpacity(2), 1);
  assert.equal(clampAvatarOpacity('bad'), 0.82);
});

test('桌宠透明度文案应格式化为百分比', () => {
  assert.equal(formatAvatarOpacityLabel(0.82), '82%');
  assert.equal(formatAvatarOpacityLabel(0.93), '93%');
});

test('桌宠透明度保存失败时应回滚到之前的值', async () => {
  const config = { avatar_opacity: 0.82 };

  await assert.rejects(
    updateAvatarOpacitySetting(config, 0.6, async () => {
      throw new Error('opacity save failed');
    }),
    /opacity save failed/
  );

  assert.equal(config.avatar_opacity, 0.82);
});
