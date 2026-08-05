import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

test('未冻结发行输入时 Release workflow 必须是明确失败的手动门禁', () => {
  const source = readFileSync(new URL('./.github/workflows/release.yml', import.meta.url), 'utf8');

  assert.match(source, /workflow_dispatch/);
  assert.match(source, /Formal release is disabled/);
  assert.match(source, /exit 1/);
  assert.doesNotMatch(source, /push:\s*[\s\S]*tags:|ncipollo\/release-action|TAURI_SIGNING_PRIVATE_KEY/);
});
