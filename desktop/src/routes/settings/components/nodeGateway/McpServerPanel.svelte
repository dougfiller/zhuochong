<script>
  import { t } from '$lib/i18n/index.js';
  import { createEventDispatcher } from 'svelte';

  export let config;
  export let saving = false;
  export let mcpDbPath = '';
  export let mcpConfigPath = '';
  export let mcpConfigJson = '';

  const dispatch = createEventDispatcher();

  function toggle() {
    config.mcp_server_enabled = !config.mcp_server_enabled;
    dispatch('save');
  }

  async function copyPath(text, labelKey) {
    try {
      await navigator.clipboard.writeText(text);
      dispatch('toast', { message: t('nodeGatewayPage.mcpServerPathCopied', { label: t(`nodeGatewayPage.${labelKey}`) }), type: 'success' });
    } catch (e) {
      dispatch('toast', { message: t('nodeGatewayPage.tokenCopyFailed'), type: 'error' });
    }
  }

  async function copyMcpConfig() {
    try {
      await navigator.clipboard.writeText(mcpConfigJson);
      dispatch('toast', { message: t('nodeGatewayPage.mcpServerConfigCopied'), type: 'success' });
    } catch (e) {
      dispatch('toast', { message: t('nodeGatewayPage.tokenCopyFailed'), type: 'error' });
    }
  }
</script>

<div class="rounded-2xl border border-slate-200/80 bg-slate-50/90 p-4 space-y-4 dark:border-[#30363d]/80 dark:bg-[#21262d]/40">
  <div class="flex items-center justify-between gap-3">
    <div class="flex items-center gap-2">
      <div class="flex h-7 w-7 items-center justify-center rounded-lg bg-emerald-100 dark:bg-emerald-900/30">
        <svg class="w-4 h-4 text-emerald-600 dark:text-emerald-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
        </svg>
      </div>
      <div>
        <span class="text-sm font-semibold text-slate-700 dark:text-[#c9d1d9]">MCP Server</span>
        {#if config.mcp_server_enabled}
          <span class="settings-chip-success ml-1.5">{t('nodeGatewayPage.mcpServerEnabled')}</span>
        {:else}
          <span class="settings-chip-neutral ml-1.5">{t('nodeGatewayPage.mcpServerDisabled')}</span>
        {/if}
      </div>
    </div>
    <button
      type="button"
      on:click={toggle}
      disabled={saving}
      class="switch-track {config.mcp_server_enabled ? 'bg-primary-500' : 'bg-slate-300 dark:bg-[#484f58]'} {saving ? 'opacity-60 cursor-not-allowed' : ''}"
      role="switch"
      aria-label={t('nodeGatewayPage.mcpServer')}
      aria-checked={config.mcp_server_enabled}
    >
      <span class="switch-thumb {config.mcp_server_enabled ? 'translate-x-5' : 'translate-x-0'}"></span>
    </button>
  </div>

  <p class="text-[11px] text-slate-400 dark:text-[#636c76]">{t('nodeGatewayPage.mcpServerDescription')}</p>

  {#if config.mcp_server_enabled}
    <div class="space-y-3">
      <p class="text-[11px] text-slate-400 dark:text-[#636c76]">{t('nodeGatewayPage.mcpServerBinaryHint')}</p>
      <div class="space-y-1.5">
        {#if mcpDbPath}
          <div class="flex flex-col gap-0.5 rounded-lg bg-white/70 px-3 py-1.5 ring-1 ring-slate-200/70 dark:bg-[#161b22]/20 dark:ring-[#30363d]/70">
            <div class="flex items-center justify-between gap-2">
              <span class="text-[11px] text-slate-500 dark:text-[#7d8590]">{t('nodeGatewayPage.mcpServerDbPath')}</span>
              <button type="button" class="text-[10px] text-primary-600 hover:underline focus:outline-none" on:click={() => copyPath(mcpDbPath, 'mcpServerDbPath')}>{t('nodeGatewayPage.mcpServerCopyPath')}</button>
            </div>
            <span class="font-mono text-[11px] text-slate-700 dark:text-[#adbac7] break-all select-all" title={mcpDbPath}>{mcpDbPath}</span>
          </div>
        {/if}
        {#if mcpConfigPath}
          <div class="flex flex-col gap-0.5 rounded-lg bg-white/70 px-3 py-1.5 ring-1 ring-slate-200/70 dark:bg-[#161b22]/20 dark:ring-[#30363d]/70">
            <div class="flex items-center justify-between gap-2">
              <span class="text-[11px] text-slate-500 dark:text-[#7d8590]">{t('nodeGatewayPage.mcpServerConfigPath')}</span>
              <button type="button" class="text-[10px] text-primary-600 hover:underline focus:outline-none" on:click={() => copyPath(mcpConfigPath, 'mcpServerConfigPath')}>{t('nodeGatewayPage.mcpServerCopyPath')}</button>
            </div>
            <span class="font-mono text-[11px] text-slate-700 dark:text-[#adbac7] break-all select-all" title={mcpConfigPath}>{mcpConfigPath}</span>
          </div>
        {/if}
        <div class="flex flex-col gap-0.5 rounded-lg bg-white/70 px-3 py-1.5 ring-1 ring-slate-200/70 dark:bg-[#161b22]/20 dark:ring-[#30363d]/70">
          <div class="flex items-center justify-between gap-2">
            <span class="text-[11px] text-slate-500 dark:text-[#7d8590]">{t('nodeGatewayPage.mcpServerBinaryPath')}</span>
            <button type="button" class="text-[10px] text-primary-600 hover:underline focus:outline-none" on:click={() => copyPath('work-review-mcp-server', 'mcpServerBinaryPath')}>{t('nodeGatewayPage.mcpServerCopyPath')}</button>
          </div>
          <span class="font-mono text-[11px] text-slate-700 dark:text-[#adbac7] break-all select-all">work-review-mcp-server</span>
        </div>
      </div>
      <div>
        <div class="flex items-center justify-between mb-1">
          <span class="text-xs font-medium text-slate-700 dark:text-[#adbac7]">{t('nodeGatewayPage.mcpServerConfigTitle')}</span>
          <button type="button" class="settings-chip-button settings-chip-button-active text-[11px]" on:click={copyMcpConfig}>{t('nodeGatewayPage.mcpServerCopyConfig')}</button>
        </div>
        <p class="text-[11px] text-slate-400 dark:text-[#636c76] mb-1.5">{t('nodeGatewayPage.mcpServerConfigHint')}</p>
        <pre class="rounded-lg bg-slate-800 p-3 text-[11px] font-mono text-slate-400 leading-relaxed overflow-x-auto dark:bg-[#161b22]/80">{mcpConfigJson}</pre>
      </div>
    </div>
  {/if}
</div>
