<script>
  import { t } from '$lib/i18n/index.js';
  import { invoke } from '@tauri-apps/api/core';
  import { createEventDispatcher } from 'svelte';

  export let config;
  export let localStatus = { enabled: false, baseUrl: '', tokenPreview: '', lastError: null };
  export let saving = false;

  const dispatch = createEventDispatcher();
  let tokenVisible = false;
  let tokenValue = '';

  function toggle() {
    config.localhost_api_enabled = !config.localhost_api_enabled;
    dispatch('save');
  }

  async function revealToken() {
    try {
      tokenValue = await invoke('reveal_localhost_api_token');
      tokenVisible = true;
    } catch (e) {
      dispatch('toast', { message: t('nodeGatewayPage.tokenRevealFailed', { error: e }), type: 'error' });
    }
  }

  async function rotateToken() {
    try {
      await invoke('rotate_localhost_api_token');
      dispatch('reloadStatus');
      dispatch('toast', { message: t('nodeGatewayPage.tokenRotated'), type: 'success' });
    } catch (e) {
      dispatch('toast', { message: t('nodeGatewayPage.tokenRotateFailed', { error: e }), type: 'error' });
    }
  }

  async function copyToken() {
    try {
      if (!tokenValue) {
        tokenValue = await invoke('reveal_localhost_api_token');
        tokenVisible = true;
      }
      await navigator.clipboard.writeText(tokenValue);
      dispatch('toast', { message: t('nodeGatewayPage.tokenCopied'), type: 'success' });
    } catch (e) {
      dispatch('toast', { message: t('nodeGatewayPage.tokenCopyFailed'), type: 'error' });
    }
  }
</script>

<div class="rounded-xl bg-white/70 px-3.5 py-3 ring-1 ring-slate-200/70 dark:bg-[#161b22]/20 dark:ring-[#30363d]/70">
  <div class="flex items-center justify-between gap-3 mb-2">
    <div class="flex items-center gap-2">
      <div class="flex h-5 w-5 items-center justify-center rounded-md bg-primary-100 dark:bg-primary-900/30">
        <svg class="w-3 h-3 text-primary-600 dark:text-primary-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2" />
        </svg>
      </div>
      <span class="text-sm font-medium text-slate-700 dark:text-[#c9d1d9]">{t('nodeGatewayPage.localApi')}</span>
      {#if localStatus.enabled}
        <span class="settings-chip-success">{localStatus.baseUrl || t('nodeGatewayPage.localhostEnabled')}</span>
      {:else}
        <span class="settings-chip-neutral">{t('nodeGatewayPage.localhostDisabled')}</span>
      {/if}
    </div>
    <button
      type="button"
      on:click={toggle}
      disabled={saving}
      class="switch-track {config.localhost_api_enabled ? 'bg-primary-500' : 'bg-slate-300 dark:bg-[#484f58]'} {saving ? 'opacity-60 cursor-not-allowed' : ''}"
      role="switch"
      aria-label={t('nodeGatewayPage.localApi')}
      aria-checked={config.localhost_api_enabled}
    >
      <span class="switch-thumb {config.localhost_api_enabled ? 'translate-x-5' : 'translate-x-0'}"></span>
    </button>
  </div>

  {#if config.localhost_api_enabled}
    <div class="space-y-2">
      <div class="grid grid-cols-1 gap-2 md:grid-cols-2">
        <div class="rounded-lg bg-white/70 px-3 py-1.5 ring-1 ring-slate-200/70 dark:bg-[#161b22]/20 dark:ring-[#30363d]/70">
          <div class="text-[11px] text-slate-400 dark:text-[#636c76]">{t('nodeGatewayPage.apiHostLabel')}</div>
          <input
            type="text"
            aria-label={t('nodeGatewayPage.apiHostLabel')}
            bind:value={config.localhost_api_host}
            class="w-full bg-transparent text-sm font-mono text-slate-900 dark:text-[#e6edf3] focus:outline-none"
            placeholder="127.0.0.1"
          />
        </div>
        <div class="rounded-lg bg-white/70 px-3 py-1.5 ring-1 ring-slate-200/70 dark:bg-[#161b22]/20 dark:ring-[#30363d]/70">
          <div class="text-[11px] text-slate-400 dark:text-[#636c76]">{t('nodeGatewayPage.apiPortLabel')}</div>
          <input
            type="number"
            aria-label={t('nodeGatewayPage.apiPortLabel')}
            bind:value={config.localhost_api_port}
            on:blur={() => {
              if (!Number.isInteger(config.localhost_api_port) || config.localhost_api_port <= 0 || config.localhost_api_port > 65535) {
                config.localhost_api_port = 47831;
              }
            }}
            class="w-full bg-transparent text-sm font-mono text-slate-900 dark:text-[#e6edf3] focus:outline-none"
            min="1"
            max="65535"
            placeholder="47831"
          />
        </div>
      </div>
      <div class="flex items-center justify-between rounded-lg bg-white/70 px-3 py-1.5 ring-1 ring-slate-200/70 dark:bg-[#161b22]/20 dark:ring-[#30363d]/70">
        <span class="text-[11px] text-slate-400 dark:text-[#636c76]">{t('nodeGatewayPage.localhostAddress')}</span>
        <span class="text-sm font-mono text-slate-700 dark:text-[#adbac7]">{localStatus.baseUrl}</span>
      </div>
      <div class="rounded-lg bg-white/70 px-3 py-2 ring-1 ring-slate-200/70 dark:bg-[#161b22]/20 dark:ring-[#30363d]/70">
        <div class="flex items-center justify-between gap-2 mb-1">
          <span class="text-[11px] text-slate-400 dark:text-[#636c76]">{t('nodeGatewayPage.localhostToken')}</span>
          <div class="flex gap-1">
            <button type="button" class="settings-chip-button" on:click={revealToken}>{t('nodeGatewayPage.revealToken')}</button>
            <button type="button" class="settings-chip-button" on:click={copyToken}>{t('nodeGatewayPage.copyToken')}</button>
            <button type="button" class="settings-chip-button settings-chip-button-active" on:click={rotateToken}>{t('nodeGatewayPage.rotateToken')}</button>
          </div>
        </div>
        <div class="font-mono text-[11px] text-slate-500 dark:text-[#7d8590] break-all">
          {tokenVisible ? tokenValue || t('nodeGatewayPage.empty') : localStatus.tokenPreview || t('nodeGatewayPage.empty')}
        </div>
      </div>
      <p class="text-[11px] text-slate-400 dark:text-[#636c76]">{t('nodeGatewayPage.apiHostHint')}</p>
    </div>
  {/if}
</div>
