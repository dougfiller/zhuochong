const TEMPLATE_FALLBACK_HINTS = [
  '由基础模板生成',
  '使用基础模板生成',
  '由基礎模板生成',
  '使用基礎模板生成',
  'generated from the base template',
];

function normalizeMode(mode) {
  return (mode || '').toString().trim().toLowerCase();
}

function containsTemplateFallbackHint(content) {
  const normalizedContent = (content || '').toString();
  const contentLower = normalizedContent.toLowerCase();

  return TEMPLATE_FALLBACK_HINTS.some((hint) => {
    const normalizedHint = hint.toLowerCase();
    return normalizedHint === 'generated from the base template'
      ? contentLower.includes(normalizedHint)
      : normalizedContent.includes(hint);
  });
}

export function resolveReportMeta(reportData, currentConfig) {
  const configMode = normalizeMode(currentConfig?.ai_mode);
  const fallbackReason = (reportData?.fallback_reason || '').toString().trim() || null;

  let reportMode = normalizeMode(reportData?.ai_mode || configMode);

  if (containsTemplateFallbackHint(reportData?.content)) {
    reportMode = 'local';
  }

  if (!reportData) {
    reportMode = configMode;
  }

  const showUsageMismatchNotice = configMode === 'summary' && reportMode === 'local';

  return {
    reportMode,
    showUsageMismatchNotice,
    fallbackReason,
  };
}
