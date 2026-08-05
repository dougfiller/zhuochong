function toDuration(value) {
  const duration = Number(value);
  return Number.isFinite(duration) && duration > 0 ? duration : 0;
}

function findPrimaryActiveRange(category, hourlyBreakdown) {
  const activeHours = Object.entries(hourlyBreakdown || {})
    .map(([hour, segments]) => ({
      hour: Number(hour),
      duration: (segments || [])
        .filter((segment) => segment?.category === category)
        .reduce((sum, segment) => sum + toDuration(segment?.duration), 0),
    }))
    .filter((item) => Number.isInteger(item.hour) && item.hour >= 0 && item.hour <= 23 && item.duration > 0)
    .sort((left, right) => left.hour - right.hour);

  if (activeHours.length === 0) return null;

  const ranges = [];
  let current = null;
  for (const item of activeHours) {
    if (!current || item.hour !== current.endHour + 1) {
      current = { startHour: item.hour, endHour: item.hour, duration: item.duration };
      ranges.push(current);
      continue;
    }
    current.endHour = item.hour;
    current.duration += item.duration;
  }

  return ranges.sort(
    (left, right) => right.duration - left.duration || left.startHour - right.startHour,
  )[0];
}

function collectPrimaryApps(category, appBreakdown, limit) {
  const totals = new Map();
  for (const bucket of appBreakdown || []) {
    for (const app of bucket?.apps || []) {
      if (app?.category !== category) continue;
      const appName = String(app?.app_name || '').trim();
      if (!appName) continue;
      totals.set(appName, (totals.get(appName) || 0) + toDuration(app?.duration));
    }
  }

  return [...totals.entries()]
    .map(([appName, duration]) => ({ appName, duration }))
    .filter((app) => app.duration > 0)
    .sort((left, right) => right.duration - left.duration || left.appName.localeCompare(right.appName))
    .slice(0, limit);
}

export function buildCategoryCompositionSummary({
  category,
  compositionTotals = {},
  hourlyBreakdown = {},
  appBreakdown = [],
  primaryAppLimit = 3,
} = {}) {
  const categoryKey = String(category || '');
  const duration = toDuration(compositionTotals?.[categoryKey]);
  const totalDuration = Object.values(compositionTotals || {})
    .reduce((sum, value) => sum + toDuration(value), 0);

  return {
    category: categoryKey,
    duration,
    percentage: totalDuration > 0 ? Math.round((duration / totalDuration) * 100) : 0,
    activeRange: findPrimaryActiveRange(categoryKey, hourlyBreakdown),
    primaryApps: collectPrimaryApps(categoryKey, appBreakdown, Math.max(0, primaryAppLimit)),
  };
}
