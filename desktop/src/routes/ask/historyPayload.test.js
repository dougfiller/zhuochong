import test from 'node:test';
import assert from 'node:assert/strict';
import { summarizeStepsForHistory, buildHistoryPayload } from './historyPayload.js';

function deepFreeze(value) {
  if (!value || typeof value !== 'object' || Object.isFrozen(value)) return value;
  Object.freeze(value);
  for (const child of Object.values(value)) {
    deepFreeze(child);
  }
  return value;
}

// ---------- summarizeStepsForHistory：工具结果三态 ----------

test('summarizeStepsForHistory: 空步骤或仅 running 步骤返回 null', () => {
  assert.equal(summarizeStepsForHistory([]), null);
  assert.equal(summarizeStepsForHistory(null), null);
  assert.equal(summarizeStepsForHistory(undefined), null);
  assert.equal(
    summarizeStepsForHistory([
      { tool: 'search_memory', status: 'running', ok: true, hits: 3 },
    ]),
    null,
  );
});

test('summarizeStepsForHistory: ok=true 标记成功，ok=false 标记失败', () => {
  const steps = [
    { tool: 'query_activities', status: 'done', ok: true },
    { tool: 'web_search', status: 'done', ok: false },
  ];
  assert.equal(
    summarizeStepsForHistory(steps),
    '[工具：query_activities✓ | web_search↯]',
  );
});

test('summarizeStepsForHistory: ok 缺失或非法时标记未知', () => {
  const unknownOkValues = [undefined, null, 'true', 1];
  const steps = unknownOkValues.map((ok, index) => ({
    tool: `tool_${index}`,
    status: 'done',
    ...(ok === undefined ? {} : { ok }),
  }));
  assert.equal(
    summarizeStepsForHistory(steps),
    '[工具：tool_0? | tool_1? | tool_2? | tool_3?]',
  );
});

test('summarizeStepsForHistory: search_memory 成功且 hits 为有限非负数时写出实际条数', () => {
  const steps = [
    { tool: 'search_memory', status: 'done', ok: true, hits: 0 },
    { tool: 'search_memory', status: 'done', ok: true, hits: 5 },
  ];
  assert.equal(
    summarizeStepsForHistory(steps),
    '[工具：search_memory→0条 | search_memory→5条]',
  );
});

test('summarizeStepsForHistory: search_memory 成功但 hits 缺失或非法时只标成功，不伪造 0 条', () => {
  const steps = [
    { tool: 'search_memory', status: 'done', ok: true },
    { tool: 'search_memory', status: 'done', ok: true, hits: -1 },
    { tool: 'search_memory', status: 'done', ok: true, hits: Number.NaN },
    { tool: 'search_memory', status: 'done', ok: true, hits: Number.POSITIVE_INFINITY },
  ];
  assert.equal(
    summarizeStepsForHistory(steps),
    '[工具：search_memory✓ | search_memory✓ | search_memory✓ | search_memory✓]',
  );
});

test('summarizeStepsForHistory: search_memory 的失败和未知状态优先于 hits', () => {
  const steps = [
    { tool: 'search_memory', status: 'done', ok: false, hits: 7 },
    { tool: 'search_memory', status: 'done', hits: 7 },
  ];
  assert.equal(
    summarizeStepsForHistory(steps),
    '[工具：search_memory↯ | search_memory?]',
  );
});

test('summarizeStepsForHistory: 跳过工具名缺失和未完成步骤，保留调用顺序', () => {
  const steps = [
    { tool: '', status: 'done', ok: true },
    { tool: 'search_memory', status: 'done', ok: true, hits: 0 },
    { tool: 'query_activities', status: 'running', ok: true },
    { tool: 'query_activities', status: 'done', ok: true },
    { tool: 'search_memory', status: 'done', ok: true, hits: 2 },
  ];
  assert.equal(
    summarizeStepsForHistory(steps),
    '[工具：search_memory→0条 | query_activities✓ | search_memory→2条]',
  );
});

// ---------- buildHistoryPayload：完整轮次 ----------

test('buildHistoryPayload: 只输出完整 user→非 streaming assistant 轮次及 role/content', () => {
  const messages = [
    { role: 'user', content: '你好', attachments: [{ name: 'a.txt' }] },
    {
      role: 'assistant',
      content: '你好，有什么可以帮你？',
      streaming: false,
      modelName: 'test-model',
      references: [{ id: 1 }],
      cards: [{ id: 2 }],
    },
  ];

  assert.deepEqual(buildHistoryPayload(messages), [
    { role: 'user', content: '你好' },
    { role: 'assistant', content: '你好，有什么可以帮你？' },
  ]);
});

test('buildHistoryPayload: 请求失败的 assistant 对应 user 一并丢弃', () => {
  const messages = [
    { role: 'user', content: '问题1' },
    { role: 'assistant', content: '有效回答', streaming: false },
    { role: 'user', content: '问题2' },
    { role: 'assistant', content: '请求失败：网络错误', streaming: false, failed: true },
  ];

  assert.deepEqual(buildHistoryPayload(messages), [
    { role: 'user', content: '问题1' },
    { role: 'assistant', content: '有效回答' },
  ]);
});

test('buildHistoryPayload: streaming assistant 对应的 user 一并丢弃', () => {
  const messages = [
    { role: 'user', content: '问题1' },
    { role: 'assistant', content: '完成的回答', streaming: false },
    { role: 'user', content: '问题2' },
    { role: 'assistant', content: '半截', streaming: true },
  ];

  assert.deepEqual(buildHistoryPayload(messages), [
    { role: 'user', content: '问题1' },
    { role: 'assistant', content: '完成的回答' },
  ]);
});

test('buildHistoryPayload: 丢弃孤立 user、孤立 assistant 和被后续 user 中断的轮次', () => {
  const messages = [
    { role: 'assistant', content: '孤立回答', streaming: false },
    { role: 'user', content: '孤立问题' },
    { role: 'user', content: '有效问题' },
    { role: 'assistant', content: '有效回答', streaming: false },
    { role: 'user', content: '末尾未回答问题' },
  ];

  assert.deepEqual(buildHistoryPayload(messages), [
    { role: 'user', content: '有效问题' },
    { role: 'assistant', content: '有效回答' },
  ]);
});

test('buildHistoryPayload: 最多保留最近 4 个完整轮次', () => {
  const messages = [];
  for (let i = 1; i <= 6; i += 1) {
    messages.push({ role: 'user', content: `问题${i}` });
    messages.push({ role: 'assistant', content: `回答${i}`, streaming: false });
  }

  assert.deepEqual(
    buildHistoryPayload(messages),
    [3, 4, 5, 6].flatMap((i) => [
      { role: 'user', content: `问题${i}` },
      { role: 'assistant', content: `回答${i}` },
    ]),
  );
});

test('buildHistoryPayload: 忽略其他角色，但不破坏完整 user/assistant 轮次', () => {
  const messages = [
    { role: 'system', content: '系统消息' },
    { role: 'user', content: '有效问题' },
    { role: 'tool', content: '工具中间消息' },
    { role: 'assistant', content: '有效回答', streaming: false },
    { role: 'tool', content: '尾部工具消息' },
  ];

  assert.deepEqual(buildHistoryPayload(messages), [
    { role: 'user', content: '有效问题' },
    { role: 'assistant', content: '有效回答' },
  ]);
});

test('buildHistoryPayload: assistant 已完成步骤追加到回答内容尾部', () => {
  const messages = [
    { role: 'user', content: '今天做了什么' },
    {
      role: 'assistant',
      content: '今天你主要在写代码。',
      streaming: false,
      steps: [
        { tool: 'search_memory', status: 'done', ok: true, hits: 0 },
        { tool: 'query_activities', status: 'done', ok: true },
        { tool: 'web_search', status: 'done' },
      ],
    },
  ];

  assert.deepEqual(buildHistoryPayload(messages), [
    { role: 'user', content: '今天做了什么' },
    {
      role: 'assistant',
      content:
        '今天你主要在写代码。\n\n[工具：search_memory→0条 | query_activities✓ | web_search?]',
    },
  ]);
});

test('buildHistoryPayload: content 缺失时按空字符串处理', () => {
  const messages = [
    { role: 'user' },
    { role: 'assistant', streaming: false, steps: [] },
  ];

  assert.deepEqual(buildHistoryPayload(messages), [
    { role: 'user', content: '' },
    { role: 'assistant', content: '' },
  ]);
});

test('buildHistoryPayload: 不修改输入消息、步骤或嵌套字段', () => {
  const messages = [
    { role: 'user', content: '问题', attachments: [{ name: '只读附件' }] },
    {
      role: 'assistant',
      content: '回答',
      streaming: false,
      steps: [
        {
          tool: 'search_memory',
          status: 'done',
          ok: true,
          hits: 1,
          references: [{ id: 'ref-1' }],
        },
      ],
    },
  ];
  const snapshot = structuredClone(messages);
  deepFreeze(messages);

  const result = buildHistoryPayload(messages);

  assert.deepEqual(messages, snapshot);
  assert.deepEqual(result, [
    { role: 'user', content: '问题' },
    { role: 'assistant', content: '回答\n\n[工具：search_memory→1条]' },
  ]);
});

test('buildHistoryPayload: 输入非数组时返回空数组', () => {
  assert.deepEqual(buildHistoryPayload(null), []);
  assert.deepEqual(buildHistoryPayload(undefined), []);
  assert.deepEqual(buildHistoryPayload('not array'), []);
});

test('端到端：只回传已完成上一轮，并携带可靠的工具摘要', () => {
  const messages = [
    { role: 'user', content: '今天做了什么' },
    {
      role: 'assistant',
      content: '今天你主要在写代码，共 4 小时。',
      streaming: false,
      steps: [
        { tool: 'search_memory', status: 'done', ok: true, hits: 0 },
        { tool: 'query_activities', status: 'done', ok: true },
      ],
    },
    { role: 'user', content: '那详细说说写代码的部分' },
  ];

  assert.deepEqual(buildHistoryPayload(messages), [
    { role: 'user', content: '今天做了什么' },
    {
      role: 'assistant',
      content:
        '今天你主要在写代码，共 4 小时。\n\n[工具：search_memory→0条 | query_activities✓]',
    },
  ]);
});

test('成功步骤的 digest 应以受预算限制的附注进入历史', () => {
  const messages = [
    { role: 'user', content: '今天做了什么' },
    {
      role: 'assistant',
      content: '主要在写代码。',
      streaming: false,
      steps: [
        { tool: 'query_activities', status: 'done', ok: true, digest: 'Top应用: Xcode(120分)、Chrome(40分)' },
        { tool: 'web_search', status: 'done', ok: false, digest: '不应出现：失败步骤不带数据' },
      ],
    },
    { role: 'user', content: '那 Chrome 主要在看什么' },
  ];

  const payload = buildHistoryPayload(messages);
  const assistantEntry = payload.find((m) => m.role === 'assistant');
  assert.ok(assistantEntry.content.includes('[上轮工具数据摘要]'));
  assert.ok(assistantEntry.content.includes('query_activities: Top应用: Xcode(120分)'));
  assert.ok(!assistantEntry.content.includes('不应出现'));
  // 工具符号摘要仍在
  assert.ok(assistantEntry.content.includes('[工具：'));
});
