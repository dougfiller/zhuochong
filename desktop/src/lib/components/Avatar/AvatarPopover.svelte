<script>
  import { formatBubbleMessage } from './bubbleMessage.js';
  import { t } from '$lib/i18n/index.js';

  export let bubble = null;
  export let flipLeft = false;
  export let onClose = () => {};
  export let onCopyWechatSuggestion = () => {};
  export let onDismissWechatSuggestion = () => {};
  export let copyPending = false;
  export let copyError = '';

  $: bubbleMessage = formatBubbleMessage(bubble?.message);
  $: panelStyle =
    bubble?.tone === 'success'
      ? 'background: linear-gradient(180deg, rgba(236, 253, 245, 0.98), rgba(209, 250, 229, 0.95)); color: rgb(6, 78, 59); border-color: rgba(167, 243, 208, 0.96); backdrop-filter: blur(12px) saturate(1.04);'
      : 'background: rgba(255, 255, 255, 0.96); color: rgb(15, 23, 42); border-color: rgba(226, 232, 240, 0.96); backdrop-filter: blur(12px) saturate(1.04);';
  $: innerPanelStyle = 'border-color: rgba(255, 255, 255, 0.72);';
  $: tailStyle =
    bubble?.tone === 'success'
      ? 'background: linear-gradient(180deg, rgba(236, 253, 245, 0.98), rgba(209, 250, 229, 0.95)); border-color: rgba(167, 243, 208, 0.96);'
      : 'background: rgba(255, 255, 255, 0.96); border-color: rgba(226, 232, 240, 0.96);';
  $: tailDotStyle =
    bubble?.tone === 'success'
      ? 'background: rgba(236, 253, 245, 0.98);'
      : 'background: rgba(255, 255, 255, 0.94);';
  $: compactBubbleMessage = !bubbleMessage?.includes('\n') && (bubbleMessage?.trim().length ?? 0) <= 14;
  $: isWechatSuggestion = bubble?.kind === 'wechatSuggestion';
  // 约束弹框宽度不超出桌宠窗口：82vw 在 276px 窗口内 ≈ 226px，留出边距
  $: bubblePanelStyle = compactBubbleMessage
    ? 'width: fit-content; min-width: 120px; max-width: min(82vw, 196px);'
    : 'width: min(82vw, 336px); min-width: 160px; max-width: min(82vw, 336px);';
</script>

{#if bubble}
  <div class="absolute inset-0 z-20 overflow-visible pointer-events-none">
    <div class={`avatar-popover-anchor absolute ${flipLeft ? 'left-[6%]' : 'right-[6%]'} top-[8px]`}>
      <div class="relative overflow-visible">
        <div
          class="pointer-events-auto relative rounded-[16px] border shadow-[0_10px_24px_rgba(15,23,42,0.1),0_3px_10px_rgba(15,23,42,0.05)]"
          style="{bubblePanelStyle} min-height: 40px; padding: 6px 14px 7px 14px; {panelStyle}"
        >
          {#if bubble?.persistent && !isWechatSuggestion}
            <button
              type="button"
              class="absolute inset-0 rounded-[16px]"
              aria-label={t('avatar.dismissReminder')}
              on:click={onClose}
            ></button>
          {/if}
          <div
            class="pointer-events-none absolute inset-[1px] rounded-[15px] border"
            style={innerPanelStyle}
          ></div>
          {#if bubble?.persistent && !isWechatSuggestion}
            <button
              type="button"
              class="absolute right-1.5 top-1.5 z-10 inline-flex h-5 w-5 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-900/6 hover:text-slate-700"
              aria-label={t('avatar.dismissReminder')}
              on:click={onClose}
            >
              ×
            </button>
          {/if}
          <div
            class:pr-8={bubble?.persistent && !isWechatSuggestion}
            class:wechat-suggestion-message={isWechatSuggestion}
            class="relative text-xs font-semibold leading-[1.35] tracking-[0.01em]"
            style="display: block; min-height: 27px; max-height: 140px; {isWechatSuggestion ? 'max-height: 220px; overflow-y: auto;' : 'overflow-y: hidden;'} text-align: {compactBubbleMessage ? 'center' : 'left'}; word-break: normal; overflow-wrap: anywhere; white-space: normal;"
          >
            {bubbleMessage}
          </div>
          {#if isWechatSuggestion}
            {#if copyError}
              <p class="relative mt-2 text-[11px] font-medium text-rose-700" role="alert">{copyError}</p>
            {/if}
            <div class="relative mt-2 flex items-center justify-end gap-2">
              <button
                type="button"
                class="rounded-md border border-slate-300 px-2 py-1 text-[11px] font-semibold text-slate-700 transition hover:bg-slate-100 disabled:cursor-not-allowed disabled:opacity-60"
                aria-label={t('avatar.wechatSuggestionDismiss')}
                disabled={copyPending}
                on:click|stopPropagation={onDismissWechatSuggestion}
              >
                {t('avatar.wechatSuggestionDismiss')}
              </button>
              <button
                type="button"
                class="rounded-md bg-sky-600 px-2 py-1 text-[11px] font-semibold text-white transition hover:bg-sky-700 disabled:cursor-not-allowed disabled:opacity-60"
                aria-label={t('avatar.wechatSuggestionCopy')}
                disabled={copyPending}
                on:click|stopPropagation={onCopyWechatSuggestion}
              >
                {copyPending ? t('avatar.wechatSuggestionCopying') : t('avatar.wechatSuggestionCopy')}
              </button>
            </div>
          {/if}
        </div>
        <div
          class={`bubble-tail absolute ${flipLeft ? 'right-[18px]' : 'left-[18px]'} top-[calc(100%-5px)] h-[12px] w-[12px] rotate-45 rounded-[3px] border shadow-[0_6px_14px_rgba(15,23,42,0.06)]`}
          style={tailStyle}
        ></div>
        <div
          class={`bubble-tail-dot absolute ${flipLeft ? 'right-[26px]' : 'left-[26px]'} top-[calc(100%-1px)] h-[8px] w-[8px] rounded-full shadow-[0_4px_12px_rgba(15,23,42,0.05)]`}
          style={tailDotStyle}
        ></div>
      </div>
    </div>
  </div>
{/if}
