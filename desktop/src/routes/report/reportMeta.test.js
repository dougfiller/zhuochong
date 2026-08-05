import test from 'node:test';
import assert from 'node:assert/strict';

import { resolveReportMeta } from './reportMeta.js';

test('AI 增强配置与模板回退日报应检测到不匹配', () => {
  const meta = resolveReportMeta(
    {
      ai_mode: 'summary',
      model_name: 'gpt-4.1',
      fallback_reason: '请求失败，已回退到基础模板',
      content: '---\n*注：AI 分析暂不可用，使用基础模板生成。*',
    },
    {
      ai_mode: 'summary',
      text_model: {
        model: 'gpt-4.1',
      },
    }
  );

  assert.equal(meta.reportMode, 'local');
  assert.equal(meta.showUsageMismatchNotice, true);
  assert.equal(meta.fallbackReason, '请求失败，已回退到基础模板');
});

test('没有已保存日报时应回退为当前配置模式', () => {
  const meta = resolveReportMeta(null, {
    ai_mode: 'summary',
    text_model: {
      model: 'qwen2.5',
    },
  });

  assert.equal(meta.reportMode, 'summary');
  assert.equal(meta.showUsageMismatchNotice, false);
  assert.equal(meta.fallbackReason, null);
});

test('当前日报与当前配置不一致时应显示不匹配提示', () => {
  const meta = resolveReportMeta(
    {
      ai_mode: 'local',
      model_name: null,
      content: '# 工作日报',
    },
    {
      ai_mode: 'summary',
      text_model: {
        model: 'gemma3:27b',
      },
    }
  );

  assert.equal(meta.reportMode, 'local');
  assert.equal(meta.showUsageMismatchNotice, true);
  assert.equal(meta.fallbackReason, null);
});

test('已保存日报存在回退原因时应优先暴露该友好原因', () => {
  const meta = resolveReportMeta(
    {
      ai_mode: 'local',
      model_name: null,
      fallback_reason: '返回空内容，已回退到基础模板',
      content: '# 工作日报',
    },
    {
      ai_mode: 'summary',
      text_model: {
        model: 'gpt-5.4',
      },
    }
  );

  assert.equal(meta.showUsageMismatchNotice, true);
  assert.equal(meta.fallbackReason, '返回空内容，已回退到基础模板');
});
