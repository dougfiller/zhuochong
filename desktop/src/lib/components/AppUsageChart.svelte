<script>
  import { onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { formatDurationLocalized, locale, t } from '$lib/i18n/index.js';
  import { appIconStore, getIconCacheKey, preloadAppIcons } from '../stores/iconCache.js';
  import { resolveAppIconSrc } from '../utils/appVisuals.js';

  export let data = [];
  export let mode = 'row';
  export let embedded = false;

  // 订阅全局图标缓存
  let appIcons = {};
  const unsubIcons = appIconStore.subscribe(v => appIcons = v);
  onDestroy(() => unsubIcons());

  // 展开/收起状态
  const DEFAULT_COUNT = 6;
  let expanded = false;
  $: currentLocale = $locale;

  // 格式化时长
  function formatDuration(seconds) {
    currentLocale;
    return formatDurationLocalized(seconds, { compact: true });
  }

  // 颜色列表
  const colors = [
    '#3b82f6', '#10b981', '#f59e0b', '#ef4444', '#8b5cf6',
    '#ec4899', '#06b6d4', '#84cc16', '#f97316', '#6366f1',
  ];

  // 数据变化时预加载图标
  $: if (data) {
    preloadAppIcons(
      displayApps.map(a => ({
        appName: a.app_name,
        executablePath: a.executable_path,
      })),
      invoke
    );
  }

  // 展开时显示全部，收起时显示前 8
  $: displayApps = expanded ? data : data.slice(0, DEFAULT_COUNT);
  $: hasMore = data.length > DEFAULT_COUNT;
  $: maxDuration = displayApps.length > 0 ? Math.max(1, ...displayApps.map(a => a.duration || 0)) : 1;
  $: columnShellClass = embedded
    ? 'app-usage-chart__columns app-usage-chart__columns-embedded'
    : 'app-usage-chart__columns rounded-2xl border border-slate-100 bg-white/90 p-4 dark:border-[#30363d]/60 dark:bg-[#21262d]/70';
  $: plotClass = embedded
    ? 'app-usage-chart__plot relative rounded-[22px] bg-slate-50/90 px-3 pb-3 pt-4 dark:bg-[#161b22]/40'
    : 'app-usage-chart__plot relative rounded-2xl bg-slate-50 px-3 pb-3 pt-4 dark:bg-[#161b22]/40';
</script>

<div class="space-y-2" data-locale={currentLocale}>
  {#if mode === 'column'}
    <div class={columnShellClass}>
      <div class={plotClass}>
        <div class="pointer-events-none absolute inset-x-3 top-4 bottom-14">
          <div class="absolute inset-x-0 top-0 border-t border-dashed border-slate-200 dark:border-[#30363d]/80"></div>
          <div class="absolute inset-x-0 top-1/2 border-t border-dashed border-slate-200 dark:border-[#30363d]/80"></div>
          <div class="absolute inset-x-0 bottom-0 border-t border-dashed border-slate-200 dark:border-[#30363d]/80"></div>
        </div>

        <div class="relative flex h-52 items-end gap-3 overflow-x-auto pb-2">
          {#each displayApps as app, i}
            {@const iconSrc = resolveAppIconSrc(
              app.app_name,
              appIcons[getIconCacheKey({ appName: app.app_name, executablePath: app.executable_path })]
            )}
            <div class="min-w-[5.5rem] flex-1">
              <div class="mb-2 text-center text-[11px] font-medium whitespace-nowrap tabular-nums text-slate-500 dark:text-[#7d8590]">
                {formatDuration(app.duration)}
              </div>
              <div class="mx-auto flex h-32 w-12 items-end rounded-2xl bg-slate-100 p-1 dark:bg-[#30363d]/50">
                <div
                  class="app-usage-chart__bar w-full rounded-2xl transition-all duration-500"
                  style="height: {Math.max((app.duration / maxDuration) * 100, 6)}%; background-color: {colors[i % colors.length]}; opacity: 0.88"
                ></div>
              </div>
              <div class="mt-3 flex items-center justify-center gap-1.5 px-1">
                {#if iconSrc}
                  <img src={iconSrc} alt="" class="h-5 w-5 rounded-md object-cover" />
                {:else}
                  <span class="inline-flex h-5 w-5 items-center justify-center rounded-md bg-slate-100 text-[10px] text-slate-500 dark:bg-[#30363d]">{i + 1}</span>
                {/if}
                <span class="min-w-0 truncate text-[11px] font-medium text-slate-700 dark:text-[#adbac7]">{app.app_name}</span>
              </div>
            </div>
          {/each}
        </div>
      </div>
    </div>
  {:else}
    <div class="app-usage-chart__rows">
      {#each displayApps as app, i}
        {@const iconSrc = resolveAppIconSrc(
          app.app_name,
          appIcons[getIconCacheKey({ appName: app.app_name, executablePath: app.executable_path })]
        )}
        <div class="app-usage-chart__row">
          <span class="app-usage-chart__heading">
            <span class="app-usage-chart__icon">
              {#if iconSrc}
                <img src={iconSrc} alt="" class="h-5 w-5 rounded-md object-cover" />
              {:else}
                <span>{i + 1}</span>
              {/if}
            </span>
            <span class="app-usage-chart__copy">
              <span class="app-usage-chart__name">{app.app_name}</span>
              <span class="app-usage-chart__meta">{t('overview.appActivityCount', { count: app.count || 0 })}</span>
            </span>
          </span>
          <span class="app-usage-chart__track">
            <span
              class="app-usage-chart__bar"
              style="width: {Math.max((app.duration / maxDuration) * 100, 2)}%; background-color: {colors[i % colors.length]}; opacity: 0.8"
            ></span>
          </span>
          <span class="app-usage-chart__duration">{formatDuration(app.duration)}</span>
        </div>
      {/each}
    </div>
  {/if}

  <div class="app-usage-chart__footer">
    <span>{t('overview.appsFooter', { count: data.length })}</span>
    {#if hasMore}
      <span aria-hidden="true">·</span>
      <button type="button" on:click={() => expanded = !expanded}>
        {expanded ? t('overview.appUsageCollapse') : t('overview.viewAll')}
      </button>
    {/if}
  </div>
</div>
