import { test } from 'node:test';
import assert from 'node:assert';
import { assistantStore } from './lib/stores/assistant.js';

function snapshot() {
  let s;
  const unsub = assistantStore.subscribe((v) => {
    s = v;
  });
  unsub();
  return s;
}

test('normalizeMessage 为消息补齐 id/steps/streaming 默认值（兼容旧 localStorage）', () => {
  assistantStore.reset();
  assistantStore.appendMessage({ role: 'assistant', content: 'hi' });
  const m = snapshot().messages[0];
  assert.ok(m.id, '应自动生成 id');
  assert.deepEqual(m.steps, []);
  assert.equal(m.streaming, false);
  assert.deepEqual(m.references, []);
  assert.deepEqual(m.toolLabels, []);
});

test('updateLastStreaming 增量更新当前 streaming 消息（步骤 → 命中 → 收尾）', () => {
  assistantStore.reset();
  assistantStore.appendMessage({ role: 'user', content: '今天做了什么' });
  assistantStore.appendMessage({ role: 'assistant', content: '', streaming: true, steps: [] });

  // StepStart
  assistantStore.updateLastStreaming((m) => ({
    ...m,
    steps: [...m.steps, { tool: 'search_memory', label: '记忆检索', status: 'running' }],
  }));
  // StepResult
  assistantStore.updateLastStreaming((m) => ({
    ...m,
    steps: m.steps.map((s, i) =>
      i === m.steps.length - 1 ? { ...s, status: 'done', hits: 3 } : s
    ),
    references: [{ title: 'r1', timestamp: 1 }],
  }));
  // Done
  assistantStore.updateLastStreaming((m) => ({ ...m, content: '答案', streaming: false }));

  const msgs = snapshot().messages;
  assert.equal(msgs.length, 2);
  assert.equal(msgs[1].content, '答案');
  assert.equal(msgs[1].streaming, false);
  assert.equal(msgs[1].steps.length, 1);
  assert.equal(msgs[1].steps[0].status, 'done');
  assert.equal(msgs[1].steps[0].hits, 3);
  assert.equal(msgs[1].references.length, 1);
});

test('updateLastStreaming 在无 streaming 消息时不改动状态', () => {
  assistantStore.reset();
  assistantStore.appendMessage({ role: 'user', content: '问' });
  const beforeLen = snapshot().messages.length;
  assistantStore.updateLastStreaming((m) => ({ ...m, content: '不应出现' }));
  const after = snapshot();
  assert.equal(after.messages.length, beforeLen);
  assert.ok(!after.messages.some((m) => m.content === '不应出现'));
});

test('setSelectedModelId 默认标记用户已手动选择（hasUserSelectedModel=true）', () => {
  assistantStore.reset();
  assert.equal(snapshot().hasUserSelectedModel, false);
  assistantStore.setSelectedModelId('some-model-id');
  assert.equal(snapshot().hasUserSelectedModel, true);
});

test('setSelectedModelId 传 userInitiated:false 时不标记用户已选（程序初始化）', () => {
  assistantStore.reset();
  assistantStore.setSelectedModelId('auto-selected', { userInitiated: false });
  assert.equal(snapshot().hasUserSelectedModel, false);
  assert.equal(snapshot().selectedModelId, 'auto-selected');
});

test('用户手动切回基础模板后不再自动选模型（尊重用户选择）', () => {
  assistantStore.reset();
  // 程序自动选中（首次）
  assistantStore.setSelectedModelId('deepseek-profile', { userInitiated: false });
  assert.equal(snapshot().hasUserSelectedModel, false);
  // 用户手动切回基础模板
  assistantStore.setSelectedModelId('__basic__');
  assert.equal(snapshot().hasUserSelectedModel, true);
  assert.equal(snapshot().selectedModelId, '__basic__');
});

test('updateMessageById 在两个 streaming 消息并存时只更新目标消息', () => {
  assistantStore.reset();
  assistantStore.setMessages([
    { id: 'stream-a', role: 'assistant', content: '回答 A', streaming: true },
    { id: 'stream-b', role: 'assistant', content: '回答 B', streaming: true },
  ]);

  assistantStore.updateMessageById('stream-b', (message) => ({
    ...message,
    content: `${message.content}（更新）`,
  }));

  const messages = snapshot().messages;
  assert.equal(messages[0].content, '回答 A');
  assert.equal(messages[0].streaming, true);
  assert.equal(messages[1].content, '回答 B（更新）');
  assert.equal(messages[1].streaming, true);
});

test('updateMessageById 找不到消息 ID 时保持状态对象不变', () => {
  assistantStore.reset();
  assistantStore.setMessages([
    { id: 'existing', role: 'assistant', content: '原内容', streaming: true },
  ]);
  const before = snapshot();
  let updaterCalled = false;

  assistantStore.updateMessageById('missing', (message) => {
    updaterCalled = true;
    return { ...message, content: '不应出现' };
  });

  assert.strictEqual(snapshot(), before);
  assert.equal(updaterCalled, false);
});

test('sending 只允许当前请求结束，旧请求 finally 不会释放新请求', () => {
  assistantStore.reset();

  assistantStore.beginSending('request-old');
  assert.equal(snapshot().sending, true);
  assert.equal(snapshot().sendingRequestId, 'request-old');

  // 旧组件卸载时释放自己的请求，新组件随后开始另一请求。
  assistantStore.finishSending('request-old');
  assistantStore.beginSending('request-new');

  // 旧请求稍后进入 finally，不能清理新请求的 sending 状态。
  assistantStore.finishSending('request-old');
  assert.equal(snapshot().sending, true);
  assert.equal(snapshot().sendingRequestId, 'request-new');

  assistantStore.finishSending('request-new');
  assert.equal(snapshot().sending, false);
  assert.equal(snapshot().sendingRequestId, null);
});
