// 错误展示策略：后端精心编写的中文/多语言错误消息直接给用户看（可操作），
// 而 JS 技术异常（TypeError/undefined 等）对用户毫无意义且暴露实现细节——
// 归拢为友好文案，技术细节进 console 供排查。

const TECHNICAL_PATTERNS = [
  /TypeError/i,
  /ReferenceError/i,
  /SyntaxError/i,
  /RangeError/i,
  /Cannot read propert/i,
  /undefined is not/i,
  /null is not/i,
  /is not a function/i,
  /is not defined/i,
  /Failed to fetch/i,
  /NetworkError/i,
  /\[object [A-Z]/,
  /^\s*at\s+\S+\s+\(/m, // 堆栈行
];

const WECHAT_ERROR_KEYS = {
  WX_BUSY: 'wechat.errors.busy',
  WX_NOT_FOREGROUND: 'wechat.errors.notForeground',
  WX_PROFILE_UNSUPPORTED: 'wechat.errors.profileUnsupported',
  WX_WINDOW_UNSUPPORTED: 'wechat.errors.profileUnsupported',
  WX_CAPTURE_FAILED: 'wechat.errors.captureFailed',
  WX_CAPTURE_TIMEOUT: 'wechat.errors.captureFailed',
  WX_OCR_EMPTY: 'wechat.errors.ocrFailed',
  WX_OCR_UNAVAILABLE: 'wechat.errors.ocrFailed',
  WX_OCR_FAILED: 'wechat.errors.ocrFailed',
  WX_GROUP_CHAT_UNSUPPORTED: 'wechat.errors.groupUnsupported',
  WX_TEXT_MODEL_UNAVAILABLE: 'wechat.errors.modelUnavailable',
  WX_REQUEST_CANCELLED: 'wechat.errors.requestCancelled',
  WX_REQUEST_STALE: 'wechat.errors.requestCancelled',
  LLM_FAILED: 'wechat.errors.modelFailed',
};

/**
 * Maps only stable WeChat contract tokens to localized, actionable copy.
 * Raw Tauri errors are never suitable for this UI because they can contain
 * implementation details.
 */
export function formatWechatUserError(error, fallback, translate) {
  const text = String(error instanceof Error ? error.message : error ?? '');
  const code = Object.keys(WECHAT_ERROR_KEYS).find((candidate) =>
    new RegExp(`(?:^|[^A-Z0-9_])${candidate}(?:$|[^A-Z0-9_])`).test(text),
  );
  return code ? translate(WECHAT_ERROR_KEYS[code]) : fallback;
}

/**
 * 把任意错误转成适合展示给用户的文本。
 *
 * @param {unknown} error - 捕获到的错误（Error/字符串/后端消息）
 * @param {string} fallback - 技术性错误时展示的友好文案（已 i18n）
 * @returns {string}
 */
export function formatUserError(error, fallback) {
  const text = String(error instanceof Error ? error.message : error ?? '').trim();
  if (!text) {
    return fallback;
  }
  if (TECHNICAL_PATTERNS.some((pattern) => pattern.test(text))) {
    console.error('技术错误详情（不向用户展示）:', error);
    return fallback;
  }
  return text;
}
