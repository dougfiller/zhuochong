export function shouldShowPromptAppliedToast({
  configAiMode,
  customPrompt,
  reportAiMode,
}) {
  const normalizedConfigMode = (configAiMode || '').toString().trim().toLowerCase();
  const normalizedReportMode = (reportAiMode || '').toString().trim().toLowerCase();
  const trimmedPrompt = (customPrompt || '').toString().trim();

  return (
    normalizedConfigMode === 'summary' &&
    normalizedReportMode === 'summary' &&
    trimmedPrompt.length > 0
  );
}
