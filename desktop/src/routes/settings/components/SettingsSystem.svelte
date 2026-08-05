<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { locale, t } from '$lib/i18n/index.js';
  import { showToast } from '$lib/stores/toast.js';

  $: currentLocale = $locale;

  let runtimePlatform = '';
  let permissionStatus = null;
  let linuxSessionSupport = null;
  let refreshing = false;
  let gnomeExtensionInstalling = false;
  let pendingPermissionItem = null;
  let permissionDetailsExpanded = false;
  let permissionDetailsTouched = false;

  function normalizePermissionStatus(rawStatus) {
    if (!rawStatus || typeof rawStatus !== 'object') {
      return null;
    }

    return {
      screenCapture: Boolean(rawStatus.screen_capture),
      accessibility: Boolean(rawStatus.accessibility),
      inputMonitoring: Boolean(rawStatus.input_monitoring),
      screenshotSupported: Boolean(rawStatus.screenshot_supported),
      avatarInputSupported: Boolean(rawStatus.avatar_input_supported),
      allGranted: Boolean(rawStatus.all_granted),
    };
  }

  function buildPermissionSummary(platform, rawPermissionStatus, support, macCount) {
    if (platform === 'macos' && rawPermissionStatus) {
      return {
        ready: macCount,
        total: 3,
        pending: Math.max(0, 3 - macCount),
        attention: macCount < 3,
        platformLabel: 'macOS',
      };
    }

    if (platform === 'windows' && rawPermissionStatus) {
      const ready = Number(rawPermissionStatus.screenshotSupported) + Number(rawPermissionStatus.avatarInputSupported);
      return {
        ready,
        total: 2,
        pending: Math.max(0, 2 - ready),
        attention: ready < 2,
        platformLabel: 'Windows',
      };
    }

    if (platform === 'linux' && support) {
      const hasGnomeExtensionRow = support.desktopEnvironment === 'gnome';
      const total = hasGnomeExtensionRow ? 3 : 2;
      const ready =
        Number(support.screenshotSupported) +
        Number(support.avatarInputSupportLevel !== 'none') +
        Number(hasGnomeExtensionRow && support.gnomeAvatarExtensionEnabled);

      return {
        ready,
        total,
        pending: Math.max(0, total - ready),
        attention: ready < total || Boolean(support.gnomeAvatarExtensionNeedsRelogin),
        platformLabel: 'Linux',
      };
    }

    return {
      ready: 0,
      total: 0,
      pending: 0,
      attention: false,
      platformLabel: '',
    };
  }

  $: macPermissionItems = permissionStatus
    ? [
        {
          id: 'screen_capture',
          labelKey: 'settingsAppearance.avatarScreenCapturePermission',
          descriptionKey: 'settingsAppearance.avatarScreenCapturePermissionHint',
          granted: permissionStatus.screenCapture,
        },
        {
          id: 'accessibility',
          labelKey: 'settingsAppearance.avatarAccessibilityPermission',
          descriptionKey: 'settingsAppearance.avatarAccessibilityPermissionHint',
          granted: permissionStatus.accessibility,
        },
        {
          id: 'input_monitoring',
          labelKey: 'settingsAppearance.avatarInputMonitoringPermission',
          descriptionKey: 'settingsAppearance.avatarInputMonitoringPermissionHint',
          granted: permissionStatus.inputMonitoring,
        },
      ]
    : [];

  $: macReadyCount = macPermissionItems.filter((item) => item.granted).length;
  $: linuxInputSupportLabelKey = linuxSessionSupport?.avatarInputSupportLevel === 'full'
    ? 'settingsAppearance.avatarInputFull'
    : linuxSessionSupport?.avatarInputSupportLevel === 'mouse-only'
      ? 'settingsAppearance.avatarInputMouseOnly'
      : 'settingsAppearance.avatarInputUnavailable';
  $: permissionSummary = buildPermissionSummary(
    runtimePlatform,
    permissionStatus,
    linuxSessionSupport,
    macReadyCount
  );
  $: permissionNeedsAttention = permissionSummary.attention;
  $: if (!permissionDetailsTouched) {
    permissionDetailsExpanded = permissionNeedsAttention;
  }
  $: showPermissionDetails = permissionDetailsExpanded;

  async function refreshPlatformSupport(showNotice = false) {
    refreshing = true;

    try {
      runtimePlatform = await invoke('get_runtime_platform');
      permissionStatus = normalizePermissionStatus(await invoke('check_permissions'));

      if (runtimePlatform === 'linux') {
        linuxSessionSupport = await invoke('get_linux_session_support');
      } else {
        linuxSessionSupport = null;
      }

      if (showNotice && runtimePlatform === 'macos') {
        showToast(t('settingsGeneral.permissionsRefreshNotice'), 'info');
      }
    } catch (error) {
      console.error('读取系统权限状态失败:', error);
      showToast(t('settingsGeneral.permissionsLoadFailed', { error }), 'error');
      runtimePlatform = '';
      permissionStatus = null;
      linuxSessionSupport = null;
    } finally {
      refreshing = false;
    }
  }

  onMount(() => {
    refreshPlatformSupport();
  });

  function handleRefreshClick() {
    refreshPlatformSupport(true);
  }

  function togglePermissionDetails() {
    permissionDetailsTouched = true;
    permissionDetailsExpanded = !permissionDetailsExpanded;
  }

  function beginPermissionSetup(item) {
    pendingPermissionItem = item;
  }

  function closePermissionSetup() {
    pendingPermissionItem = null;
  }

  async function openPermissionSettings(permission) {
    try {
      await invoke('open_permission_settings', { permission });
    } catch (error) {
      console.error('打开系统权限设置失败:', error);
      showToast(t('settingsGeneral.permissionsOpenFailed', { error }), 'error');
    }
  }

  async function confirmPermissionSetup() {
    const permission = pendingPermissionItem?.id;
    closePermissionSetup();

    if (!permission) {
      return;
    }

    await openPermissionSettings(permission);
  }

  async function installGnomeAvatarExtension() {
    if (gnomeExtensionInstalling) {
      return;
    }

    gnomeExtensionInstalling = true;
    try {
      const result = await invoke('install_gnome_avatar_extension');
      showToast(
        result.message,
        result.requiresRelogin ? 'warning' : result.enabled ? 'success' : 'info'
      );
      await refreshPlatformSupport();
    } catch (error) {
      console.error('自动安装 GNOME 桌宠扩展失败:', error);
      showToast(t('settingsAppearance.avatarGnomeExtensionInstallFailed', { error }), 'error');
    } finally {
      gnomeExtensionInstalling = false;
    }
  }

  function permissionSetupMessageKey(permissionId) {
    if (permissionId === 'input_monitoring') {
      return 'settingsGeneral.permissionsInputMonitoringGuide';
    }

    if (permissionId === 'accessibility') {
      return 'settingsGeneral.permissionsAccessibilityGuide';
    }

    return 'settingsGeneral.permissionsScreenCaptureGuide';
  }
</script>

<div class="settings-block permission-overview" data-locale={currentLocale}>
  <div class="permission-summary-strip">
    <div class="permission-summary-copy">
      <div class="permission-summary-title">{t('settingsGeneral.permissionsTitle')}</div>
      <div class="permission-summary-meta">
        {#if permissionSummary.total > 0}
          <span class={`permission-summary-badge ${permissionNeedsAttention ? 'permission-summary-badge-warn' : 'permission-summary-badge-ready'}`}>
            {permissionNeedsAttention
              ? t('settingsGeneral.permissionsSummaryAttention', {
                  ready: permissionSummary.ready,
                  total: permissionSummary.total,
                  pending: permissionSummary.pending,
                })
              : t('settingsGeneral.permissionsSummaryReady', {
                  ready: permissionSummary.ready,
                  total: permissionSummary.total,
                })}
          </span>
          <span class="permission-summary-platform">{permissionSummary.platformLabel}</span>
        {:else}
          <span class="permission-summary-hint">{t('settingsGeneral.permissionsDescription')}</span>
        {/if}
      </div>
    </div>

    <div class="permission-summary-actions">
      <button
        type="button"
        class="permission-summary-toggle"
        on:click={togglePermissionDetails}
      >
        {showPermissionDetails
          ? t('settingsGeneral.permissionsHideDetails')
          : t('settingsGeneral.permissionsDetails')}
      </button>

      <button
        type="button"
        class="permission-refresh-button"
        on:click={handleRefreshClick}
        disabled={refreshing}
      >
        {refreshing
          ? t('settingsGeneral.permissionsRefreshing')
          : t('settingsGeneral.permissionsRefresh')}
      </button>
    </div>
  </div>

  {#if showPermissionDetails}
    <div class="permission-details-panel">
      {#if runtimePlatform === 'macos' && permissionStatus}
        {#each macPermissionItems as item}
          <div class={`permission-item-card ${item.granted ? 'permission-item-card-ready' : 'permission-item-card-action'}`}>
            <div class="permission-item-main">
              <div class="permission-item-leading">
                <span class={`permission-item-marker ${item.granted ? 'permission-item-marker-ready' : 'permission-item-marker-action'}`}></span>
                <div class="min-w-0 flex-1">
                  <div class="permission-item-title">{t(item.labelKey)}</div>
                  <div class="permission-item-copy">{t(item.descriptionKey)}</div>
                </div>
              </div>

              {#if item.granted}
                <div class="permission-status-pill permission-status-pill-ready">
                  {t('settingsGeneral.permissionsMacStatusAvailable')}
                </div>
              {:else}
                <button
                  type="button"
                  class="permission-status-pill permission-status-pill-action"
                  on:click={() => beginPermissionSetup(item)}
                >
                  {t('settingsGeneral.permissionsOpen')}
                </button>
              {/if}
            </div>
          </div>
        {/each}
      {:else if runtimePlatform === 'windows' && permissionStatus}
        <div class={`permission-item-card ${permissionStatus.screenshotSupported ? 'permission-item-card-ready' : 'permission-item-card-action'}`}>
          <div class="permission-item-main">
            <div class="permission-item-leading">
              <span class={`permission-item-marker ${permissionStatus.screenshotSupported ? 'permission-item-marker-ready' : 'permission-item-marker-action'}`}></span>
              <div class="min-w-0 flex-1">
                <div class="permission-item-title">{t('settingsAppearance.avatarScreenshotSupportTitle')}</div>
                <div class="permission-item-copy">{t('settingsGeneral.permissionsWindowsScreenshotHint')}</div>
              </div>
            </div>
            <div class={`permission-status-pill ${permissionStatus.screenshotSupported ? 'permission-status-pill-ready' : 'permission-status-pill-warn'}`}>
              {permissionStatus.screenshotSupported
                ? t('settingsGeneral.permissionGranted')
                : t('settingsGeneral.permissionMissing')}
            </div>
          </div>
        </div>

        <div class={`permission-item-card ${permissionStatus.avatarInputSupported ? 'permission-item-card-ready' : 'permission-item-card-action'}`}>
          <div class="permission-item-main">
            <div class="permission-item-leading">
              <span class={`permission-item-marker ${permissionStatus.avatarInputSupported ? 'permission-item-marker-ready' : 'permission-item-marker-action'}`}></span>
              <div class="min-w-0 flex-1">
                <div class="permission-item-title">{t('settingsAppearance.avatarInputSupportTitle')}</div>
                <div class="permission-item-copy">{t('settingsGeneral.permissionsWindowsInputHint')}</div>
              </div>
            </div>
            <div class={`permission-status-pill ${permissionStatus.avatarInputSupported ? 'permission-status-pill-ready' : 'permission-status-pill-warn'}`}>
              {permissionStatus.avatarInputSupported
                ? t('settingsGeneral.permissionGranted')
                : t('settingsGeneral.permissionMissing')}
            </div>
          </div>
        </div>
      {:else if runtimePlatform === 'linux' && linuxSessionSupport}
        <div class={`permission-item-card ${linuxSessionSupport.screenshotSupported ? 'permission-item-card-ready' : 'permission-item-card-action'}`}>
          <div class="permission-item-main">
            <div class="permission-item-leading">
              <span class={`permission-item-marker ${linuxSessionSupport.screenshotSupported ? 'permission-item-marker-ready' : 'permission-item-marker-action'}`}></span>
              <div class="min-w-0 flex-1">
                <div class="permission-item-title">{t('settingsAppearance.avatarScreenshotSupportTitle')}</div>
                <div class="permission-item-copy">{t('settingsGeneral.permissionsLinuxScreenshotHint')}</div>
              </div>
            </div>
            <div class={`permission-status-pill ${linuxSessionSupport.screenshotSupported ? 'permission-status-pill-ready' : 'permission-status-pill-warn'}`}>
              {linuxSessionSupport.screenshotSupported
                ? t('settingsGeneral.permissionGranted')
                : t('settingsGeneral.permissionMissing')}
            </div>
          </div>
        </div>

        <div class={`permission-item-card ${linuxSessionSupport.avatarInputSupportLevel === 'none' ? 'permission-item-card-action' : 'permission-item-card-ready'}`}>
          <div class="permission-item-main">
            <div class="permission-item-leading">
              <span class={`permission-item-marker ${linuxSessionSupport.avatarInputSupportLevel === 'none' ? 'permission-item-marker-action' : 'permission-item-marker-ready'}`}></span>
              <div class="min-w-0 flex-1">
                <div class="permission-item-title">{t('settingsAppearance.avatarInputSupportTitle')}</div>
                <div class="permission-item-copy">
                  {t('settingsGeneral.permissionsLinuxInputHint')}
                  <span class="permission-inline-meta">
                    {linuxSessionSupport.sessionType} / {linuxSessionSupport.desktopEnvironment}
                  </span>
                </div>
              </div>
            </div>
            <div class={`permission-status-pill ${linuxSessionSupport.avatarInputSupportLevel === 'none' ? 'permission-status-pill-warn' : 'permission-status-pill-ready'}`}>
              {t(linuxInputSupportLabelKey)}
            </div>
          </div>
        </div>

        {#if linuxSessionSupport.desktopEnvironment === 'gnome'}
          <div class={`permission-item-card ${(linuxSessionSupport.gnomeAvatarExtensionEnabled || linuxSessionSupport.gnomeAvatarExtensionNeedsRelogin) ? 'permission-item-card-ready' : 'permission-item-card-action'}`}>
            <div class="permission-item-main">
              <div class="permission-item-leading">
                <span class={`permission-item-marker ${(linuxSessionSupport.gnomeAvatarExtensionEnabled || linuxSessionSupport.gnomeAvatarExtensionNeedsRelogin) ? 'permission-item-marker-ready' : 'permission-item-marker-action'}`}></span>
                <div class="min-w-0 flex-1">
                  <div class="permission-item-title">{t('settingsAppearance.avatarGnomeExtensionTitle')}</div>
                  <div class="permission-item-copy">{t('settingsGeneral.permissionsGnomeExtensionHint')}</div>
                </div>
              </div>

              {#if linuxSessionSupport.gnomeAvatarExtensionEnabled}
                <div class="permission-status-pill permission-status-pill-ready">
                  {t('settingsAppearance.avatarGnomeExtensionReady')}
                </div>
              {:else if linuxSessionSupport.gnomeAvatarExtensionNeedsRelogin}
                <div class="permission-status-pill permission-status-pill-warn">
                  {t('settingsAppearance.avatarGnomeExtensionRelogin')}
                </div>
              {:else}
                <button
                  type="button"
                  class="permission-status-pill permission-status-pill-action"
                  on:click={installGnomeAvatarExtension}
                  disabled={gnomeExtensionInstalling}
                >
                  {gnomeExtensionInstalling
                    ? t('settingsAppearance.avatarGnomeExtensionInstalling')
                    : t('settingsAppearance.avatarGnomeExtensionInstall')}
                </button>
              {/if}
            </div>
          </div>
        {/if}
      {/if}
    </div>
  {/if}
</div>

{#if pendingPermissionItem}
  <div class="fixed inset-0 z-[90] flex items-end justify-center bg-slate-950/42 px-4 pb-6 pt-10 backdrop-blur-sm sm:items-center">
    <div
      class="permission-setup-dialog w-full max-w-[440px] rounded-lg bg-white p-6 dark:bg-[#161b22]"
      role="dialog"
      aria-modal="true"
      aria-labelledby="permission-setup-title"
    >
      <div class="permission-setup-accent"></div>
      <h3 id="permission-setup-title" class="permission-setup-title">
        {t(pendingPermissionItem.labelKey)}
      </h3>

      <p class="permission-setup-copy">
        {t(permissionSetupMessageKey(pendingPermissionItem.id))}
      </p>

      <div class="permission-setup-actions">
        <button
          type="button"
          class="permission-setup-button permission-setup-button-muted"
          on:click={closePermissionSetup}
        >
          {t('settingsGeneral.permissionsLater')}
        </button>
        <button
          type="button"
          class="permission-setup-button permission-setup-button-primary"
          on:click={confirmPermissionSetup}
        >
          {t('settingsGeneral.permissionsOpenNow')}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .permission-overview {}

  .permission-summary-strip {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px 14px;
    min-height: 42px;
  }

  .permission-summary-copy {
    min-width: 0;
    flex: 1;
  }

  .permission-summary-title {
    font-size: 13px;
    font-weight: 700;
    line-height: 1.35;
    color: rgb(30, 41, 59);
  }

  .permission-summary-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }

  .permission-summary-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 24px;
    padding: 0 10px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 700;
    white-space: nowrap;
  }

  .permission-summary-badge-ready {
    background: rgba(16, 185, 129, 0.12);
    color: rgb(5, 150, 105);
  }

  .permission-summary-badge-warn {
    background: rgba(245, 158, 11, 0.14);
    color: rgb(180, 83, 9);
  }

  .permission-summary-platform,
  .permission-summary-hint {
    font-size: 11px;
    font-weight: 600;
    color: rgb(100, 116, 139);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .permission-summary-actions {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .permission-summary-toggle,
  .permission-refresh-button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 32px;
    padding: 0 12px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
    transition: border-color 160ms ease, background-color 160ms ease, color 160ms ease;
  }

  .permission-summary-toggle {
    border: 1px solid transparent;
    background: rgba(241, 245, 249, 0.86);
    color: rgb(71, 85, 105);
  }

  .permission-summary-toggle:hover {
    background: rgb(226, 232, 240);
    color: rgb(51, 65, 85);
  }

  .permission-refresh-button {
    border: 1px solid rgba(203, 213, 225, 0.96);
    background: rgba(255, 255, 255, 0.96);
    color: rgb(51, 65, 85);
  }

  .permission-refresh-button:hover {
    border-color: rgb(148, 163, 184);
    background: rgb(248, 250, 252);
  }

  .permission-refresh-button:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .permission-details-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid rgba(226, 232, 240, 0.92);
  }

  .permission-item-card {
    border-radius: 8px;
    border: 1px solid rgba(226, 232, 240, 0.9);
    background: rgba(248, 250, 252, 0.7);
    padding: 10px 12px;
  }

  .permission-item-card-ready {
    background: rgba(240, 253, 244, 0.72);
  }

  .permission-item-card-action {
    background: rgba(255, 241, 242, 0.72);
  }

  .permission-item-main {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .permission-item-leading {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    min-width: 0;
    flex: 1;
  }

  .permission-item-marker {
    width: 7px;
    height: 7px;
    border-radius: 999px;
    margin-top: 6px;
    flex-shrink: 0;
  }

  .permission-item-marker-ready {
    background: rgb(16, 185, 129);
    box-shadow: 0 0 0 3px rgba(16, 185, 129, 0.1);
  }

  .permission-item-marker-action {
    background: rgb(244, 63, 94);
    box-shadow: 0 0 0 3px rgba(244, 63, 94, 0.1);
  }

  .permission-item-title {
    font-size: 12.5px;
    font-weight: 700;
    line-height: 1.35;
    color: rgb(30, 41, 59);
  }

  .permission-item-copy {
    margin-top: 2px;
    font-size: 11px;
    line-height: 1.55;
    color: rgb(100, 116, 139);
  }

  .permission-inline-meta {
    margin-left: 8px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
    font-size: 10px;
    color: rgb(148, 163, 184);
  }

  .permission-status-pill {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 28px;
    padding: 0 10px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 700;
    white-space: nowrap;
    transition: background-color 160ms ease, color 160ms ease, border-color 160ms ease;
  }

  .permission-status-pill-ready {
    background: rgba(16, 185, 129, 0.12);
    color: rgb(5, 150, 105);
  }

  .permission-status-pill-warn {
    background: rgba(245, 158, 11, 0.14);
    color: rgb(180, 83, 9);
  }

  .permission-status-pill-action {
    background: rgba(244, 63, 94, 0.08);
    color: rgb(225, 29, 72);
  }

  .permission-status-pill-action:hover {
    background: rgba(244, 63, 94, 0.12);
    color: rgb(190, 24, 93);
  }

  .permission-status-pill:disabled {
    cursor: wait;
    opacity: 0.64;
  }

  .permission-setup-dialog {
    position: relative;
    overflow: hidden;
    border: 1px solid rgba(226, 232, 240, 0.96);
    box-shadow: 0 18px 42px rgba(15, 23, 42, 0.16);
  }

  .permission-setup-accent {
    width: 36px;
    height: 4px;
    border-radius: 999px;
    background: linear-gradient(90deg, rgba(59, 130, 246, 0.88), rgba(99, 102, 241, 0.74));
    margin-bottom: 16px;
  }

  .permission-setup-title {
    font-size: 24px;
    line-height: 1.2;
    font-weight: 700;
    letter-spacing: 0;
    color: rgb(15, 23, 42);
  }

  .permission-setup-copy {
    margin-top: 14px;
    font-size: 14px;
    line-height: 1.75;
    color: rgb(51, 65, 85);
  }

  .permission-setup-actions {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 22px;
  }

  .permission-setup-button {
    min-height: 44px;
    border-radius: 8px;
    padding: 0 16px;
    font-size: 14px;
    font-weight: 700;
    transition: background-color 160ms ease, color 160ms ease, transform 160ms ease;
  }

  .permission-setup-button:hover {
    transform: translateY(-1px);
  }

  .permission-setup-button-muted {
    background: rgb(241, 245, 249);
    color: rgb(51, 65, 85);
  }

  .permission-setup-button-muted:hover {
    background: rgb(226, 232, 240);
  }

  .permission-setup-button-primary {
    background: rgb(59, 130, 246);
    color: white;
  }

  .permission-setup-button-primary:hover {
    background: rgb(37, 99, 235);
  }

  :global(.dark) .permission-overview-platform {
    color: rgb(148, 163, 184);
  }

  :global(.dark) .permission-summary-title {
    color: rgb(241, 245, 249);
  }

  :global(.dark) .permission-summary-platform,
  :global(.dark) .permission-summary-hint {
    color: rgb(148, 163, 184);
  }

  :global(.dark) .permission-summary-toggle {
    background: rgba(30, 41, 59, 0.7);
    color: rgb(203, 213, 225);
  }

  :global(.dark) .permission-summary-toggle:hover {
    background: rgba(51, 65, 85, 0.78);
    color: rgb(241, 245, 249);
  }

  :global(.dark) .permission-refresh-button {
    border-color: rgba(71, 85, 105, 0.9);
    background: rgba(15, 23, 42, 0.72);
    color: rgb(226, 232, 240);
  }

  :global(.dark) .permission-refresh-button:hover {
    border-color: rgba(100, 116, 139, 0.96);
    background: rgba(30, 41, 59, 0.82);
  }

  :global(.dark) .permission-details-panel {
    border-top-color: rgba(51, 65, 85, 0.92);
  }

  :global(.dark) .permission-item-card {
    border-color: rgba(51, 65, 85, 0.92);
    background: rgba(15, 23, 42, 0.44);
  }

  :global(.dark) .permission-item-card-ready {
    background: rgba(6, 78, 59, 0.18);
  }

  :global(.dark) .permission-item-card-action {
    background: rgba(76, 5, 25, 0.18);
  }

  :global(.dark) .permission-item-title {
    color: rgb(241, 245, 249);
  }

  :global(.dark) .permission-item-copy {
    color: rgb(148, 163, 184);
  }

  :global(.dark) .permission-setup-title {
    color: rgb(248, 250, 252);
  }

  :global(.dark) .permission-setup-copy {
    color: rgb(203, 213, 225);
  }

  :global(.dark) .permission-setup-dialog {
    border-color: rgba(51, 65, 85, 0.92);
  }

  @media (min-width: 640px) {
    .permission-setup-actions {
      flex-direction: row;
    }

    .permission-setup-button {
      flex: 1;
    }
  }

  @media (max-width: 640px) {
    .permission-summary-actions {
      width: 100%;
    }

    .permission-summary-toggle,
    .permission-refresh-button {
      flex: 1;
    }
  }
</style>
