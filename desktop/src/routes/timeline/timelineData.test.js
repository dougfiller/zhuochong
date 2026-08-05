import test from 'node:test';
import assert from 'node:assert/strict';

import { prepareTimelineActivities, upsertTimelineActivity } from './timelineData.js';

test('时间线不应按应用名二次合并不同窗口记录', () => {
  const input = [
    {
      id: 101,
      timestamp: 1710000010,
      app_name: 'Windows Terminal',
      window_title: 'npm run tauri dev',
      screenshot_path: 'screenshots/2026-03-29/0010.jpg',
      duration: 240,
      browser_url: null,
    },
    {
      id: 102,
      timestamp: 1710000090,
      app_name: 'Windows Terminal',
      window_title: 'cargo test',
      screenshot_path: 'screenshots/2026-03-29/0090.jpg',
      duration: 180,
      browser_url: null,
    },
  ];

  const result = prepareTimelineActivities(input);

  assert.equal(result.length, 2);
  assert.deepEqual(
    result.map((item) => item.id),
    [102, 101]
  );
});

test('实时更新只按 id 覆盖，否则应插入新活动', () => {
  const current = [
    {
      id: 201,
      timestamp: 1710000010,
      app_name: 'Windows Terminal',
      window_title: 'npm run tauri dev',
      screenshot_path: 'screenshots/2026-03-29/0010.jpg',
      duration: 240,
      browser_url: null,
    },
  ];

  const appended = upsertTimelineActivity(current, {
    id: 202,
    timestamp: 1710000100,
    app_name: 'Windows Terminal',
    window_title: 'cargo test',
    screenshot_path: 'screenshots/2026-03-29/0100.jpg',
    duration: 60,
    browser_url: null,
  });

  assert.equal(appended.length, 2);
  assert.equal(appended[0].id, 202);
  assert.equal(appended[1].id, 201);

  const replaced = upsertTimelineActivity(appended, {
    id: 202,
    timestamp: 1710000160,
    app_name: 'Windows Terminal',
    window_title: 'cargo test --lib',
    screenshot_path: 'screenshots/2026-03-29/0160.jpg',
    duration: 120,
    browser_url: null,
  });

  assert.equal(replaced.length, 2);
  assert.equal(replaced[0].id, 202);
  assert.equal(replaced[0].window_title, 'cargo test --lib');
  assert.equal(replaced[0].screenshot_path, 'screenshots/2026-03-29/0160.jpg');
});
