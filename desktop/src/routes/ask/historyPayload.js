/**
 * 把助手多轮对话历史构造成发给后端的 payload。
 *
 * 关键设计：assistant 消息里如果带"已完成"的工具步骤（steps），
 * 会把步骤摘要以紧凑内联标记追加到 content 尾部再发出。
 *
 * 为什么这么做：后端把这些消息转成 LLM 的 messages 数组时只取
 * {role, content}（ask.rs 的 Vec<AssistantChatMessage> → Vec<Message>），
 * steps 字段在前端持久化但不会进 LLM。如果不带进去，模型在下一轮
 * 就看不到上一轮用过什么工具、结果如何，会重走已失败的工具路径
 * （比如上轮 search_memory 返回 0 条，下轮又先调 search_memory）。
 *
 * 摘要格式（按工具类型区分，避免数字误导）：
 * - 失败工具：`search_memory↯` （明确失败信号，最强提示"别再试"）
 * - 未知结果：`search_memory?`（旧数据或非法 ok 不猜测成功/失败）
 * - search_memory（hits 有语义，仅它会往 references 写数据）：`search_memory→0条`
 * - 其他成功工具：`query_activities✓` / `web_search✓` （只标成功，不带数字）
 *
 * 为什么不对所有工具都用 hits 数字：collect_references 全项目只在
 * search_memory 调用一次（tools.rs:391），query_activities/web_search/fetch_url
 * 即便成功返回大量数据，hits 也恒为 0。若统一写"N条"，模型会把成功的
 * query_activities→0条 误读为"这工具没用过"，下轮避开它（比不改还糟）。
 *
 * 多步骤用 ` | ` 分隔，整体包方括号：
 *   `[工具：search_memory→0条 | query_activities✓ | web_search↯]`
 */

/** 单个步骤元素的形状（与 Ask.svelte 里的 stepStart/stepResult 事件对齐）。 */
// { tool: string, label: string, status: 'running' | 'done',
//   ok?: boolean, hits?: number, references?: any[] }

/** 工具名 → 摘要片段（仅 status=done 的步骤会调用）。 */
function summarizeDoneStep(step) {
  const tool = step.tool;
  if (step.ok === false) {
    return `${tool}↯`;
  }
  if (step.ok !== true) {
    return `${tool}?`;
  }
  // search_memory 的 hits 有真实语义（collect_references 仅它调用）。
  // 其他工具（query_activities/web_search/fetch_url）的 hits 恒为 0，
  // 写出来会误导模型，所以只标成功符号。
  if (tool === 'search_memory' && Number.isFinite(step.hits) && step.hits >= 0) {
    return `${tool}→${step.hits}条`;
  }
  return `${tool}✓`;
}

/**
 * 把一组工具步骤压缩成一行摘要。
 *
 * @param {Array} steps - 消息上的 steps 数组
 * @returns {string|null} 摘要字符串（已含方括号），或 null（无已完成步骤时）
 */
export function summarizeStepsForHistory(steps) {
  if (!Array.isArray(steps) || steps.length === 0) return null;

  const doneSteps = steps.filter(
    (s) => s && s.status === 'done' && typeof s.tool === 'string' && s.tool,
  );
  if (doneSteps.length === 0) return null;

  const parts = doneSteps.map(summarizeDoneStep);
  return `[工具：${parts.join(' | ')}]`;
}

/** 每轮工具结果摘要的字符预算（防止历史膨胀挤掉真实对话）。 */
const HISTORY_DIGEST_BUDGET_PER_ROUND = 600;

/**
 * 把成功步骤携带的 digest（后端 StepResult 附带的结果截断摘要）拼成
 * 历史附注，让模型在追问时无需重查工具即可引用上一轮数据。
 * 超出预算的部分丢弃（保序：先到的工具优先保留）。
 *
 * @param {Array} steps - 消息上的 steps 数组
 * @returns {string|null}
 */
export function buildStepDigestForHistory(steps) {
  if (!Array.isArray(steps) || steps.length === 0) return null;

  const lines = [];
  let used = 0;
  for (const step of steps) {
    if (!step || step.status !== 'done' || step.ok !== true) continue;
    if (typeof step.digest !== 'string' || !step.digest.trim()) continue;
    const line = `${step.tool}: ${step.digest.trim()}`;
    if (used + line.length > HISTORY_DIGEST_BUDGET_PER_ROUND) {
      const remain = HISTORY_DIGEST_BUDGET_PER_ROUND - used;
      if (remain > 40) {
        lines.push(`${line.slice(0, remain)}…`);
      }
      break;
    }
    lines.push(line);
    used += line.length;
  }

  if (lines.length === 0) return null;
  return `[上轮工具数据摘要]\n${lines.join('\n')}`;
}

/**
 * 把 store 里的 messages 数组转成发给后端 chat_work_assistant 的 history。
 *
 * - 只保留完整的 user → 非 streaming assistant 轮次
 * - 孤立消息和 streaming 中断轮次整体丢弃
 * - 最多取最近 4 个完整轮次
 * - assistant 消息若带已完成工具步骤，content 尾部追加摘要
 *
 * @param {Array} messages - assistantStore 的 messages
 * @returns {Array<{role: string, content: string}>}
 */
export function buildHistoryPayload(messages) {
  const list = Array.isArray(messages) ? messages : [];
  const rounds = [];
  let pendingUser = null;

  for (const message of list) {
    if (!message) continue;

    if (message.role === 'user') {
      // 连续 user 表示前一条没有得到完整回答，只保留最新候选问题。
      pendingUser = message;
      continue;
    }

    if (message.role !== 'assistant' || !pendingUser) continue;

    // streaming 或失败的 assistant 都是不完整轮次，问题和半截/错误文本不能进入历史。
    if (message.streaming || message.failed === true) {
      pendingUser = null;
      continue;
    }

    const baseContent = String(message.content ?? '');
    const summary = summarizeStepsForHistory(message.steps);
    const digest = buildStepDigestForHistory(message.steps);
    const suffixes = [summary, digest].filter(Boolean).join('\n');
    rounds.push([
      { role: 'user', content: String(pendingUser.content ?? '') },
      {
        role: 'assistant',
        content: suffixes
          ? baseContent
            ? `${baseContent}\n\n${suffixes}`
            : suffixes
          : baseContent,
      },
    ]);
    pendingUser = null;
  }

  return rounds.slice(-4).flat();
}
