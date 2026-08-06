import test from 'node:test';
import assert from 'node:assert/strict';
import { formatWechatUserError } from './errorDisplay.js';

const translate = (key) => `localized:${key}`;

test('微信错误只映射白名单稳定码，不回显未知后端错误', () => {
  assert.equal(
    formatWechatUserError('request failed: WX_NOT_FOREGROUND', 'fallback', translate),
    'localized:wechat.errors.notForeground',
  );
  assert.equal(
    formatWechatUserError('secret endpoint /tmp/trace.json', 'fallback', translate),
    'fallback',
  );
});
