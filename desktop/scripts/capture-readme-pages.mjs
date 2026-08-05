import { chromium } from 'playwright';
import { access, mkdir, readFile } from 'node:fs/promises';
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

export const CAPTURE_TIMEZONE = 'Asia/Shanghai';
const CAPTURE_DATE = '2026-07-28';
const VIEWPORT = { width: 1491, height: 841 };
const FIXED_TIME = '2026-07-28T10:30:00+08:00';
const BASE_URL = normalizeBaseUrl(process.env.README_CAPTURE_BASE_URL || 'http://127.0.0.1:5173');

const LOCALES = [
  { locale: 'zh-CN', dir: 'Introduction_zh' },
  { locale: 'en', dir: 'Introduction_en' },
  { locale: 'zh-TW', dir: 'Introduction_tw' },
];

export function normalizeBaseUrl(value) {
  const url = new URL(value);
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    throw new Error('README_CAPTURE_BASE_URL 必须是 HTTP(S) URL。');
  }
  if (!url.pathname.endsWith('/')) {
    url.pathname += '/';
  }
  return url;
}

export function isAllowedRequestUrl(value, baseUrl = BASE_URL) {
  const requestUrl = new URL(value);
  return requestUrl.origin === baseUrl.origin
    || requestUrl.protocol === 'data:'
    || requestUrl.protocol === 'blob:';
}

export function createCaptureContextOptions(locale) {
  return {
    viewport: VIEWPORT,
    deviceScaleFactor: 2,
    locale,
    timezoneId: CAPTURE_TIMEZONE,
    colorScheme: 'light',
    reducedMotion: 'reduce',
  };
}

export const README_SCREENSHOT_LABELS = [
  '概览',
  '时间线',
  '时间线详情',
  '小时总结',
  '日报',
  '助手',
  '设置-通用',
  '设置-外观',
  '设置-AI模型',
  '设置-桌面化身',
  '设置-隐私',
  '设置-存储',
  '接入管理',
  '关于',
];
export const WORKFLOW_FRAME_LABELS = ['概览', '时间线', '日报'];

const categories = [
  { key: 'development', name: 'Development', color: '#3b82f6', icon: '💻', is_system: true },
  { key: 'communication', name: 'Communication', color: '#8b5cf6', icon: '💬', is_system: true },
  { key: 'browser', name: 'Browser', color: '#06b6d4', icon: '🌐', is_system: true },
  { key: 'office', name: 'Office', color: '#10b981', icon: '📝', is_system: true },
  { key: 'other', name: 'Other', color: '#94a3b8', icon: '📁', is_system: true },
];

const appUsage = [
  { app_name: 'Cursor', duration: 7_200, count: 126, executable_path: null, screenshot_url: null },
  { app_name: 'Google Chrome', duration: 4_200, count: 84, executable_path: null, screenshot_url: null },
  { app_name: 'Terminal', duration: 2_700, count: 47, executable_path: null, screenshot_url: null },
  { app_name: 'Slack', duration: 1_800, count: 36, executable_path: null, screenshot_url: null },
  { app_name: 'Notion', duration: 1_500, count: 21, executable_path: null, screenshot_url: null },
  { app_name: 'Figma', duration: 1_200, count: 18, executable_path: null, screenshot_url: null },
  { app_name: 'Mail', duration: 900, count: 12, executable_path: null, screenshot_url: null },
  { app_name: 'Calendar', duration: 900, count: 8, executable_path: null, screenshot_url: null },
];

const domains = [
  {
    domain: 'github.com',
    duration: 1_200,
    page_count: 2,
    semantic_category: 'development',
    urls: [
      { url: 'https://github.com/example/work-review/pulls', duration: 700 },
      { url: 'https://github.com/example/work-review/actions', duration: 500 },
    ],
  },
  {
    domain: 'docs.rs',
    duration: 600,
    page_count: 1,
    semantic_category: 'office',
    urls: [{ url: 'https://docs.rs/tauri/latest/tauri/', duration: 600 }],
  },
  {
    domain: 'figma.com',
    duration: 540,
    page_count: 1,
    semantic_category: 'office',
    urls: [{ url: 'https://figma.com/file/readme-refresh', duration: 540 }],
  },
  {
    domain: 'openai.com',
    duration: 480,
    page_count: 1,
    semantic_category: 'development',
    urls: [{ url: 'https://platform.openai.com/docs', duration: 480 }],
  },
  {
    domain: 'linear.app',
    duration: 420,
    page_count: 1,
    semantic_category: 'communication',
    urls: [{ url: 'https://linear.app/example/project/readme', duration: 420 }],
  },
  {
    domain: 'svelte.dev',
    duration: 360,
    page_count: 2,
    semantic_category: 'development',
    urls: [
      { url: 'https://svelte.dev/docs/svelte/overview', duration: 220 },
      { url: 'https://svelte.dev/docs/svelte/run-time-svelte', duration: 140 },
    ],
  },
  {
    domain: 'tauri.app',
    duration: 300,
    page_count: 1,
    semantic_category: 'development',
    urls: [{ url: 'https://tauri.app/develop/', duration: 300 }],
  },
  {
    domain: 'npmjs.com',
    duration: 240,
    page_count: 1,
    semantic_category: 'development',
    urls: [{ url: 'https://www.npmjs.com/package/playwright', duration: 240 }],
  },
];

const hourlyActivity = Array.from({ length: 24 }, (_, hour) => ({
  hour,
  duration: ({
    8: 420,
    9: 2_640,
    10: 3_180,
    11: 2_760,
    12: 540,
    13: 420,
    14: 2_520,
    15: 2_940,
    16: 2_460,
    17: 1_980,
    18: 1_080,
  })[hour] || 0,
}));

const hourlyAppBreakdown = [
  { hour: 8, apps: [{ app_name: 'Slack', category: 'communication', duration: 420 }] },
  { hour: 9, apps: [{ app_name: 'Cursor', category: 'development', duration: 1_680 }, { app_name: 'Google Chrome', category: 'browser', duration: 720 }, { app_name: 'Slack', category: 'communication', duration: 240 }] },
  { hour: 10, apps: [{ app_name: 'Cursor', category: 'development', duration: 1_860 }, { app_name: 'Terminal', category: 'development', duration: 780 }, { app_name: 'Google Chrome', category: 'browser', duration: 540 }] },
  { hour: 11, apps: [{ app_name: 'Cursor', category: 'development', duration: 1_440 }, { app_name: 'Google Chrome', category: 'browser', duration: 840 }, { app_name: 'Notion', category: 'office', duration: 480 }] },
  { hour: 12, apps: [{ app_name: 'Slack', category: 'communication', duration: 300 }, { app_name: 'Notion', category: 'office', duration: 240 }] },
  { hour: 13, apps: [{ app_name: 'Google Chrome', category: 'browser', duration: 420 }] },
  { hour: 14, apps: [{ app_name: 'Cursor', category: 'development', duration: 1_260 }, { app_name: 'Terminal', category: 'development', duration: 720 }, { app_name: 'Slack', category: 'communication', duration: 540 }] },
  { hour: 15, apps: [{ app_name: 'Cursor', category: 'development', duration: 1_380 }, { app_name: 'Google Chrome', category: 'browser', duration: 900 }, { app_name: 'Notion', category: 'office', duration: 660 }] },
  { hour: 16, apps: [{ app_name: 'Terminal', category: 'development', duration: 900 }, { app_name: 'Google Chrome', category: 'browser', duration: 720 }, { app_name: 'Slack', category: 'communication', duration: 840 }] },
  { hour: 17, apps: [{ app_name: 'Cursor', category: 'development', duration: 840 }, { app_name: 'Notion', category: 'office', duration: 540 }, { app_name: 'Slack', category: 'communication', duration: 600 }] },
  { hour: 18, apps: [{ app_name: 'Google Chrome', category: 'browser', duration: 540 }, { app_name: 'Notion', category: 'office', duration: 540 }] },
];

const currentStats = {
  total_duration: 20_400,
  screenshot_count: 286,
  app_usage: appUsage,
  category_usage: [
    { category: 'development', duration: 9_900 },
    { category: 'browser', duration: 4_200 },
    { category: 'communication', duration: 1_800 },
    { category: 'office', duration: 4_500 },
  ],
  browser_duration: 4_200,
  url_usage: [],
  domain_usage: domains.slice(0, 6),
  domain_total_count: domains.length,
  browser_usage: [
    { browser_name: 'Google Chrome', duration: 4_200, executable_path: null, domains },
  ],
  work_time_duration: 18_840,
  overtime_duration: 1_560,
  hourly_activity_distribution: hourlyActivity,
};

export function validateCaptureFixtures() {
  const sumDuration = (items) => items.reduce((total, item) => total + item.duration, 0);
  if (sumDuration(appUsage) !== currentStats.total_duration) {
    throw new Error('README 模拟应用时长与总投入不一致');
  }
  if (sumDuration(currentStats.category_usage) !== currentStats.total_duration) {
    throw new Error('README 模拟分类时长与总投入不一致');
  }
  if (sumDuration(domains) > currentStats.browser_duration) {
    throw new Error('README 模拟网站时长超过浏览器时长');
  }
  for (const domain of domains) {
    if (sumDuration(domain.urls) !== domain.duration) {
      throw new Error(`README 模拟网站 ${domain.domain} 的页面时长不一致`);
    }
  }
}

const previousStats = {
  ...currentStats,
  total_duration: 16_920,
  screenshot_count: 238,
  app_usage: appUsage.map((item) => ({ ...item, duration: Math.round(item.duration * 0.82) })),
  category_usage: [
    { category: 'development', duration: 8_220 },
    { category: 'browser', duration: 3_720 },
    { category: 'communication', duration: 2_820 },
    { category: 'office', duration: 2_160 },
  ],
};

const rangeDailyTotals = [
  { date: '2026-07-22', total_duration: 15_900 },
  { date: '2026-07-23', total_duration: 18_240 },
  { date: '2026-07-24', total_duration: 17_520 },
  { date: '2026-07-25', total_duration: 6_480 },
  { date: '2026-07-26', total_duration: 4_860 },
  { date: '2026-07-27', total_duration: 19_260 },
  { date: CAPTURE_DATE, total_duration: currentStats.total_duration },
];

const timelineTitles = {
  'zh-CN': [
    '统一常驻网站与应用排行的行分隔样式',
    '核对多语言 README 功能介绍与截图清单',
    '运行概览、时间线和日报的构建验证',
    '同步三语界面预览与发布说明',
    '整理截图脚本的当前数据契约',
    '复核设置页在不同语言下的版式',
  ],
  en: [
    'Align row separators across website and app rankings',
    'Review the multilingual README copy and screenshot checklist',
    'Run build verification for Overview, Timeline, and Daily Report',
    'Sync localized interface previews and release notes',
    'Update the screenshot fixtures to the current data contract',
    'Review Settings layouts in every documented language',
  ],
  'zh-TW': [
    '統一常駐網站與應用排行的列分隔樣式',
    '核對多語言 README 功能介紹與截圖清單',
    '執行概覽、時間線和日報的構建驗證',
    '同步三語介面預覽與發佈說明',
    '整理截圖腳本的目前資料契約',
    '複核設定頁在不同語言下的版式',
  ],
};

const timelineBlueprints = [
  { id: 601, time: '2026-07-28T10:22:00+08:00', app: 'Cursor', category: 'development', duration: 2_520 },
  { id: 602, time: '2026-07-28T09:34:00+08:00', app: 'Google Chrome', category: 'browser', duration: 1_680, url: 'https://github.com/example/work-review/pulls' },
  { id: 603, time: '2026-07-28T09:02:00+08:00', app: 'Terminal', category: 'development', duration: 1_320 },
  { id: 604, time: '2026-07-28T08:35:00+08:00', app: 'Slack', category: 'communication', duration: 1_080 },
  { id: 605, time: '2026-07-28T08:12:00+08:00', app: 'Notion', category: 'office', duration: 840 },
  { id: 606, time: '2026-07-28T07:50:00+08:00', app: 'Figma', category: 'office', duration: 660 },
];

function buildTimelineActivities(locale) {
  return timelineBlueprints.map((item, index) => ({
    id: item.id,
    timestamp: Math.floor(Date.parse(item.time) / 1000),
    app_name: item.app,
    window_title: timelineTitles[locale][index],
    screenshot_path: `readme-capture-${item.id}.png`,
    screenshot_url: null,
    ocr_text: null,
    category: item.category,
    duration: item.duration,
    browser_url: item.url || null,
    executable_path: null,
    semantic_category: item.category,
    semantic_confidence: 96,
  }));
}

const hourlySummaries = {
  'zh-CN': [
    { hour: 8, summary: '整理 README 截图范围与三语文案。\n确认需要覆盖全部核心页面和设置标签。', main_apps: 'Notion, Slack, Figma', activity_count: 18, total_duration: 2_940 },
    { hour: 9, summary: '集中更新截图脚本和当前接口模拟数据。\n同时核对时间线详情与时段摘要抽屉。', main_apps: 'Cursor, Google Chrome, Terminal', activity_count: 31, total_duration: 3_480 },
    { hour: 10, summary: '完成概览排行分隔样式并执行自动化验证。\n准备生成三种语言的最终文档资产。', main_apps: 'Cursor, Terminal, Work Review', activity_count: 24, total_duration: 2_760 },
  ],
  en: [
    { hour: 8, summary: 'Reviewed the README screenshot scope and localized copy.\nConfirmed coverage for every core page and Settings tab.', main_apps: 'Notion, Slack, Figma', activity_count: 18, total_duration: 2_940 },
    { hour: 9, summary: 'Updated the capture script and fixtures for the current data contracts.\nChecked the Timeline detail and hourly summary drawers.', main_apps: 'Cursor, Google Chrome, Terminal', activity_count: 31, total_duration: 3_480 },
    { hour: 10, summary: 'Finished the ranking separators and automated verification.\nPrepared the final documentation assets in all three languages.', main_apps: 'Cursor, Terminal, Work Review', activity_count: 24, total_duration: 2_760 },
  ],
  'zh-TW': [
    { hour: 8, summary: '整理 README 截圖範圍與三語文案。\n確認需要覆蓋全部核心頁面和設定標籤。', main_apps: 'Notion, Slack, Figma', activity_count: 18, total_duration: 2_940 },
    { hour: 9, summary: '集中更新截圖腳本和目前介面模擬資料。\n同時核對時間線詳情與時段摘要抽屜。', main_apps: 'Cursor, Google Chrome, Terminal', activity_count: 31, total_duration: 3_480 },
    { hour: 10, summary: '完成概覽排行分隔樣式並執行自動化驗證。\n準備產生三種語言的最終文件資產。', main_apps: 'Cursor, Terminal, Work Review', activity_count: 24, total_duration: 2_760 },
  ],
};

function buildHourlySummaries(locale) {
  return hourlySummaries[locale].map((summary, index) => ({
    id: 700 + index,
    date: CAPTURE_DATE,
    representative_screenshots: null,
    created_at: Math.floor(Date.parse('2026-07-28T10:25:00+08:00') / 1000),
    ...summary,
  }));
}

const config = {
  theme: 'light',
  ui_visual_style: 'b',
  background_image: null,
  background_opacity: 0.25,
  background_blur: 1,
  daily_work_goal_minutes: 420,
  standard_work_hours: 7.5,
  work_time_enabled: true,
  work_time_segments: [
    { start_hour: 9, start_minute: 0, end_hour: 12, end_minute: 0 },
    { start_hour: 13, start_minute: 30, end_hour: 18, end_minute: 0 },
  ],
  work_start_hour: 9,
  work_start_minute: 0,
  work_end_hour: 18,
  work_end_minute: 0,
  auto_start: true,
  auto_start_silent: true,
  hide_dock_icon: false,
  lightweight_mode: false,
  break_reminder_enabled: true,
  break_reminder_interval_minutes: 50,
  avatar_enabled: true,
  avatar_scale: 0.9,
  avatar_opacity: 0.82,
  avatar_preset: 'original-standard',
  avatar_persona: 'assistant',
  avatar_click_through: false,
  avatar_proactive_ai_enabled: false,
  idle_threshold_minutes: 5,
  goal_notifications: true,
  memory_enabled: true,
  daily_report_auto_generate_time: '18:30',
  daily_report_custom_prompt: '',
  daily_report_export_dir: null,
  daily_report_auto_export: false,
  daily_report_prompt_presets: [],
  daily_report_system_prompt_override: '',
  daily_report_pinned_blocks: [],
  daily_report_hidden_blocks: [],
  screenshot_interval: 30,
  storage: {
    screenshot_retention_days: 14,
    metadata_retention_days: 60,
    storage_limit_mb: 4096,
    jpeg_quality: 85,
    max_image_width: 1920,
    screenshots_enabled: true,
    screenshot_display_mode: 'active_window',
    screenshot_width_mode: 'auto',
  },
  remote_storage: {
    provider: 'none',
    s3: {
      endpoint: '',
      region: 'us-east-1',
      bucket: '',
      access_key: '',
      secret_key: '',
      path_prefix: '',
      public_url_base: null,
    },
    webdav: {
      url: '',
      username: '',
      password: '',
      path_prefix: '',
      public_url_base: null,
    },
  },
  privacy: {
    app_rules: [
      { app_name: 'Mail', level: 'anonymized' },
      { app_name: 'Password Manager', level: 'ignored' },
    ],
    excluded_keywords: ['confidential', 'private-key'],
    excluded_domains: ['bank.example'],
  },
  app_category_rules: [],
  ai_mode: 'summary',
  ai_provider: { provider: 'ollama', endpoint: 'http://127.0.0.1:11434', api_key: null, model: 'llava', vision_model: 'llava' },
  text_model: { provider: 'ollama', endpoint: 'http://127.0.0.1:11434', api_key: null, model: 'qwen2.5:7b' },
  text_model_profiles: [],
  assistant_web_access_enabled: false,
  memory_semantic_enabled: false,
  node_devices: [],
  node_gateway: { device_name: 'Work Review Demo' },
  mcp_server_enabled: true,
  localhost_api_enabled: true,
  localhost_api_port: 47831,
  localhost_api_host: '127.0.0.1',
  telegram_bot_enabled: false,
  telegram_bot_token: null,
  telegram_bot_proxy: null,
  feishu_bot_enabled: false,
  feishu_app_id: null,
  feishu_app_secret: null,
  wecom_bot_enabled: false,
  dingtalk_bot_enabled: false,
};

const reportContent = {
  'zh-CN': `# 2026 年 7 月 28 日工作日报\n\n> 今天围绕产品界面打磨与文档体验升级展开，核心功能推进稳定。\n\n## 今日概览\n\n完成概览与日报页面的信息层级优化，并统一浅色主题下的视觉节奏。全天有效投入 **5 小时 40 分钟**，工作重心集中在开发与验证。\n\n## 重点进展\n\n- 完成新版概览主视觉和关键指标区域。\n- 优化日报目录、摘要与数据卡片的阅读顺序。\n- 更新多语言 README 页面截图与演示工作流。\n\n## 专注与协作\n\n上午的高专注时段用于实现和调试，下午集中处理视觉验收、文案同步与构建验证。沟通时间保持在合理范围，没有打断主要开发节奏。\n\n## 明日计划\n\n1. 继续检查不同窗口尺寸下的响应式表现。\n2. 完成剩余文档一致性检查。\n3. 整理发布前验证清单。`,
  en: `# Daily Work Report — July 28, 2026\n\n> Today focused on refining the product interface and improving the documentation experience, with steady progress across the core workflow.\n\n## Today's Overview\n\nCompleted the information hierarchy updates for Overview and Daily Report, while aligning the visual rhythm of the light theme. Effective work time reached **5 hours 40 minutes**, led by development and verification.\n\n## Key Progress\n\n- Completed the refreshed Overview hero and KPI area.\n- Improved the reading order of the report outline, summary, and data cards.\n- Updated multilingual README screenshots and the workflow preview.\n\n## Focus and Collaboration\n\nThe morning focus window was used for implementation and debugging. The afternoon centered on visual QA, copy synchronization, and build verification. Communication stayed focused without disrupting the main development flow.\n\n## Next Steps\n\n1. Continue checking responsive behavior at different window sizes.\n2. Complete the remaining documentation consistency review.\n3. Finalize the pre-release verification checklist.`,
  'zh-TW': `# 2026 年 7 月 28 日工作日報\n\n> 今天圍繞產品介面打磨與文件體驗升級展開，核心功能推進穩定。\n\n## 今日概覽\n\n完成概覽與日報頁面的資訊層級優化，並統一淺色主題下的視覺節奏。全天有效投入 **5 小時 40 分鐘**，工作重心集中在開發與驗證。\n\n## 重點進展\n\n- 完成新版概覽主視覺和關鍵指標區域。\n- 優化日報目錄、摘要與資料卡片的閱讀順序。\n- 更新多語言 README 頁面截圖與示範工作流。\n\n## 專注與協作\n\n上午的高專注時段用於實作和除錯，下午集中處理視覺驗收、文案同步與構建驗證。溝通時間保持在合理範圍，沒有打斷主要開發節奏。\n\n## 明日計畫\n\n1. 繼續檢查不同視窗尺寸下的響應式表現。\n2. 完成剩餘文件一致性檢查。\n3. 整理發布前驗證清單。`,
};

function buildReport(locale) {
  return {
    date: CAPTURE_DATE,
    locale,
    content: reportContent[locale],
    ai_mode: 'summary',
    model_name: 'Local Summary Model',
    fallback_reason: null,
    created_at: 1785205200,
  };
}

function mockPayload(locale) {
  return {
    locale,
    captureDate: CAPTURE_DATE,
    config,
    currentStats,
    previousStats,
    categories,
    hourlyAppBreakdown,
    rangeDailyTotals,
    timelineActivities: buildTimelineActivities(locale),
    hourlySummaries: buildHourlySummaries(locale),
    domains,
    report: buildReport(locale),
  };
}

async function installTauriMock(context, locale) {
  await context.addInitScript((mock) => {
    window.localStorage.setItem('work-review.locale', mock.locale);
    window.localStorage.setItem('theme', 'light');

    const callbacks = new Map();
    let callbackId = 1;
    const registerCallback = (callback, once = false) => {
      const id = callbackId++;
      callbacks.set(id, (data) => {
        if (once) callbacks.delete(id);
        return callback?.(data);
      });
      return id;
    };

    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: 'main' },
        currentWebview: { windowLabel: 'main', label: 'main' },
      },
      callbacks,
      transformCallback: registerCallback,
      unregisterCallback: (id) => callbacks.delete(id),
      runCallback: (id, data) => callbacks.get(id)?.(data),
      convertFileSrc: (filePath) => filePath,
      invoke: async (cmd, args = {}) => {
        switch (cmd) {
          case 'plugin:event|listen':
            return args.handler;
          case 'plugin:event|unlisten':
          case 'plugin:event|emit':
          case 'plugin:event|emit_to':
            return null;
          case 'plugin:window|is_visible':
            return true;
          case 'get_platform':
          case 'get_runtime_platform':
            return 'macos';
          case 'get_config':
            return structuredClone(mock.config);
          case 'save_config':
          case 'set_app_locale':
          case 'set_report_block_preference':
          case 'update_report_content':
          case 'pause_recording':
          case 'resume_recording':
            return null;
          case 'get_recording_state':
            return [true, false];
          case 'get_background_image':
            return null;
          case 'get_today_stats':
          case 'get_daily_stats':
            return args.date === '2026-07-21'
              ? structuredClone(mock.previousStats)
              : structuredClone(mock.currentStats);
          case 'get_overview_stats':
            return args.mode === 'date' && args.dateFrom === '2026-07-21'
              ? structuredClone(mock.previousStats)
              : structuredClone(mock.currentStats);
          case 'get_overview_domains':
            return { domains: structuredClone(mock.domains), total_count: mock.domains.length };
          case 'get_timeline':
            return args.date === mock.captureDate ? structuredClone(mock.timelineActivities) : [];
          case 'get_activity':
            return structuredClone(mock.timelineActivities.find((activity) => activity.id === args.id) || null);
          case 'get_hourly_summaries':
            return args.date === mock.captureDate ? structuredClone(mock.hourlySummaries) : [];
          case 'get_screenshot_thumbnail':
          case 'get_screenshot_full':
            return window.__README_CAPTURE_SCREENSHOT_BASE64__ || null;
          case 'get_hourly_app_breakdown':
            return structuredClone(mock.hourlyAppBreakdown);
          case 'get_range_daily_totals':
            return structuredClone(mock.rangeDailyTotals);
          case 'get_saved_report':
            return args.date === mock.captureDate ? structuredClone(mock.report) : null;
          case 'generate_report':
            return null;
          case 'get_categories':
            return structuredClone(mock.categories);
          case 'get_semantic_categories':
          case 'get_custom_semantic_categories':
            return [];
          case 'list_assistant_conversations':
          case 'get_assistant_messages':
            return [];
          case 'get_ai_providers':
            return [
              { id: 'ollama', name: 'Ollama', endpoint: 'http://127.0.0.1:11434' },
              { id: 'openai', name: 'OpenAI', endpoint: 'https://api.openai.com/v1' },
            ];
          case 'get_storage_stats':
            return {
              total_files: 1286,
              total_size_mb: 768,
              storage_limit_mb: 4096,
              retention_days: 14,
              oldest_file_date: '2026-07-15',
            };
          case 'get_data_dir':
            return '/Users/demo/Library/Application Support/Work Review';
          case 'get_default_data_dir':
            return '/Users/demo/Library/Application Support/Work Review';
          case 'get_node_gateway_status':
            return { deviceId: 'work-review-demo', protocolVersion: '1.0', deviceName: 'Work Review Demo' };
          case 'get_localhost_api_status':
            return { enabled: true, baseUrl: 'http://127.0.0.1:47831', tokenPreview: 'wr_••••••••demo', lastError: null };
          case 'get_telegram_bot_status':
            return { running: false, starting: false, allowedChatIds: [], lastError: null };
          case 'get_update_settings':
            return { autoCheck: true, lastCheck: null };
          case 'plugin:app|version':
            return '1.0.56';
          case 'check_permissions':
            return {
              screen_capture: true,
              accessibility: true,
              input_monitoring: true,
              screenshot_supported: true,
              avatar_input_supported: true,
              all_granted: true,
            };
          case 'get_running_apps':
            return ['Cursor', 'Google Chrome', 'Terminal', 'Slack', 'Notion', 'Figma'];
          case 'get_recent_apps':
            return ['Mail', 'Calendar', 'Preview', 'Finder'];
          case 'is_autostart_enabled':
            return true;
          case 'should_check_updates':
            return false;
          case 'get_app_icon':
            return null;
          default:
            console.warn(`[README capture] 未处理的 Tauri 命令: ${cmd}`);
            return null;
        }
      },
    };
  }, mockPayload(locale));
}

async function waitForStablePage(page, selector) {
  await page.waitForSelector(selector, { state: 'visible', timeout: 20_000 });
  await page.waitForFunction(() => document.fonts?.status === 'loaded', null, { timeout: 10_000 });
  await page.waitForTimeout(250);
}

async function saveScreenshot(page, outputPath) {
  // 固定鼠标和焦点，避免 hover/focus 与颜色过渡造成二进制级截图差异。
  await page.mouse.move(0, 0);
  await page.evaluate(() => {
    if (document.activeElement instanceof HTMLElement) {
      document.activeElement.blur();
    }
  });
  await page.screenshot({
    path: outputPath,
    fullPage: false,
    animations: 'disabled',
    caret: 'hide',
    style: `
      *, *::before, *::after {
        animation: none !important;
        caret-color: transparent !important;
        transition: none !important;
      }
      .overview-panel-featured .page-control-btn-icon {
        align-self: flex-start !important;
        height: 36px !important;
        min-height: 36px !important;
      }
    `,
  });
}

async function capturePage(page, route, selector, outputPath) {
  await page.goto(new URL(route, BASE_URL).href, { waitUntil: 'networkidle' });
  await waitForStablePage(page, selector);
  await saveScreenshot(page, outputPath);
}

async function captureSettingsTab(page, tabIndex, outputPath) {
  const tab = page.locator('.settings-tab-rail-item').nth(tabIndex);
  await tab.click();
  await page.waitForFunction(
    (index) => document.querySelectorAll('.settings-tab-rail-item')[index]?.getAttribute('aria-current') === 'page',
    tabIndex,
  );
  await waitForStablePage(page, '.settings-stage-shell .settings-card');
  await saveScreenshot(page, outputPath);
}

async function checkCaptureDependencies() {
  for (const command of ['ffmpeg', 'ffprobe']) {
    const result = spawnSync(command, ['-version'], { encoding: 'utf8' });
    if (result.error?.code === 'ENOENT' || result.status !== 0) {
      throw new Error(`未找到可用的 ${command}。请先安装 ffmpeg 工具链，并确认 \`${command} -version\` 可正常执行。`);
    }
  }

  try {
    await access(chromium.executablePath());
  } catch {
    throw new Error('未找到 Playwright Chromium。请先执行 `npx playwright install chromium`。');
  }
}

function createWorkflowGif(framePaths, outputPath) {
  const inputArgs = framePaths.flatMap((framePath) => [
    '-loop', '1',
    '-framerate', '8',
    '-t', '2.5',
    '-i', framePath,
  ]);
  const result = spawnSync('ffmpeg', [
    '-hide_banner',
    '-loglevel', 'error',
    '-y',
    ...inputArgs,
    '-filter_complex',
    '[0:v][1:v][2:v]concat=n=3:v=1:a=0,fps=8,scale=960:541:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=192[p];[s1][p]paletteuse=dither=sierra2_4a',
    '-loop', '0',
    outputPath,
  ], { encoding: 'utf8' });

  if (result.status !== 0) {
    throw new Error(`ffmpeg 生成 ${outputPath} 失败：${result.stderr || result.stdout}`);
  }
}

async function main() {
  validateCaptureFixtures();
  await checkCaptureDependencies();
  const browser = await chromium.launch({
    headless: true,
    args: [
      '--disable-gpu',
      '--disable-lcd-text',
      '--disable-skia-runtime-opts',
      '--font-render-hinting=none',
      '--force-color-profile=srgb',
    ],
  });
  try {
    for (const item of LOCALES) {
      const outDir = path.resolve('docs', item.dir);
      await mkdir(outDir, { recursive: true });

      const context = await browser.newContext(createCaptureContextOptions(item.locale));
      await context.route('**/*', (route) => {
        if (isAllowedRequestUrl(route.request().url())) {
          return route.continue();
        }
        return route.abort();
      });
      await installTauriMock(context, item.locale);

      const pageErrors = [];
      const page = await context.newPage();
      page.on('console', (message) => {
        if (message.type() === 'warning' || message.type() === 'error') {
          console.warn(`[${item.locale}] 浏览器 ${message.type()}: ${message.text()}`);
        }
      });
      page.on('pageerror', (error) => {
        pageErrors.push(error.message);
        console.error(`[${item.locale}] 页面错误:`, error);
      });
      await page.clock.install({ time: new Date(FIXED_TIME) });

      const outputPath = (label) => path.join(outDir, `${label}.png`);
      const overviewPath = outputPath('概览');
      const timelinePath = outputPath('时间线');
      const reportPath = outputPath('日报');

      console.log(`[${item.locale}] 截取概览`);
      await capturePage(page, '/#/', '.overview-editorial-shell', overviewPath);
      await page.waitForFunction(() => !document.querySelector('.overview-editorial-shell .animate-pulse'), null, { timeout: 15_000 }).catch(() => {});
      await saveScreenshot(page, overviewPath);
      const overviewBase64 = (await readFile(overviewPath)).toString('base64');
      await page.evaluate((value) => {
        window.__README_CAPTURE_SCREENSHOT_BASE64__ = value;
      }, overviewBase64);

      console.log(`[${item.locale}] 截取时间线`);
      await capturePage(
        page,
        `/#/timeline?date=${CAPTURE_DATE}`,
        '.timeline-editorial-board',
        timelinePath,
      );

      console.log(`[${item.locale}] 截取时间线详情`);
      await page.locator('.timeline-entry').first().click();
      await waitForStablePage(page, '.timeline-detail-drawer');
      await page.waitForSelector('.timeline-detail-preview-image', { state: 'visible', timeout: 10_000 });
      await saveScreenshot(page, outputPath('时间线详情'));
      await page.locator('.timeline-detail-header button').last().click();
      await page.waitForSelector('.timeline-detail-drawer', { state: 'detached' });

      console.log(`[${item.locale}] 截取时段摘要`);
      await page.locator('.timeline-summary-action').click();
      await waitForStablePage(page, '.hourly-summary-drawer');
      await page.waitForSelector('.hourly-summary-item', { state: 'visible', timeout: 10_000 });
      await saveScreenshot(page, outputPath('小时总结'));
      await page.locator('.hourly-summary-close').click();
      await page.waitForSelector('.hourly-summary-drawer', { state: 'detached' });

      console.log(`[${item.locale}] 截取日报`);
      await capturePage(page, '/#/report', '.report-article-card', reportPath);

      console.log(`[${item.locale}] 截取助手`);
      await capturePage(page, '/#/ask', '.ask-workbench-shell', outputPath('助手'));

      console.log(`[${item.locale}] 截取设置页`);
      await capturePage(page, '/#/settings', '.settings-stage-shell .settings-card', outputPath('设置-通用'));
      const settingsTabs = [
        [1, '设置-外观'],
        [2, '设置-AI模型'],
        [3, '设置-桌面化身'],
        [4, '设置-隐私'],
        [5, '设置-存储'],
        [6, '接入管理'],
      ];
      for (const [tabIndex, label] of settingsTabs) {
        await captureSettingsTab(page, tabIndex, outputPath(label));
      }

      console.log(`[${item.locale}] 截取关于`);
      await capturePage(page, '/#/about', '.about-editorial-shell', outputPath('关于'));

      for (const label of README_SCREENSHOT_LABELS) {
        await access(outputPath(label));
      }

      if (pageErrors.length > 0) {
        throw new Error(`[${item.locale}] 截图过程中出现页面错误：${pageErrors.join(' | ')}`);
      }

      console.log(`[${item.locale}] 生成工作流 GIF`);
      await createWorkflowGif(
        [overviewPath, timelinePath, reportPath],
        path.join(outDir, '工作流.gif'),
      );

      await context.close();
    }
  } finally {
    await browser.close();
  }
}

const entryPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : '';
if (import.meta.url === entryPath) {
  main().catch((error) => {
    console.error(error);
    process.exit(1);
  });
}
