<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { t } from '$lib/i18n/index.js';
  import { confirm } from '$lib/stores/confirm.js';
  import { showToast } from '$lib/stores/toast.js';

  export let config;
  const dispatch = createEventDispatcher();
  let status = null;
  let loading = true;
  let sources = [];
  let maintenance = { operation: 'idle', state: 'open', completed: 0, total: 0 };
  let operationId = null;
  let polling = null;
  $: busy = Boolean(operationId) || maintenance?.maintenance === 'closed';
  $: canRebuild = !busy && sources.some((source) => source.sourceState === 'active');
  async function refresh() {
    loading = true;
    try {
      [status, { sources }, maintenance] = await Promise.all([
        invoke('get_knowledge_settings_status'),
        invoke('list_knowledge_sources'),
        invoke('get_knowledge_maintenance_status', { input: { operationId } }),
      ]);
    } catch (_) { status = null; sources = []; }
    finally { loading = false; }
  }
  function changed() { dispatch('change', config); refresh(); }
  function startPolling() {
    if (!polling) polling = setInterval(refresh, 750);
  }
  function stopPolling() {
    if (polling) { clearInterval(polling); polling = null; }
  }
  async function waitForOperation() {
    while (operationId) {
      await refresh();
      if (maintenance?.state === 'succeeded') return true;
      if (maintenance?.state === 'failed') return false;
      await new Promise((resolve) => setTimeout(resolve, 750));
    }
    return false;
  }
  async function importSource() {
    if (busy) return;
    const selectedRoot = await openDialog({ directory: true, multiple: false });
    if (!selectedRoot || Array.isArray(selectedRoot)) return;
    try {
      const receipt = await invoke('start_knowledge_source_import', { input: { selectedRoot } });
      operationId = receipt.operationId;
      startPolling();
      if (await waitForOperation()) showToast(t('settingsKnowledge.importSuccess'), 'success');
      else showToast(t('settingsKnowledge.operationFailed'), 'error');
    }
    catch (_) { showToast(t('settingsKnowledge.operationFailed'), 'error'); }
    finally { operationId = null; await refresh(); stopPolling(); }
  }
  async function mutate(source, operation) {
    if (busy || !(await confirm({ title: t(`settingsKnowledge.${operation}Title`), message: t(`settingsKnowledge.${operation}Confirm`), tone: operation === 'deny' ? 'error' : 'warning' }))) return;
    try { await invoke(`${operation}_knowledge_source`, { input: { sourceId: source.sourceId } }); showToast(t('settingsKnowledge.operationSuccess'), 'success'); }
    catch (_) { showToast(t('settingsKnowledge.operationFailed'), 'error'); }
    finally { refresh(); }
  }
  async function rebuild() {
    if (!canRebuild || !(await confirm({ title: t('settingsKnowledge.rebuildTitle'), message: t('settingsKnowledge.rebuildConfirm'), tone: 'warning' }))) return;
    const selectedRoots = await openDialog({ directory: true, multiple: true });
    if (!Array.isArray(selectedRoots) || !selectedRoots.length) return;
    try {
      const receipt = await invoke('start_knowledge_rebuild', { input: { selectedRoots } });
      operationId = receipt.operationId;
      startPolling();
      if (await waitForOperation()) showToast(t('settingsKnowledge.operationSuccess'), 'success');
      else showToast(t('settingsKnowledge.operationFailed'), 'error');
    }
    catch (_) { showToast(t('settingsKnowledge.operationFailed'), 'error'); }
    finally { operationId = null; await refresh(); stopPolling(); }
  }
  onMount(() => { refresh(); return stopPolling; });
</script>

<div class="settings-card mb-5">
  <h3 class="settings-card-title">{t('settingsKnowledge.title')}</h3>
  <p class="settings-card-desc">{t('settingsKnowledge.notReady')}</p>
  <p class="mb-4 text-sm text-amber-700 dark:text-amber-200" role="status">{t('settingsKnowledge.m1NoM2')}</p>
  <div class="space-y-4">
    <label class="block text-sm font-medium">
      {t('settingsKnowledge.scope')}
      <select class="settings-input mt-1 w-full" bind:value={config.knowledge.scopeMode} on:change={changed}>
        <option value={null}>{t('settingsKnowledge.scopeUnset')}</option>
        <option value="conversation">{t('settingsKnowledge.scopeConversation')}</option>
        <option value="selected_conversations">{t('settingsKnowledge.scopeSelected')}</option>
        <option value="global_user_selected">{t('settingsKnowledge.scopeGlobal')}</option>
      </select>
    </label>
    <div class="grid grid-cols-2 gap-3">
      <label class="block text-sm font-medium">{t('settingsKnowledge.topK')}<input class="settings-input mt-1 w-full" type="number" min="1" max="12" bind:value={config.knowledge.topK} on:change={changed} /></label>
      <label class="block text-sm font-medium">{t('settingsKnowledge.tokenBudget')}<input class="settings-input mt-1 w-full" type="number" min="256" max="4096" bind:value={config.knowledge.tokenBudget} on:change={changed} /></label>
    </div>
    <label class="flex items-center gap-2 text-sm"><input type="checkbox" bind:checked={config.knowledge.sameConversationBoost} on:change={changed} />{t('settingsKnowledge.sameConversationBoost')}</label>
    <fieldset class="rounded-lg border border-gray-200 p-3 dark:border-gray-700">
      <legend class="px-1 text-sm font-medium">{t('settingsKnowledge.localEmbedding')}</legend>
      <label class="block text-sm">{t('settingsKnowledge.endpoint')}<input class="settings-input mt-1 w-full" bind:value={config.knowledge.localEmbedding.endpoint} on:change={changed} /></label>
      <label class="mt-3 block text-sm">{t('settingsKnowledge.embeddingModel')}<input class="settings-input mt-1 w-full" bind:value={config.knowledge.localEmbedding.model} on:change={changed} /></label>
    </fieldset>
    <p class="text-sm text-amber-700 dark:text-amber-200" role="status">{loading ? t('settingsKnowledge.loading') : (status?.notReadyReason || 'KB_NOT_READY')}</p>
    <section class="rounded-lg border border-gray-200 p-3 dark:border-gray-700">
      <h4 class="text-sm font-medium">{t('settingsKnowledge.sourcesTitle')}</h4>
      <p class="mt-1 text-xs text-gray-500">{t('settingsKnowledge.sourceSummary', { count: sources.length })}</p>
      {#if busy}<p class="mt-2 text-xs text-amber-700" role="status">{t('settingsKnowledge.maintenance', { completed: maintenance.completed, total: maintenance.total })}</p>{/if}
      <div class="mt-3 flex flex-wrap gap-2">
        <button class="settings-button" disabled={busy} on:click={importSource}>{t('settingsKnowledge.importAction')}</button>
        <button class="settings-button" disabled={!canRebuild} on:click={rebuild}>{t('settingsKnowledge.rebuildAction')}</button>
      </div>
      {#if sources.length}
        <ul class="mt-3 space-y-2 text-xs text-gray-500">
        {#each sources as source}
          <li class="rounded border border-gray-100 p-2 dark:border-gray-800">
            <div>{source.sourceId.slice(0, 14)} · {source.coverageKind} · {source.sourceState} · {source.importStatus}</div>
            <div>{t('settingsKnowledge.sourceCounts', { lineage: source.lineageCount, messages: source.messageCount, conversations: source.conversationCount })}</div>
            <div class="mt-1 flex gap-2"><button disabled={busy || source.sourceState !== 'active'} on:click={() => mutate(source, 'retire')}>{t('settingsKnowledge.retireAction')}</button><button disabled={busy || source.sourceState === 'denied'} on:click={() => mutate(source, 'deny')}>{t('settingsKnowledge.denyAction')}</button></div>
          </li>
        {/each}
        </ul>
      {/if}
    </section>
  </div>
</div>
