<script>
  import { onMount, onDestroy, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { open } from '@tauri-apps/plugin-shell';
  import { ask, save as saveDialog } from '@tauri-apps/plugin-dialog';
  import { cache } from '../../lib/stores/cache.js';
  import { recordingStore, isActiveRecording } from '../../lib/stores/recording.js';
  import { showToast } from '../../lib/stores/toast.js';
  import { appIconStore, getIconCacheKey, preloadAppIcons } from '../../lib/stores/iconCache.js';
  import { categoryStore, hexToRGBA } from '../../lib/stores/categories.js';
  import {
    formatDurationLocalized,
    formatLocalizedTime,
    locale,
    t,
    translateCategoryLabel,
  } from '$lib/i18n/index.js';
  import { formatUserError } from '$lib/utils/errorDisplay.js';
  import { trapFocus } from '$lib/utils/focusTrap.js';
  import { isValidLocalDateString } from '$lib/utils/dateValidation.js';
  import {
    getPreferredTimelineAppName,
    shouldPreferTimelineFallbackIcon,
  } from '$lib/utils/appDisplay.js';
  import { resolveAppIconSrc } from '../../lib/utils/appVisuals.js';
  import { formatBrowserUrlForDisplay } from '../../lib/utils/browserUrl.js';
  import { getViewportPopoverPlacement } from '../../lib/utils/popoverPosition.js';
  import { prepareTimelineActivities, upsertTimelineActivity } from './timelineData.js';
  import LocalizedDatePicker from '../../lib/components/LocalizedDatePicker.svelte';
  import HourlySummaryDrawer from './HourlySummaryDrawer.svelte';
  import { confirm } from '../../lib/stores/confirm.js';

  // 获取本地日期（避免 UTC 时区问题）
  function getLocalDateString() {
    const now = new Date();
    const year = now.getFullYear();
    const month = String(now.getMonth() + 1).padStart(2, '0');
    const day = String(now.getDate()).padStart(2, '0');
    return `${year}-${month}-${day}`;
  }

  let activities = [];
  let hourlySummaries = [];
  let loading = true;
  let error = null;
  let selectedDate = getLocalDateString();
  let selectedActivity = null;
  let showSummaryDrawer = false;
  let summaryRefreshing = false;
  let summaryRefreshError = null;
  let summaryRefreshRequestId = 0;
  let summaryTrigger;
  let detailTrigger;
  let detailCloseButton;
  let categoryTrigger;
  let categoryPopover;
  let categoryPopoverStyle = '';
  let showCategoryPopover = false;
  let unlisten = null;
  let componentDestroyed = false;
  let currentTime = new Date();
  let clockInterval;
  let handleVisibilityChange;
  let handleTimelineFocus;
  let appIcons = {};

  // LRU 缓存：防止长时间运行内存无限增长
  // 缩略图 ~80KB/条，60 条 ≈ 5MB；高清图 ~300KB/条，20 条 ≈ 6MB
  const THUMBNAIL_CACHE_LIMIT = 60;
  const FULLIMAGE_CACHE_LIMIT = 20;
  let thumbnailCache = {};
  let thumbnailKeys = [];   // 插入顺序追踪，用于淘汰最旧条目
  let fullImageCache = {};
  let fullImageKeys = [];
  $: currentLocale = $locale;

  // 向 LRU 缓存中写入，超出上限时淘汰最旧条目释放内存
  function lruSet(cache, keys, limit, key, value) {
    if (!(key in cache)) {
      keys.push(key);
    }
    cache[key] = value;
    while (keys.length > limit) {
      const evicted = keys.shift();
      delete cache[evicted];
    }
  }

  // 清空图片缓存（日期切换时调用，释放旧数据占用的内存）
  function clearImageCaches() {
    thumbnailCache = {};
    thumbnailKeys = [];
    fullImageCache = {};
    fullImageKeys = [];
  }

  const unsubIcons = appIconStore.subscribe(v => appIcons = v);

  function readTimelineQuery() {
    if (typeof window === 'undefined') {
      return new URLSearchParams();
    }

    // hash 路由：query 在 location.hash（如 #/timeline?date=2026-06-22），不在 location.search
    const hash = window.location.hash;
    const queryIndex = hash.indexOf('?');
    const search = queryIndex >= 0 ? hash.slice(queryIndex + 1) : '';
    return new URLSearchParams(search);
  }

  function readRequestedTimelineDate() {
    const nextDate = readTimelineQuery().get('date');
    return nextDate && isValidLocalDateString(nextDate) ? nextDate : null;
  }

  function readRequestedSummaryOpen() {
    return readTimelineQuery().get('summary') === '1';
  }

  // summary=1 只作为一次性的旧路由兼容指令，消费后立即从地址中移除。
  function consumeRequestedSummaryOpen() {
    if (typeof window === 'undefined') return;

    const params = readTimelineQuery();
    if (params.get('summary') !== '1') return;

    params.delete('summary');
    const hash = window.location.hash;
    const queryIndex = hash.indexOf('?');
    const routeHash = queryIndex >= 0 ? hash.slice(0, queryIndex) : hash;
    const nextQuery = params.toString();
    const nextHash = `${routeHash}${nextQuery ? `?${nextQuery}` : ''}`;
    window.history.replaceState(
      window.history.state,
      '',
      `${window.location.pathname}${window.location.search}${nextHash}`
    );
  }

  function applyTimelineFocus(payload) {
    const nextDate =
      typeof payload?.date === 'string' && isValidLocalDateString(payload.date)
        ? payload.date
        : null;

    if (!nextDate) {
      return;
    }

    selectedActivity = null;
    if (selectedDate === nextDate) {
      loadTimeline();
      return;
    }

    selectedDate = nextDate;
  }

  // 分类元数据（从 store 动态获取，支持自定义分类）
  let categorySaving = false;
  let showCreateCategory = false;
  let newCategoryName = '';
  let newCategoryColor = '#6366f1';
  let newCategoryIcon = '🏷️';

  // 重命名分类
  let showRenameCategory = false;
  let renameCategoryKey = '';
  let renameCategoryName = '';
  let renameCategoryColor = '#6366f1';
  let renameCategoryIcon = '🏷️';

  function startRenameCategory(cat) {
    renameCategoryKey = cat.key;
    renameCategoryName = cat.name;
    renameCategoryColor = cat.color;
    renameCategoryIcon = cat.icon;
    showRenameCategory = true;
  }

  async function saveRenameCategory() {
    const name = renameCategoryName.trim();
    if (!name) return;
    categorySaving = true;
    try {
      await invoke('save_custom_category', {
        key: renameCategoryKey,
        name,
        color: renameCategoryColor,
        icon: renameCategoryIcon,
      });
      await categoryStore.refresh();
      showRenameCategory = false;
      showToast(t('timeline.categoryRenamed'), 'success');
    } catch (e) {
      showToast(e.toString(), 'error');
    } finally {
      categorySaving = false;
    }
  }

  // 创建分类后的应用确认（内联渲染，确保在详情弹窗之上）
  let pendingApplyCategory = null; // { key, name }
  function cancelApplyCategory() { pendingApplyCategory = null; }
  async function confirmApplyCategory() {
    if (!pendingApplyCategory || !selectedActivity) return;
    const { key } = pendingApplyCategory;
    pendingApplyCategory = null;
    await doChangeAppCategory(selectedActivity, key);
  }

  // 修改分类确认（内联渲染，替代全局 confirm）
  let pendingChangeCategory = null; // { activity, category, categoryName }
  function cancelChangeCategory() { pendingChangeCategory = null; }
  async function confirmChangeCategory() {
    if (!pendingChangeCategory) return;
    const { activity, category } = pendingChangeCategory;
    pendingChangeCategory = null;
    await doChangeAppCategory(activity, category);
  }

  // 弹出确认 → 用户点击分类按钮时触发
  async function changeAppCategory(activity, nextCategory) {
    if (!activity || !nextCategory || categorySaving) return;
    if ((activity.category || 'other') === nextCategory) return;
    const targetInfo = getCategoryMeta(nextCategory);
    pendingChangeCategory = {
      activity,
      category: nextCategory,
      categoryName: targetInfo.name,
    };
  }

  function selectActivityCategory(nextCategory) {
    prepareCategoryConfirmation();
    changeAppCategory(selectedActivity, nextCategory);
  }

  // 从分类 Popover 进入二次确认前，先把焦点交还给稳定存在的分类入口。
  // 确认层的 trapFocus 会记录该入口，并在关闭时自动恢复焦点。
  function prepareCategoryConfirmation() {
    showCategoryPopover = false;
    categoryPopoverStyle = '';
    categoryTrigger?.focus();
  }

  // 保存期间分类入口会暂时禁用；恢复可用后，仅在焦点无人接管时重新聚焦。
  async function restoreCategoryTriggerAfterSaving() {
    await tick();
    if (!selectedActivity || !categoryTrigger || typeof document === 'undefined') return;

    const activeElement = document.activeElement;
    if (
      activeElement
      && activeElement !== document.body
      && activeElement !== document.documentElement
    ) return;

    categoryTrigger?.focus();
  }

  // 确认后实际执行分类修改
  async function doChangeAppCategory(activity, nextCategory) {
    categorySaving = true;
    try {
      const targetInfo = getCategoryMeta(nextCategory);
      const updatedCount = await invoke('set_app_category_rule', {
        appName: activity.app_name,
        category: nextCategory,
        syncHistory: true,
      });

      const appMatchKey = normalizeAppMatchKey(activity.app_name);
      activities = activities.map((item) =>
        normalizeAppMatchKey(item.app_name) === appMatchKey
          ? { ...item, category: nextCategory }
          : item
      );

      if (selectedActivity && normalizeAppMatchKey(selectedActivity.app_name) === appMatchKey) {
        selectedActivity = { ...selectedActivity, category: nextCategory };
      }

      cache.invalidate('overview');

      showToast(
        t('timeline.categoryUpdated', {
          appName: activity.app_name,
          category: targetInfo.name,
          count: updatedCount,
        }),
        'success'
      );
    } catch (e) {
      console.error('修改应用默认分类失败:', e);
      showToast(
        t('timeline.categoryUpdateFailed', {
          appName: activity.app_name,
          error: e,
        }),
        'error'
      );
    } finally {
      categorySaving = false;
      await restoreCategoryTriggerAfterSaving();
    }
  }

  // 隐私规则快捷设置
  let privacySaving = false;
  let pendingPrivacyRule = null; // { level, levelLabel }

  function getCurrentPrivacyLevel(appName) {
    return selectedActivity?._privacyLevel || 'full';
  }

  function requestPrivacyRule(level) {
    if (!selectedActivity || privacySaving) return;
    if (getCurrentPrivacyLevel() === level) return;
    const levelLabels = {
      full: t('timeline.detail.privacyFull'),
      anonymized: t('timeline.detail.privacyAnonymized'),
      ignored: t('timeline.detail.privacyIgnored'),
    };
    pendingPrivacyRule = { level, levelLabel: levelLabels[level] };
  }

  function cancelPrivacyRule() { pendingPrivacyRule = null; }

  async function confirmPrivacyRule() {
    if (!pendingPrivacyRule || !selectedActivity) return;
    const { level } = pendingPrivacyRule;
    pendingPrivacyRule = null;
    privacySaving = true;
    try {
      const config = await invoke('get_config');
      if (!config.privacy) config.privacy = {};
      if (!config.privacy.app_rules) config.privacy.app_rules = [];

      if (level === 'full') {
        config.privacy.app_rules = config.privacy.app_rules.filter(
          r => r.app_name !== selectedActivity.app_name
        );
      } else {
        const idx = config.privacy.app_rules.findIndex(
          r => r.app_name === selectedActivity.app_name
        );
        if (idx >= 0) {
          config.privacy.app_rules[idx].level = level;
        } else {
          config.privacy.app_rules.push({ app_name: selectedActivity.app_name, level });
        }
      }

      await invoke('save_config', { config });

      selectedActivity = { ...selectedActivity, _privacyLevel: level };
      cache.invalidate('overview');

      const levelLabels = {
        full: t('timeline.detail.privacyFull'),
        anonymized: t('timeline.detail.privacyAnonymized'),
        ignored: t('timeline.detail.privacyIgnored'),
      };
      showToast(
        t('timeline.detail.privacySetSuccess', {
          appName: selectedActivity.app_name,
          level: levelLabels[level],
        }),
        'success'
      );

      if (level === 'ignored') {
        closeDetail();
        loadTimeline();
      }
    } catch (e) {
      console.error('设置记录策略失败:', e);
      showToast(
        t('timeline.detail.privacySetFailed', { error: e }),
        'error'
      );
    } finally {
      privacySaving = false;
    }
  }

  // 打开详情时加载当前隐私级别
  async function loadPrivacyLevel(activity) {
    try {
      const config = await invoke('get_config');
      const rules = config.privacy?.app_rules || [];
      const rule = rules.find(r => r.app_name === activity.app_name);
      return rule ? rule.level : 'full';
    } catch {
      return 'full';
    }
  }

  const CATEGORY_EMOJIS = [
    '💻', '🌐', '💬', '📝', '🎨', '🎮', '📁',
    '⚡', '📊', '🔧', '🛠️', '💡', '🎯', '📌',
    '🏷️', '🏠', '📚', '🎵', '📷', '🔬', '🧪',
    '💼', '🧑‍💻', '🧑‍🎨', '📱', '🚀', '⭐', '🔒',
  ];

  function getCategoryMeta(category) {
    return categoryStore.getCategoryMeta(category || 'other');
  }

  function getCategoryDisplayName(cat) {
    const translatedCategoryName = translateCategoryLabel(cat.key);
    const isKnownSystemCategory = cat.is_system || translatedCategoryName !== cat.key;
    return isKnownSystemCategory ? translatedCategoryName : (cat.name || translatedCategoryName);
  }

  function iconStyle(info) {
    // 通过 CSS 变量让明暗两套主题分别取不同透明度，避免暗色下出现近实心浅色块
    return `--icon-bg-light: ${hexToRGBA(info.color, 0.95)}; --icon-bg-dark: ${hexToRGBA(info.color, 0.3)}`;
  }

  async function createCustomCategory() {
    const name = newCategoryName.trim();
    if (!name) {
      showToast(t('timeline.categoryNameRequired'), 'error');
      return;
    }
    try {
      // 生成 key：只保留小写字母、数字、连字符；中文字符转为 hash 片段确保 key 非空且合法
      let key = name.toLowerCase().replace(/[^a-z0-9-]/g, '-').replace(/-+/g, '-').replace(/^-|-$/g, '');
      if (!key || key === '-') {
        let hash = 0;
        for (let i = 0; i < name.length; i++) {
          hash = ((hash << 5) - hash + name.charCodeAt(i)) | 0;
        }
        key = 'cat-' + Math.abs(hash).toString(36);
      }
      await invoke('save_custom_category', {
        key,
        name,
        color: newCategoryColor,
        icon: newCategoryIcon,
      });
      await categoryStore.refresh();
      showCreateCategory = false;
      newCategoryName = '';
      showToast(t('timeline.categoryCreated'), 'success');

      // 创建成功后弹窗确认是否应用到当前应用
      if (selectedActivity) {
        prepareCategoryConfirmation();
        pendingApplyCategory = { key, name };
      }
    } catch (e) {
      showToast(e.toString(), 'error');
    }
  }

  // 格式化时间
  function formatTime(timestamp) {
    return formatLocalizedTime(new Date(timestamp * 1000), {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
    });
  }

  // 格式化时长
  function formatDuration(seconds) {
    return formatDurationLocalized(seconds);
  }

  function formatTimelineAnchor(timestamp) {
    return formatLocalizedTime(new Date(timestamp * 1000), {
      hour: '2-digit',
      minute: '2-digit',
    });
  }

  function getTimelineIconSrc(activity) {
    const preferredAppName = getPreferredTimelineAppName(activity);
    const base64 = appIcons[getIconCacheKey({
      appName: activity.app_name,
      executablePath: activity.executable_path,
    })];

    if (shouldPreferTimelineFallbackIcon(activity)) {
      return resolveAppIconSrc(preferredAppName, null);
    }

    return resolveAppIconSrc(
      preferredAppName,
      base64
    );
  }

  function getTimelineTitle(activity) {
    return formatWindowTitle(activity.window_title, activity.app_name, activity.browser_url);
  }

  function getTimelineAppName(activity) {
    return getPreferredTimelineAppName(activity);
  }

  function getTimelineThumbnail(activity) {
    if (!activity?.screenshot_path) {
      return null;
    }
    return thumbnailCache[activity.screenshot_path] || null;
  }

  function normalizeAppMatchKey(appName) {
    return (appName || '').trim().toLowerCase();
  }

  // 优化窗口标题显示
  function formatWindowTitle(title, appName, browserUrl = null) {
    // 如果有有效标题
    if (title && title.trim() !== '') {
      // 移除常见的应用名称后缀
      let cleanTitle = title
        .replace(/ - Google Chrome$/i, '')
        .replace(/ - Chrome$/i, '')
        .replace(/ - Mozilla Firefox$/i, '')
        .replace(/ - Firefox$/i, '')
        .replace(/ - Safari$/i, '')
        .replace(/ - Microsoft Edge$/i, '')
        .replace(/ - Visual Studio Code$/i, '')
        .replace(/ · GitHub$/i, '')
        .replace(/ - YouTube$/i, '')
        .trim();
      
      // 如果标题太长，截断
      if (cleanTitle.length > 60) {
        cleanTitle = cleanTitle.substring(0, 57) + '...';
      }
      
      return cleanTitle || title;
    }
    
    // 无标题时，如果有 URL 显示域名
    if (browserUrl) {
      try {
        const url = new URL(formatBrowserUrlForDisplay(browserUrl));
        return url.hostname;
      } catch {
        return formatBrowserUrlForDisplay(browserUrl).substring(0, 40);
      }
    }
    
    // 完全无信息
    return t('timeline.inUse', { appName });
  }

  // 加载缩略图（列表用，400px），使用 LRU 缓存控制内存
  async function loadThumbnail(screenshotPath) {
    if (!screenshotPath) {
      return null;
    }
    if (thumbnailCache[screenshotPath]) {
      return thumbnailCache[screenshotPath];
    }
    try {
      const base64 = await invoke('get_screenshot_thumbnail', { path: screenshotPath });
      const dataUrl = `data:image/jpeg;base64,${base64}`;
      lruSet(thumbnailCache, thumbnailKeys, THUMBNAIL_CACHE_LIMIT, screenshotPath, dataUrl);
      thumbnailCache = { ...thumbnailCache };
      return dataUrl;
    } catch (e) {
      console.warn('加载缩略图失败:', e);
      return null;
    }
  }

  // 加载高分辨率图片（详情用，1200px），使用 LRU 缓存控制内存
  async function loadFullImage(screenshotPath) {
    if (!screenshotPath) {
      return null;
    }
    if (fullImageCache[screenshotPath]) {
      return fullImageCache[screenshotPath];
    }
    try {
      const base64 = await invoke('get_screenshot_full', { path: screenshotPath });
      const dataUrl = `data:image/jpeg;base64,${base64}`;
      lruSet(fullImageCache, fullImageKeys, FULLIMAGE_CACHE_LIMIT, screenshotPath, dataUrl);
      return dataUrl;
    } catch (e) {
      console.warn('加载高清图失败:', e);
      return await loadThumbnail(screenshotPath);
    }
  }

  async function preloadTimelineLeadThumbnails(items) {
    const leadItems = items
      .filter((activity) => activity?.screenshot_path)
      .slice(0, 6);

    await Promise.all(
      leadItems.map((activity) => loadThumbnail(activity.screenshot_path))
    );
  }

  const PAGE_SIZE = 12; // 每次加载 12 条 (3行 x 4列)
  const FEATURED_DURATION_THRESHOLD = 20 * 60;
  const FEATURED_CONTEXT_THRESHOLD = 10 * 60;
  const FEATURED_MIN_GAP = 2;
  const FEATURED_MAX_ITEMS = 4;
  let offset = 0;
  let hasMore = true;
  let loadingMore = false;

  function selectFeaturedActivityIds(items) {
    const featuredIds = [];
    const maxFeaturedCount = Math.min(FEATURED_MAX_ITEMS, Math.max(1, Math.ceil(items.length / 4)));
    let lastFeaturedIndex = -99;

    for (let index = 0; index < items.length; index += 1) {
      const activity = items[index];
      const previous = items[index - 1];

      if (!activity?.id || !activity.screenshot_path) {
        continue;
      }

      let score = 0;
      if ((activity.duration || 0) >= FEATURED_DURATION_THRESHOLD) {
        score += 3;
      } else if ((activity.duration || 0) >= FEATURED_CONTEXT_THRESHOLD) {
        score += 1;
      }
      if (activity.browser_url) {
        score += 1;
      }
      if (
        previous
        && (normalizeAppMatchKey(previous.app_name) !== normalizeAppMatchKey(activity.app_name)
          || (previous.category || 'other') !== (activity.category || 'other'))
      ) {
        score += 1;
      }
      if (index === 0) {
        score += 1;
      }
      if (score < 3 || index - lastFeaturedIndex < FEATURED_MIN_GAP) {
        continue;
      }

      featuredIds.push(activity.id);
      lastFeaturedIndex = index;

      if (featuredIds.length >= maxFeaturedCount) {
        break;
      }
    }

    if (featuredIds.length === 0) {
      const fallback = items.find((activity) => activity?.id && activity.screenshot_path);
      if (fallback) {
        featuredIds.push(fallback.id);
      }
    }

    return featuredIds;
  }

  let loadTimelineRequestId = 0;
  let loadMoreRequestId = 0;

  // 加载时间线数据（重置）
  async function loadTimeline() {
    // 禁用缓存：每次都从后端加载最新数据，确保数据一致性
    // 后端已实现 GROUP BY 聚合，无需前端缓存旧数据

    const requestId = ++loadTimelineRequestId;
    const requestDate = selectedDate;
    loadMoreRequestId += 1;
    loadingMore = false;

    // 2. 缓存未命中，请求后端
    loading = true;
    error = null;
    offset = 0;
    hasMore = true;
    // 日期切换时释放旧图片缓存，防止内存无限增长
    clearImageCaches();

    try {
      const [activitiesData, summariesData] = await Promise.all([
        invoke('get_timeline', { date: requestDate, limit: PAGE_SIZE, offset: 0 }),
        invoke('get_hourly_summaries', { date: requestDate }),
      ]);

      if (requestId !== loadTimelineRequestId || requestDate !== selectedDate) return;

      const preparedActivities = prepareTimelineActivities(activitiesData);
      await preloadTimelineLeadThumbnails(preparedActivities);
      if (requestId !== loadTimelineRequestId || requestDate !== selectedDate) return;

      activities = preparedActivities;

      hourlySummaries = summariesData;
      offset = activities.length;
      hasMore = activitiesData.length >= PAGE_SIZE;
      
      // 保存到缓存（直接使用后端返回结果）
      cache.setTimeline(requestDate, activities, summariesData);
      
      // 预加载缩略图
      activities.slice(6).forEach(a => loadThumbnail(a.screenshot_path));
      
      // 后台预加载前 6 张高清图（避免点击时等待）
      activities.slice(0, 6).forEach(a => loadFullImage(a.screenshot_path));
      
      // 预加载应用图标（获取唯一应用名并批量加载）
      const uniqueIconEntries = Array.from(
        new Map(
          activities.map((activity) => [
            getIconCacheKey({ appName: activity.app_name, executablePath: activity.executable_path }),
            { appName: activity.app_name, executablePath: activity.executable_path },
          ])
        ).values()
      );
      preloadAppIcons(uniqueIconEntries, invoke);
    } catch (e) {
      if (requestId !== loadTimelineRequestId || requestDate !== selectedDate) return;
      error = formatUserError(e, t('common.loadFailedRetry'));
      console.error('获取时间线失败:', e);
    } finally {
      if (requestId === loadTimelineRequestId && requestDate === selectedDate) {
        loading = false;
      }
    }
  }

  // 加载更多
  async function loadMore() {
    if (loadingMore || !hasMore) return;
    const requestId = ++loadMoreRequestId;
    const requestDate = selectedDate;
    const requestOffset = offset;
    loadingMore = true;

    try {
      const moreActivities = await invoke('get_timeline', { 
        date: requestDate,
        limit: PAGE_SIZE, 
        offset: requestOffset,
      });

      if (requestId !== loadMoreRequestId || requestDate !== selectedDate) return;

      if (moreActivities.length > 0) {
        const prepared = prepareTimelineActivities(moreActivities);
        // Deduplicate against existing activities (offset drift from real-time updates)
        const existingIds = new Set(activities.map(a => a.id));
        const newItems = prepared.filter(a => !existingIds.has(a.id));
        activities = [...activities, ...newItems];
        // Always increment by full fetched count to keep DB pagination in sync
        offset = requestOffset + moreActivities.length;
        // 预加载新图片
        moreActivities.forEach(a => loadThumbnail(a.screenshot_path));
        const iconEntries = Array.from(
          new Map(
            moreActivities.map((activity) => [
              getIconCacheKey({ appName: activity.app_name, executablePath: activity.executable_path }),
              { appName: activity.app_name, executablePath: activity.executable_path },
            ])
          ).values()
        );
        preloadAppIcons(iconEntries, invoke);
      }
      
      if (moreActivities.length < PAGE_SIZE) {
        hasMore = false;
      }
    } catch (e) {
      if (requestId !== loadMoreRequestId || requestDate !== selectedDate) return;
      console.error('加载更多失败:', e);
    } finally {
      if (requestId === loadMoreRequestId && requestDate === selectedDate) {
        loadingMore = false;
      }
    }
  }

  // 打开时段摘要抽屉，并静默刷新一次当前日期的数据。
  async function refreshHourlySummaries() {
    const requestId = ++summaryRefreshRequestId;
    const requestDate = selectedDate;
    summaryRefreshing = true;
    summaryRefreshError = null;

    try {
      const summariesData = await invoke('get_hourly_summaries', { date: requestDate });
      if (requestId !== summaryRefreshRequestId || requestDate !== selectedDate) return;
      hourlySummaries = summariesData;
    } catch (e) {
      if (requestId !== summaryRefreshRequestId || requestDate !== selectedDate) return;
      console.warn('刷新小时摘要失败:', e);
      summaryRefreshError = t('timelineSummary.refreshFailed');
    } finally {
      if (requestId === summaryRefreshRequestId && requestDate === selectedDate) {
        summaryRefreshing = false;
      }
    }
  }

  async function openSummaryDrawer() {
    await closeDetail(false);
    showSummaryDrawer = true;
    summaryRefreshError = null;
    void refreshHourlySummaries();
  }

  async function closeSummaryDrawer(restoreFocus = true) {
    showSummaryDrawer = false;
    summaryRefreshRequestId += 1;
    summaryRefreshing = false;
    summaryRefreshError = null;
    if (restoreFocus) {
      await tick();
      summaryTrigger?.focus();
    }
  }

  function updateCategoryPopoverPosition() {
    if (!showCategoryPopover || !categoryTrigger || typeof window === 'undefined') {
      categoryPopoverStyle = '';
      return;
    }

    const position = getViewportPopoverPlacement(categoryTrigger.getBoundingClientRect(), {
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
      preferredWidth: 352,
    });
    const verticalStyle = position.top === null
      ? `top: auto; bottom: ${position.bottom}px;`
      : `top: ${position.top}px; bottom: auto;`;
    categoryPopoverStyle = `left: ${position.left}px; width: ${position.width}px; max-height: ${position.maxHeight}px; ${verticalStyle}`;
  }

  async function closeCategoryPopover() {
    showCategoryPopover = false;
    categoryPopoverStyle = '';
    await tick();
    categoryTrigger?.focus();
  }

  async function toggleCategoryPopover() {
    if (showCategoryPopover) {
      await closeCategoryPopover();
      return;
    }

    showCategoryPopover = true;
    await tick();
    updateCategoryPopoverPosition();
    await tick();
    categoryPopover?.focus();
  }

  function handleCategoryPopoverKeydown(event) {
    if (!showCategoryPopover || event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    void closeCategoryPopover();
  }

  function cancelPendingAction() {
    if (pendingDeleteCategory) {
      cancelDeleteCategory();
    } else if (pendingApplyCategory) {
      cancelApplyCategory();
    } else if (pendingPrivacyRule) {
      cancelPrivacyRule();
    } else if (pendingChangeCategory) {
      cancelChangeCategory();
    }
  }

  function handleTimelineWindowKeydown(event) {
    if (
      event.key === 'Escape'
      && (pendingDeleteCategory || pendingApplyCategory || pendingPrivacyRule || pendingChangeCategory)
    ) {
      event.preventDefault();
      event.stopPropagation();
      cancelPendingAction();
      return;
    }

    handleCategoryPopoverKeydown(event);
  }

  function handleDetailDismiss() {
    if (showCategoryPopover) {
      void closeCategoryPopover();
      return;
    }
    void closeDetail();
  }

  function handleDetailOverlayKeydown(event) {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    handleDetailDismiss();
  }

  function handleDetailScroll() {
    if (showCategoryPopover) updateCategoryPopoverPosition();
  }

  // 查看活动详情
  let viewActivityRequestId = 0;
  async function viewActivity(activity, trigger = null) {
    await closeSummaryDrawer(false);
    detailTrigger = trigger;
    showCategoryPopover = false;
    const requestId = ++viewActivityRequestId;
    const previewThumbnail = getTimelineThumbnail(activity);
    selectedActivity = {
      ...activity,
      thumbnail: getTimelineThumbnail(activity),
      thumbnailLoading: !!activity.screenshot_path,
    };
    await tick();
    detailCloseButton?.focus();

    const freshActivityPromise = activity.id
      ? invoke('get_activity', { id: activity.id }).catch((e) => {
          console.warn('获取最新活动数据失败:', e);
          return null;
        })
      : Promise.resolve(null);
    const fullImagePromise = activity.screenshot_path
      ? loadFullImage(activity.screenshot_path)
      : Promise.resolve(previewThumbnail);

    const [freshActivity, thumbnail, privacyLevel] = await Promise.all([freshActivityPromise, fullImagePromise, loadPrivacyLevel(activity)]);
    if (requestId !== viewActivityRequestId) return;

    const resolvedActivity = freshActivity || activity;

    selectedActivity = {
      ...resolvedActivity,
      thumbnail: thumbnail || previewThumbnail,
      thumbnailLoading: false,
      _privacyLevel: privacyLevel,
    };
  }

  // 打开外部链接
  async function openUrl(url) {
    if (url) {
      try {
        await open(url);
      } catch (e) {
        console.error('打开链接失败:', e);
      }
    }
  }

  let exportingTimeline = false;

  // 导出当前日期的时间线为 JSON
  // OCR 文本可能含屏幕内容，弹一个确认让用户选择是否一并导出
  async function exportTimelineJson() {
    if (exportingTimeline) return;
    if (!activities.length) {
      showToast(t('timeline.exportNothing'), 'error');
      return;
    }

    const includeOcr = await ask(t('timeline.exportIncludeOcrMessage'), {
      title: t('timeline.exportIncludeOcrTitle'),
      kind: 'info',
      okLabel: t('timeline.exportIncludeOcrYes'),
      cancelLabel: t('timeline.exportIncludeOcrNo'),
    });

    const targetPath = await saveDialog({
      defaultPath: `timeline-${selectedDate}.json`,
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (!targetPath) return;

    exportingTimeline = true;
    try {
      const savedPath = await invoke('export_timeline_json', {
        date: selectedDate,
        targetPath,
        includeOcr,
      });
      showToast(t('timeline.exportSuccess', { path: savedPath }), 'success');
    } catch (e) {
      showToast(t('timeline.exportFailed', { error: e }), 'error');
    } finally {
      exportingTimeline = false;
    }
  }

  // 删除自定义分类
  let pendingDeleteCategory = null; // { key, name }
  function cancelDeleteCategory() { pendingDeleteCategory = null; }
  async function confirmDeleteCategory() {
    if (!pendingDeleteCategory) return;
    const { key, name } = pendingDeleteCategory;
    pendingDeleteCategory = null;
    categorySaving = true;
    try {
      const affected = await invoke('delete_custom_category', { key });
      await categoryStore.refresh();
      cache.invalidate('overview');

      // 如果当前选中的应用使用了被删除的分类，更新本地状态
      if (selectedActivity && (selectedActivity.category || 'other') === key) {
        selectedActivity = { ...selectedActivity, category: 'other' };
      }
      // 后端已把历史记录统一改回退分类，这里同步所有本地行（不只限当前应用）
      activities = activities.map((item) =>
        (item.category || 'other') === key ? { ...item, category: 'other' } : item
      );

      showToast(
        t('timeline.categoryDeleted', { category: name, count: affected }),
        'success'
      );
    } catch (e) {
      showToast(e.toString(), 'error');
    } finally {
      categorySaving = false;
      await restoreCategoryTriggerAfterSaving();
    }
  }

  // 关闭详情并把焦点交还给打开详情的时间线记录。
  async function closeDetail(restoreFocus = true) {
    viewActivityRequestId += 1;
    selectedActivity = null;
    categorySaving = false;
    showCategoryPopover = false;
    categoryPopoverStyle = '';
    showCreateCategory = false;
    showRenameCategory = false;
    pendingChangeCategory = null;
    pendingApplyCategory = null;
    pendingDeleteCategory = null;
    if (restoreFocus) {
      await tick();
      detailTrigger?.focus();
    }
    detailTrigger = null;
  }

  // 删除单条活动记录（连带截图）
  async function deleteActivity(activity) {
    if (!activity?.id) return;
    const ok = await confirm({
      tone: 'warning',
      title: t('timeline.deleteActivityTitle'),
      message: t('timeline.deleteActivityMessage', {
        appName: getPreferredTimelineAppName(activity) || activity.app_name,
        time: formatTimelineAnchor(activity.timestamp),
      }),
      confirmText: t('timeline.confirmDelete'),
      cancelText: t('timeline.cancel'),
    });
    if (!ok) return;
    try {
      await invoke('delete_activity', { id: activity.id });
      closeDetail();
      cache.invalidate('overview');
      await loadTimeline();
      showToast(t('timeline.activityDeleted'), 'success');
    } catch (e) {
      showToast(e.toString(), 'error');
    }
  }

  // ===== 批量清理记录（日期 / 时间段 / 应用）=====
  let showCleanupPanel = false;
  let cleanupMode = 'date'; // 'date' | 'range' | 'app'
  let cleanupRangeStart = '';
  let cleanupRangeEnd = '';
  let cleanupRangeStartTime = '';
  let cleanupRangeEndTime = '';
  let cleanupApp = '';
  let cleanupBusy = false;

  // 从已加载活动提取候选应用名（去重排序）
  $: cleanupAppCandidates = Array.from(
    new Set(activities.map((a) => getPreferredTimelineAppName(a) || a.app_name)),
  )
    .filter(Boolean)
    .sort();

  // 本地时区的“日期 + 可选时刻”→ Unix 秒
  function localDateToTs(dateStr, timeStr) {
    if (!dateStr) return 0;
    const [y, m, d] = dateStr.split('-').map(Number);
    const hh = timeStr ? Number(timeStr.split(':')[0]) : 0;
    const mm = timeStr ? Number(timeStr.split(':')[1]) : 0;
    return Math.floor(new Date(y, m - 1, d, hh, mm, 0).getTime() / 1000);
  }

  async function doCleanupByDate() {
    if (!selectedDate || cleanupBusy) return;
    const ok = await confirm({
      tone: 'warning',
      title: t('timeline.deleteByDateTitle'),
      message: t('timeline.deleteByDateMessage', { date: selectedDate }),
      confirmText: t('timeline.confirmDelete'),
      cancelText: t('timeline.cancel'),
    });
    if (!ok) return;
    cleanupBusy = true;
    try {
      const res = await invoke('delete_activities_by_date', { date: selectedDate });
      cache.invalidate('overview');
      await loadTimeline();
      showToast(
        t('timeline.deletedByDate', { count: res?.deleted ?? 0, date: selectedDate }),
        'success',
      );
    } catch (e) {
      showToast(e.toString(), 'error');
    } finally {
      cleanupBusy = false;
    }
  }

  async function doCleanupByRange() {
    if (cleanupBusy) return;
    if (!cleanupRangeStart || !cleanupRangeEnd) {
      showToast(t('timeline.noActivitiesToDelete'), 'error');
      return;
    }
    const startTs = localDateToTs(cleanupRangeStart, cleanupRangeStartTime);
    const endBase = localDateToTs(cleanupRangeEnd, cleanupRangeEndTime);
    const endTs = cleanupRangeEndTime ? endBase + 59 : endBase + 86399;
    if (endTs <= startTs) {
      showToast(t('timeline.noActivitiesToDelete'), 'error');
      return;
    }
    const ok = await confirm({
      tone: 'warning',
      title: t('timeline.deleteByRangeTitle'),
      message: t('timeline.deleteByRangeMessage', {
        start: `${cleanupRangeStart}${cleanupRangeStartTime ? ' ' + cleanupRangeStartTime : ''}`,
        end: `${cleanupRangeEnd}${cleanupRangeEndTime ? ' ' + cleanupRangeEndTime : ''}`,
      }),
      confirmText: t('timeline.confirmDelete'),
      cancelText: t('timeline.cancel'),
    });
    if (!ok) return;
    cleanupBusy = true;
    try {
      const res = await invoke('delete_activities_by_range', { startTs, endTs });
      cache.invalidate('overview');
      await loadTimeline();
      showToast(t('timeline.deletedByRange', { count: res?.deleted ?? 0 }), 'success');
    } catch (e) {
      showToast(e.toString(), 'error');
    } finally {
      cleanupBusy = false;
    }
  }

  async function doCleanupByApp() {
    if (cleanupBusy || !cleanupApp) return;
    const ok = await confirm({
      tone: 'warning',
      title: t('timeline.deleteByAppTitle'),
      message: t('timeline.deleteByAppMessage', { appName: cleanupApp }),
      confirmText: t('timeline.confirmDelete'),
      cancelText: t('timeline.cancel'),
    });
    if (!ok) return;
    cleanupBusy = true;
    try {
      const res = await invoke('delete_activities_by_app', { appName: cleanupApp });
      cache.invalidate('overview');
      await loadTimeline();
      showToast(
        t('timeline.deletedByApp', { count: res?.deleted ?? 0, appName: cleanupApp }),
        'success',
      );
    } catch (e) {
      showToast(e.toString(), 'error');
    } finally {
      cleanupBusy = false;
    }
  }

  // 记录上次加载的日期
  let lastLoadedDate = null;
  let featuredActivityIds = new Set();

  // 日期变化时重新加载，同时让旧日期的静默摘要请求立即失效。
  $: if (selectedDate && selectedDate !== lastLoadedDate) {
    lastLoadedDate = selectedDate;
    summaryRefreshRequestId += 1;
    summaryRefreshing = false;
    summaryRefreshError = null;
    loadTimeline();
  }

  $: featuredActivityIds = new Set(selectFeaturedActivityIds(activities));

  // 检查是否是今天
  $: isToday = selectedDate === getLocalDateString();
  // 圆点绿+脉冲还需要"正在录制"：停止记录后圆点应变灰（issue #131）
  $: recordingState = $recordingStore;
  $: timelineDotActive = isToday && isActiveRecording(recordingState);

  onMount(async () => {
    const requestedDate = readRequestedTimelineDate();
    if (requestedDate) {
      selectedDate = requestedDate;
    }
    if (readRequestedSummaryOpen()) {
      showSummaryDrawer = true;
      consumeRequestedSummaryOpen();
    }

    handleTimelineFocus = (event) => applyTimelineFocus(event.detail);
    window.addEventListener('timeline-focus-date', handleTimelineFocus);
    categoryStore.refresh();

    if (!document.hidden) {
      clockInterval = setInterval(() => {
        currentTime = new Date();
      }, 1000);
    }

    handleVisibilityChange = () => {
      if (document.hidden) {
        clearInterval(clockInterval);
        clockInterval = null;
      } else {
        currentTime = new Date();
        // 防御性清理：visible 路径前若残留 interval 也先清掉，避免双倍触发
        if (clockInterval) clearInterval(clockInterval);
        clockInterval = setInterval(() => {
          currentTime = new Date();
        }, 1000);
        if (isToday) {
          loadTimeline();
        }
      }
    };
    document.addEventListener('visibilitychange', handleVisibilityChange);
    
    // 初始加载通过响应式触发
    
    // 监听新截屏事件，智能更新（合并或新增）
    // 核心逻辑：后端已完成聚合，前端只按 id 替换，否则视作新活动插入
    try {
      const un = await listen('screenshot-taken', (event) => {
        if (isToday && !document.hidden) {
          const newActivity = event.payload;
          if (newActivity?.screenshot_path) {
            loadThumbnail(newActivity.screenshot_path);
          }
          activities = upsertTimelineActivity(activities, newActivity);
          cache.invalidate('overview');
        }
      });
      // 组件可能在 await 期间已销毁，避免监听器泄漏
      if (componentDestroyed) {
        un();
      } else {
        unlisten = un;
      }
    } catch (e) {
      console.warn('注册 screenshot-taken 监听失败:', e);
    }

  });

  onDestroy(() => {
    componentDestroyed = true;
    if (unlisten) unlisten();
    if (clockInterval) clearInterval(clockInterval);
    if (handleVisibilityChange) document.removeEventListener('visibilitychange', handleVisibilityChange);
    if (handleTimelineFocus) window.removeEventListener('timeline-focus-date', handleTimelineFocus);
    unsubIcons();
  });
</script>

<svelte:window on:resize={handleDetailScroll} on:keydown={handleTimelineWindowKeydown} />

<div class="page-shell" data-locale={currentLocale}>
  <!-- 页面标题 -->
  <div class="page-header">
    <div class="page-title-group">
      <div class="page-title-badge">
        <svg fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="M5 7h14M5 12h9M5 17h14" />
          <circle cx="17" cy="12" r="2.5" stroke-width="1.8" />
        </svg>
      </div>
      <div class="page-title-copy">
        <h2>{t('timeline.title')}</h2>
        <p>
        {t('timeline.subtitle')}
        {#if isToday}
          <span class="ms-1.5 inline-flex items-center gap-1.5">
            <span class="w-1.5 h-1.5 rounded-full {timelineDotActive ? 'bg-emerald-500 animate-pulse' : 'bg-slate-400 dark:bg-[#484f58]'}"></span>
            <span class="font-mono text-xs text-emerald-600 dark:text-emerald-400">{formatLocalizedTime(currentTime, { hour: '2-digit', minute: '2-digit' })}</span>
          </span>
        {/if}
        </p>
      </div>
    </div>
    <div class="page-toolbar">
      {#key `timeline-date-${currentLocale}`}
        <LocalizedDatePicker
          bind:value={selectedDate}
          localeCode={currentLocale}
          triggerClass="page-control-input w-auto"
        />
      {/key}
      <button
        class="page-control-btn-icon text-rose-500 hover:text-rose-600 dark:text-rose-400"
        on:click={() => (showCleanupPanel = true)}
        title={t('timeline.cleanupRecords')}
      >
        <svg class="timeline-toolbar-icon h-[1.125rem] w-[1.125rem]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6M1 7h22M9 7V4a1 1 0 011-1h4a1 1 0 011 1v3" />
        </svg>
      </button>
      <button class="page-control-btn-icon" on:click={loadTimeline} title={t('timeline.refreshTitle')}>
        <svg class="timeline-toolbar-icon h-[1.125rem] w-[1.125rem] text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
      </button>
      <button
        class="page-control-btn-icon"
        on:click={exportTimelineJson}
        disabled={exportingTimeline || !activities.length}
        title={t('timeline.exportTitle')}
      >
        {#if exportingTimeline}
          <div class="timeline-toolbar-icon h-[1.125rem] w-[1.125rem] animate-spin rounded-full border-2 border-current border-t-transparent"></div>
        {:else}
          <svg class="timeline-toolbar-icon h-[1.125rem] w-[1.125rem] text-slate-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 10v6m0 0l-3-3m3 3l3-3M5 20h14a2 2 0 002-2V9a2 2 0 00-2-2h-4l-2-2H5a2 2 0 00-2 2v11a2 2 0 002 2z" />
          </svg>
        {/if}
      </button>
    </div>
  </div>

  {#if loading}
    <div class="flex items-center justify-center h-64">
      <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-500"></div>
    </div>
  {:else if error}
    <div class="page-banner-error">
      <div>
        <p class="font-semibold">{t('timeline.loadError')}</p>
        <p class="text-sm mt-1">{error}</p>
      </div>
      <button class="page-action-brand" on:click={loadTimeline}>{t('timeline.retry')}</button>
    </div>
  {:else if activities.length === 0}
    <div class="empty-state-lg">
      <div class="empty-state-icon">
        <span class="text-2xl">📝</span>
      </div>
      <p class="empty-state-copy">{t('timeline.empty')}</p>
    </div>
  {:else}
    <div class="page-card timeline-editorial-board overflow-hidden p-0">
      <div class="timeline-summary-strip">
        <div class="timeline-summary-copy">
          <span>{t('timeline.recordSummary', { dateLabel: isToday ? t('timeline.todayLabel') : selectedDate, count: activities.length })}</span>
          <span class="timeline-summary-divider">|</span>
          <span>00:00 - {activities[0] ? formatTime(activities[0].timestamp) : '--:--'}</span>
        </div>

        <button
          bind:this={summaryTrigger}
          type="button"
          class="page-control-btn timeline-summary-action"
          aria-haspopup="dialog"
          aria-expanded={showSummaryDrawer}
          on:click={openSummaryDrawer}
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 19v-6a2 2 0 00-2-2H5a2 2 0 00-2 2v6a2 2 0 002 2h2a2 2 0 002-2zm0 0V9a2 2 0 012-2h2a2 2 0 012 2v10m-6 0a2 2 0 002 2h2a2 2 0 002-2m0 0V5a2 2 0 012-2h2a2 2 0 012 2v14a2 2 0 01-2 2h-2a2 2 0 01-2-2z" />
          </svg>
          {t('timeline.periodSummary')}
          {#if hourlySummaries.length > 0}
            <span class="px-1.5 py-0.5 text-xs bg-primary-100 dark:bg-primary-900/30 text-primary-600 dark:text-primary-400 rounded-full">{hourlySummaries.length}</span>
          {/if}
          <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
          </svg>
        </button>
      </div>

      <!-- 时间线列表 -->
      <div class="timeline-editorial-shell">
        <div class="timeline-rail" aria-hidden="true"></div>
        {#each activities as activity, i}
          {@const info = getCategoryMeta(activity.category)}
          {@const featured = featuredActivityIds.has(activity.id)}
          {@const timelineTitle = getTimelineTitle(activity)}
          <button
            class={`timeline-entry ${featured ? 'timeline-entry-featured' : 'timeline-entry-compact'}`}
            on:click={(event) => viewActivity(activity, event.currentTarget)}
          >
            <div class="timeline-entry-anchor">
              <div class="timeline-entry-time">{formatTimelineAnchor(activity.timestamp)}</div>
              <div class={`timeline-entry-marker ${featured ? 'timeline-entry-marker-featured' : ''}`}></div>
            </div>

            {#if featured}
              <div class="timeline-entry-card timeline-entry-card-featured">
                <div class="timeline-featured-media">
                  {#if getTimelineThumbnail(activity)}
                    <img
                      src={getTimelineThumbnail(activity)}
                      alt={t('timeline.detail.screenshotAlt')}
                      class="timeline-featured-image"
                    />
                  {:else}
                    <div class="timeline-featured-image timeline-featured-image-placeholder">
                      <div class="timeline-featured-image-glow"></div>
                    </div>
                  {/if}
                </div>
                <div class="timeline-featured-copy">
                  <div class="timeline-entry-meta timeline-entry-meta-featured">
                    <div class="timeline-entry-app">
                      <div class="timeline-app-icon"
                           style={iconStyle(info)}>
                        {#if getTimelineIconSrc(activity)}
                          <img src={getTimelineIconSrc(activity)}
                               alt={activity.app_name}
                               class="timeline-app-icon-image app-icon object-cover" />
                        {:else}
                          <span>{info.icon}</span>
                        {/if}
                      </div>
                      <div class="timeline-entry-heading timeline-entry-heading-featured">
                        <span class="timeline-entry-app-name">{getTimelineAppName(activity)}</span>
                        <span class="timeline-entry-category timeline-entry-category-pill">{info.name}</span>
                      </div>
                    </div>
                    <div class="timeline-entry-duration-chip">{formatDuration(activity.duration)}</div>
                  </div>
                  <p class="timeline-entry-title timeline-entry-title-featured" title={activity.window_title}>
                    {timelineTitle}
                  </p>
                  {#if activity.browser_url}
                    <p class="timeline-entry-url">{formatBrowserUrlForDisplay(activity.browser_url)}</p>
                  {/if}
                </div>
              </div>
            {:else}
              <div class="timeline-entry-card timeline-entry-card-compact timeline-entry-card-compact-grid">
                <div class="timeline-entry-app timeline-entry-app-compact">
                  <div class="timeline-app-icon"
                       style={iconStyle(info)}>
                    {#if getTimelineIconSrc(activity)}
                      <img src={getTimelineIconSrc(activity)}
                           alt={activity.app_name}
                           class="timeline-app-icon-image app-icon object-cover" />
                    {:else}
                      <span>{info.icon}</span>
                    {/if}
                  </div>
                  <div class="timeline-entry-heading">
                    <span class="timeline-entry-app-name">{getTimelineAppName(activity)}</span>
                    <span class="timeline-entry-category timeline-entry-category-pill">{info.name}</span>
                  </div>
                </div>
                <p class="timeline-entry-title timeline-entry-title-compact" title={activity.window_title}>
                  {timelineTitle}
                </p>
                <div class="timeline-entry-tail timeline-entry-tail-compact">
                  <span class="timeline-entry-duration">{formatDuration(activity.duration)}</span>
                  <svg class="timeline-entry-arrow" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
                  </svg>
                </div>
              </div>
            {/if}
          </button>
        {/each}
      </div>

      <!-- 加载更多按钮 -->
      {#if hasMore}
        <div class="timeline-load-more">
          <button
            on:click={loadMore}
            disabled={loadingMore}
            class="timeline-load-more-btn"
          >
            {#if loadingMore}
              <div class="animate-spin rounded-full h-4 w-4 border-b-2 border-slate-500"></div>
              {t('timeline.loadingMore')}
            {:else}
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
              </svg>
              {t('timeline.loadMore')}
            {/if}
          </button>
        </div>
      {:else if activities.length > 0}
        <div class="timeline-load-more timeline-load-more-end">
          {t('timeline.noMore')}
        </div>
      {/if}
    </div>
  {/if}
</div>

<HourlySummaryDrawer
  open={showSummaryDrawer}
  date={selectedDate}
  summaries={hourlySummaries}
  loading={loading}
  refreshing={summaryRefreshing}
  error={summaryRefreshError}
  on:close={() => closeSummaryDrawer()}
/>

<!-- 活动详情右侧抽屉 -->
{#if selectedActivity}
  {@const info = getCategoryMeta(selectedActivity.category)}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="timeline-detail-overlay fixed inset-0 z-[140] bg-slate-950/52 backdrop-blur-md flex items-center justify-end p-4 animate-fadeIn"
    role="presentation"
    on:click|self={handleDetailDismiss}
    on:keydown={handleDetailOverlayKeydown}
  >
    <aside
      class="timeline-detail-drawer"
      use:trapFocus
      role="dialog"
      aria-modal="true"
      aria-labelledby="timeline-detail-title"
      on:scroll={handleDetailScroll}
    >
      <!-- 头部 -->
      <div class="timeline-detail-header">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-3">
            <div class="timeline-app-icon timeline-app-icon-lg"
                 style={iconStyle(info)}>
              {#if getTimelineIconSrc(selectedActivity)}
                <img src={getTimelineIconSrc(selectedActivity)}
                     alt={selectedActivity.app_name}
                     class="timeline-app-icon-image timeline-app-icon-image-lg app-icon object-cover" />
              {:else}
                {info.icon}
              {/if}
            </div>
            <div>
              <h3 id="timeline-detail-title" class="text-lg font-semibold text-slate-900 dark:text-[#e6edf3]">{getTimelineAppName(selectedActivity)}</h3>
              <p class="text-sm text-slate-500 dark:text-[#7d8590]">{info.name}</p>
            </div>
          </div>
          <div class="flex items-center gap-1">
            <button
              class="btn btn-ghost text-rose-500 hover:text-rose-600 dark:text-rose-400"
              title={t('timeline.deleteActivity')}
              on:click={() => deleteActivity(selectedActivity)}
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6M1 7h22M9 7V4a1 1 0 011-1h4a1 1 0 011 1v3" />
              </svg>
            </button>
            <button bind:this={detailCloseButton} class="btn btn-ghost" aria-label={t('window.close')} on:click={() => closeDetail()}>
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>
      </div>

      <!-- 内容 -->
      <div class="timeline-detail-body">
        <section class="timeline-detail-hero" aria-label={t('timeline.detail.recordTime')}>
          <div class="timeline-detail-hero-item">
            <span>{t('timeline.detail.recordTime')}</span>
            <strong>{formatTime(selectedActivity.timestamp)}</strong>
          </div>
          <span class="timeline-detail-hero-divider" aria-hidden="true"></span>
          <div class="timeline-detail-hero-item">
            <span>{t('timeline.detail.duration')}</span>
            <strong>{formatDuration(selectedActivity.duration)}</strong>
          </div>
        </section>

        <section class="timeline-detail-preview">
          <div class="timeline-detail-section-heading">
            <span>{t('timeline.detail.screenshot')}</span>
          </div>
          <div class="timeline-detail-preview-frame">
            {#if selectedActivity.thumbnail}
              <img src={selectedActivity.thumbnail} alt={t('timeline.detail.screenshotAlt')} class="timeline-detail-preview-image" />
              {#if selectedActivity.thumbnailLoading}
                <span class="timeline-detail-preview-loading-indicator" aria-hidden="true">
                  <span class="animate-spin rounded-full h-3.5 w-3.5 border-b-2 border-primary-500"></span>
                </span>
              {/if}
            {:else if selectedActivity.thumbnailLoading}
              <div class="timeline-detail-preview-state">
                <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-primary-500"></div>
              </div>
            {:else if selectedActivity.screenshot_path}
              <div class="timeline-detail-preview-state text-slate-400 dark:text-[#7d8590]">
                <span>{t('timeline.detail.screenshotLoadFailed')}</span>
              </div>
            {:else}
              <div class="timeline-detail-preview-state text-slate-400 dark:text-[#7d8590]">
                <span>{t('timeline.detail.screenshotMissing')}</span>
              </div>
            {/if}
          </div>
        </section>

        <section class="timeline-detail-meta">
          <div class="timeline-detail-meta-row">
            <span>{t('timeline.detail.windowTitle')}</span>
            <p>{selectedActivity.window_title || t('timeline.noTitle')}</p>
          </div>
          {#if selectedActivity.browser_url}
            <div class="timeline-detail-meta-row">
              <span>{t('timeline.detail.visitedUrl')}</span>
              <button
                on:click={() => openUrl(selectedActivity.browser_url)}
                class="timeline-detail-url"
              >
                {formatBrowserUrlForDisplay(selectedActivity.browser_url)}
              </button>
            </div>
          {/if}
        </section>

        <section class="timeline-detail-settings">
        <div class="timeline-category-section timeline-detail-setting-row">
          <div class="flex items-center justify-between gap-3">
            <div>
              <span class="text-sm font-medium text-slate-500 dark:text-[#7d8590]">{t('timeline.detail.appCategory')}</span>
              <p class="mt-1 text-xs text-slate-500 dark:text-[#7d8590]">
                {t('timeline.detail.appCategoryHelp')}
              </p>
            </div>
            {#if categorySaving}
              <span class="text-xs text-slate-400 dark:text-[#7d8590]">{t('timeline.detail.saving')}</span>
            {/if}
          </div>

          <div class="timeline-category-control">
            <button
              bind:this={categoryTrigger}
              type="button"
              class="timeline-category-trigger"
              aria-haspopup="dialog"
              aria-expanded={showCategoryPopover}
              disabled={categorySaving}
              on:click={toggleCategoryPopover}
            >
              <span class="timeline-category-dot" style={`background-color: ${info.color}`}></span>
              <span>{info.name}</span>
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
              </svg>
            </button>

            {#if showCategoryPopover}
              <div
                bind:this={categoryPopover}
                class="timeline-category-popover"
                role="dialog"
                tabindex="-1"
                aria-label={t('timeline.detail.appCategory')}
                style={categoryPopoverStyle}
              >
                <div class="timeline-category-options">
                  {#each $categoryStore as cat}
                    <div class="timeline-category-option-row">
                      <button
                        type="button"
                        class="timeline-category-option"
                        class:timeline-category-option-active={(selectedActivity.category || 'other') === cat.key}
                        aria-pressed={(selectedActivity.category || 'other') === cat.key}
                        disabled={categorySaving}
                        on:click={() => selectActivityCategory(cat.key)}
                      >
                        <span class="timeline-category-dot" style={`background-color: ${cat.color}`}></span>
                        <span class="timeline-category-option-name">{getCategoryDisplayName(cat)}</span>
                        {#if (selectedActivity.category || 'other') === cat.key}
                          <span class="timeline-category-check" aria-hidden="true">✓</span>
                        {/if}
                      </button>
                      {#if !cat.is_system}
                        <div class="timeline-category-option-actions">
                          <button
                            type="button"
                            disabled={categorySaving}
                            title={t('timeline.renameCategory')}
                            aria-label={t('timeline.renameCategory')}
                            on:click={() => {
                              showCreateCategory = false;
                              startRenameCategory(cat);
                            }}
                          >
                            <span aria-hidden="true">✎</span>
                          </button>
                          <button
                            type="button"
                            disabled={categorySaving}
                            title={t('timeline.deleteCategory')}
                            aria-label={t('timeline.deleteCategory')}
                            on:click={() => {
                              prepareCategoryConfirmation();
                              pendingDeleteCategory = { key: cat.key, name: getCategoryDisplayName(cat) };
                            }}
                          >
                            <span aria-hidden="true">×</span>
                          </button>
                        </div>
                      {/if}
                    </div>
                  {/each}
                </div>

                <button
                  type="button"
                  class="timeline-category-create-trigger"
                  disabled={categorySaving}
                  on:click={() => {
                    showRenameCategory = false;
                    showCreateCategory = !showCreateCategory;
                  }}
                >
                  <span aria-hidden="true">{showCreateCategory ? '×' : '+'}</span>
                  <span>{t('timeline.createCategory')}</span>
                </button>

                {#if showCreateCategory}
                  <div class="timeline-category-editor">
                    <p>{t('timeline.createCategoryHint')}</p>
                    <div class="timeline-category-editor-fields">
                      <input
                        type="text"
                        bind:value={newCategoryName}
                        placeholder={t('timeline.categoryNamePlaceholder')}
                      />
                      <input type="color" bind:value={newCategoryColor} aria-label={t('timeline.detail.appCategory')} />
                      <span>{newCategoryIcon}</span>
                    </div>
                    <div class="timeline-category-emoji-grid">
                      {#each CATEGORY_EMOJIS as emoji}
                        <button
                          type="button"
                          class:timeline-category-emoji-active={newCategoryIcon === emoji}
                          on:click={() => newCategoryIcon = emoji}
                        >{emoji}</button>
                      {/each}
                    </div>
                    <div class="timeline-category-editor-actions">
                      <button type="button" on:click={() => showCreateCategory = false}>{t('timeline.cancel')}</button>
                      <button type="button" class="timeline-category-editor-primary" on:click={createCustomCategory}>{t('timeline.confirmChange')}</button>
                    </div>
                  </div>
                {/if}

                {#if showRenameCategory}
                  <div class="timeline-category-editor">
                    <p>{t('timeline.renameCategory')}</p>
                    <div class="timeline-category-editor-fields">
                      <input
                        type="text"
                        bind:value={renameCategoryName}
                        placeholder={t('timeline.categoryNamePlaceholder')}
                      />
                      <input type="color" bind:value={renameCategoryColor} aria-label={t('timeline.detail.appCategory')} />
                      <span>{renameCategoryIcon}</span>
                    </div>
                    <div class="timeline-category-emoji-grid">
                      {#each CATEGORY_EMOJIS as emoji}
                        <button
                          type="button"
                          class:timeline-category-emoji-active={renameCategoryIcon === emoji}
                          on:click={() => renameCategoryIcon = emoji}
                        >{emoji}</button>
                      {/each}
                    </div>
                    <div class="timeline-category-editor-actions">
                      <button type="button" on:click={() => showRenameCategory = false}>{t('timeline.cancel')}</button>
                      <button type="button" class="timeline-category-editor-primary" on:click={saveRenameCategory}>{t('timeline.confirmChange')}</button>
                    </div>
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        </div>

        <!-- 记录策略快捷设置 -->
        <div class="timeline-detail-setting-row">
          <div class="flex items-center justify-between gap-3">
            <div>
              <span class="text-sm font-medium text-slate-500 dark:text-[#7d8590]">{t('timeline.detail.privacyRule')}</span>
              <p class="mt-1 text-xs text-slate-500 dark:text-[#7d8590]">
                {t('timeline.detail.privacyRuleHelp')}
              </p>
            </div>
            {#if privacySaving}
              <span class="text-xs text-slate-400 dark:text-[#7d8590]">{t('timeline.detail.saving')}</span>
            {/if}
          </div>
          <div class="mt-3 flex gap-2">
            {#each [
              { value: 'full', label: t('timeline.detail.privacyFull'), activeClass: 'settings-segment-success' },
              { value: 'anonymized', label: t('timeline.detail.privacyAnonymized'), activeClass: 'settings-segment-warn' },
              { value: 'ignored', label: t('timeline.detail.privacyIgnored'), activeClass: 'settings-segment-danger' },
            ] as opt}
              <button
                on:click={() => requestPrivacyRule(opt.value)}
                class="segment-btn flex-1 text-center border border-slate-200 dark:border-[#484f58] rounded-lg {(selectedActivity._privacyLevel || 'full') === opt.value ? opt.activeClass : 'settings-segment-idle'}"
                disabled={privacySaving}
              >
                {opt.label}
              </button>
            {/each}
          </div>
          <p class="text-xs mt-1.5 {[
            { full: 'settings-text-success', anonymized: 'settings-text-warn', ignored: 'settings-text-danger' }
          ][0][(selectedActivity._privacyLevel || 'full')] || 'settings-subtle'}">
            {{
              full: t('settingsPrivacy.fullDesc'),
              anonymized: t('settingsPrivacy.anonymizedDesc'),
              ignored: t('settingsPrivacy.ignoredDesc'),
            }[(selectedActivity._privacyLevel || 'full')] || ''}
          </p>
        </div>
        </section>
      </div>
    </aside>
  </div>
{/if}

<!-- 批量清理记录面板（z-index 高于详情弹窗） -->
{#if showCleanupPanel}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="fixed inset-0 z-[150] bg-slate-950/40 backdrop-blur-sm flex items-center justify-center animate-fadeIn p-4"
    role="button"
    tabindex="0"
    on:click|self={() => !cleanupBusy && (showCleanupPanel = false)}
    on:keydown={(e) => e.key === 'Escape' && !cleanupBusy && (showCleanupPanel = false)}
  >
    <div class="w-full max-w-lg rounded-2xl border border-slate-200 dark:border-[#30363d] bg-white dark:bg-[#161b22] shadow-2xl">
      <div class="flex items-center justify-between p-5 border-b border-slate-200 dark:border-[#30363d]">
        <h3 class="text-base font-semibold text-slate-900 dark:text-[#e6edf3]">{t('timeline.cleanupRecordsTitle')}</h3>
        <button class="btn btn-ghost" on:click={() => (showCleanupPanel = false)} disabled={cleanupBusy}>
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
      <div class="p-5 space-y-4">
        <p class="text-xs text-slate-500 dark:text-[#7d8590] leading-relaxed">{t('timeline.cleanupRecordsHint')}</p>

        <div class="flex gap-2">
          {#each [{ key: 'date', label: t('timeline.deleteByDate') }, { key: 'range', label: t('timeline.deleteByRange') }, { key: 'app', label: t('timeline.deleteByApp') }] as tab}
            <button
              class="flex-1 px-3 py-1.5 text-xs font-medium rounded-lg border transition-colors {cleanupMode === tab.key ? 'border-rose-400 bg-rose-50 text-rose-600 dark:bg-rose-900/20 dark:text-rose-300 dark:border-rose-700' : 'border-slate-200 dark:border-[#30363d] text-slate-600 dark:text-[#adbac7] hover:bg-slate-50 dark:hover:bg-[#21262d]'}"
              on:click={() => (cleanupMode = tab.key)}
            >
              {tab.label}
            </button>
          {/each}
        </div>

        {#if cleanupMode === 'date'}
          <div class="space-y-3">
            <p class="text-sm text-slate-700 dark:text-[#adbac7] leading-relaxed">
              {t('timeline.deleteByDateMessage', { date: selectedDate })}
            </p>
            <button
              class="w-full px-4 py-2 text-sm rounded-lg bg-red-600 text-white hover:bg-red-700 disabled:opacity-50"
              on:click={doCleanupByDate}
              disabled={cleanupBusy}
            >
              {t('timeline.deleteByDate')}
            </button>
          </div>
        {:else if cleanupMode === 'range'}
          <div class="space-y-3">
            <LocalizedDatePicker
              mode="range"
              bind:startDate={cleanupRangeStart}
              bind:endDate={cleanupRangeEnd}
              localeCode={currentLocale}
              triggerClass="page-control-input w-auto"
            />
            <div class="grid grid-cols-2 gap-3">
              <label class="text-xs text-slate-500 dark:text-[#7d8590] flex flex-col gap-1">
                <span>{t('datePicker.startDate')}</span>
                <input type="time" bind:value={cleanupRangeStartTime} class="page-control-input" />
              </label>
              <label class="text-xs text-slate-500 dark:text-[#7d8590] flex flex-col gap-1">
                <span>{t('datePicker.endDate')}</span>
                <input type="time" bind:value={cleanupRangeEndTime} class="page-control-input" />
              </label>
            </div>
            <button
              class="w-full px-4 py-2 text-sm rounded-lg bg-red-600 text-white hover:bg-red-700 disabled:opacity-50"
              on:click={doCleanupByRange}
              disabled={cleanupBusy || !cleanupRangeStart || !cleanupRangeEnd}
            >
              {t('timeline.deleteByRange')}
            </button>
          </div>
        {:else}
          <div class="space-y-3">
            {#if cleanupAppCandidates.length === 0}
              <p class="text-sm text-slate-500 dark:text-[#7d8590]">{t('timeline.noActivitiesToDelete')}</p>
            {:else}
              <select class="page-control-input w-full" bind:value={cleanupApp}>
                <option value="">{t('timeline.selectApp')}</option>
                {#each cleanupAppCandidates as app}
                  <option value={app}>{app}</option>
                {/each}
              </select>
              <button
                class="w-full px-4 py-2 text-sm rounded-lg bg-red-600 text-white hover:bg-red-700 disabled:opacity-50"
                on:click={doCleanupByApp}
                disabled={cleanupBusy || !cleanupApp}
              >
                {t('timeline.deleteByApp')}
              </button>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<!-- 分类修改确认（页面顶层，z-index 高于详情弹窗 z-[140]） -->
{#if selectedActivity && (pendingChangeCategory || pendingApplyCategory || pendingDeleteCategory || pendingPrivacyRule)}
  {@const isApply = !!pendingApplyCategory}
  {@const isDelete = !!pendingDeleteCategory}
  {@const isPrivacy = !!pendingPrivacyRule}
  {@const confirmAction = isDelete ? confirmDeleteCategory : (isApply ? confirmApplyCategory : (isPrivacy ? confirmPrivacyRule : confirmChangeCategory))}
  {@const cancelAction = isDelete ? cancelDeleteCategory : (isApply ? cancelApplyCategory : (isPrivacy ? cancelPrivacyRule : cancelChangeCategory))}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="fixed inset-0 z-[150] bg-slate-950/40 backdrop-blur-sm flex items-center justify-center animate-fadeIn"
    role="presentation"
    on:click|self={cancelAction}
  >
    <div
      class="timeline-action-confirm-dialog"
      use:trapFocus
      role="dialog"
      aria-modal="true"
      aria-labelledby="timeline-action-confirm-title"
      tabindex="-1"
    >
      {#if isDelete}
        <h3 id="timeline-action-confirm-title" class="text-base font-semibold text-slate-900 dark:text-[#e6edf3]">
          {t('timeline.deleteCategoryTitle')}
        </h3>
        <p class="mt-2 text-sm text-slate-700 dark:text-[#7d8590] leading-relaxed">
          {t('timeline.deleteCategoryMessage', { category: pendingDeleteCategory.name })}
        </p>
        <div class="mt-5 flex justify-end gap-2">
          <button
            on:click={cancelAction}
            class="px-4 py-2 text-sm rounded-lg text-slate-500 hover:text-slate-700 dark:text-[#7d8590] dark:hover:text-[#c9d1d9] border border-slate-200 dark:border-[#30363d]"
          >
            {t('timeline.cancel')}
          </button>
          <button
            on:click={confirmAction}
            class="px-4 py-2 text-sm rounded-lg bg-red-600 text-white hover:bg-red-700"
          >
            {t('timeline.confirmDelete')}
          </button>
        </div>
      {:else if isPrivacy}
        <h3 id="timeline-action-confirm-title" class="text-base font-semibold text-slate-900 dark:text-[#e6edf3]">
          {t('timeline.detail.privacyRule')}
        </h3>
        <p class="mt-2 text-sm text-slate-700 dark:text-[#7d8590] leading-relaxed">
          {t('timeline.detail.privacyConfirmMessage', {
            appName: selectedActivity.app_name,
            level: pendingPrivacyRule.levelLabel,
          })}
        </p>
        <div class="mt-5 flex justify-end gap-2">
          <button
            on:click={cancelAction}
            class="px-4 py-2 text-sm rounded-lg text-slate-500 hover:text-slate-700 dark:text-[#7d8590] dark:hover:text-[#c9d1d9] border border-slate-200 dark:border-[#30363d]"
          >
            {t('timeline.cancel')}
          </button>
          <button
            on:click={confirmAction}
            class="px-4 py-2 text-sm rounded-lg bg-primary-600 text-white hover:bg-primary-700"
          >
            {t('timeline.confirmChange')}
          </button>
        </div>
      {:else}
        {@const categoryName = isApply ? pendingApplyCategory.name : pendingChangeCategory.categoryName}
        {@const appName = selectedActivity.app_name}
        <h3 id="timeline-action-confirm-title" class="text-base font-semibold text-slate-900 dark:text-[#e6edf3]">
          {t('timeline.changeCategoryTitle')}
        </h3>
        <p class="mt-2 text-sm text-slate-700 dark:text-[#7d8590] leading-relaxed">
          {t('timeline.changeCategoryMessage', { appName, category: categoryName })}
        </p>
        <div class="mt-5 flex justify-end gap-2">
          <button
            on:click={cancelAction}
            class="px-4 py-2 text-sm rounded-lg text-slate-500 hover:text-slate-700 dark:text-[#7d8590] dark:hover:text-[#c9d1d9] border border-slate-200 dark:border-[#30363d]"
          >
            {t('timeline.cancel')}
          </button>
          <button
            on:click={confirmAction}
            class="px-4 py-2 text-sm rounded-lg bg-primary-600 text-white hover:bg-primary-700"
          >
            {t('timeline.confirmChange')}
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .timeline-action-confirm-dialog {
    width: 100%;
    max-width: 24rem;
    margin: 1rem;
    padding: 1.5rem;
    border: 1px solid rgb(226 232 240);
    border-radius: 1rem;
    background: white;
    box-shadow: 0 24px 60px rgba(15, 23, 42, 0.28);
  }

  :global(.dark) .timeline-action-confirm-dialog {
    border-color: #30363d;
    background: #161b22;
  }

  .timeline-summary-strip {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 1.1rem 1.25rem 1rem;
    border-bottom: 1px solid rgba(226, 232, 240, 0.82);
  }

  .timeline-summary-copy {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    flex-wrap: wrap;
    color: #6b7280;
    font-size: 0.92rem;
  }

  .timeline-summary-divider {
    color: #d6d3d1;
  }

  .timeline-summary-action {
    background: rgba(255, 250, 240, 0.74);
    border-color: rgba(217, 119, 6, 0.12);
  }

  .timeline-editorial-board {
    position: relative;
    overflow: hidden;
    background: var(--editorial-surface-featured);
    border-color: rgba(255, 251, 235, 0.9);
    box-shadow:
      0 20px 48px rgba(15, 23, 42, 0.08),
      0 2px 10px rgba(15, 23, 42, 0.04),
      inset 0 1px 0 rgba(255, 255, 255, 0.85);
  }

  .timeline-editorial-board::before {
    content: '';
    position: absolute;
    inset: 0;
    background:
      linear-gradient(180deg, rgba(255, 255, 255, 0.22), transparent 28%),
      repeating-linear-gradient(
        135deg,
        rgba(120, 113, 108, 0.018) 0 6px,
        transparent 6px 16px
      );
    pointer-events: none;
  }

  .timeline-editorial-shell {
    --timeline-anchor-width: 6rem;
    position: relative;
    padding: 1.5rem 1.25rem 1.75rem;
  }

  .timeline-rail {
    position: absolute;
    inset-inline-start: calc(1.25rem + var(--timeline-anchor-width));
    top: 1.25rem;
    bottom: 1.25rem;
    width: 2px;
    border-radius: 999px;
    background: linear-gradient(180deg, rgba(31, 41, 55, 0.88), rgba(31, 41, 55, 0.08));
    opacity: 0.9;
    pointer-events: none;
  }

  .timeline-entry {
    position: relative;
    z-index: 1;
    display: grid;
    grid-template-columns: var(--timeline-anchor-width) minmax(0, 1fr);
    gap: 1rem;
    width: 100%;
    padding: 0.2rem 0;
    text-align: start;
    transition:
      transform 180ms ease,
      filter 180ms ease;
  }

  .timeline-entry + .timeline-entry {
    margin-top: 0.4rem;
  }

  .timeline-entry:hover {
    transform: translateY(-1px);
    filter: saturate(1.02);
  }

  .timeline-entry-anchor {
    position: relative;
    display: flex;
    align-items: flex-start;
    justify-content: flex-start;
    gap: 0.65rem;
    min-height: 100%;
    padding-top: 0.95rem;
  }

  .timeline-entry-time {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
    font-size: 0.82rem;
    letter-spacing: 0.08em;
    color: #57534e;
  }

  /* marker 绝对定位到 anchor 列右边缘，与 rail 共用同一水平基准（#129 对齐修复）。
     之前 marker 用 margin-left:auto + rail 用独立 calc，两者差约 14px。 */
  .timeline-entry-marker {
    position: absolute;
    top: 0.95rem;
    inset-inline-end: 0;
    width: 0.8rem;
    height: 0.8rem;
    border-radius: 999px;
    background: #1f2937;
    box-shadow:
      0 0 0 0.32rem rgba(255, 251, 235, 0.96),
      0 0 0 0.4rem rgba(31, 41, 55, 0.08);
    transition:
      transform 180ms ease,
      box-shadow 180ms ease,
      background-color 180ms ease;
  }

  .timeline-entry:hover .timeline-entry-marker,
  .timeline-entry:focus-visible .timeline-entry-marker {
    transform: scale(1.05);
    box-shadow:
      0 0 0 0.32rem rgba(255, 251, 235, 0.98),
      0 0 0 0.5rem rgba(180, 83, 9, 0.12);
  }

  .timeline-entry-marker-featured {
    background: #b45309;
  }

  .timeline-entry-card {
    position: relative;
    border-radius: 1.35rem;
    border: 1px solid rgba(17, 24, 39, 0.08);
    overflow: hidden;
    transition:
      transform 180ms ease,
      border-color 180ms ease,
      box-shadow 180ms ease;
  }

  .timeline-entry:hover .timeline-entry-card,
  .timeline-entry:focus-visible .timeline-entry-card {
    border-color: rgba(180, 83, 9, 0.14);
    box-shadow: 0 16px 36px rgba(15, 23, 42, 0.1);
  }

  .timeline-entry-card-featured {
    display: grid;
    grid-template-columns: minmax(12rem, 16.5rem) minmax(0, 1fr);
    gap: 1rem;
    padding: 0.9rem;
    background: rgba(255, 255, 255, 0.78);
    box-shadow:
      0 16px 36px rgba(15, 23, 42, 0.08),
      inset 0 1px 0 rgba(255, 255, 255, 0.85);
  }

  .timeline-entry-card-compact {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.8rem 1rem;
    align-items: center;
    padding: 1rem 1.05rem;
    background: rgba(255, 255, 255, 0.62);
    backdrop-filter: blur(8px);
  }

  .timeline-featured-media {
    min-width: 0;
  }

  .timeline-featured-image {
    width: 100%;
    aspect-ratio: 1.38;
    border-radius: 1rem;
    object-fit: cover;
    background:
      linear-gradient(135deg, rgba(191, 219, 254, 0.82), rgba(254, 243, 199, 0.9)),
      repeating-linear-gradient(45deg, rgba(255, 255, 255, 0.2) 0 8px, rgba(255, 255, 255, 0.03) 8px 16px);
    border: 1px solid rgba(255, 255, 255, 0.62);
  }

  .timeline-featured-image-placeholder {
    position: relative;
    overflow: hidden;
  }

  .timeline-featured-image-glow {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(circle at 20% 20%, rgba(255, 255, 255, 0.52), transparent 36%),
      linear-gradient(135deg, rgba(191, 219, 254, 0.52), rgba(254, 243, 199, 0.68));
  }

  .timeline-featured-copy {
    min-width: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 0.85rem;
  }

  .timeline-entry-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }

  .timeline-entry-meta-featured {
    align-items: flex-start;
    gap: 1rem;
  }

  .timeline-entry-app {
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 0.85rem;
    flex: 1 1 auto;
  }

  .timeline-app-icon {
    width: 2.75rem;
    height: 2.75rem;
    border-radius: 1rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    flex-shrink: 0;
    color: #111827;
    background: var(--icon-bg-light, rgba(226, 232, 240, 0.95));
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.65);
  }

  .timeline-app-icon-lg {
    width: 3.2rem;
    height: 3.2rem;
    border-radius: 1.05rem;
    font-size: 1.5rem;
  }

  .timeline-app-icon-blue {
    background: rgba(219, 234, 254, 0.95);
  }

  .timeline-app-icon-green {
    background: rgba(220, 252, 231, 0.95);
  }

  .timeline-app-icon-yellow {
    background: rgba(254, 249, 195, 0.95);
  }

  .timeline-app-icon-purple {
    background: rgba(237, 233, 254, 0.95);
  }

  .timeline-app-icon-pink {
    background: rgba(252, 231, 243, 0.95);
  }

  .timeline-app-icon-red {
    background: rgba(254, 226, 226, 0.95);
  }

  .timeline-app-icon-gray {
    background: rgba(241, 245, 249, 0.95);
  }

  .timeline-app-icon-image {
    width: 1.9rem;
    height: 1.9rem;
    border-radius: 0.7rem;
  }

  .timeline-app-icon-image-lg {
    width: 2.25rem;
    height: 2.25rem;
    border-radius: 0.8rem;
  }

  .timeline-entry-heading {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
  }

  .timeline-entry-heading-featured {
    align-items: flex-start;
    gap: 0.45rem;
  }

  .timeline-entry-app-name {
    display: block;
    font-size: 0.98rem;
    font-weight: 600;
    color: #111827;
    letter-spacing: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .timeline-entry-category {
    font-size: 0.72rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: #a16207;
  }

  .timeline-entry-category-pill {
    display: inline-flex;
    align-items: center;
    align-self: flex-start;
    min-height: 1.5rem;
    max-width: max-content;
    padding: 0.2rem 0.58rem;
    border-radius: 999px;
    border: 1px solid rgba(217, 119, 6, 0.18);
    background: rgba(255, 247, 237, 0.92);
    color: #b45309;
    font-size: 0.7rem;
    font-weight: 600;
    letter-spacing: 0.06em;
    line-height: 1;
    text-transform: none;
    white-space: nowrap;
    writing-mode: horizontal-tb;
  }

  .timeline-entry-duration-chip {
    flex-shrink: 0;
    padding: 0.4rem 0.7rem;
    border-radius: 999px;
    background: rgba(255, 247, 237, 0.92);
    color: #9a3412;
    font-size: 0.78rem;
    font-weight: 600;
  }

  .timeline-entry-title {
    min-width: 0;
    color: #1f2937;
    margin: 0;
  }

  .timeline-entry-title-featured {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    font-size: 1.02rem;
    line-height: 1.55;
    font-weight: 600;
    letter-spacing: 0;
  }

  .timeline-entry-title-compact {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.92rem;
    color: #57534e;
  }

  .timeline-entry-url {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.83rem;
    color: #78716c;
  }

  .timeline-entry-tail {
    display: inline-flex;
    align-items: center;
    gap: 0.65rem;
    color: #78716c;
    white-space: nowrap;
  }

  .timeline-entry-card-compact-grid {
    grid-template-columns: minmax(0, 1fr) auto;
    grid-template-areas:
      'app app'
      'title meta';
    align-items: start;
  }

  .timeline-entry-app-compact {
    grid-area: app;
  }

  .timeline-entry-card-compact-grid .timeline-entry-title-compact {
    grid-area: title;
  }

  .timeline-entry-tail-compact {
    grid-area: meta;
    justify-self: end;
    align-self: end;
  }

  .timeline-entry-duration {
    font-size: 0.85rem;
    font-weight: 500;
  }

  .timeline-entry-arrow {
    width: 1rem;
    height: 1rem;
    color: #a8a29e;
    flex-shrink: 0;
  }

  .timeline-load-more {
    position: relative;
    padding: 0 1.25rem 1.4rem;
    padding-inline-start: calc(1.25rem + var(--timeline-anchor-width));
  }

  .timeline-load-more-btn {
    width: 100%;
    min-height: 2.75rem;
    padding: 0.65rem 1rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    font-size: 0.92rem;
    color: #57534e;
    border-radius: 1rem;
    border: 1px dashed rgba(120, 113, 108, 0.35);
    background: rgba(255, 255, 255, 0.54);
    transition:
      border-style 180ms ease,
      background-color 180ms ease;
  }

  .timeline-load-more-btn:hover:enabled {
    border-style: solid;
    background: rgba(255, 255, 255, 0.72);
  }

  .timeline-load-more-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .timeline-load-more-end {
    color: #a8a29e;
    text-align: center;
    font-size: 0.78rem;
  }

  .timeline-detail-overlay {
    overflow: hidden;
  }

  .timeline-detail-drawer {
    width: min(42rem, 100%);
    height: calc(100vh - 2rem);
    overflow-y: auto;
    position: relative;
    border: 1px solid rgba(148, 163, 184, 0.24);
    border-radius: 1.25rem;
    background: var(--editorial-surface-featured);
    box-shadow: -18px 0 48px rgba(15, 23, 42, 0.18);
  }

  .timeline-detail-header {
    position: sticky;
    top: 0;
    z-index: 5;
    padding: 1.15rem 1.35rem;
    border-bottom: 1px solid rgba(148, 163, 184, 0.2);
    background: color-mix(in srgb, var(--editorial-surface-featured) 94%, transparent);
    backdrop-filter: blur(18px);
  }

  .timeline-detail-body {
    display: grid;
    gap: 1.35rem;
    padding: 1.15rem 1.35rem 1.5rem;
  }

  .timeline-detail-hero {
    display: flex;
    align-items: center;
    gap: 1rem;
    min-height: 2.75rem;
  }

  .timeline-detail-hero-item {
    display: grid;
    gap: 0.18rem;
  }

  .timeline-detail-hero-item span,
  .timeline-detail-section-heading,
  .timeline-detail-meta-row > span {
    color: #78716c;
    font-size: 0.76rem;
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .timeline-detail-hero-item strong {
    color: #292524;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
    font-size: 0.94rem;
    font-weight: 600;
  }

  .timeline-detail-hero-divider {
    width: 1px;
    height: 1.75rem;
    background: rgba(148, 163, 184, 0.28);
  }

  .timeline-detail-preview,
  .timeline-detail-meta,
  .timeline-detail-settings {
    min-width: 0;
  }

  .timeline-detail-section-heading {
    margin-bottom: 0.55rem;
  }

  .timeline-detail-preview-frame {
    min-height: 13rem;
    overflow: hidden;
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 0.9rem;
    background: rgba(148, 163, 184, 0.1);
  }

  .timeline-detail-preview-state {
    min-height: 13rem;
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 2rem;
    font-size: 0.84rem;
    text-align: center;
  }

  .timeline-detail-preview-image {
    display: block;
    width: 100%;
    max-height: 25rem;
    object-fit: contain;
  }

  .timeline-detail-preview-loading-indicator {
    position: absolute;
    top: 0.65rem;
    right: 0.65rem;
    width: 1.75rem;
    height: 1.75rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid rgba(148, 163, 184, 0.24);
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.82);
    box-shadow: 0 4px 12px rgba(15, 23, 42, 0.08);
  }

  .timeline-detail-meta {
    display: grid;
    gap: 0.85rem;
  }

  .timeline-detail-meta-row {
    display: grid;
    grid-template-columns: 6.25rem minmax(0, 1fr);
    align-items: baseline;
    gap: 1rem;
  }

  .timeline-detail-meta-row p,
  .timeline-detail-url {
    min-width: 0;
    margin: 0;
    color: #292524;
    font-size: 0.92rem;
    line-height: 1.55;
    overflow-wrap: anywhere;
    text-align: start;
  }

  .timeline-detail-url {
    padding: 0;
    border: 0;
    color: #b45309;
    background: transparent;
    cursor: pointer;
  }

  .timeline-detail-url:hover {
    text-decoration: underline;
  }

  .timeline-detail-settings {
    display: grid;
    gap: 1.2rem;
    padding-top: 1.25rem;
    border-top: 1px solid rgba(148, 163, 184, 0.2);
  }

  .timeline-detail-setting-row {
    min-width: 0;
  }

  .timeline-category-section {
    position: relative;
  }

  .timeline-category-control {
    position: relative;
    margin-top: 0.75rem;
  }

  .timeline-category-trigger {
    width: 100%;
    min-height: 2.7rem;
    display: flex;
    align-items: center;
    gap: 0.65rem;
    padding: 0.6rem 0.75rem;
    border: 1px solid rgba(148, 163, 184, 0.28);
    border-radius: 0.8rem;
    color: #292524;
    background: rgba(255, 255, 255, 0.82);
    font-size: 0.88rem;
    text-align: start;
  }

  .timeline-category-trigger:focus-visible,
  .timeline-category-option:focus-visible,
  .timeline-category-create-trigger:focus-visible,
  .timeline-category-option-actions button:focus-visible,
  .timeline-category-editor button:focus-visible,
  .timeline-category-editor input:focus-visible {
    outline: 2px solid rgba(217, 119, 6, 0.55);
    outline-offset: 2px;
  }

  .timeline-category-trigger svg {
    width: 0.95rem;
    height: 0.95rem;
    margin-inline-start: auto;
    color: #a8a29e;
  }

  .timeline-category-dot {
    width: 0.62rem;
    height: 0.62rem;
    flex: 0 0 auto;
    border-radius: 999px;
    box-shadow: 0 0 0 2px rgba(148, 163, 184, 0.12);
  }

  .timeline-category-popover {
    position: fixed;
    z-index: 152;
    overflow-y: auto;
    padding: 0.42rem;
    border: 1px solid rgba(148, 163, 184, 0.24);
    border-radius: 0.9rem;
    background: #fff;
    box-shadow: 0 18px 42px rgba(15, 23, 42, 0.14);
  }

  .timeline-category-options {
    display: grid;
    gap: 0.18rem;
  }

  .timeline-category-option-row {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }

  .timeline-category-option {
    min-width: 0;
    min-height: 2.35rem;
    display: flex;
    flex: 1;
    align-items: center;
    gap: 0.62rem;
    padding: 0.48rem 0.62rem;
    border: 0;
    border-radius: 0.65rem;
    color: #57534e;
    background: transparent;
    font-size: 0.84rem;
    text-align: start;
  }

  .timeline-category-option:hover,
  .timeline-category-option-active {
    color: #292524;
    background: #f5f5f4;
  }

  .timeline-category-option-name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .timeline-category-check {
    margin-inline-start: auto;
    color: #b45309;
    font-weight: 800;
  }

  .timeline-category-option-actions {
    display: flex;
    align-items: center;
    gap: 0.12rem;
  }

  .timeline-category-option-actions button {
    width: 1.9rem;
    height: 1.9rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 0.55rem;
    color: #a8a29e;
    background: transparent;
    font-size: 0.76rem;
  }

  .timeline-category-option-actions button:hover {
    color: #57534e;
    background: #f5f5f4;
  }

  .timeline-category-create-trigger {
    width: 100%;
    min-height: 2.3rem;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    margin-top: 0.3rem;
    border: 1px dashed rgba(148, 163, 184, 0.32);
    border-radius: 0.65rem;
    color: #78716c;
    background: transparent;
    font-size: 0.8rem;
  }

  .timeline-category-editor {
    display: grid;
    gap: 0.65rem;
    margin-top: 0.42rem;
    padding: 0.72rem;
    border: 1px solid rgba(148, 163, 184, 0.2);
    border-radius: 0.72rem;
    background: #fafaf9;
  }

  .timeline-category-editor p {
    margin: 0;
    color: #78716c;
    font-size: 0.74rem;
  }

  .timeline-category-editor-fields {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 2rem 1.5rem;
    align-items: center;
    gap: 0.45rem;
  }

  .timeline-category-editor-fields input[type='text'] {
    min-width: 0;
    padding: 0.42rem 0.55rem;
    border: 1px solid rgba(148, 163, 184, 0.28);
    border-radius: 0.55rem;
    background: #fff;
    font-size: 0.8rem;
  }

  .timeline-category-editor-fields input[type='color'] {
    width: 2rem;
    height: 2rem;
    padding: 0;
    border: 0;
    border-radius: 0.45rem;
    background: transparent;
  }

  .timeline-category-emoji-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 0.2rem;
  }

  .timeline-category-emoji-grid button {
    width: 1.85rem;
    height: 1.85rem;
    border: 0;
    border-radius: 0.45rem;
    background: transparent;
  }

  .timeline-category-emoji-grid button:hover,
  .timeline-category-emoji-active {
    background: #e7e5e4 !important;
  }

  .timeline-category-editor-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.45rem;
  }

  .timeline-category-editor-actions button {
    padding: 0.4rem 0.68rem;
    border: 1px solid rgba(148, 163, 184, 0.24);
    border-radius: 0.55rem;
    color: #78716c;
    background: #fff;
    font-size: 0.76rem;
  }

  .timeline-category-editor-actions .timeline-category-editor-primary {
    color: #fff;
    border-color: #d97706;
    background: #d97706;
  }

  :global(.dark) .timeline-summary-copy {
    color: #94a3b8;
  }

  :global(.dark) .timeline-summary-divider {
    color: #475569;
  }

  :global(.dark) .timeline-summary-action {
    background: rgba(51, 65, 85, 0.72);
    border-color: rgba(245, 158, 11, 0.16);
  }

  :global(.dark) .timeline-editorial-board {
    background: var(--editorial-surface-featured);
    border-color: rgba(71, 85, 105, 0.5);
    box-shadow: 0 24px 54px rgba(2, 6, 23, 0.3);
  }

  :global(.dark) .timeline-editorial-board::before {
    display: none;
  }

  :global(.dark) .timeline-rail {
    background: linear-gradient(180deg, rgba(71, 85, 105, 0.62), rgba(48, 54, 61, 0.2));
  }

  :global(.dark) .timeline-entry-time {
    color: #cbd5e1;
  }

  :global(.dark) .timeline-entry-marker {
    background: #64748b;
    box-shadow:
      0 0 0 0.32rem rgba(15, 23, 42, 0.96),
      0 0 0 0.5rem rgba(148, 163, 184, 0.08);
  }

  :global(.dark) .timeline-entry:hover .timeline-entry-marker,
  :global(.dark) .timeline-entry:focus-visible .timeline-entry-marker {
    box-shadow:
      0 0 0 0.32rem rgba(15, 23, 42, 0.98),
      0 0 0 0.55rem rgba(245, 158, 11, 0.16);
  }

  :global(.dark) .timeline-entry-marker-featured {
    background: #fbbf24;
  }

  :global(.dark) .timeline-entry-card {
    border-color: rgba(148, 163, 184, 0.12);
  }

  :global(.dark) .timeline-entry:hover .timeline-entry-card,
  :global(.dark) .timeline-entry:focus-visible .timeline-entry-card {
    border-color: rgba(251, 191, 36, 0.18);
    box-shadow: 0 18px 42px rgba(2, 6, 23, 0.34);
  }

  :global(.dark) .timeline-entry-card-featured {
    background: rgba(15, 23, 42, 0.66);
  }

  :global(.dark) .timeline-summary-strip {
    border-bottom-color: rgba(71, 85, 105, 0.72);
  }

  :global(.dark) .timeline-entry-card-compact {
    background: rgba(15, 23, 42, 0.54);
  }

  :global(.dark) .timeline-featured-image {
    border-color: rgba(148, 163, 184, 0.12);
  }

  :global(.dark) .timeline-entry-app-name,
  :global(.dark) .timeline-entry-title {
    color: #f8fafc;
  }

  :global(.dark) .timeline-entry-category {
    color: #fbbf24;
  }

  :global(.dark) .timeline-entry-category-pill {
    border-color: rgba(245, 158, 11, 0.22);
    background: rgba(120, 53, 15, 0.28);
    color: #fcd34d;
  }

  :global(.dark) .timeline-entry-title-compact,
  :global(.dark) .timeline-entry-url,
  :global(.dark) .timeline-entry-tail {
    color: #94a3b8;
  }

  :global(.dark) .timeline-entry-duration-chip {
    background: rgba(120, 53, 15, 0.26);
    color: #fdba74;
  }

  :global(.dark) .timeline-app-icon {
    color: #e6edf3;
    background: var(--icon-bg-dark, rgba(51, 65, 85, 0.74));
    box-shadow: none;
  }

  :global(.dark) .timeline-app-icon-blue {
    background: rgba(30, 64, 175, 0.34);
  }

  :global(.dark) .timeline-app-icon-green {
    background: rgba(22, 101, 52, 0.34);
  }

  :global(.dark) .timeline-app-icon-yellow {
    background: rgba(133, 77, 14, 0.34);
  }

  :global(.dark) .timeline-app-icon-purple {
    background: rgba(91, 33, 182, 0.34);
  }

  :global(.dark) .timeline-app-icon-pink {
    background: rgba(157, 23, 77, 0.34);
  }

  :global(.dark) .timeline-app-icon-red {
    background: rgba(153, 27, 27, 0.34);
  }

  :global(.dark) .timeline-app-icon-gray {
    background: rgba(51, 65, 85, 0.74);
  }

  :global(.dark) .timeline-load-more-btn {
    color: #cbd5e1;
    border-color: rgba(148, 163, 184, 0.24);
    background: rgba(15, 23, 42, 0.48);
  }

  :global(.dark) .timeline-load-more-btn:hover:enabled {
    background: rgba(15, 23, 42, 0.68);
  }

  :global(.dark) .timeline-load-more-end {
    color: #64748b;
  }

  :global(.dark) .timeline-detail-drawer {
    border-color: rgba(48, 54, 61, 0.88);
    background: #161b22;
    box-shadow: -18px 0 48px rgba(0, 0, 0, 0.28);
  }

  :global(.dark) .timeline-detail-header {
    border-color: rgba(48, 54, 61, 0.8);
    background: rgba(22, 27, 34, 0.94);
  }

  :global(.dark) .timeline-detail-hero-item span,
  :global(.dark) .timeline-detail-section-heading,
  :global(.dark) .timeline-detail-meta-row > span {
    color: #7d8590;
  }

  :global(.dark) .timeline-detail-hero-item strong,
  :global(.dark) .timeline-detail-meta-row p {
    color: #e6edf3;
  }

  :global(.dark) .timeline-detail-hero-divider,
  :global(.dark) .timeline-detail-settings {
    border-color: rgba(48, 54, 61, 0.8);
  }

  :global(.dark) .timeline-detail-hero-divider {
    background: rgba(48, 54, 61, 0.8);
  }

  :global(.dark) .timeline-detail-preview-frame {
    background: rgba(48, 54, 61, 0.38);
  }

  :global(.dark) .timeline-detail-preview-loading-indicator {
    border-color: rgba(48, 54, 61, 0.8);
    background: rgba(22, 27, 34, 0.82);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.18);
  }

  :global(.dark) .timeline-detail-url {
    color: #d29922;
  }

  :global(.dark) .timeline-category-trigger,
  :global(.dark) .timeline-category-popover,
  :global(.dark) .timeline-category-editor,
  :global(.dark) .timeline-category-editor-fields input[type='text'],
  :global(.dark) .timeline-category-editor-actions button {
    border-color: rgba(48, 54, 61, 0.8);
    color: #adbac7;
    background: #21262d;
  }

  :global(.dark) .timeline-category-option {
    color: #adbac7;
  }

  :global(.dark) .timeline-category-option:hover,
  :global(.dark) .timeline-category-option-active,
  :global(.dark) .timeline-category-option-actions button:hover,
  :global(.dark) .timeline-category-emoji-grid button:hover,
  :global(.dark) .timeline-category-emoji-active {
    color: #e6edf3;
    background: #30363d !important;
  }

  :global(.dark) .timeline-category-create-trigger {
    color: #8b949e;
    border-color: rgba(48, 54, 61, 0.8);
  }

  :global(.dark) .timeline-category-check {
    color: #d29922;
  }

  :global(.dark) .timeline-category-editor-actions .timeline-category-editor-primary {
    color: #fff;
    border-color: #9e6a03;
    background: #9e6a03;
  }

  @media (max-width: 860px) {
    .timeline-entry-card-featured {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 640px) {
    .page-shell {
      padding-inline: 0.5rem;
    }

    .timeline-detail-overlay {
      padding: 0;
    }

    .timeline-detail-drawer {
      width: 100%;
      height: 100vh;
      border-inline-end: 0;
      border-radius: 0;
    }

    .timeline-detail-header,
    .timeline-detail-body {
      padding-inline: 1rem;
    }

    .timeline-detail-body {
      gap: 1.15rem;
    }

    .timeline-detail-preview-frame,
    .timeline-detail-preview-state {
      min-height: 10rem;
    }

    .timeline-detail-meta-row {
      grid-template-columns: 1fr;
      gap: 0.25rem;
    }

    .timeline-summary-strip {
      align-items: flex-start;
      flex-direction: column;
      padding: 1rem 0.85rem 0.9rem;
    }

    .timeline-editorial-shell {
      --timeline-anchor-width: 0;
      padding: 0.6rem 0.5rem 1rem;
    }

    .timeline-rail {
      display: none;
    }

    .timeline-entry {
      grid-template-columns: minmax(0, 1fr);
      gap: 0.35rem;
    }

    .timeline-entry-card-compact-grid {
      grid-template-columns: minmax(0, 1fr);
      grid-template-areas:
        'app'
        'title'
        'meta';
      padding: 0.85rem 0.8rem;
    }

    .timeline-entry-app {
      gap: 0.65rem;
    }

    .timeline-app-icon {
      width: 2.5rem;
      height: 2.5rem;
      border-radius: 0.85rem;
    }

    .timeline-entry-tail-compact {
      justify-self: start;
    }

    .timeline-entry-anchor {
      display: block;
      min-height: 0;
      padding: 0.4rem 0.25rem 0;
    }

    .timeline-entry-time {
      font-size: 0.74rem;
      letter-spacing: 0.05em;
    }

    .timeline-entry-marker {
      display: none;
    }

    .timeline-entry-card-compact {
      grid-template-columns: 1fr;
    }

    .timeline-entry-tail {
      justify-content: space-between;
    }

    .timeline-load-more {
      padding: 0 0.5rem 1rem;
    }
  }
</style>
