import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import zhCN from './lib/i18n/locales/zh-CN.js';
import zhTW from './lib/i18n/locales/zh-TW.js';
import en from './lib/i18n/locales/en.js';
import ar from './lib/i18n/locales/ar.js';
import { MODEL_PROVIDER_DISPLAY_NAMES } from './routes/ask/modelPresentation.js';

const CJK = /[一-鿿]/;

// #116 专项：Ask 助手页英文模式中文残留排查（静态扫描，防回归）。
// 运行时 AI 动态生成的 starter/回答不在此覆盖范围（由模型行为决定）。

test('Ask.svelte 无硬编码中文 UI 文本（英文模式不泄漏）', async () => {
  const src = await readFile(
    new URL('../src/routes/ask/Ask.svelte', import.meta.url),
    'utf8',
  );
  const offenders = [];
  src.split('\n').forEach((raw, i) => {
    const line = raw.trim();
    if (!CJK.test(line)) return;
    // 合法中文：注释 / 日志 / provider 三语结构值 / 段落匹配关键词 / t()/tm() 调用
    if (/^(\/\/|\*|\/\*|<!--)/.test(line)) return;
    if (/console\.|devLog\(/.test(line)) return;
    if (/'zh-CN'\s*:|'zh-TW'\s*:/.test(line)) return;
    if (/[ (]t\(['"]|[ (]tm\(['"]|translateCategory/.test(line)) return;
    if (
      /结论|依据|关键发现|本期概览|重点工作|核心观察|风险与提醒|下阶段建议|工作复盘|主要意图|主要工作|待跟进事项|代表性 Session|相关记录依据|我基于周报复盘|我基于意图识别|我基于 Session 聚合|我基于记忆检索/.test(
        line,
      )
    ) {
      return;
    }
    offenders.push(`${i + 1}: ${line}`);
  });
  assert.deepEqual(
    offenders,
    [],
    `Ask.svelte 疑似硬编码中文（英文模式会泄漏）:\n${offenders.join('\n')}`,
  );
});

test('Ask 每个 provider 都含 en 字段（英文模式不 fallback 到中文）', () => {
  const providers = Object.entries(MODEL_PROVIDER_DISPLAY_NAMES);
  assert.ok(providers.length > 0, '应提供模型 provider 展示名称');

  for (const [providerId, labels] of providers) {
    assert.equal(typeof labels.en, 'string', `${providerId} 缺少 en 字段`);
    assert.ok(labels.en.trim().length > 0, `${providerId} 的 en 字段不能为空`);
  }
});

test('Ask 四语言均提供非空的工具执行失败文案', () => {
  const locales = { zhCN, zhTW, en, ar };

  for (const [locale, messages] of Object.entries(locales)) {
    assert.equal(
      typeof messages.ask.stepFailed,
      'string',
      `${locale} 缺少 ask.stepFailed`,
    );
    assert.ok(
      messages.ask.stepFailed.trim().length > 0,
      `${locale} 的 ask.stepFailed 不能为空`,
    );
  }
});

test('助手欢迎态、参考记录与随机问题池应覆盖四种语言', () => {
  const locales = { zhCN, zhTW, en, ar };
  const requiredKeys = [
    'welcomeTitle',
    'welcomeBrief',
    'recordContext',
    'contextScope',
    'contextSources',
    'referenceTrail',
  ];

  for (const [locale, messages] of Object.entries(locales)) {
    for (const key of requiredKeys) {
      assert.equal(typeof messages.ask[key], 'string', `${locale} 缺少 ask.${key}`);
      assert.ok(messages.ask[key].trim().length > 0, `${locale} 的 ask.${key} 不能为空`);
    }
    assert.match(messages.ask.referenceTrail, /\{count\}/);
    assert.ok(Array.isArray(messages.ask.starterPrompts), `${locale} 的问题池必须是数组`);
    assert.ok(messages.ask.starterPrompts.length >= 16, `${locale} 至少提供 16 条随机问题`);
    assert.equal(
      new Set(messages.ask.starterPrompts.map((item) => item.trim())).size,
      messages.ask.starterPrompts.length,
      `${locale} 的随机问题不能重复`,
    );
  }
});

test('中文欢迎文案与参考记录说明应简短明确', () => {
  assert.equal(zhCN.ask.welcomeTitle, '问问你的工作记录');
  assert.equal(zhCN.ask.welcomeBrief, '提炼重点，发现线索。');
  assert.equal(zhCN.ask.recordContext, '参考记录');
  assert.equal(zhCN.ask.contextScope, '根据问题自动选择时间范围');
  assert.ok(zhCN.ask.welcomeTitle.length <= 10);
  assert.ok(zhCN.ask.welcomeBrief.length <= 10);
});

test('中文随机问题池应包含分析、复盘和行动类问题', () => {
  const joined = zhCN.ask.starterPrompts.join('\n');
  assert.match(joined, /工作重心/);
  assert.match(joined, /自动化/);
  assert.match(joined, /专注/);
  assert.match(joined, /没有真正收口/);
});
