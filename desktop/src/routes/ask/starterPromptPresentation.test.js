import test from 'node:test';
import assert from 'node:assert/strict';
import { selectStarterPrompts } from './starterPromptPresentation.js';

function sequenceRandom(values) {
  let index = 0;
  return () => values[index++ % values.length];
}

test('随机快捷问题应固定返回四条且自动去重', () => {
  const result = selectStarterPrompts({
    localPrompts: ['A', 'B', 'C', 'D', 'E', 'A', '  B  '],
    dynamicPrompts: ['D', 'F', '', null],
    random: sequenceRandom([0.8, 0.1, 0.6, 0.3, 0.9, 0.2]),
  });

  assert.equal(result.length, 4);
  assert.equal(new Set(result).size, 4);
  assert.ok(result.every((prompt) => typeof prompt === 'string' && prompt.trim() === prompt));
});

test('随机函数可注入，便于稳定验证不同排列', () => {
  const prompts = ['A', 'B', 'C', 'D', 'E', 'F'];
  const first = selectStarterPrompts({
    localPrompts: prompts,
    random: sequenceRandom([0, 0, 0, 0, 0]),
  });
  const second = selectStarterPrompts({
    localPrompts: prompts,
    random: sequenceRandom([0.99, 0.99, 0.99, 0.99, 0.99]),
  });

  assert.notDeepEqual(first, second);
});

test('候选足够时应避免连续两轮展示完全相同的四条', () => {
  const prompts = ['A', 'B', 'C', 'D', 'E', 'F'];
  const previousPrompts = ['C', 'D', 'E', 'F'];
  const result = selectStarterPrompts({
    localPrompts: prompts,
    previousPrompts,
    random: sequenceRandom([0.99, 0.99, 0.99, 0.99, 0.99]),
  });

  assert.notDeepEqual(result, previousPrompts);
  assert.equal(result.length, 4);
});

test('候选不足四条时返回全部可用问题而不是制造重复项', () => {
  const result = selectStarterPrompts({
    localPrompts: ['A', 'B', 'A'],
    dynamicPrompts: ['C'],
    random: () => 0.5,
  });

  assert.deepEqual(new Set(result), new Set(['A', 'B', 'C']));
  assert.equal(result.length, 3);
});
