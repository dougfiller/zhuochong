import test from 'node:test';
import assert from 'node:assert/strict';

import { readFile } from 'node:fs/promises';

import {
  CAPTURE_TIMEZONE,
  README_SCREENSHOT_LABELS,
  WORKFLOW_FRAME_LABELS,
  createCaptureContextOptions,
  isAllowedRequestUrl,
  normalizeBaseUrl,
  validateCaptureFixtures,
} from './capture-readme-pages.mjs';

test('截图上下文应固定上海时区与媒体规格所需视口', () => {
  assert.deepEqual(createCaptureContextOptions('en'), {
    viewport: { width: 1491, height: 841 },
    deviceScaleFactor: 2,
    locale: 'en',
    timezoneId: CAPTURE_TIMEZONE,
    colorScheme: 'light',
    reducedMotion: 'reduce',
  });
  assert.equal(CAPTURE_TIMEZONE, 'Asia/Shanghai');
});

test('截图脚本应覆盖 README 引用的全部页面，并用核心流程生成 GIF', () => {
  assert.deepEqual(README_SCREENSHOT_LABELS, [
    '概览',
    '时间线',
    '时间线详情',
    '小时总结',
    '日报',
    '助手',
    '设置-通用',
    '设置-外观',
    '设置-AI模型',
    '设置-桌面化身',
    '设置-隐私',
    '设置-存储',
    '接入管理',
    '关于',
  ]);
  assert.deepEqual(WORKFLOW_FRAME_LABELS, ['概览', '时间线', '日报']);
});

test('截图模拟统计应保持总投入、应用、分类与网站口径一致', () => {
  assert.doesNotThrow(validateCaptureFixtures);
});

test('截图配置应使用与 Rust RemoteStorageConfig 一致的字段名并兼容跨平台换行', async () => {
  const lfSource = (await readFile(new URL('./capture-readme-pages.mjs', import.meta.url), 'utf8'))
    .replaceAll('\r\n', '\n');

  for (const source of [lfSource, lfSource.replaceAll('\n', '\r\n')]) {
    const remoteStorage = source.match(/remote_storage:\s*\{[\s\S]*?\r?\n  \},\r?\n  privacy:/)?.[0] ?? '';

    assert.match(remoteStorage, /access_key:\s*''/);
    assert.match(remoteStorage, /secret_key:\s*''/);
    assert.match(remoteStorage, /region:\s*'us-east-1'/);
    assert.doesNotMatch(remoteStorage, /access_key_id|secret_access_key|enabled:/);
  }
});

test('截图地址应规范化，并仅放行配置地址的同源请求', () => {
  const baseUrl = normalizeBaseUrl('http://localhost:4173/demo');

  assert.equal(baseUrl.href, 'http://localhost:4173/demo/');
  assert.equal(isAllowedRequestUrl('http://localhost:4173/assets/app.js', baseUrl), true);
  assert.equal(isAllowedRequestUrl('http://127.0.0.1:4173/assets/app.js', baseUrl), false);
  assert.equal(isAllowedRequestUrl('https://example.com/favicon.ico', baseUrl), false);
  assert.equal(isAllowedRequestUrl('data:image/png;base64,AA==', baseUrl), true);
  assert.equal(isAllowedRequestUrl('blob:http://localhost:4173/id', baseUrl), true);
  assert.throws(() => normalizeBaseUrl('ftp://localhost:4173'), /HTTP\(S\)/);
});
