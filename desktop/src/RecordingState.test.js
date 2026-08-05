import test from 'node:test';
import assert from 'node:assert/strict';
import { recordingStore, isActiveRecording } from './lib/stores/recording.js';

function snapshot() {
  let s;
  const unsub = recordingStore.subscribe((v) => {
    s = v;
  });
  unsub();
  return s;
}

test('recordingStore 初始状态为录制中且未暂停', () => {
  recordingStore.reset();
  const s = snapshot();
  assert.equal(s.isRecording, true);
  assert.equal(s.isPaused, false);
});

test('setState 正确写入录制与暂停标志', () => {
  recordingStore.reset();
  recordingStore.setState(false, true);
  const s = snapshot();
  assert.equal(s.isRecording, false);
  assert.equal(s.isPaused, true);
});

test('isActiveRecording：录制中且未暂停 → true（圆点应绿+脉冲）', () => {
  recordingStore.setState(true, false);
  assert.equal(isActiveRecording(snapshot()), true);
});

test('isActiveRecording：暂停状态 → false（停止记录后圆点应变灰，issue #131）', () => {
  recordingStore.setState(true, true);
  assert.equal(isActiveRecording(snapshot()), false);
});

test('isActiveRecording：未录制 → false', () => {
  recordingStore.setState(false, false);
  assert.equal(isActiveRecording(snapshot()), false);
});

test('isActiveRecording：对 undefined/null 安全降级为 false', () => {
  assert.equal(isActiveRecording(undefined), false);
  assert.equal(isActiveRecording(null), false);
  assert.equal(isActiveRecording({}), false);
  assert.equal(isActiveRecording({ isRecording: true }), true);
  assert.equal(isActiveRecording({ isRecording: true, isPaused: true }), false);
});

test('setState 对非布尔输入做布尔强制（防御后端事件类型漂移）', () => {
  recordingStore.setState(1, 0);
  const s = snapshot();
  assert.equal(s.isRecording, true);
  assert.equal(s.isPaused, false);

  recordingStore.setState(undefined, undefined);
  const s2 = snapshot();
  assert.equal(s2.isRecording, false);
  assert.equal(s2.isPaused, false);
});
