import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('微信和知识库设置使用与save_config相同的camelCase payload', async () => {
  const [settings, wechat, knowledge, scopePicker] = await Promise.all([
    readFile(new URL('./Settings.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./components/SettingsWechat.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./components/SettingsKnowledge.svelte', import.meta.url), 'utf8'),
    readFile(new URL('../../lib/components/KnowledgeScopePicker.svelte', import.meta.url), 'utf8'),
  ]);

  assert.match(settings, /invoke\('save_config', \{ config \}\)/);
  assert.match(settings, /compatibilityProfileId: null/);
  assert.match(settings, /scopeMode: null/);
  assert.match(settings, /localEmbedding: \{ provider:/);
  assert.doesNotMatch(settings, /compatibility_profile_id|scope_mode|local_embedding/);
  for (const field of [
    'compatibilityProfileId',
    'textModelProfileId',
    'contentRetentionEnabled',
    'contentRetentionDays',
  ]) {
    assert.match(wechat, new RegExp(`config\\.wechat\\.${field}`));
  }
  for (const field of [
    'lastScopeHintKeys',
    'topK',
    'tokenBudget',
    'sameConversationBoost',
    'localEmbedding',
  ]) {
    assert.match(knowledge, new RegExp(`config\\.knowledge\\.${field}`));
  }
  assert.doesNotMatch(wechat, /config\.wechat\.[a-z]+_[a-z_]+/);
  assert.doesNotMatch(knowledge, /config\.knowledge\.[a-z]+_[a-z_]+/);
  assert.doesNotMatch(knowledge, /config\.knowledge\.knowledgeSources/);
  assert.doesNotMatch(knowledge, /bind:value=\{config\.knowledge\.scopeMode\}/);
  assert.match(scopePicker, /bindOne/);
  assert.match(scopePicker, /selectMany/);
  assert.match(scopePicker, /confirmGlobal/);
  assert.match(scopePicker, /let selectedKeys = \[\]/);
  assert.match(scopePicker, /historyHint/);
  assert.doesNotMatch(scopePicker, /selectedKeys\s*=\s*hintKeys/);
  assert.match(knowledge, /openDialog\(\{ directory: true, multiple: false \}\)/);
  assert.match(knowledge, /invoke\('start_knowledge_source_import'/);
  assert.match(knowledge, /invoke\('list_knowledge_sources'\)/);
  assert.match(knowledge, /let operationId = null/);
  assert.match(knowledge, /operationId = receipt\.operationId/);
  assert.match(knowledge, /await waitForOperation\(\)/);
  assert.match(knowledge, /get_knowledge_maintenance_status', \{ input: \{ operationId \} \}/);
  assert.match(knowledge, /if \(!selectedRoot \|\| Array\.isArray\(selectedRoot\)\) return/);
  assert.match(knowledge, /\$: busy = Boolean\(operationId\)/);
  assert.match(knowledge, /selectedRoots/);
  assert.doesNotMatch(knowledge, /showToast\([^\n]*selectedRoot/);
  assert.match(knowledge, /async function loadReplyHistory\(\)/);
  assert.match(knowledge, /async function showReplySources\(requestId\)/);
  assert.match(knowledge, /invoke\('get_wechat_reply_sources', \{ input: \{ requestId \} \}\)/);
  assert.match(knowledge, /entry\.stageName === 'generating' && entry\.m2/);
  const mountBody = knowledge.match(/onMount\(\(\) => \{([^}]*)\}\);/)?.[1] || '';
  assert.doesNotMatch(mountBody, /loadReplyHistory|showReplySources|get_wechat_reply_sources/);
  assert.doesNotMatch(knowledge, /input:\s*\{\s*hitId|sourcePath|messageId|conversationId|hitScore/);
});
