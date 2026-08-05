import { writable } from 'svelte/store';

export const BASIC_ASSISTANT_MODEL_ID = '__basic__';

const STORAGE_KEY = 'work-review-assistant-state';
const DEFAULT_STATE = {
  messages: [],
  selectedModelId: BASIC_ASSISTANT_MODEL_ID,
  // 标记用户是否曾手动操作过模型选择器。
  // false = 从未选过（首次打开），助手页可自动选中已配置的模型（issue #133）；
  // 一旦用户手动切换（含切回基础模板），就置 true，不再自动覆盖用户选择。
  hasUserSelectedModel: false,
  sending: false,
  sendingRequestId: null,
  // 当前会话在 SQLite 中的 id（P3 持久化）；null = 尚未落库的新对话
  conversationId: null,
};

function genId() {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) return crypto.randomUUID();
  return `m-${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

function normalizeMessage(message) {
  return {
    ...message,
    id: message?.id || genId(),
    cards: Array.isArray(message?.cards) ? message.cards : [],
    references: Array.isArray(message?.references) ? message.references : [],
    toolLabels: Array.isArray(message?.toolLabels) ? message.toolLabels : [],
    steps: Array.isArray(message?.steps) ? message.steps : [],
    streaming: Boolean(message?.streaming),
  };
}

function loadState() {
  if (typeof window === 'undefined') {
    return DEFAULT_STATE;
  }

  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return DEFAULT_STATE;
    }

    const parsed = JSON.parse(raw);
    return {
      messages: Array.isArray(parsed?.messages)
        ? parsed.messages.map((message) => normalizeMessage(message))
        : [],
      selectedModelId:
        typeof parsed?.selectedModelId === 'string' && parsed.selectedModelId.trim()
          ? parsed.selectedModelId
          : BASIC_ASSISTANT_MODEL_ID,
      hasUserSelectedModel: Boolean(parsed?.hasUserSelectedModel),
      sending: false,
      sendingRequestId: null,
      conversationId:
        typeof parsed?.conversationId === 'number' ? parsed.conversationId : null,
    };
  } catch (error) {
    console.warn('加载助手会话缓存失败:', error);
    return DEFAULT_STATE;
  }
}

function persistState(state) {
  if (typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch (error) {
    console.warn('保存助手会话缓存失败:', error);
  }
}

function createAssistantStore() {
  const { subscribe, set, update } = writable(loadState());

  let _persistTimer = null;
  subscribe((state) => {
    if (_persistTimer) clearTimeout(_persistTimer);
    _persistTimer = setTimeout(() => {
      _persistTimer = null;
      persistState(state);
    }, 500);
  });

  return {
    subscribe,
    appendMessage: (message) =>
      update((state) => ({
        ...state,
        messages: [...state.messages, normalizeMessage(message)].slice(-40),
      })),
    clearMessages: () =>
      update((state) => ({
        ...state,
        messages: [],
      })),
    setSelectedModelId: (selectedModelId, { userInitiated = true } = {}) =>
      update((state) => ({
        ...state,
        selectedModelId:
          typeof selectedModelId === 'string' && selectedModelId.trim()
            ? selectedModelId
            : BASIC_ASSISTANT_MODEL_ID,
        // 只有用户手动操作（handleModelChange）才标记；程序内部初始化不算
        hasUserSelectedModel: userInitiated ? true : state.hasUserSelectedModel,
      })),
    setMessages: (messages) =>
      update((state) => ({
        ...state,
        messages: Array.isArray(messages)
          ? messages.slice(-40).map((message) => normalizeMessage(message))
          : [],
      })),
    // P3：绑定/切换 SQLite 会话（同时替换内存消息）
    setConversation: (conversationId, messages) =>
      update((state) => ({
        ...state,
        conversationId: typeof conversationId === 'number' ? conversationId : null,
        messages: Array.isArray(messages)
          ? messages.map((message) => normalizeMessage(message))
          : [],
      })),
    setConversationId: (conversationId) =>
      update((state) => ({
        ...state,
        conversationId: typeof conversationId === 'number' ? conversationId : null,
      })),
    beginSending: (requestId) =>
      update((state) => ({
        ...state,
        sending: true,
        sendingRequestId: requestId,
      })),
    finishSending: (requestId) =>
      update((state) => {
        if (state.sendingRequestId !== requestId) return state;
        return {
          ...state,
          sending: false,
          sendingRequestId: null,
        };
      }),
    // 增量更新当前 streaming 的 assistant message（流式事件驱动）。
    updateLastStreaming: (updater) =>
      update((state) => {
        const idx = state.messages.findIndex((m) => m.streaming);
        if (idx === -1) return state;
        const newMessages = state.messages.slice();
        newMessages[idx] = normalizeMessage(updater({ ...newMessages[idx] }));
        return { ...state, messages: newMessages };
      }),
    // 按请求对应的消息 ID 定点更新，避免并发流式消息互相覆盖。
    updateMessageById: (messageId, updater) =>
      update((state) => {
        const idx = state.messages.findIndex((message) => message.id === messageId);
        if (idx === -1) return state;
        const newMessages = state.messages.slice();
        newMessages[idx] = normalizeMessage(updater({ ...newMessages[idx] }));
        return { ...state, messages: newMessages };
      }),
    reset: () => set(DEFAULT_STATE),
  };
}

export const assistantStore = createAssistantStore();
