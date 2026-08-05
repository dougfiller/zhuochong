import test from 'node:test';
import assert from 'node:assert/strict';
import { getFallbackAppIcon } from './appVisuals.js';

function decodeSvg(dataUrl) {
  return decodeURIComponent(dataUrl.replace('data:image/svg+xml;utf8,', ''));
}

test('常见系统应用在拿不到原生图标时应使用明确的兜底图标，而不是退回字母块', () => {
  const discoverIcon = getFallbackAppIcon('Discover');
  const mailIcon = getFallbackAppIcon('Mail');
  const authIcon = getFallbackAppIcon('System Authentication');

  assert.ok(discoverIcon?.startsWith('data:image/svg+xml;utf8,'));
  assert.ok(mailIcon?.startsWith('data:image/svg+xml;utf8,'));
  assert.ok(authIcon?.startsWith('data:image/svg+xml;utf8,'));

  assert.doesNotMatch(decodeSvg(discoverIcon), /<text/i);
  assert.doesNotMatch(decodeSvg(mailIcon), /<text/i);
  assert.doesNotMatch(decodeSvg(authIcon), /<text/i);
});
