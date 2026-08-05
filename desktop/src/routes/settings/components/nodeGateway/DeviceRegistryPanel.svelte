<script>
  import { t } from '$lib/i18n/index.js';
  import { createEventDispatcher } from 'svelte';

  export let config;
  export let localStatus = { baseUrl: '' };

  const dispatch = createEventDispatcher();
</script>

<div class="space-y-2">
  <div class="flex items-center justify-between">
    <span class="text-xs font-medium text-slate-700 dark:text-[#adbac7]">{t('nodeGatewayPage.deviceRegistry')}</span>
    <button
      type="button"
      class="settings-chip-button settings-chip-button-active text-[11px]"
      on:click={() => {
        if (!config.node_devices) config.node_devices = [];
        config.node_devices = [...config.node_devices, { name: '', url: '', token: '' }];
      }}
    >
      {t('nodeGatewayPage.addDevice')}
    </button>
  </div>
  <p class="text-[11px] text-slate-400 dark:text-[#636c76]">{t('nodeGatewayPage.deviceRegistryHint')}</p>
  <div class="flex items-center gap-2 rounded-lg bg-primary-50/60 px-3 py-1.5 ring-1 ring-primary-200/60 dark:bg-primary-900/20 dark:ring-primary-800/40">
    <span class="text-xs font-medium text-primary-700 dark:text-primary-300">{t('nodeGatewayPage.localDevice')}</span>
    <span class="text-xs font-mono text-slate-500 dark:text-[#7d8590]">{localStatus?.baseUrl || '-'}</span>
  </div>
  {#each config.node_devices || [] as device, i}
    <div class="flex items-start gap-2 rounded-lg bg-white/70 px-3 py-2 ring-1 ring-slate-200/70 dark:bg-[#161b22]/20 dark:ring-[#30363d]/70">
      <div class="flex-1 min-w-0 grid gap-x-2 gap-y-1 grid-cols-[1fr_2fr]">
        <label class="block">
          <span class="text-[10px] text-slate-500 dark:text-[#7d8590]">{t('nodeGatewayPage.deviceNameCol')}</span>
          <input
            type="text"
            bind:value={device.name}
            class="mt-0.5 w-full rounded-md bg-white/80 px-2 py-1 text-sm text-slate-900 ring-1 ring-slate-200 focus:ring-primary-300 dark:bg-[#30363d]/50 dark:text-[#e6edf3] dark:ring-[#484f58] dark:focus:ring-primary-600 focus:outline-none"
            placeholder="Office PC"
          />
        </label>
        <label class="block">
          <span class="text-[10px] text-slate-500 dark:text-[#7d8590]">{t('nodeGatewayPage.deviceUrlCol')}</span>
          <input
            type="text"
            bind:value={device.url}
            class="mt-0.5 w-full rounded-md bg-white/80 px-2 py-1 text-sm font-mono text-slate-900 ring-1 ring-slate-200 focus:ring-primary-300 dark:bg-[#30363d]/50 dark:text-[#e6edf3] dark:ring-[#484f58] dark:focus:ring-primary-600 focus:outline-none"
            placeholder="http://192.168.1.100:47831"
          />
        </label>
        <label class="col-span-2 block">
          <span class="text-[10px] text-slate-500 dark:text-[#7d8590]">{t('nodeGatewayPage.deviceTokenCol')}</span>
          <input
            type="password"
            bind:value={device.token}
            class="mt-0.5 w-full rounded-md bg-white/80 px-2 py-1 text-xs font-mono text-slate-900 ring-1 ring-slate-200 focus:ring-primary-300 dark:bg-[#30363d]/50 dark:text-[#e6edf3] dark:ring-[#484f58] dark:focus:ring-primary-600 focus:outline-none"
            placeholder="wr-local-..."
          />
        </label>
      </div>
      <button
        type="button"
        class="text-xs text-red-400 hover:text-red-500 mt-3 shrink-0"
        on:click={() => {
          config.node_devices = config.node_devices.filter((_, j) => j !== i);
          dispatch('save');
        }}
      >
        {t('nodeGatewayPage.removeDevice')}
      </button>
    </div>
  {/each}
</div>
