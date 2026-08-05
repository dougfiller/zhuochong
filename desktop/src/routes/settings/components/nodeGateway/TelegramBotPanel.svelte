<script>
  import { t } from '$lib/i18n/index.js';
  import { invoke } from '@tauri-apps/api/core';
  import { createEventDispatcher } from 'svelte';

  export let config;
  export let tgBotStatus = null;
  export let saving = false;

  const dispatch = createEventDispatcher();
  let tgTokenVisible = false;

  function toggle() {
    config.telegram_bot_enabled = !config.telegram_bot_enabled;
    dispatch('save');
    if (config.telegram_bot_enabled) {
      dispatch('startTgPolling');
    }
  }

  async function generateBindCode() {
    try {
      await invoke('generate_telegram_bot_bind_code');
      dispatch('reloadStatus');
      dispatch('toast', { message: t('nodeGatewayPage.telegramBindCodeGenerated'), type: 'success' });
    } catch (e) {
      dispatch('toast', { message: t('nodeGatewayPage.telegramBindCodeFailed', { error: e }), type: 'error' });
    }
  }
</script>

<div class="rounded-xl bg-white/70 px-3.5 py-3 ring-1 ring-slate-200/70 dark:bg-[#161b22]/20 dark:ring-[#30363d]/70">
  <div class="flex items-center justify-between gap-3">
    <div class="flex items-center gap-2">
      <svg class="w-4 h-4 text-[#229ED9]" viewBox="0 0 24 24" fill="currentColor">
        <path d="M11.944 0A12 12 0 0 0 0 12a12 12 0 0 0 12 12 12 12 0 0 0 12-12A12 12 0 0 0 12 0a12 12 0 0 0-.056 0zm4.962 7.224c.1-.002.321.023.465.14a.506.506 0 0 1 .171.325c.016.093.036.306.02.472-.18 1.898-.962 6.502-1.36 8.627-.168.9-.499 1.201-.82 1.23-.696.065-1.225-.46-1.9-.902-1.056-.693-1.653-1.124-2.678-1.8-1.185-.78-.417-1.21.258-1.91.177-.184 3.247-2.977 3.307-3.23.007-.032.014-.15-.056-.212s-.174-.041-.249-.024c-.106.024-1.793 1.14-5.061 3.345-.479.33-.913.49-1.302.48-.428-.008-1.252-.241-1.865-.44-.752-.245-1.349-.374-1.297-.789.027-.216.325-.437.893-.663 3.498-1.524 5.83-2.529 6.998-3.014 3.332-1.386 4.025-1.627 4.476-1.635z"/>
      </svg>
      <span class="text-sm text-slate-700 dark:text-[#c9d1d9]">Telegram</span>
      {#if config.telegram_bot_enabled}
        <span class="settings-chip-success">{t('nodeGatewayPage.telegramEnabled')}</span>
      {/if}
    </div>
    <button
      type="button"
      on:click={toggle}
      disabled={saving}
      class="switch-track {config.telegram_bot_enabled ? 'bg-primary-500' : 'bg-slate-300 dark:bg-[#484f58]'} {saving ? 'opacity-60 cursor-not-allowed' : ''}"
      role="switch"
      aria-label="Telegram"
      aria-checked={config.telegram_bot_enabled}
    >
      <span class="switch-thumb {config.telegram_bot_enabled ? 'translate-x-5' : 'translate-x-0'}"></span>
    </button>
  </div>
  {#if config.telegram_bot_enabled}
    <div class="mt-2 space-y-2">
      <label class="block">
        <span class="text-[11px] text-slate-500 dark:text-[#7d8590]">{t('nodeGatewayPage.telegramBotToken')}</span>
        <div class="mt-0.5 relative">
          {#if tgTokenVisible}
            <input
              type="text"
              bind:value={config.telegram_bot_token}
              on:blur={() => dispatch('save')}
              class="w-full rounded-md bg-white/80 px-3 py-1.5 pr-8 text-sm font-mono text-slate-900 ring-1 ring-slate-200 focus:ring-primary-300 dark:bg-[#30363d]/50 dark:text-[#e6edf3] dark:ring-[#484f58] dark:focus:ring-primary-600 focus:outline-none"
              placeholder="123456:ABC-DEF..."
            />
          {:else}
            <input
              type="password"
              bind:value={config.telegram_bot_token}
              on:blur={() => dispatch('save')}
              class="w-full rounded-md bg-white/80 px-3 py-1.5 pr-8 text-sm font-mono text-slate-900 ring-1 ring-slate-200 focus:ring-primary-300 dark:bg-[#30363d]/50 dark:text-[#e6edf3] dark:ring-[#484f58] dark:focus:ring-primary-600 focus:outline-none"
              placeholder="123456:ABC-DEF..."
            />
          {/if}
          <button
            type="button"
            class="absolute right-1.5 top-1/2 -translate-y-1/2 p-0.5 text-slate-400 hover:text-slate-700 dark:hover:text-[#adbac7]"
            aria-label={`${t(tgTokenVisible ? 'nodeGatewayPage.hideSecret' : 'nodeGatewayPage.showSecret')}: ${t('nodeGatewayPage.telegramBotToken')}`}
            on:click={() => (tgTokenVisible = !tgTokenVisible)}
          >
            {#if tgTokenVisible}
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.878 9.878L6.59 6.59m7.532 7.532l3.29 3.29M3 3l18 18" /></svg>
            {:else}
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" /><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" /></svg>
            {/if}
          </button>
        </div>
      </label>
      {#if tgBotStatus}
        {#if tgBotStatus.starting}
          <div class="flex items-center gap-1.5 text-[11px] text-blue-500 dark:text-blue-400">
            <div class="animate-spin h-3 w-3 border-[1.5px] border-blue-400 border-t-transparent rounded-full"></div>
            ...
          </div>
        {:else if tgBotStatus.running}
          <div class="flex items-center gap-1.5 text-[11px] text-emerald-600 dark:text-emerald-400">
            <span class="inline-block h-1.5 w-1.5 rounded-full bg-emerald-500"></span>
            Bot running
          </div>
        {:else if tgBotStatus.lastError}
          <div class="flex items-center gap-1.5 text-[11px] text-amber-600 dark:text-amber-400">
            <span class="inline-block h-1.5 w-1.5 rounded-full bg-amber-500"></span>
            {tgBotStatus.lastError}
          </div>
        {/if}
      {/if}
      <label class="block">
        <span class="text-[11px] text-slate-500 dark:text-[#7d8590]">{t('nodeGatewayPage.telegramBotProxy')}</span>
        <input
          type="text"
          bind:value={config.telegram_bot_proxy}
          on:blur={() => dispatch('save')}
          class="mt-0.5 w-full rounded-md bg-white/80 px-3 py-1.5 text-sm font-mono text-slate-900 ring-1 ring-slate-200 focus:ring-primary-300 dark:bg-[#30363d]/50 dark:text-[#e6edf3] dark:ring-[#484f58] dark:focus:ring-primary-600 focus:outline-none"
          placeholder="http://127.0.0.1:7890"
        />
      </label>
      <p class="text-[11px] text-slate-400 dark:text-[#636c76]">{t('nodeGatewayPage.telegramBotProxyHint')}</p>
      <div class="rounded-lg bg-blue-50/70 px-3 py-2 ring-1 ring-blue-100 dark:bg-blue-950/20 dark:ring-blue-900/50">
        <div class="flex items-center justify-between gap-3">
          <div class="min-w-0">
            <div class="text-[11px] font-medium text-blue-700 dark:text-blue-300">{t('nodeGatewayPage.telegramBindCodeTitle')}</div>
            <div class="mt-0.5 text-[11px] text-blue-600/80 dark:text-blue-300/80">{t('nodeGatewayPage.telegramBindCodeHint')}</div>
            {#if tgBotStatus?.bindCode && !tgBotStatus?.bindCodeExpired}
              <div class="mt-1 font-mono text-sm font-semibold text-blue-800 dark:text-blue-200">/bind {tgBotStatus.bindCode}</div>
            {:else if tgBotStatus?.bindCodeExpired}
              <div class="mt-1 text-[11px] text-amber-600 dark:text-amber-300">{t('nodeGatewayPage.telegramBindCodeExpired')}</div>
            {/if}
          </div>
          <button
            type="button"
            class="shrink-0 rounded-md bg-blue-600 px-2.5 py-1.5 text-xs font-medium text-white hover:bg-blue-700 disabled:opacity-60"
            on:click={generateBindCode}
            disabled={saving}
          >
            {t('nodeGatewayPage.telegramBindCodeGenerate')}
          </button>
        </div>
      </div>
      <label class="block mt-2">
        <span class="text-[11px] text-slate-500 dark:text-[#7d8590]">{t('nodeGatewayPage.telegramAllowedChatIds')}</span>
        <input
          type="text"
          value={(config.telegram_bot_allowed_chat_ids || []).join(', ')}
          on:blur={(e) => {
            const ids = e.target.value.split(',').map(s => parseInt(s.trim(), 10)).filter(n => !isNaN(n));
            config.telegram_bot_allowed_chat_ids = ids;
            dispatch('save');
          }}
          class="mt-0.5 w-full rounded-md bg-white/80 px-3 py-1.5 text-sm font-mono text-slate-900 ring-1 ring-slate-200 focus:ring-primary-300 dark:bg-[#30363d]/50 dark:text-[#e6edf3] dark:ring-[#484f58] dark:focus:ring-primary-600 focus:outline-none"
          placeholder="123456789, 987654321"
        />
      </label>
      <p class="text-[11px] text-slate-400 dark:text-[#636c76]">{t('nodeGatewayPage.telegramAllowedChatIdsHint')}</p>
      <p class="text-[11px] text-slate-400 dark:text-[#636c76]">{t('nodeGatewayPage.telegramBotHint')}</p>
    </div>
  {/if}
</div>
