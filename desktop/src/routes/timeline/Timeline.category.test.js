import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

test('时间线详情应支持修改应用默认分类并二次确认后回填历史', async () => {
  const source = await readFile(new URL('./Timeline.svelte', import.meta.url), 'utf8');

  assert.match(source, /invoke\('set_app_category_rule'/);
  assert.match(source, /timeline\.changeCategoryMessage/);
  assert.match(source, /timeline\.detail\.appCategoryHelp/);
  assert.match(source, /pendingChangeCategory/);
  assert.match(source, /doChangeAppCategory/);
});

test('时间线详情分类选择器应按当前语言翻译内置分类', async () => {
  const source = await readFile(new URL('./Timeline.svelte', import.meta.url), 'utf8');

  assert.match(source, /translatedCategoryName = translateCategoryLabel\(cat\.key\)/);
  assert.match(source, /isKnownSystemCategory = cat\.is_system \|\| translatedCategoryName !== cat\.key/);
  assert.match(
    source,
    /return isKnownSystemCategory \? translatedCategoryName : \(cat\.name \|\| translatedCategoryName\)/
  );
  assert.doesNotMatch(
    source,
    /function getCategoryDisplayName\(cat\) \{[\s\S]*return cat\.name \|\| translateCategoryLabel\(cat\.key\);[\s\S]*\}/,
    '分类选择器不能直接优先显示 get_categories 返回的中文内置分类名'
  );
});

test('时间线详情分类修改应使用紧凑 Popover，并用色点、名称和勾选表达当前分类', async () => {
  const source = await readFile(new URL('./Timeline.svelte', import.meta.url), 'utf8');

  assert.match(source, /let showCategoryPopover = false/);
  assert.match(source, /timeline-category-trigger/);
  assert.match(source, /timeline-category-popover/);
  assert.match(source, /aria-haspopup="dialog"/);
  assert.match(source, /role="dialog"/);
  assert.match(source, /aria-pressed=\{/);
  assert.doesNotMatch(source, /role="listbox"/);
  assert.match(source, /timeline-category-dot/);
  assert.match(source, /timeline-category-check/);
  assert.match(source, /showCategoryPopover = false/);
  assert.match(source, /bind:this=\{categoryTrigger\}/);
  assert.match(source, /let categoryPopover/);
  assert.match(source, /bind:this=\{categoryPopover\}/);
  assert.match(source, /async function toggleCategoryPopover\(\)/);
  assert.match(source, /showCategoryPopover = true;[\s\S]*await tick\(\);[\s\S]*categoryPopover\?\.focus\(\)/);
  assert.match(source, /on:click=\{toggleCategoryPopover\}/);
  assert.match(source, /function handleDetailDismiss\(\)/);
  assert.match(source, /if \(showCategoryPopover\) \{[\s\S]*closeCategoryPopover\(\)/);
  assert.match(source, /on:click\|self=\{handleDetailDismiss\}/);
  assert.match(source, /handleDetailOverlayKeydown/);
  assert.match(source, /getViewportPopoverPlacement/);
  assert.match(source, /style=\{categoryPopoverStyle\}/);
  assert.match(source, /\.timeline-category-popover\s*\{[\s\S]*position:\s*fixed/);
  assert.match(source, /tabindex="-1"/);
  assert.match(source, /function handleCategoryPopoverKeydown\(event\)/);
  assert.match(source, /event\.stopPropagation\(\)/);
  assert.match(source, /categoryTrigger\?\.focus\(\)/);
});

test('分类确认层应接管键盘焦点，并在分类入口触发时恢复到分类按钮', async () => {
  const source = await readFile(new URL('./Timeline.svelte', import.meta.url), 'utf8');

  assert.match(source, /function prepareCategoryConfirmation\(\)/);
  assert.match(source, /showCategoryPopover = false;[\s\S]*categoryPopoverStyle = '';[\s\S]*categoryTrigger\?\.focus\(\)/);
  assert.ok(
    (source.match(/prepareCategoryConfirmation\(\)/g) || []).length >= 4,
    '分类选择、创建后应用、删除入口都应先把焦点交还分类触发按钮'
  );
  assert.match(
    source,
    /class="timeline-action-confirm-dialog"\s+use:trapFocus\s+role="dialog"\s+aria-modal="true"\s+aria-labelledby="timeline-action-confirm-title"\s+tabindex="-1"/
  );
  assert.equal(
    (source.match(/id="timeline-action-confirm-title"/g) || []).length,
    3,
    '删除、隐私和分类变更三个标题分支都应为确认对话框提供可访问名称'
  );
  assert.match(source, /function handleTimelineWindowKeydown\(event\)[\s\S]*cancelPendingAction\(\)/);
  assert.match(source, /<svelte:window[\s\S]*on:keydown=\{handleTimelineWindowKeydown\}/);
  const confirmDialogStart = source.indexOf('class="timeline-action-confirm-dialog"');
  const confirmDialogEnd = source.indexOf('>', confirmDialogStart);
  assert.ok(confirmDialogStart >= 0 && confirmDialogEnd > confirmDialogStart);
  assert.doesNotMatch(
    source.slice(confirmDialogStart, confirmDialogEnd),
    /on:keydown=/,
    '确认对话框的 Escape 应由 window 层处理，避免在非交互 div 上绑定键盘监听器'
  );
});

test('分类确认保存结束后应在按钮重新可用时恢复焦点，且不抢占用户主动移动的焦点', async () => {
  const source = await readFile(new URL('./Timeline.svelte', import.meta.url), 'utf8');

  assert.match(
    source,
    /async function restoreCategoryTriggerAfterSaving\(\)[\s\S]*await tick\(\)[\s\S]*document\.activeElement[\s\S]*categoryTrigger\?\.focus\(\)/
  );
  assert.ok(
    (source.match(/categorySaving = false;\s*await restoreCategoryTriggerAfterSaving\(\)/g) || []).length >= 2,
    '分类修改和分类删除保存完成后都应在触发按钮重新可用时恢复焦点'
  );
});

test('自定义分类的重命名与删除符号按钮应提供明确名称并隐藏装饰符号', async () => {
  const source = await readFile(new URL('./Timeline.svelte', import.meta.url), 'utf8');
  const actionsStart = source.indexOf('class="timeline-category-option-actions"');
  const actionsEnd = source.indexOf('</div>', actionsStart);
  const actionsSource = source.slice(actionsStart, actionsEnd);

  assert.ok(actionsStart >= 0 && actionsEnd > actionsStart, '应能定位自定义分类操作区');
  assert.match(
    actionsSource,
    /aria-label=\{t\('timeline\.renameCategory'\)\}[\s\S]*?<span aria-hidden="true">✎<\/span>/
  );
  assert.match(
    actionsSource,
    /aria-label=\{t\('timeline\.deleteCategory'\)\}[\s\S]*?<span aria-hidden="true">×<\/span>/
  );
});
