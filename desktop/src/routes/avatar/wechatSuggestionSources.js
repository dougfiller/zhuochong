export function sameWechatSuggestion(left, right) {
  return left?.requestId === right?.requestId
    && left?.suggestionGeneration === right?.suggestionGeneration
    && left?.bindingGeneration === right?.bindingGeneration;
}

export async function loadCurrentWechatSuggestionSources(invoke, suggestion, currentSuggestion) {
  if (!suggestion) {
    return null;
  }
  const result = await invoke('get_wechat_reply_sources', {
    input: { requestId: suggestion.requestId },
  });
  return sameWechatSuggestion(currentSuggestion(), suggestion) ? result : null;
}
