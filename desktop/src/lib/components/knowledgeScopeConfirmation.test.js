import test from 'node:test';
import assert from 'node:assert/strict';
import { confirmFreshScopeForNextRequest } from './knowledgeScopeConfirmation.js';

test('fresh observation is shown before the matching one-shot authorization', async () => {
  const calls = [];
  const observation = { bindingGeneration: 7, bindingObservationVersion: 12, headerClue: '虚构线索' };
  const result = await confirmFreshScopeForNextRequest({
    currentGeneration: 7,
    observe: async () => {
      calls.push('observe');
      return observation;
    },
    requestConfirmation: async (current) => {
      calls.push(`confirm:${current.headerClue}`);
      return true;
    },
    authorize: async (current) => {
      calls.push(`authorize:${current.bindingObservationVersion}`);
    },
  });

  assert.deepEqual(calls, ['observe', 'confirm:虚构线索', 'authorize:12']);
  assert.equal(result.authorized, true);
});

test('cancel and generation change never issue a one-shot authorization', async () => {
  for (const [generation, confirmResult, expectedCalls] of [
    [7, false, ['observe', 'confirm']],
    [8, true, ['observe']],
  ]) {
    const calls = [];
    const result = await confirmFreshScopeForNextRequest({
      currentGeneration: 7,
      observe: async () => {
        calls.push('observe');
        return { bindingGeneration: generation, bindingObservationVersion: 13 };
      },
      requestConfirmation: async () => {
        calls.push('confirm');
        return confirmResult;
      },
      authorize: async () => {
        calls.push('authorize');
      },
    });
    assert.deepEqual(calls, expectedCalls);
    assert.equal(result.authorized, false);
  }
});
