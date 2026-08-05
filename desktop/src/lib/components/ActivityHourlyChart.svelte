<script>
  import { locale, formatDurationLocalized, t } from '$lib/i18n/index.js';

  export let data = [];
  export let distributionTitle = '';
  export let distributionSubtitleKey = 'hourlyChart.distributionSubtitle';
  export let peakHourLabel = '';
  export let peakDurationLabel = '';
  export let embedded = false;
  // 按分类着色（堆叠柱）：categoryMode 开启时，每根柱按应用分类分段着色
  export let categoryMode = true;
  // 概览构成条选中的分类；为空时展示完整堆叠。
  export let selectedCategory = null;
  // { [hour]: [{ category, duration }, ...] }，由 Overview 从 hourly_app_breakdown 聚合
  export let categoryBreakdown = null;
  // { [category]: '#RRGGBB' }，来自 custom_categories
  export let categoryColors = null;
  // { [category]: '分类名' }，用于图例
  export let categoryNames = null;
  // 每小时×应用明细（HourlyAppBucket[]），点击柱子时展示该小时 top 应用
  export let appBreakdown = null;
  // 今日工作时长（秒），用于目标进度卡片
  export let workDuration = 0;
  // 每日工作目标（分钟），null = 不设目标
  export let workGoalMinutes = null;

  const keyHours = [0, 6, 12, 18, 23];
  let selectedHour = null;
  $: currentLocale = $locale;

  function formatHourLabel(hour) {
    return `${String(hour).padStart(2, '0')}:00`;
  }

  function formatHourRangeLabel(hour) {
    return `${formatHourLabel(hour)} - ${formatHourLabel((hour + 1) % 24)}`;
  }

  function showHourLabel(hour) {
    return keyHours.includes(hour);
  }

  function formatAxisTickLabel(seconds) {
    if (!seconds || seconds <= 0) {
      return '0';
    }
    return formatDurationLocalized(seconds, { compact: true });
  }

  function selectHour(hour) {
    selectedHour = hour;
  }

  // 统一紧凑时长格式，并跟随当前语言。
  function formatCompact(seconds) {
    return formatDurationLocalized(seconds, { compact: true });
  }

  $: buckets = Array.from({ length: 24 }, (_, hour) => {
    const existing = data.find((item) => item.hour === hour);
    return existing || { hour, duration: 0 };
  });

  $: maxDuration = Math.max(1, ...buckets.map((bucket) => bucket.duration || 0));
  $: activeBuckets = buckets.filter((bucket) => bucket.duration > 0);
  $: hasActiveData = activeBuckets.length > 0;
  $: totalDuration = buckets.reduce((sum, bucket) => sum + (bucket.duration || 0), 0);
  // 图例：从 categoryBreakdown 聚合出当前用到的分类（按时长降序）
  $: usedCategories = (() => {
    if (!categoryBreakdown) return [];
    const totals = {};
    for (const hour in categoryBreakdown) {
      for (const seg of categoryBreakdown[hour] || []) {
        totals[seg.category] = (totals[seg.category] || 0) + seg.duration;
      }
    }
    return Object.entries(totals)
      .sort((a, b) => b[1] - a[1])
      .map(([category, duration]) => ({ category, duration }));
  })();
  $: peakBucket = buckets.reduce(
    (peak, bucket) => (bucket.duration > peak.duration ? bucket : peak),
    buckets[0] || { hour: 0, duration: 0 }
  );
  $: topBuckets = [...activeBuckets]
    .sort((left, right) => right.duration - left.duration || left.hour - right.hour)
    .slice(0, 3);
  $: selectedBucket = buckets[selectedHour] || null;
  $: axisMax = (() => {
    const raw = Math.max(maxDuration, 60);
    const minute = 60;
    const candidates = [5, 10, 15, 20, 30, 45, 60, 90, 120, 180, 240, 300, 360, 480, 720]
      .map((value) => value * minute);
    return candidates.find((candidate) => candidate >= raw) || Math.ceil(raw / 3600) * 3600;
  })();
  $: yAxisTicks = [axisMax, Math.round(axisMax * 2 / 3), Math.round(axisMax / 3), 0];
  $: summaryCardClass = embedded
    ? 'min-h-[88px] rounded-[22px] bg-slate-50/90 px-2 py-3 text-center dark:bg-[#161b22]/30'
    : 'min-h-[104px] rounded-2xl border border-slate-100 bg-white p-4 text-center dark:border-[#30363d]/60 dark:bg-[#21262d]/80';
  $: summaryValueClass =
    'mt-2 text-center text-base font-semibold text-slate-900 dark:text-[#e6edf3] leading-tight';
  $: chartShellClass = embedded
    ? 'rounded-[24px] bg-transparent p-0'
    : 'rounded-2xl border border-slate-100 bg-white p-4 dark:border-[#30363d]/60 dark:bg-[#21262d]/80';
</script>

<div class="space-y-4" data-locale={currentLocale}>
  <div class="grid grid-cols-4 gap-2">
    <div class={summaryCardClass}>
      <p class="text-[13px] font-medium text-slate-400 dark:text-[#636c76]">{peakHourLabel || t('hourlyChart.peakHour')}</p>
      <p class={summaryValueClass}>
        {hasActiveData ? formatHourLabel(peakBucket.hour) : '--'}
      </p>
    </div>
    <div class={summaryCardClass}>
      <p class="text-[13px] font-medium text-slate-400 dark:text-[#636c76]">{peakDurationLabel || t('hourlyChart.peakDuration')}</p>
      <p class={summaryValueClass}>
        {hasActiveData ? formatCompact(peakBucket.duration) : '--'}
      </p>
    </div>
    <div class={summaryCardClass}>
      <p class="text-[13px] font-medium text-slate-400 dark:text-[#636c76]">{t('hourlyChart.activeHours')}</p>
      <p class={summaryValueClass}>
        {activeBuckets.length}
      </p>
    </div>
    <div class={summaryCardClass}>
      {#if workGoalMinutes && workGoalMinutes > 0}
        {@const goalSecs = workGoalMinutes * 60}
        {@const pct = Math.min(100, Math.round((workDuration / goalSecs) * 100))}
        {@const reached = workDuration >= goalSecs}
        <p class="text-[13px] font-medium {reached ? 'text-emerald-500' : 'text-slate-400 dark:text-[#636c76]'}">{t('hourlyChart.workGoal')}</p>
        <p class={summaryValueClass}>
          {formatCompact(workDuration)} <span class="text-[0.7em] text-slate-400">/ {formatCompact(goalSecs)}</span>
        </p>
        <div class="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-slate-200 dark:bg-[#30363d]">
          <div class={`h-full rounded-full transition-all duration-500 ${reached ? 'bg-emerald-500' : 'bg-primary-500'}`} style={`width: ${pct}%;`}></div>
        </div>
      {:else}
        <p class="text-[13px] font-medium text-slate-400 dark:text-[#636c76]">{t('hourlyChart.totalDuration')}</p>
        <p class={summaryValueClass}>
          {formatCompact(totalDuration)}
        </p>
      {/if}
    </div>
  </div>

  <div class={chartShellClass}>
    <div class="mb-4 flex items-center justify-between gap-3">
      <div>
        <p class="text-sm font-semibold text-slate-700 dark:text-[#c9d1d9]">
          {distributionTitle || t('hourlyChart.distributionTitle')}
        </p>
        <p class="mt-1 text-xs text-slate-500 dark:text-[#7d8590]">
          {t(distributionSubtitleKey, {
            hour: hasActiveData ? formatHourLabel(peakBucket.hour) : '--',
            duration: hasActiveData ? formatDurationLocalized(peakBucket.duration) : '--',
          })}
        </p>
      </div>
      {#if topBuckets.length > 0}
        <div class="hidden items-center gap-2 lg:flex">
          {#each topBuckets as bucket, index}
            <span class="rounded-full bg-slate-100 px-2.5 py-1 text-xs text-slate-500 dark:bg-[#30363d]/70 dark:text-[#adbac7]">
              {t('hourlyChart.topHour', { index: index + 1, hour: formatHourLabel(bucket.hour) })}
            </span>
          {/each}
        </div>
      {/if}
    </div>
    {#if usedCategories.length}
      <div class="mb-3 flex flex-wrap items-center gap-x-3 gap-y-1.5">
        {#each usedCategories as cat}
          <span class="inline-flex items-center gap-1.5 text-xs text-slate-500 dark:text-[#7d8590]">
            <span class="inline-block h-2.5 w-2.5 rounded-[3px]" style={`background: ${(categoryColors && categoryColors[cat.category]) || '#94a3b8'};`}></span>
            {(categoryNames && categoryNames[cat.category]) || cat.category}
          </span>
        {/each}
      </div>
    {/if}

    <div class="overflow-hidden rounded-2xl bg-slate-50 px-3 pb-3 pt-4 dark:bg-[#161b22]/40">
        <div class="grid grid-cols-[2.9rem_minmax(0,1fr)] gap-2">
          <div class="relative h-44">
            {#each yAxisTicks as tick, index}
              <div class="absolute inset-x-0 flex -translate-y-1/2 items-center justify-end text-[10px] font-medium text-slate-400 dark:text-[#636c76]" style={`top: ${(index / 3) * 100}%`}>
                <span class="whitespace-nowrap">{formatAxisTickLabel(tick)}</span>
              </div>
            {/each}
          </div>
          <div class="relative">
            <div class="pointer-events-none absolute inset-x-3 top-0 h-44">
              <div class="absolute inset-x-0 top-0 border-t border-dashed border-slate-200 dark:border-[#30363d]/80"></div>
              <div class="absolute inset-x-0 top-1/3 border-t border-dashed border-slate-200 dark:border-[#30363d]/80"></div>
              <div class="absolute inset-x-0 top-2/3 border-t border-dashed border-slate-200 dark:border-[#30363d]/80"></div>
              <div class="absolute inset-x-0 bottom-0 border-t border-dashed border-slate-200 dark:border-[#30363d]/80"></div>
            </div>

            <div class="relative px-3">
              <div class="grid h-44 grid-cols-[repeat(24,minmax(0,1fr))] items-end gap-1">
                {#each buckets as bucket}
                  {@const height = bucket.duration > 0 ? Math.max((bucket.duration / axisMax) * 100, 6) : 2}
                  {@const isPeak = bucket.duration > 0 && bucket.hour === peakBucket.hour}
                  <div class="relative flex h-full min-w-0 flex-col justify-end">
                    <button
                      type="button"
                      class={`w-full overflow-hidden transition-all duration-300 focus:outline-none focus:ring-2 focus:ring-sky-300 dark:focus:ring-sky-500 ${selectedHour === bucket.hour ? 'ring-2 ring-sky-300 dark:focus:ring-sky-500' : ''} ${categoryMode && bucket.duration > 0 ? '' : isPeak ? 'bg-sky-500 dark:bg-sky-400' : 'bg-slate-300 dark:bg-[#484f58]'}`}
                      style={`height: ${height}%; opacity: ${bucket.duration > 0 ? 1 : 0.35}; border-top-left-radius: 10px; border-top-right-radius: 10px;`}
                      title={`${formatHourRangeLabel(bucket.hour)} · ${formatDurationLocalized(bucket.duration)}`}
                      aria-pressed={selectedHour === bucket.hour}
                      on:click={() => selectHour(bucket.hour)}
                    >
                      {#if categoryMode && bucket.duration > 0}
                        <div class="flex h-full w-full flex-col justify-end overflow-hidden" style="border-top-left-radius: 10px; border-top-right-radius: 10px;">
                          {#each (categoryBreakdown?.[bucket.hour] || []) as seg}
                            <div
                              class={`activity-hourly-category-segment ${selectedCategory === seg.category ? 'activity-hourly-category-segment-selected' : ''} ${selectedCategory && seg.category !== selectedCategory ? 'activity-hourly-category-segment-muted' : ''}`}
                              style={`height: ${(seg.duration / bucket.duration) * 100}%; background: ${(categoryColors && categoryColors[seg.category]) || '#94a3b8'}; opacity: ${selectedCategory && seg.category !== selectedCategory ? 0.18 : 1};`}
                            ></div>
                          {/each}
                        </div>
                      {/if}
                    </button>
                  </div>
                {/each}
              </div>

              <div class="mt-3 grid grid-cols-[repeat(24,minmax(0,1fr))] gap-1">
                {#each buckets as bucket}
                  <div class="min-w-0 text-center">
                    <span class={`text-[10px] font-medium ${showHourLabel(bucket.hour) ? 'text-slate-400 dark:text-[#636c76]' : 'text-transparent'}`}>
                      {showHourLabel(bucket.hour) ? formatHourLabel(bucket.hour) : '.'}
                    </span>
                  </div>
                {/each}
              </div>
            </div>
          </div>
        </div>
      </div>

      {#if selectedBucket}
        {@const hourApps = (appBreakdown?.find(b => b.hour === selectedBucket.hour)?.apps || [])
          .filter((app) => !selectedCategory || app.category === selectedCategory)
          .slice().sort((a, b) => b.duration - a.duration).slice(0, 5)}
        <div class="activity-hourly-selected-apps mt-3 rounded-2xl bg-sky-50 px-3.5 py-3 text-left dark:bg-sky-500/10">
          <div class="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3">
            <span class="inline-flex items-center rounded-full bg-white px-2.5 py-1 text-[11px] font-semibold tracking-[0.08em] text-sky-700 shadow-[inset_0_1px_0_rgba(255,255,255,0.8)] dark:bg-[#161b22]/80 dark:text-sky-300 dark:shadow-none">
              {t('chart.currentlySelected')}
            </span>
            <span class="min-w-0 truncate text-sm font-medium text-slate-700 dark:text-[#c9d1d9]">
              {formatHourRangeLabel(selectedBucket.hour)}
            </span>
            <span class="text-sm font-semibold tabular-nums text-slate-500 dark:text-[#adbac7]">
              {formatCompact(selectedBucket.duration)}
            </span>
          </div>
          {#if hourApps.length > 0}
            <div class="mt-2 flex flex-wrap gap-x-3 gap-y-1">
              {#each hourApps as app}
                <span class="inline-flex items-center gap-1 text-[11px] text-slate-500 dark:text-[#7d8590]">
                  <span class="inline-block h-2 w-2 rounded-sm" style={`background: ${(categoryColors && categoryColors[app.category]) || '#94a3b8'};`}></span>
                  <span>{app.app_name}</span>
                  <span class="tabular-nums text-slate-400">{formatCompact(app.duration)}</span>
                </span>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
  </div>
</div>
