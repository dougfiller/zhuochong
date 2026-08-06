import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('微信和知识库设置使用与save_config相同的camelCase payload', async () => {
  const [settings, wechat, knowledge] = await Promise.all([
    readFile(new URL('./Settings.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./components/SettingsWechat.svelte', import.meta.url), 'utf8'),
    readFile(new URL('./components/SettingsKnowledge.svelte', import.meta.url), 'utf8'),
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
    'scopeMode',
    'topK',
    'tokenBudget',
    'sameConversationBoost',
    'localEmbedding',
    'knowledgeSources',
  ]) {
    assert.match(knowledge, new RegExp(`config\\.knowledge\\.${field}`));
  }
  assert.doesNotMatch(wechat, /config\.wechat\.[a-z]+_[a-z_]+/);
  assert.doesNotMatch(knowledge, /config\.knowledge\.[a-z]+_[a-z_]+/);
});
