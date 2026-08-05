import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequestEventGate } from './requestEventGate.js';

test('关闭旧请求后迟到事件不会再污染新请求', () => {
  const receivedA = [];
  const receivedB = [];
  const gateA = createRequestEventGate({
    isDestroyed: () => false,
    onEvent: (event) => {
      receivedA.push(event);
      return event.type === 'done';
    },
  });
  const gateB = createRequestEventGate({
    isDestroyed: () => false,
    onEvent: (event) => {
      receivedB.push(event);
      return event.type === 'done';
    },
  });

  gateA.handle({ type: 'token', token: '旧' });
  gateA.close();
  gateB.handle({ type: 'token', token: '新' });
  gateA.handle({ type: 'token', token: '不应写入' });
  gateA.handle({ type: 'done', answer: '旧答案' });

  assert.deepEqual(receivedA, [{ type: 'token', token: '旧' }]);
  assert.deepEqual(receivedB, [{ type: 'token', token: '新' }]);
});

test('终态事件会自动关闭请求并拒绝后续事件', () => {
  const received = [];
  const gate = createRequestEventGate({
    isDestroyed: () => false,
    onEvent: (event) => {
      received.push(event.type);
      return event.type === 'done' || event.type === 'error';
    },
  });

  assert.equal(gate.handle({ type: 'done' }), true);
  assert.equal(gate.handle({ type: 'token', token: 'late' }), false);
  assert.deepEqual(received, ['done']);
});

test('组件销毁后不再接收事件', () => {
  let destroyed = false;
  let calls = 0;
  const gate = createRequestEventGate({
    isDestroyed: () => destroyed,
    onEvent: () => {
      calls += 1;
      return false;
    },
  });

  gate.handle({ type: 'token', token: 'before' });
  destroyed = true;
  gate.handle({ type: 'token', token: 'after' });

  assert.equal(calls, 1);
});
