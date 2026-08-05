import test from 'node:test';
import assert from 'node:assert/strict';
import {
  getPreferredTimelineAppName,
  shouldPreferTimelineFallbackIcon,
} from './appDisplay.js';

test('时间线应优先显示更友好的窗口标题作为应用名', () => {
  assert.equal(
    getPreferredTimelineAppName({
      appName: 'uninstall',
      windowTitle: 'Work Review Uninstall',
    }),
    'Work Review Uninstall'
  );

  assert.equal(
    getPreferredTimelineAppName({
      appName: 'xfltd',
      windowTitle: 'XFLTD',
    }),
    'XFLTD'
  );

  assert.equal(
    getPreferredTimelineAppName({
      appName: 'Work_Review.v1.0.35_x64-setup',
      windowTitle: 'Work Review Setup',
    }),
    'Work Review Setup'
  );
});

test('时间线对安装器与原始小写进程名应优先使用 fallback icon', () => {
  assert.equal(
    shouldPreferTimelineFallbackIcon({
      appName: 'uninstall',
      windowTitle: 'Work Review Uninstall',
    }),
    true
  );

  assert.equal(
    shouldPreferTimelineFallbackIcon({
      appName: 'Work_Review.v1.0.35_x64-setup',
      windowTitle: 'Work Review Setup',
    }),
    true
  );

  assert.equal(
    shouldPreferTimelineFallbackIcon({
      appName: 'xfltd',
      windowTitle: 'XFLTD',
    }),
    true
  );

  assert.equal(
    shouldPreferTimelineFallbackIcon({
      appName: 'Microsoft Edge',
      windowTitle: 'downloads-hub',
    }),
    false
  );
});

test('时间线展示工具应兼容后端活动记录的 snake_case 字段', () => {
  assert.equal(
    getPreferredTimelineAppName({
      app_name: 'Visual Studio Code',
      window_title: '优化时间线交互与分类弹层',
    }),
    'Visual Studio Code'
  );
});

test('时间线 fallback icon 判断应兼容后端活动记录的 snake_case 字段', () => {
  assert.equal(
    shouldPreferTimelineFallbackIcon({
      app_name: 'xfltd',
      window_title: 'XFLTD',
    }),
    true
  );
});
