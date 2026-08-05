"""
Stage 5 · Lesson 5.3: Orchestrator Python 原型

运行方式: python learning/05_orchestrator.py

展示 Orchestrator 的三个职责：
1. 路由决策：简单 → FastPath，复杂 → AgentPath
2. 执行对应路径
3. 降级处理：Agent 失败 → FastPath → FallbackPath
"""

import json
import os
import sys
import re

# 复用之前的模块
from importlib.util import spec_from_file_location, module_from_spec

def load_module(name, path):
    spec = spec_from_file_location(name, path)
    mod = module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod

base = os.path.dirname(__file__)
tool_mod = load_module("tool", os.path.join(base, "01_tool.py"))
model_mod = load_module("model", os.path.join(base, "02_model.py"))
loop_mod = load_module("loop", os.path.join(base, "03_agent_loop.py"))

registry = tool_mod.ToolRegistry()
registry.register(tool_mod.search_memory_schema, tool_mod.search_memory_execute)
registry.register(tool_mod.analyze_intents_schema, tool_mod.analyze_intents_execute)
tools = registry.get_all_schemas()


# ══════════════════════════════════════════════════════════
# 第一部分：路由决策 — 判断走哪条路径
# ══════════════════════════════════════════════════════════

class QueryPath:
    """三种路径"""
    DIRECT = "direct"         # 直接回答（闲聊）
    FAST = "fast"             # 规则快速路径
    AGENT = "agent"           # Agent 循环
    FALLBACK = "fallback"     # 无模型兜底


def route_query(question: str, has_model: bool) -> tuple[str, str]:
    """
    路由决策函数。

    面试核心：这个函数决定了每个请求的命运。
    规则越简单越好——复杂的判断交给 Agent 自己做。

    返回：(路径, 决策理由)
    """
    q = question.strip().lower()

    # ── 规则 1：闲聊 / 纯问答 → 直接回答 ──
    greetings = ["你好", "嗨", "hello", "hi", "你能做什么", "帮助", "help"]
    if any(g in q for g in greetings) and len(q) < 20:
        return QueryPath.DIRECT, "简短问候/求助"

    # ── 规则 2：复杂意图 → Agent ──
    complex_patterns = [
        "对比", "比较", "趋势", "分析", "变化",  # 对比/分析类
        "为什么", "原因", "怎么回事",              # 归因类
        "建议", "优化", "改进",                    # 建议类
    ]
    if any(p in q for p in complex_patterns):
        if not has_model:
            return QueryPath.FALLBACK, "复杂查询但无模型，降级到模板"
        return QueryPath.AGENT, "检测到复杂意图关键词"

    # ── 规则 3：包含多个时间段 → Agent ──
    time_keywords = ["今天", "昨天", "本周", "上周", "本月", "这个月", "上月", "上个月", "最近"]
    matched_times = [k for k in time_keywords if k in q]
    if len(matched_times) >= 2:
        if not has_model:
            return QueryPath.FALLBACK, "多时间段查询但无模型，降级到模板"
        return QueryPath.AGENT, f"检测到多个时间段：{'、'.join(matched_times)}"

    # ── 规则 4：简单时间查询 → FastPath ──
    if any(k in q for k in time_keywords):
        return QueryPath.FAST, f"简单时间查询：{matched_times[0] if matched_times else '?'}"

    # ── 规则 5：包含明确关键词 → FastPath ──
    simple_patterns = ["做了什么", "工作记录", "时间分布", "待办", "总结"]
    if any(p in q for p in simple_patterns):
        return QueryPath.FAST, "简单工作查询"

    # ── 兜底：不确定 → Agent（如果有模型）──
    if has_model:
        return QueryPath.AGENT, "无法明确分类，走 Agent 兜底"
    else:
        return QueryPath.FALLBACK, "无模型，降级到模板兜底"


# ══════════════════════════════════════════════════════════
# 第二部分：Orchestrator — 指挥官
# ══════════════════════════════════════════════════════════

class Orchestrator:
    """
    Agent 指挥官。

    面试要点：Orchestrator 不是重新实现所有逻辑，
    而是把 Stage 1-4 的组件组装起来，加上路由决策。
    """

    def __init__(self, has_model: bool = True):
        self.has_model = has_model

    def handle(self, question: str) -> dict:
        """
        处理用户请求的总入口。

        对应 Rust: Orchestrator::handle()
        """
        # ① 路由决策
        path, reason = route_query(question, self.has_model)

        result = {
            "question": question,
            "path": path,
            "reason": reason,
        }

        # ② 执行对应路径
        if path == QueryPath.DIRECT:
            result["answer"] = self._direct_answer(question)
            result["latency"] = "<1ms"

        elif path == QueryPath.FAST:
            result["answer"] = self._fast_answer(question)
            result["latency"] = "~50ms"

        elif path == QueryPath.AGENT:
            try:
                agent_result = loop_mod.agent_run(question, max_iterations=5)
                result["answer"] = agent_result["answer"]
                result["iterations"] = agent_result["iterations"]
                result["trace"] = agent_result["trace"]
                result["latency"] = f"~{agent_result['iterations'] * 1500}ms"
            except Exception as e:
                # Agent 失败 → 降级到 FastPath
                result["path"] = QueryPath.FAST + "（Agent降级）"
                result["answer"] = self._fast_answer(question)
                result["error"] = str(e)
                result["latency"] = "~50ms（降级）"

        elif path == QueryPath.FALLBACK:
            result["answer"] = self._fallback_answer(question)
            result["latency"] = "<1ms"

        return result

    def _direct_answer(self, question: str) -> str:
        """直接回答路径（闲聊/求助）"""
        q = question.lower()
        if "你好" in q or "hi" in q or "hello" in q:
            return "你好！我是你的工作助手，可以帮你分析工作时间、查看记录、对比效率等。请问你想了解什么？"
        if "你能做什么" in q or "帮助" in q:
            return ("我可以帮你：\n"
                    "1. 查看某天/某周的工作记录\n"
                    "2. 分析时间分布（编码/会议/文档占比）\n"
                    "3. 对比不同时间段的效率变化\n"
                    "4. 搜索特定的工作内容\n"
                    "请告诉我你想了解什么？")
        return "请告诉我你想了解的工作信息。"

    def _fast_answer(self, question: str) -> str:
        """
        FastPath（规则快速路径）。

        这里模拟你现有的 chat_work_assistant 逻辑：
        用 parse_temporal_range 提取日期 → 查数据 → 格式化回答。
        """
        # 模拟：直接调一个工具获取数据
        args = {"query": question, "date_from": "2026-06-09", "date_to": "2026-06-09"}
        result = registry.execute("search_memory", args)
        return f"[FastPath] 基于规则查询的结果：\n{result}"

    def _fallback_answer(self, question: str) -> str:
        """FallbackPath（无模型时的模板回答）"""
        return ("我目前无法使用 AI 模型进行分析，但你可以尝试：\n"
                "- 询问具体某天的工作记录\n"
                "- 使用时间关键词（今天、昨天、本周等）\n"
                "- 配置 AI 模型后可以获得更智能的分析")


# ══════════════════════════════════════════════════════════
# 演示
# ══════════════════════════════════════════════════════════

if __name__ == "__main__":
    print("=" * 60)
    print("Stage 5 · Orchestrator Python 原型")
    print("=" * 60)

    # 有模型的 Orchestrator
    print("\n📦 有 AI 模型时的路由：\n")
    orch = Orchestrator(has_model=True)

    test_questions = [
        "你好",
        "今天做了什么",
        "这个月的时间分布",
        "对比上个月和这个月的工作效率",
        "为什么最近编码时间下降了",
        "上个月和这个月有什么变化",
    ]

    for q in test_questions:
        result = orch.handle(q)
        print(f"  Q: {q}")
        print(f"  路径: {result['path']:8s} | 理由: {result['reason']}")
        if "answer" in result:
            preview = result["answer"][:60].replace(chr(10), " ")
            print(f"  回答: {preview}...")
        print()

    # 无模型的 Orchestrator
    print("=" * 60)
    print("📦 无 AI 模型时的路由（降级模式）：\n")
    orch_no_model = Orchestrator(has_model=False)

    for q in ["对比上个月和这个月", "今天做了什么"]:
        result = orch_no_model.handle(q)
        print(f"  Q: {q}")
        print(f"  路径: {result['path']:16s} | 理由: {result['reason']}")
        print()
