function normalizeSummary(text) {
  return (text || '').replace(/\s+/g, ' ').trim();
}

function splitSummarySentences(text) {
  return normalizeSummary(text)
    .split(/[。！？!?]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function splitSummaryClauses(text) {
  return text
    .split(/[，,；;、]/)
    .map((item) => item.trim())
    .filter(Boolean);
}

function ensureChineseStop(text) {
  if (!text) {
    return '';
  }
  return `${text.replace(/[。！？!?]+$/g, '').trim()}。`;
}

export function orderHourlySummariesForDisplay(summaries = []) {
  const displaySummaries = Array.isArray(summaries) ? summaries : [];
  return [...displaySummaries].sort(
    (left, right) => Number(right.hour) - Number(left.hour)
  );
}

export function formatHourRange(hour) {
  const numericHour = Number(hour);
  const normalizedHour = Number.isFinite(numericHour)
    ? Math.max(0, Math.min(23, Math.trunc(numericHour)))
    : 0;
  const start = String(normalizedHour).padStart(2, '0');
  const end = String(normalizedHour + 1).padStart(2, '0');
  return `${start}:00–${end}:00`;
}

export function getFullSummary(text) {
  return normalizeSummary(text);
}

export function getPrimarySummary(text) {
  const normalized = normalizeSummary(text);
  if (!normalized) {
    return '';
  }

  const firstSentence = splitSummarySentences(normalized)[0] || normalized;
  const clauses = splitSummaryClauses(firstSentence);

  if (clauses.length >= 3) {
    return clauses.slice(0, 2).join('，');
  }

  return firstSentence;
}

export function getSecondarySummary(text) {
  const normalized = normalizeSummary(text);
  if (!normalized) {
    return '';
  }

  const sentences = splitSummarySentences(normalized);
  const firstSentence = sentences[0] || normalized;
  const clauses = splitSummaryClauses(firstSentence);

  if (clauses.length >= 3) {
    return ensureChineseStop(clauses.slice(2).join('，'));
  }

  if (sentences.length > 1) {
    return ensureChineseStop(sentences.slice(1).join('。'));
  }

  return '';
}

export function getSummaryDisplayParts(text, expanded = false) {
  if (expanded) {
    return {
      primary: getFullSummary(text),
      secondary: '',
    };
  }

  return {
    primary: getPrimarySummary(text),
    secondary: getSecondarySummary(text),
  };
}

export function getMainApps(mainApps) {
  return (mainApps || '')
    .split(/[，,]/)
    .map((item) => item.trim())
    .filter(Boolean)
    .slice(0, 4);
}

export function getSummaryRhythmTone(totalDuration = 0) {
  if (totalDuration >= 45 * 60) {
    return 'deep';
  }

  if (totalDuration >= 20 * 60) {
    return 'steady';
  }

  return 'light';
}
