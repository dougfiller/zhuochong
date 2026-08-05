/** 与后端 executor.rs 的 CANCELLED_ANSWER 常量保持一致。 */
export const CANCELLED_ANSWER_MARKER = '已按你的要求停止。';

function mergeReferences(existing, incoming) {
  const current = Array.isArray(existing) ? existing : [];
  const additions = Array.isArray(incoming) ? incoming : [];
  if (additions.length === 0) return current;

  const seen = new Set(
    current.map((reference) =>
      `${reference.sourceId ?? ''}|${reference.timestamp}|${reference.title}`
    )
  );
  const merged = [...current];
  for (const reference of additions) {
    const key = `${reference.sourceId ?? ''}|${reference.timestamp}|${reference.title}`;
    if (!seen.has(key)) {
      seen.add(key);
      merged.push(reference);
    }
  }
  return merged;
}

function findLatestRunningStep(steps, tool) {
  for (let index = steps.length - 1; index >= 0; index -= 1) {
    if (steps[index]?.status === 'running' && steps[index]?.tool === tool) {
      return index;
    }
  }
  return -1;
}

/**
 * 将单个后端流式事件归约到指定消息，不依赖组件或全局 store。
 * terminal=true 表示收到 done/error，调用方应停止接受后续事件。
 */
export function reduceStreamEvent(message, event, fallbackError) {
  if (!event || typeof event !== 'object') {
    return { message, terminal: false };
  }

  switch (event.type) {
    case 'stepStart': {
      const steps = Array.isArray(message.steps) ? message.steps : [];
      return {
        message: {
          ...message,
          steps: [
            ...steps,
            { tool: event.tool, label: event.label, status: 'running' },
          ],
        },
        terminal: false,
      };
    }
    case 'stepResult': {
      const steps = Array.isArray(message.steps) ? message.steps : [];
      const targetIndex = findLatestRunningStep(steps, event.tool);
      const eventReferences = Array.isArray(event.references) ? event.references : [];
      const nextSteps =
        targetIndex === -1
          ? steps
          : steps.map((step, index) =>
              index === targetIndex
                ? {
                    ...step,
                    status: 'done',
                    ok: event.ok === true ? true : event.ok === false ? false : undefined,
                    hits:
                      Number.isFinite(event.hits) && event.hits >= 0
                        ? event.hits
                        : undefined,
                    references: eventReferences,
                    // 工具结果摘要：随下一轮历史回传给模型，追问时无需重查
                    ...(typeof event.digest === 'string' ? { digest: event.digest } : {}),
                  }
                : step
            );
      return {
        message: {
          ...message,
          steps: nextSteps,
          references: mergeReferences(message.references, eventReferences),
        },
        terminal: false,
      };
    }
    case 'confirmRequest': {
      // 行动确认卡片：找到该工具最近的 running 步骤，标记为待确认
      const steps = Array.isArray(message.steps) ? message.steps : [];
      const targetIndex = findLatestRunningStep(steps, event.tool);
      const confirmPatch = {
        confirmId: event.confirmId,
        summary: typeof event.summary === 'string' ? event.summary : '',
        confirmStatus: 'pending',
      };
      const nextSteps =
        targetIndex === -1
          ? [
              ...steps,
              {
                tool: event.tool,
                label: event.label,
                status: 'running',
                ...confirmPatch,
              },
            ]
          : steps.map((step, index) =>
              index === targetIndex ? { ...step, ...confirmPatch } : step
            );
      return {
        message: { ...message, steps: nextSteps },
        terminal: false,
      };
    }
    case 'token':
      return {
        message: {
          ...message,
          content: `${message.content ?? ''}${typeof event.token === 'string' ? event.token : ''}`,
        },
        terminal: false,
      };
    case 'done': {
      // 用户主动停止：保留已流式出来的部分内容，不被停止占位文案整体覆盖
      const cancelled =
        event.answer === CANCELLED_ANSWER_MARKER && (message.content ?? '').trim();
      return {
        message: {
          ...message,
          content: cancelled ? message.content : event.answer ?? message.content,
          references: event.references?.length ? event.references : message.references,
          toolLabels: event.toolLabels?.length ? event.toolLabels : message.toolLabels,
          streaming: false,
          failed: false,
          stopped: Boolean(cancelled) || event.answer === CANCELLED_ANSWER_MARKER,
        },
        terminal: true,
      };
    }
    case 'error':
      return {
        message: {
          ...message,
          content: event.error || message.content || fallbackError,
          streaming: false,
          failed: true,
        },
        terminal: true,
      };
    default:
      return { message, terminal: false };
  }
}
