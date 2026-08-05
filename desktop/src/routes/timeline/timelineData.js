function getActivityGroupKey(activity) {
  const appName = activity.app_name || '';
  const browserUrl = activity.browser_url;
  const normalizedUrl = browserUrl ? browserUrl.replace(/\/+$/, '') : '';
  if (browserUrl && browserUrl.trim()) {
    return `url:${appName}|${normalizedUrl}`;
  }
  return `app:${appName}|${activity.window_title || ''}`;
}

export function prepareTimelineActivities(activitiesData) {
  return [...activitiesData].sort((a, b) => {
    if (b.timestamp !== a.timestamp) {
      return b.timestamp - a.timestamp;
    }
    return (b.id || 0) - (a.id || 0);
  });
}

export function upsertTimelineActivity(currentActivities, newActivity) {
  const existingById = currentActivities.findIndex((activity) => activity.id === newActivity.id);
  if (existingById >= 0) {
    return currentActivities.map((activity) =>
      activity.id === newActivity.id ? newActivity : activity
    );
  }

  // Match by GROUP BY key (app_name + browser_url/window_title) to avoid duplicates
  const newGroupKey = getActivityGroupKey(newActivity);
  const existingByGroup = currentActivities.findIndex(
    (activity) => activity.id !== newActivity.id && getActivityGroupKey(activity) === newGroupKey
  );
  if (existingByGroup >= 0) {
    // Update visual fields (timestamp, screenshot) but keep existing id and duration.
    // The backend emits per-DB-row activities; duration accumulation here would be
    // incorrect because the same group may span multiple DB rows that the initial
    // DB query already aggregates. Later id-match replacements for the NEW row would
    // also overwrite any accumulated total. Keeping the original id ensures subsequent
    // real-time updates for the same group still match the correct entry.
    const existing = currentActivities[existingByGroup];
    const merged = {
      ...existing,
      timestamp: newActivity.timestamp,
      screenshot_path: newActivity.screenshot_path || existing.screenshot_path,
    };
    return prepareTimelineActivities(
      currentActivities.map((activity, idx) =>
        idx === existingByGroup ? merged : activity
      )
    );
  }

  return prepareTimelineActivities([newActivity, ...currentActivities]);
}
