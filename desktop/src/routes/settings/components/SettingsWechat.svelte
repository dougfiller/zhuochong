<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '$lib/i18n/index.js';
  import { showToast } from '$lib/stores/toast.js';

  export let config;
  const dispatch = createEventDispatcher();
  let status = null;
  let loading = true;
  let deletingContent = false;

  async function refresh() {
    loading = true;
    try { status = await invoke('get_wechat_settings_status'); } catch (_) { status = null; }
    finally { loading = false; }
  }
  function changed() { dispatch('change', config); }
  function trustedModelProfiles() {
    return (config.text_model_profiles || []).filter((profile) =>
      profile.test_status === 'success' && profile.model_config?.model?.trim(),
    );
  }
  async function deleteRetainedContent() {
    if (deletingContent || !confirm(t('settingsWechat.deleteConfirm'))) {
      return;
    }
    deletingContent = true;
    try {
      const result = await invoke('delete_wechat_reply_content');
      showToast(t('settingsWechat.deleteSuccess', {
        deleted: result?.deletedRequestDirectories || 0,
        failed: result?.failedEntries || 0,
      }), 'success');
    } catch (_) {
      showToast(t('settingsWechat.deleteFailed'), 'error');
    } finally {
      deletingContent = false;
    }
  }
  onMount(refresh);
</script>

<div class="settings-card mb-5">
  <h3 class="settings-card-title">{t('settingsWechat.title')}</h3>
  <p class="settings-card-desc">{t('settingsWechat.notReady')}</p>
  {#if loading}
    <p class="text-sm text-gray-500">{t('settingsWechat.loading')}</p>
  {:else}
    <div class="space-y-4">
      <label class="block text-sm font-medium">
        {t('settingsWechat.profile')}
        <select class="settings-input mt-1 w-full" bind:value={config.wechat.compatibilityProfileId} on:change={changed}>
          <option value={null}>{t('settingsWechat.profileUnset')}</option>
          {#each status?.catalogOptions || [] as profile}
            <option value={profile.id}>{profile.label} · v{profile.version}</option>
          {/each}
        </select>
      </label>
      <label class="block text-sm font-medium">
        {t('settingsWechat.model')}
        <select class="settings-input mt-1 w-full" bind:value={config.wechat.textModelProfileId} on:change={changed}>
          <option value={null}>{t('settingsWechat.modelUnset')}</option>
          {#each trustedModelProfiles() as profile}
            <option value={profile.id}>{profile.name}</option>
          {/each}
        </select>
      </label>
      <p class="text-sm text-amber-700 dark:text-amber-200" role="status">
        {status?.selectedProfileValid && status?.selectedModelValid ? t('settingsWechat.ready') : t('settingsWechat.needsSetup')}
      </p>
      <p class="text-xs text-gray-500" role="status">{t(`settingsWechat.phase.${status?.requestPhase || 'idle'}`)}</p>
      <p class="text-xs text-gray-500">{t('settingsWechat.autoTriggerOff')}</p>
      <label class="flex items-center gap-2 text-sm">
        <input type="checkbox" bind:checked={config.wechat.contentRetentionEnabled} on:change={changed} />
        {t('settingsWechat.retention')}
      </label>
      {#if config.wechat.contentRetentionEnabled}
        <label class="block text-sm font-medium">
          {t('settingsWechat.retentionDays')}
          <input class="settings-input mt-1 w-full" type="number" min="1" max="30" bind:value={config.wechat.contentRetentionDays} on:change={changed} />
        </label>
      {/if}
      <p class="text-xs text-gray-500">{t('settingsWechat.noAutomation')}</p>
      <button type="button" class="settings-button-secondary" disabled={deletingContent} on:click={deleteRetainedContent}>
        {deletingContent ? t('common.processing') : t('settingsWechat.deleteContent')}
      </button>
    </div>
  {/if}
</div>
