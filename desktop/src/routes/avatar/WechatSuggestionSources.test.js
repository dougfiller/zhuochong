import test from 'node:test';
import assert from 'node:assert/strict';
import { loadCurrentWechatSuggestionSources } from './wechatSuggestionSources.js';

const suggestion = {
  requestId: 'request-1',
  suggestionGeneration: 2,
  bindingGeneration: 3,
};

test('current suggestion sources load only after an explicit call and pass requestId only', async () => {
  const calls = [];
  const invoke = async (...args) => {
    calls.push(args);
    return { hasSourceDetails: true, items: [] };
  };

  assert.equal(await loadCurrentWechatSuggestionSources(invoke, null, () => null), null);
  assert.equal(calls.length, 0);
  assert.deepEqual(
    await loadCurrentWechatSuggestionSources(invoke, suggestion, () => suggestion),
    { hasSourceDetails: true, items: [] },
  );
  assert.deepEqual(calls, [[
    'get_wechat_reply_sources',
    { input: { requestId: 'request-1' } },
  ]]);
});

test('a replaced suggestion cannot display the old request sources', async () => {
  const replacement = { ...suggestion, requestId: 'request-2' };
  const result = await loadCurrentWechatSuggestionSources(
    async () => ({ hasSourceDetails: true, items: [{ ordinal: 1 }] }),
    suggestion,
    () => replacement,
  );
  assert.equal(result, null);
});
