"""
Stage 4 · Lesson 4.3: Memory 层 Python 原型

运行方式: python learning/04_memory.py

展示三种记忆管理策略：
1. 滑动窗口（最简单）
2. Token 预算控制
3. 混合策略（摘要 + 窗口）
"""

import json


# ══════════════════════════════════════════════════════════
# 第一部分：估算 Token 数量（简化版）
# ══════════════════════════════════════════════════════════

def estimate_tokens(text: str) -> int:
    """
    粗略估算 Token 数。
    面试要点：中文 1 个字 ≈ 2-3 tokens，英文 1 个单词 ≈ 1-2 tokens。
    这里用简单估算：字符数 / 2。
    真实项目应该用 tiktoken 库精确计算。
    """
    return max(1, len(text) // 2)


# ══════════════════════════════════════════════════════════
# 第二部分：ConversationMemory — 对话记忆管理器
# ══════════════════════════════════════════════════════════

class ConversationMemory:
    """
    对话记忆管理器。

    职责：
    1. 维护对话历史
    2. 控制发给 LLM 的内容量（不超过 Token 预算）
    3. 旧消息自动截断或压缩

    面试核心：Memory 不是"记住所有东西"，而是"在预算内保留最有用的信息"。
    """

    def __init__(self, max_messages: int = 20, max_tokens: int = 8000):
        """
        参数：
          max_messages: 最多保留多少条消息（硬上限，防爆炸）
          max_tokens: 发给 LLM 时最多用多少 tokens（软上限，用于截断）
        """
        self.messages = []          # 完整对话历史
        self.summary = None         # 旧消息的摘要（如果有）
        self.max_messages = max_messages
        self.max_tokens = max_tokens

    def add(self, role: str, content: str):
        """添加一条消息到历史"""
        self.messages.append({"role": role, "content": content})

        # 超过上限时自动压缩旧消息
        if len(self.messages) > self.max_messages:
            self._compact()

    def _compact(self):
        """
        压缩旧消息。

        面试要点：这里有两种策略：
        - 简单版：直接丢弃最旧的消息（滑动窗口）
        - 高级版：用 LLM 把旧消息压缩成摘要

        这里实现简单版（高级版需要额外 LLM 调用，后面 Stage 讲）
        """
        # 保留最近一半的消息，旧的一半"压缩"成一行摘要
        split_point = len(self.messages) // 2
        old_messages = self.messages[:split_point]
        recent_messages = self.messages[split_point:]

        # 生成简单摘要（不用 LLM，用规则提取关键信息）
        roles = set(m["role"] for m in old_messages)
        user_msgs = [m["content"][:30] for m in old_messages if m["role"] == "user"]
        self.summary = (
            f"[早期对话摘要：用户问了{len(user_msgs)}个问题，"
            f"涉及：{'、'.join(user_msgs[:3])}]"
        )

        self.messages = recent_messages

    def get_context(self, max_tokens: int = None) -> list[dict]:
        """
        获取要在下次 LLM 调用中使用的上下文。

        这是 Memory 层最核心的方法：
        从历史消息中选出不超过 Token 预算的部分。

        策略：从最新的消息往前取，取到 Token 预算满了为止。
        """
        budget = max_tokens or self.max_tokens
        result = []
        used_tokens = 0

        # 如果有摘要，先加上
        if self.summary:
            summary_tokens = estimate_tokens(self.summary)
            if used_tokens + summary_tokens <= budget:
                result.insert(0, {"role": "system", "content": self.summary})
                used_tokens += summary_tokens

        # 从最新的消息往前取
        for msg in reversed(self.messages):
            msg_tokens = estimate_tokens(msg["content"])
            if used_tokens + msg_tokens > budget:
                break  # 预算满了，停止
            result.insert(0 if not self.summary else 1, msg)
            used_tokens += msg_tokens

        return result

    def stats(self) -> dict:
        """返回当前记忆状态的统计"""
        total_tokens = sum(estimate_tokens(m["content"]) for m in self.messages)
        if self.summary:
            total_tokens += estimate_tokens(self.summary)
        return {
            "total_messages": len(self.messages),
            "total_tokens": total_tokens,
            "has_summary": self.summary is not None,
            "summary_preview": self.summary[:60] if self.summary else None,
        }


# ══════════════════════════════════════════════════════════
# 演示：看看 Memory 层怎么工作
# ══════════════════════════════════════════════════════════

if __name__ == "__main__":
    print("=" * 60)
    print("Stage 4 · Memory 层 Python 原型")
    print("=" * 60)

    memory = ConversationMemory(max_messages=10, max_tokens=4000)

    # 模拟一段对话
    conversations = [
        ("user", "你好，你能做什么？"),
        ("assistant", "我可以帮你分析工作时间、查看工作记录、对比不同时间段等。"),
        ("user", "今天做了什么？"),
        ("assistant", "今天你编码了4小时（VS Code），开会2小时（腾讯会议），文档1小时。"),
        ("user", "那昨天呢？"),
        ("assistant", "昨天你主要在做code review，总共审了5个PR，花了3小时。"),
        ("user", "这两天编码时间对比一下"),
        ("assistant", "今天编码4h vs 昨天0h（昨天在做review）。编码时间大幅增加。"),
        ("user", "上周整体情况怎么样？"),
        ("assistant", "上周总工时40h：编码28h(70%)、会议8h(20%)、文档4h(10%)。"),
        ("user", "上个月和这个月对比呢？"),
        ("assistant", "上月总工时165h vs 本月目前65h。编码占比从73%降到69%。"),
    ]

    print("\n📝 逐步添加对话消息：")
    for role, content in conversations:
        memory.add(role, content)
        stats = memory.stats()
        print(f"  + [{role:9s}] {content[:30]:30s}  | 消息数={stats['total_messages']:2d}  tokens≈{stats['total_tokens']:5d}"
              f"{'  ⚡ 已压缩' if stats['has_summary'] else ''}")

    # 看看最终发给 LLM 的上下文
    print("\n" + "=" * 60)
    print("📊 最终发给 LLM 的上下文（Token 预算 4000）：")
    print("=" * 60)

    context = memory.get_context()
    total_context_tokens = sum(estimate_tokens(m["content"]) for m in context)
    print(f"  实际条数：{len(context)}  实际 tokens：≈{total_context_tokens}\n")

    for msg in context:
        preview = msg["content"][:60]
        tokens = estimate_tokens(msg["content"])
        print(f"  [{msg['role']:9s}] {preview:60s} (~{tokens}t)")

    # 展示 Memory 统计
    print("\n" + "=" * 60)
    print("📊 Memory 状态统计：")
    print("=" * 60)
    stats = memory.stats()
    print(f"  原始消息数：{stats['total_messages']}")
    print(f"  总 Token 数：≈{stats['total_tokens']}")
    print(f"  有摘要：{'是' if stats['has_summary'] else '否'}")
    if stats['summary_preview']:
        print(f"  摘要预览：{stats['summary_preview']}")

    # 展示不同预算下的效果
    print("\n" + "=" * 60)
    print("🔍 不同 Token 预算下的上下文对比：")
    print("=" * 60)

    for budget in [1000, 2000, 4000]:
        ctx = memory.get_context(max_tokens=budget)
        ctx_tokens = sum(estimate_tokens(m["content"]) for m in ctx)
        print(f"\n  预算 {budget}t → 实际 {ctx_tokens}t，{len(ctx)} 条消息：")
        for msg in ctx:
            print(f"    [{msg['role']:9s}] {msg['content'][:40]}")
