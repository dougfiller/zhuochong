<script>
  import { t } from '$lib/i18n/index.js';
  import { invoke } from '@tauri-apps/api/core';
  import { createEventDispatcher } from 'svelte';

  export let localStatus = { enabled: false, baseUrl: '', tokenPreview: '' };

  const dispatch = createEventDispatcher();
  let examplesExpanded = false;
  let activeApiCategory = 'all';

  $: curlToken = localStatus.tokenPreview || '';
  $: curlBase = localStatus.baseUrl || 'http://127.0.0.1:47831';

  function curlCommand(method, path, body) {
    const headers = ['-H', '"Content-Type: application/json"'];
    if (curlToken && curlToken !== '—') {
      headers.push('-H', `"Authorization: Bearer ${curlToken}"`);
    }
    const parts = ['curl', '-X', method, `"${curlBase}${path}"`, ...headers];
    if (body) {
      parts.push('-d', `'${JSON.stringify(body)}'`);
    }
    return parts.join(' \\\n  ');
  }

  async function copyCurl(cmd) {
    try {
      await navigator.clipboard.writeText(cmd);
      dispatch('toast', { message: t('nodeGatewayPage.curlCopied'), type: 'success' });
    } catch (e) {
      dispatch('toast', { message: t('nodeGatewayPage.tokenCopyFailed'), type: 'error' });
    }
  }

  const examples = [
    { cat: 'system', method: 'GET', path: '/health', desc: t('nodeGatewayPage.exampleHealthDesc'), cmd: curlCommand('GET', '/health') },
    { cat: 'system', method: 'GET', path: '/v1/device', desc: t('nodeGatewayPage.exampleDeviceDesc'), cmd: curlCommand('GET', '/v1/device') },
    { cat: 'system', method: 'GET', path: '/v1/storage/stats', desc: t('nodeGatewayPage.exampleStorageStatsDesc'), cmd: curlCommand('GET', '/v1/storage/stats') },
    { cat: 'report', method: 'GET', path: '/v1/reports', desc: t('nodeGatewayPage.exampleReportsDesc'), cmd: curlCommand('GET', '/v1/reports') },
    { cat: 'report', method: 'GET', path: '/v1/reports/:date', desc: t('nodeGatewayPage.exampleReportByDateDesc'), cmd: curlCommand('GET', '/v1/reports/2025-01-15') },
    { cat: 'report', method: 'GET', path: '/v1/reports/generate', desc: t('nodeGatewayPage.exampleGenerateDesc'), cmd: curlCommand('GET', '/v1/reports/generate?date=2025-01-15') },
    { cat: 'report', method: 'POST', path: '/v1/reports/export-markdown', desc: t('nodeGatewayPage.exampleExportDesc'), cmd: curlCommand('POST', '/v1/reports/export-markdown', { date: '2025-01-15', content: '# Report', export_dir: '/path/to/dir' }) },
    { cat: 'report', method: 'GET', path: '/v1/weekly-review', desc: t('nodeGatewayPage.exampleWeeklyReviewDesc'), cmd: curlCommand('GET', '/v1/weekly-review?date_from=2025-01-13&date_to=2025-01-19') },
    { cat: 'timeline', method: 'GET', path: '/v1/timeline/:date', desc: t('nodeGatewayPage.exampleTimelineDesc'), cmd: curlCommand('GET', '/v1/timeline/2025-01-15') },
    { cat: 'timeline', method: 'GET', path: '/v1/activities/:date', desc: t('nodeGatewayPage.exampleActivitiesDesc'), cmd: curlCommand('GET', '/v1/activities/2025-01-15') },
    { cat: 'timeline', method: 'GET', path: '/v1/hourly-summaries/:date', desc: t('nodeGatewayPage.exampleHourlySummariesDesc'), cmd: curlCommand('GET', '/v1/hourly-summaries/2025-01-15') },
    { cat: 'stats', method: 'GET', path: '/v1/stats/today', desc: t('nodeGatewayPage.exampleStatsTodayDesc'), cmd: curlCommand('GET', '/v1/stats/today') },
    { cat: 'stats', method: 'GET', path: '/v1/stats/daily/:date', desc: t('nodeGatewayPage.exampleStatsDailyDesc'), cmd: curlCommand('GET', '/v1/stats/daily/2025-01-15') },
    { cat: 'stats', method: 'GET', path: '/v1/stats/overview', desc: t('nodeGatewayPage.exampleStatsOverviewDesc'), cmd: curlCommand('GET', '/v1/stats/overview?mode=week&date=2025-01-17') },
    { cat: 'stats', method: 'GET', path: '/v1/hourly-app-breakdown/:date', desc: t('nodeGatewayPage.exampleHourlyBreakdownDesc'), cmd: curlCommand('GET', '/v1/hourly-app-breakdown/2025-01-15') },
    { cat: 'stats', method: 'GET', path: '/v1/apps/recent', desc: t('nodeGatewayPage.exampleAppsRecentDesc'), cmd: curlCommand('GET', '/v1/apps/recent') },
    { cat: 'stats', method: 'GET', path: '/v1/apps/category-overview', desc: t('nodeGatewayPage.exampleAppsCategoryDesc'), cmd: curlCommand('GET', '/v1/apps/category-overview') },
    { cat: 'stats', method: 'GET', path: '/v1/categories', desc: t('nodeGatewayPage.exampleCategoriesDesc'), cmd: curlCommand('GET', '/v1/categories') },
    { cat: 'stats', method: 'GET', path: '/v1/categories/semantic', desc: t('nodeGatewayPage.exampleSemanticCategoriesDesc'), cmd: curlCommand('GET', '/v1/categories/semantic') },
  ];
</script>

<div>
  <button
    type="button"
    class="flex w-full items-center justify-between gap-2 rounded-xl bg-white/70 px-3.5 py-2.5 ring-1 ring-slate-200/70 dark:bg-[#161b22]/20 dark:ring-[#30363d]/70"
    on:click={() => (examplesExpanded = !examplesExpanded)}
  >
    <div class="flex items-center gap-2">
      <svg class="w-3.5 h-3.5 text-slate-500 dark:text-[#7d8590]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
      </svg>
      <span class="text-sm font-medium text-slate-700 dark:text-[#c9d1d9]">{t('nodeGatewayPage.apiExamples')}</span>
      <span class="text-[10px] text-slate-400">19 endpoints</span>
    </div>
    <svg class="w-4 h-4 text-slate-400 transition-transform {examplesExpanded ? 'rotate-180' : ''}" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
    </svg>
  </button>

  {#if examplesExpanded}
    <div class="mt-2 space-y-2">
      <p class="text-[11px] text-slate-400 dark:text-[#636c76]">{t('nodeGatewayPage.apiExamplesHint')}</p>
      <div class="flex flex-wrap gap-1.5">
        {#each [
          { key: 'all', label: t('nodeGatewayPage.apiCategoryAll') },
          { key: 'report', label: t('nodeGatewayPage.apiCategoryReport') },
          { key: 'timeline', label: t('nodeGatewayPage.apiCategoryTimeline') },
          { key: 'stats', label: t('nodeGatewayPage.apiCategoryStats') },
          { key: 'system', label: t('nodeGatewayPage.apiCategorySystem') },
        ] as cat}
          <button
            type="button"
            on:click={() => activeApiCategory = cat.key}
            class="px-2.5 py-1 rounded-lg text-[11px] font-medium transition-colors
                   {activeApiCategory === cat.key
                     ? 'bg-slate-700 text-white dark:bg-[#30363d] dark:text-[#c9d1d9]'
                     : 'bg-slate-100 text-slate-500 hover:bg-slate-200 dark:bg-[#21262d] dark:text-[#7d8590] dark:hover:bg-[#30363d]'}"
          >{cat.label}</button>
        {/each}
      </div>
      {#each examples as example}
        {#if activeApiCategory === 'all' || activeApiCategory === example.cat}
          <div class="rounded-lg ring-1 ring-slate-200/70 dark:ring-[#30363d]/70 overflow-hidden">
            <div class="flex items-center gap-2 bg-white/90 px-3 py-1.5 dark:bg-[#21262d]/60">
              <span class="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-bold leading-none
                {example.method === 'POST'
                  ? 'bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-400'
                  : 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-400'}"
              >{example.method}</span>
              <span class="min-w-0 text-xs font-mono font-medium text-slate-700 dark:text-[#c9d1d9] truncate">{example.path}</span>
              <span class="ml-auto shrink-0">
                <button type="button" class="rounded-md px-2 py-0.5 text-[10px] font-medium text-slate-500 hover:bg-slate-100 hover:text-slate-700 dark:text-[#7d8590] dark:hover:bg-[#30363d] dark:hover:text-[#c9d1d9] transition-colors" on:click={() => copyCurl(example.cmd)}>
                  {t('nodeGatewayPage.copyCurl')}
                </button>
              </span>
            </div>
            <div class="border-t border-slate-100 dark:border-[#30363d]/50">
              <div class="px-3 py-1 text-[11px] text-slate-500 dark:text-[#7d8590]">{example.desc}</div>
              <button
                type="button"
                class="mx-2 mb-1.5 block w-[calc(100%-1rem)] overflow-x-auto rounded-md bg-slate-800/90 px-2.5 py-1.5 text-left text-[11px] font-mono leading-relaxed text-emerald-300/90 dark:bg-[#161b22]/90"
                on:click={() => copyCurl(example.cmd)}
              ><span class="whitespace-pre">{example.cmd}</span></button>
            </div>
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>
