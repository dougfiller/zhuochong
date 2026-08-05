// 弹窗焦点陷阱（Svelte action）：
// - 打开时把焦点移入弹窗（优先第一个可聚焦元素）
// - Tab / Shift+Tab 在弹窗内循环，不会跑到底层页面
// - 关闭时把焦点还给打开前的元素
//
// 用法：<div use:trapFocus role="dialog" aria-modal="true">…</div>

const FOCUSABLE_SELECTOR = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

export function trapFocus(node) {
  const previouslyFocused =
    typeof document !== 'undefined' && document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null;

  function focusables() {
    return Array.from(node.querySelectorAll(FOCUSABLE_SELECTOR)).filter(
      (el) => el.offsetParent !== null || el === document.activeElement
    );
  }

  function handleKeydown(event) {
    if (event.key !== 'Tab') return;
    const items = focusables();
    if (!items.length) {
      event.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;

    if (event.shiftKey) {
      if (active === first || !node.contains(active)) {
        event.preventDefault();
        last.focus();
      }
    } else if (active === last || !node.contains(active)) {
      event.preventDefault();
      first.focus();
    }
  }

  // 初始聚焦：微任务延迟，等 Svelte 完成挂载动画类
  queueMicrotask(() => {
    const items = focusables();
    (items[0] ?? node).focus?.();
  });

  node.addEventListener('keydown', handleKeydown);

  return {
    destroy() {
      node.removeEventListener('keydown', handleKeydown);
      previouslyFocused?.focus?.();
    },
  };
}
