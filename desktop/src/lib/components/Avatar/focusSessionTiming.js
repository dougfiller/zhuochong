export function freezeFocusSession(session, nowMs) {
  if (!session) {
    return null;
  }
  return Math.max(0, session.endsAtMs - nowMs);
}

export function restoreFocusSession(session, remainingMs, nowMs) {
  if (!session || remainingMs <= 0) {
    return null;
  }
  return {
    ...session,
    endsAtMs: nowMs + remainingMs,
  };
}
