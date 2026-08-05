import test from 'node:test';
import assert from 'node:assert/strict';
import { isValidLocalDateString } from './dateValidation.js';

test('本地 ISO 日期校验应拒绝不存在的月份和日期', () => {
  assert.equal(isValidLocalDateString('2026-07-28'), true);
  assert.equal(isValidLocalDateString('2024-02-29'), true);
  assert.equal(isValidLocalDateString('2026-02-29'), false);
  assert.equal(isValidLocalDateString('2026-02-31'), false);
  assert.equal(isValidLocalDateString('2026-99-99'), false);
  assert.equal(isValidLocalDateString('2026-7-28'), false);
  assert.equal(isValidLocalDateString(null), false);
});
