export async function confirmFreshScopeForNextRequest({
  currentGeneration,
  observe,
  requestConfirmation,
  authorize,
}) {
  const observation = await observe();
  if (observation.bindingGeneration !== currentGeneration) {
    return { observation, authorized: false, stale: true };
  }
  if (!(await requestConfirmation(observation))) {
    return { observation, authorized: false, stale: false };
  }
  await authorize(observation);
  return { observation, authorized: true, stale: false };
}
