"""
Stage 3 · Lesson 3.3: Agent Loop Python 原型

运行方式: python learning/03_agent_loop.py

这是整个 Agent 的心脏——一个 ~50 行的循环。
它把 Stage 1 (Tools) 和 Stage 2 (Model) 串在一起。

核心逻辑：
    while 未结束:
        LLM 决策 → 调工具？→ 执行工具 → 继续
                     → 直接回答？→ 返回给用户
"""

import json
import os
import sys

# ══════════════════════════════════════════════════════════
# 复用 Stage 1 和 Stage 2 的代码
# ══════════════════════════════════════════════════════════

from importlib.util import spec_from_file_location, module_from_spec

def load_module(name, path):
    spec = spec_from_file_location(name, path)
    mod = module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod

tool_module = load_module("tool_module",
    os.path.join(os.path.dirname(__file__), "01_tool.py"))
model_module = load_module("model_module",
    os.path.join(os.path.dirname(__file__), "02_model.py"))

registry = tool_module.ToolRegistry()
registry.register(tool_module.search_memory_schema, tool_module.search_memory_execute)
registry.register(tool_module.analyze_intents_schema, tool_module.analyze_intents_execute)

tools = registry.get_all_schemas()
provider = model_module.create_provider()


# ══════════════════════════════════════════════════════════
# Agent Loop — 整个 Agent 的心脏，只有 ~50 行
# ══════════════════════════════════════════════════════════

def agent_run(question: str, max_iterations: int = 8) -> dict:
    """
    Agent Loop 核心。

    面试核心：整个 Agent 的智能就来自这个循环。
    LLM 在每一轮都能看到之前所有的工具调用和结果，
    然后自己决定下一步做什么。

    参数：
      question: 用户的问题
      max_iterations: 最大迭代次数（防止无限循环）

    返回：
      {
        "answer": "最终回答",
        "iterations": 用了几轮,
        "trace": [每轮的决策记录]  ← 用于调试和展示
      }
    """
    messages = [model_module.Message(role=model_module.Role.USER, content=question)]
    trace = []

    for i in range(max_iterations):
        # ── 第 1 步：调用 LLM ──
        response = provider.chat(messages, tools)

        # ── 第 2 步：判断 LLM 的意图 ──
        if response.stop_reason == model_module.StopReason.STOP:
            # LLM 给出了最终回答 → 循环结束
            trace.append({
                "round": i + 1,
                "action": "final_answer",
                "content": response.content[:100] if response.content else ""
            })
            return {
                "answer": response.content or "（无回答）",
                "iterations": i + 1,
                "trace": trace
            }

        elif response.stop_reason == model_module.StopReason.TOOL_CALL:
            # LLM 想调工具 → 执行工具，结果追加到对话历史
            for tc in response.tool_calls:
                trace.append({
                    "round": i + 1,
                    "action": "tool_call",
                    "tool": tc.name,
                    "arguments": tc.arguments
                })

                # ① 记录 assistant 的工具调用（LLM 说了要调什么）
                messages.append(model_module.Message(
                    role=model_module.Role.ASSISTANT,
                    content=response.content,
                    tool_calls=response.tool_calls
                ))

                # ② 执行工具
                result = registry.execute(tc.name, tc.arguments)

                # ③ 把工具结果追加到对话历史（LLM 下一轮能看到）
                messages.append(model_module.Message(
                    role=model_module.Role.TOOL_RESULT,
                    content=result,
                    tool_call_id=tc.id
                ))

    # ── 超过最大迭代次数 → 强制要求回答 ──
    trace.append({
        "round": max_iterations,
        "action": "max_iterations_reached",
        "content": "达到最大迭代次数，强制结束"
    })
    return {
        "answer": "抱歉，我在处理这个问题时进行了过多步骤。请尝试更具体地描述你的问题。",
        "iterations": max_iterations,
        "trace": trace
    }


# ══════════════════════════════════════════════════════════
# 演示：三个场景
# ══════════════════════════════════════════════════════════

if __name__ == "__main__":
    scenarios = [
        ("简单问答（不调工具）", "你好，你能做什么？"),
        ("单次工具调用", "这周的时间分布怎么样？"),
        ("多轮工具调用", "对比上个月和这个月的工作时间分布"),
    ]

    for title, question in scenarios:
        print("=" * 60)
        print(f"场景：{title}")
        print(f"用户：{question}")
        print("-" * 60)

        result = agent_run(question)

        print(f"\n📊 执行追踪（共 {result['iterations']} 轮）：")
        for step in result["trace"]:
            if step["action"] == "final_answer":
                print(f"  轮{step['round']}: ✅ 最终回答 → {step['content'][:60]}...")
            elif step["action"] == "tool_call":
                print(f"  轮{step['round']}: 🔧 调用 {step['tool']}({json.dumps(step['arguments'], ensure_ascii=False)})")
            elif step["action"] == "max_iterations_reached":
                print(f"  轮{step['round']}: ⚠️ 达到最大迭代次数")

        print(f"\n💬 回答：\n{result['answer']}\n")
