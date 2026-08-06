<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '$lib/i18n/index.js';

  export let config;
  const dispatch = createEventDispatcher();
  let status = null;
  let loading = true;
  async function refresh() {
    loading = true;
    try { status = await invoke('get_knowledge_settings_status'); } catch (_) { status = null; }
    finally { loading = false; }
  }
  function changed() { dispatch('change', config); refresh(); }
  onMount(refresh);
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
    <p class="text-xs text-gray-500">{t('settingsKnowledge.sourceSummary', { count: status?.sourceCount || 0 })}</p>
    {#if config.knowledge.knowledgeSources.length}
      <ul class="text-xs text-gray-500 space-y-1">
        {#each config.knowledge.knowledgeSources as source}
          <li>{source.sourceId} · {source.sourceState} · {source.priority} · {source.lineage?.length || 0}</li>
        {/each}
      </ul>
    {/if}
  </div>
</div>
