<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '$lib/i18n/index.js';
  import { confirm } from '$lib/stores/confirm.js';
  import { formatWechatUserError } from '$lib/utils/errorDisplay.js';
  import { confirmFreshScopeForNextRequest } from './knowledgeScopeConfirmation.js';

  export let compact = false;
  export let hintKeys = [];
  export let onHintsChange = () => {};

  let binding = null;
  let catalog = null;
  let mode = null;
  let selectedKeys = [];
  let observation = null;
  let busy = false;
  let error = '';

  $: conversations = catalog?.conversations || [];
  $: selected = new Set(selectedKeys);
  $: canObserve = mode === 'global_user_selected'
    || (mode === 'conversation' && selectedKeys.length === 1 && singleBindable(selectedKeys[0]))
    || (mode === 'selected_conversations' && selectedKeys.length >= 1 && selectedKeys.length <= 32);

  function displayFacts(conversation) {
    return [conversation.displayName, conversation.isGroup, conversation.startedAtMs, conversation.endedAtMs, conversation.messageCount].join('|');
  }

  function singleBindable(scopeKey) {
    const target = conversations.find((conversation) => conversation.scopeKey === scopeKey);
    return Boolean(target) && conversations.filter((candidate) => displayFacts(candidate) === displayFacts(target)).length === 1 && target.displayName !== '未提供名称';
  }

  function timeRange(conversation) {
    if (conversation.startedAtMs == null || conversation.endedAtMs == null) return '—';
    const start = new Date(conversation.startedAtMs).toLocaleDateString();
    const end = new Date(conversation.endedAtMs).toLocaleDateString();
    return start === end ? start : `${start}–${end}`;
  }

  function safeError(value) {
    return formatWechatUserError(value, t('wechat.errors.requestCancelled'), t);
  }

  async function refresh() {
    try {
      [binding, catalog] = await Promise.all([
        invoke('get_knowledge_scope_binding_status'),
        invoke('list_knowledge_conversations'),
      ]);
    } catch (_) {
      catalog = null;
    }
  }

  function chooseMode(nextMode) {
    mode = nextMode;
    selectedKeys = [];
    observation = null;
    error = '';
  }

  function toggle(scopeKey) {
    observation = null;
    if (mode === 'conversation') {
      selectedKeys = [scopeKey];
      return;
    }
    selectedKeys = selected.has(scopeKey)
      ? selectedKeys.filter((key) => key !== scopeKey)
      : [...selectedKeys, scopeKey].slice(0, 32);
  }

  async function observe() {
    if (!canObserve || !binding || busy) return;
    busy = true;
    error = '';
    try {
      observation = await invoke('begin_knowledge_scope_observation', {
        input: {
          sessionNonce: binding.sessionNonce,
          expectedBindingGeneration: binding.bindingGeneration,
        },
      });
      binding = { ...binding, bindingGeneration: observation.bindingGeneration, bindingObservationVersion: observation.bindingObservationVersion };
    } catch (value) {
      error = safeError(value);
    } finally {
      busy = false;
    }
  }

  async function bindScope() {
    if (!observation || !binding || busy) return;
    if (mode === 'global_user_selected' && !(await confirm({
      title: t('knowledgeScope.globalTitle'),
      message: t('knowledgeScope.globalConfirm', { count: conversations.length }),
      tone: 'warning',
    }))) return;
    busy = true;
    error = '';
    try {
      binding = await invoke('confirm_knowledge_scope_binding', {
        input: {
          sessionNonce: binding.sessionNonce,
          expectedBindingGeneration: observation.bindingGeneration,
          expectedObservationVersion: observation.bindingObservationVersion,
          expectedCatalogGeneration: catalog.catalogGeneration,
          scope: { kind: mode, keys: mode === 'global_user_selected' ? [] : selectedKeys },
          headerConfirmed: true,
          globalConfirmed: mode === 'global_user_selected',
        },
      });
      if (mode !== 'global_user_selected') onHintsChange([...selectedKeys].sort());
      mode = null;
      selectedKeys = [];
      observation = null;
    } catch (value) {
      error = safeError(value);
    } finally {
      busy = false;
    }
  }

  async function clearBinding() {
    if (!binding || binding.state !== 'bound' || busy) return;
    busy = true;
    try {
      binding = await invoke('clear_knowledge_scope_binding', {
        input: { sessionNonce: binding.sessionNonce, expectedBindingGeneration: binding.bindingGeneration },
      });
      mode = null;
      selectedKeys = [];
      observation = null;
    } catch (value) {
      error = safeError(value);
    } finally {
      busy = false;
    }
  }

  async function confirmNextRequest() {
    if (!binding?.requiresPerRequestConfirmation || busy) return;
    busy = true;
    error = '';
    try {
      const generation = binding.bindingGeneration;
      const result = await confirmFreshScopeForNextRequest({
        currentGeneration: generation,
        observe: () => invoke('begin_knowledge_scope_observation', {
          input: { sessionNonce: binding.sessionNonce, expectedBindingGeneration: generation },
        }),
        requestConfirmation: (nextObservation) => confirm({
          title: t('knowledgeScope.nextRequestTitle'),
          message: `${t('knowledgeScope.nextRequestConfirm', { count: binding.selectedCount })}\n${nextObservation.headerReliable ? nextObservation.headerClue : t('knowledgeScope.headerUnreliable')}`,
          tone: 'warning',
        }),
        authorize: (nextObservation) => invoke('confirm_knowledge_scope_for_next_request', {
          input: {
            sessionNonce: binding.sessionNonce,
            expectedBindingGeneration: nextObservation.bindingGeneration,
            expectedObservationVersion: nextObservation.bindingObservationVersion,
          },
        }),
      });
      if (result.stale) {
        await refresh();
        return;
      }
      binding = { ...binding, bindingGeneration: result.observation.bindingGeneration, bindingObservationVersion: result.observation.bindingObservationVersion };
    } catch (value) {
      error = safeError(value);
    } finally {
      busy = false;
    }
  }

  onMount(refresh);
</script>

<section class:compact class="knowledge-scope-picker rounded-lg border border-gray-200 p-3 dark:border-gray-700">
  <div class="flex flex-wrap gap-2">
    <button type="button" class="settings-button" class:active={mode === 'conversation'} on:click={() => chooseMode('conversation')}>{t('knowledgeScope.bindOne')}</button>
    <button type="button" class="settings-button" class:active={mode === 'selected_conversations'} on:click={() => chooseMode('selected_conversations')}>{t('knowledgeScope.selectMany')}</button>
    <button type="button" class="settings-button" class:active={mode === 'global_user_selected'} on:click={() => chooseMode('global_user_selected')}>{t('knowledgeScope.confirmGlobal')}</button>
  </div>
  <p class="mt-2 text-xs text-gray-500">{binding?.state === 'bound' ? t('knowledgeScope.bound', { count: binding.selectedCount }) : t('knowledgeScope.unbound')}</p>

  {#if mode && mode !== 'global_user_selected'}
    <ul class="mt-2 max-h-40 space-y-1 overflow-y-auto text-xs">
      {#each conversations as conversation}
        <li>
          <label class="flex items-start gap-2 rounded border border-gray-100 p-2 dark:border-gray-800">
            <input
              type={mode === 'conversation' ? 'radio' : 'checkbox'}
              name="knowledge-scope"
              checked={selected.has(conversation.scopeKey)}
              disabled={mode === 'conversation' && !singleBindable(conversation.scopeKey)}
              on:change={() => toggle(conversation.scopeKey)}
            />
            <span>
              <span class="font-medium">{conversation.displayName}</span>
              <span> · {conversation.isGroup ? t('knowledgeScope.group') : t('knowledgeScope.direct')} · {conversation.messageCount}</span>
              <span class="block text-gray-500">{timeRange(conversation)}</span>
              {#if hintKeys.includes(conversation.scopeKey)}<span class="ml-1 text-amber-700">{t('knowledgeScope.historyHint')}</span>{/if}
              {#if mode === 'conversation' && !singleBindable(conversation.scopeKey)}<span class="block text-rose-700">{t('knowledgeScope.ambiguous')}</span>{/if}
            </span>
          </label>
        </li>
      {/each}
    </ul>
  {/if}

  {#if mode}
    <div class="mt-2 flex flex-wrap items-center gap-2">
      <button type="button" class="settings-button" disabled={!canObserve || busy} on:click={observe}>{t('knowledgeScope.observeHeader')}</button>
      {#if observation}
        <span class="text-xs text-gray-600">{observation.headerReliable ? observation.headerClue : t('knowledgeScope.headerUnreliable')}</span>
        <button type="button" class="settings-button" disabled={busy} on:click={bindScope}>{t('knowledgeScope.confirmBinding')}</button>
      {/if}
    </div>
  {/if}
  {#if binding?.state === 'bound'}
    <div class="mt-2 flex gap-2">
      {#if binding.requiresPerRequestConfirmation}<button type="button" class="text-xs text-amber-700" on:click={confirmNextRequest}>{t('knowledgeScope.confirmNextRequest')}</button>{/if}
      <button type="button" class="text-xs text-rose-700" on:click={clearBinding}>{t('knowledgeScope.clear')}</button>
    </div>
  {/if}
  {#if error}<p class="mt-2 text-xs text-rose-700" role="alert">{error}</p>{/if}
</section>

<style>
  .knowledge-scope-picker.compact { font-size: 11px; max-height: 220px; overflow-y: auto; }
  .active { border-color: rgb(2 132 199); color: rgb(3 105 161); }
</style>
