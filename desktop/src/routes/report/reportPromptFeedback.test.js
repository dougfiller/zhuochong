import test from 'node:test';
import assert from 'node:assert/strict';

import { shouldShowPromptAppliedToast } from './reportPromptFeedback.js';

test('仅在 AI 增强且附加提示词非空并实际使用 AI 时提示已应用附加提示词', () => {
  assert.equal(
    shouldShowPromptAppliedToast({
      configAiMode: 'summary',
      customPrompt: '请在小结最后一句加上固定短句',
      reportAiMode: 'summary',
    }),
    true
  );
});

test('基础模板模式不应提示已应用附加提示词', () => {
  assert.equal(
    shouldShowPromptAppliedToast({
      configAiMode: 'local',
      customPrompt: '请在小结最后一句加上固定短句',
      reportAiMode: 'local',
    }),
    false
  );
});

test('附加提示词为空时不应提示已应用附加提示词', () => {
  assert.equal(
    shouldShowPromptAppliedToast({
      configAiMode: 'summary',
      customPrompt: '   ',
      reportAiMode: 'summary',
    }),
    false
  );
});

test('AI 增强生成失败回退为基础模板时不应提示已应用附加提示词', () => {
  assert.equal(
    shouldShowPromptAppliedToast({
      configAiMode: 'summary',
      customPrompt: '请在小结最后一句加上固定短句',
      reportAiMode: 'local',
    }),
    false
  );
});
