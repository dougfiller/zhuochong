import test from 'node:test';
import assert from 'node:assert/strict';
import { resolveModelOptionLabel } from './modelPresentation.js';

const translations = {
  'zh-CN': {
    'ask.basicTemplate': '基础模板',
    'ask.aiEnhanced': 'AI 增强',
  },
  en: {
    'ask.basicTemplate': 'Template',
    'ask.aiEnhanced': 'AI Enhanced',
  },
};

function createTranslate(locale) {
  return (key) => translations[locale]?.[key] || key;
}

test('基础模型应显示当前语言的基础模板名称', () => {
  assert.equal(
    resolveModelOptionLabel('__basic__', [], 'zh-CN', createTranslate('zh-CN')),
    '基础模板',
  );
});

test('配置模型应优先显示用户配置的档案名称', () => {
  const profiles = [
    {
      id: 'work-model',
      name: '工作专用模型',
      model_config: { provider: 'openai', model: 'gpt-4.1' },
    },
  ];

  assert.equal(
    resolveModelOptionLabel('work-model', profiles, 'zh-CN', createTranslate('zh-CN')),
    '工作专用模型',
  );
});

test('无效模型选择应回退到当前语言的基础模板名称', () => {
  assert.equal(
    resolveModelOptionLabel('missing-model', [], 'en', createTranslate('en')),
    'Template',
  );
});

test('缺少档案名称时应随 locale 变化本地化 provider 名称', () => {
  const profiles = [
    {
      id: 'local-model',
      name: '   ',
      model_config: { provider: 'ollama', model: 'qwen3:8b' },
    },
  ];

  assert.equal(
    resolveModelOptionLabel('local-model', profiles, 'zh-CN', createTranslate('zh-CN')),
    'Ollama (本地) · qwen3:8b',
  );
  assert.equal(
    resolveModelOptionLabel('local-model', profiles, 'en', createTranslate('en')),
    'Ollama (Local) · qwen3:8b',
  );
});
