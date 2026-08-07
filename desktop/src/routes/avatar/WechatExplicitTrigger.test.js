import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('微信生成入口只在三秒可取消倒计时结束后调用无输入 command', async () => {
  const [source, popover, scopePicker] = await Promise.all([
    readFile(new URL('./AvatarWindow.svelte', import.meta.url), 'utf8'),
    readFile(new URL('../../lib/components/Avatar/AvatarPopover.svelte', import.meta.url), 'utf8'),
    readFile(new URL('../../lib/components/KnowledgeScopePicker.svelte', import.meta.url), 'utf8'),
  ]);
  assert.match(source, /const WECHAT_PREPARE_SECONDS = 3/);
  assert.match(source, /clearWechatPrepareTimer\(\)/);
  assert.match(source, /await invoke\('generate_wechat_reply'\)/);
  assert.match(source, /if \(wechatPrepareSeconds \|\| wechatGeneratePending \|\| wechatSuggestionBubble\)/);
  assert.doesNotMatch(source, /invoke\('generate_wechat_reply',\s*\{/);
  assert.match(popover, /KnowledgeScopePicker compact/);
  assert.match(scopePicker, /begin_knowledge_scope_observation/);
  assert.match(scopePicker, /confirm_knowledge_scope_binding/);
  assert.match(scopePicker, /globalConfirmed: mode === 'global_user_selected'/);
});
