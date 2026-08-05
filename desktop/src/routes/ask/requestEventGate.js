/**
 * 为单次 Ask 请求创建事件门闩。
 *
 * 终态、超时主动 close 或组件销毁后，迟到事件都会被忽略，避免旧请求更新新回答。
 */
export function createRequestEventGate({ isDestroyed, onEvent }) {
  let accepting = true;

  return {
    handle(event) {
      if (!accepting || isDestroyed()) return false;
      const terminal = onEvent(event) === true;
      if (terminal) accepting = false;
      return terminal;
    },
    close() {
      accepting = false;
    },
  };
}
