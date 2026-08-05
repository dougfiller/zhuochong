const BASIC_ASSISTANT_MODEL_ID = '__basic__';

export const MODEL_PROVIDER_DISPLAY_NAMES = {
  ollama: {
    'zh-CN': 'Ollama (本地)',
    en: 'Ollama (Local)',
    'zh-TW': 'Ollama（本機）',
  },
  openai: {
    'zh-CN': 'OpenAI 兼容',
    en: 'OpenAI Compatible',
    'zh-TW': 'OpenAI 相容',
  },
  siliconflow: {
    'zh-CN': '硅基流动',
    en: 'SiliconFlow',
    'zh-TW': '矽基流動',
  },
  deepseek: { 'zh-CN': 'DeepSeek', en: 'DeepSeek', 'zh-TW': 'DeepSeek' },
  qwen: { 'zh-CN': '通义千问', en: 'Qwen', 'zh-TW': '通義千問' },
  zhipu: { 'zh-CN': '智谱清言', en: 'Zhipu', 'zh-TW': '智譜清言' },
  moonshot: { 'zh-CN': 'Kimi', en: 'Moonshot Kimi', 'zh-TW': 'Kimi' },
  doubao: { 'zh-CN': '豆包', en: 'Doubao', 'zh-TW': '豆包' },
  minimax: { 'zh-CN': 'MiniMax', en: 'MiniMax', 'zh-TW': 'MiniMax' },
  gemini: { 'zh-CN': 'Google Gemini', en: 'Google Gemini', 'zh-TW': 'Google Gemini' },
  claude: { 'zh-CN': 'Anthropic Claude', en: 'Anthropic Claude', 'zh-TW': 'Anthropic Claude' },
  openrouter: { 'zh-CN': 'OpenRouter', en: 'OpenRouter', 'zh-TW': 'OpenRouter' },
  groq: { 'zh-CN': 'Groq', en: 'Groq', 'zh-TW': 'Groq' },
  xai: { 'zh-CN': 'xAI Grok', en: 'xAI Grok', 'zh-TW': 'xAI Grok' },
  mistral: { 'zh-CN': 'Mistral', en: 'Mistral', 'zh-TW': 'Mistral' },
  lmstudio: {
    'zh-CN': 'LM Studio (本地)',
    en: 'LM Studio (Local)',
    'zh-TW': 'LM Studio（本機）',
  },
  custom: {
    'zh-CN': '自定义接口',
    en: 'Custom endpoint',
    'zh-TW': '自訂介面',
  },
};

function translatedLabel(translate, key) {
  const label = typeof translate === 'function' ? translate(key) : '';
  return typeof label === 'string' && label.trim() ? label : key;
}

function localizedProviderName(providerId, locale) {
  return MODEL_PROVIDER_DISPLAY_NAMES[providerId]?.[locale]
    || MODEL_PROVIDER_DISPLAY_NAMES[providerId]?.en
    || providerId
    || '';
}

export function resolveModelOptionLabel(selectedModelId, modelProfiles, locale, translate) {
  const basicLabel = translatedLabel(translate, 'ask.basicTemplate');
  if (selectedModelId === BASIC_ASSISTANT_MODEL_ID) return basicLabel;

  const profiles = Array.isArray(modelProfiles) ? modelProfiles : [];
  const profile = profiles.find((item) => item?.id === selectedModelId);
  if (!profile) return basicLabel;

  const profileName = profile.name?.trim();
  if (profileName) return profileName;

  const providerName = localizedProviderName(profile.model_config?.provider, locale);
  const modelName = profile.model_config?.model?.trim();
  if (providerName && modelName) return `${providerName} · ${modelName}`;
  if (modelName) return modelName;

  return translatedLabel(translate, 'ask.aiEnhanced');
}
