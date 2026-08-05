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
