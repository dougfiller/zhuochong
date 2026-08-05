const DEFAULT_MARGIN = 12;
const DEFAULT_GAP = 8;
const DEFAULT_MIN_HEIGHT = 180;
const DEFAULT_WIDTH = 352;

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

/**
 * 根据触发器与视口尺寸计算 fixed Popover 的安全位置。
 * 优先向下展开；下方空间不足且上方更宽裕时自动翻转。
 */
export function getViewportPopoverPlacement(rect, options = {}) {
  const viewportWidth = Math.max(0, Number(options.viewportWidth) || 0);
  const viewportHeight = Math.max(0, Number(options.viewportHeight) || 0);
  const margin = Math.max(0, Number(options.margin) || DEFAULT_MARGIN);
  const gap = Math.max(0, Number(options.gap) || DEFAULT_GAP);
  const minHeight = Math.max(0, Number(options.minHeight) || DEFAULT_MIN_HEIGHT);
  const preferredWidth = Math.max(0, Number(options.preferredWidth) || DEFAULT_WIDTH);

  const width = Math.min(preferredWidth, Math.max(0, viewportWidth - margin * 2));
  const maxLeft = Math.max(margin, viewportWidth - margin - width);
  const left = clamp((Number(rect?.right) || 0) - width, margin, maxLeft);
  const triggerTop = Number(rect?.top) || 0;
  const triggerBottom = Number(rect?.bottom) || triggerTop;
  const spaceBelow = Math.max(0, viewportHeight - margin - triggerBottom - gap);
  const spaceAbove = Math.max(0, triggerTop - margin - gap);
  const placement = spaceBelow < minHeight && spaceAbove > spaceBelow ? 'top' : 'bottom';

  if (placement === 'top') {
    return {
      placement,
      left,
      width,
      maxHeight: spaceAbove,
      top: null,
      bottom: Math.max(margin, viewportHeight - triggerTop + gap),
    };
  }

  return {
    placement,
    left,
    width,
    maxHeight: spaceBelow,
    top: Math.min(viewportHeight - margin, triggerBottom + gap),
    bottom: null,
  };
}
