"""
Stage 1 · Lesson 1.2: Python Tool 定义原型

运行方式: python learning/01_tool.py

这个脚本展示 Agent 的 Tool 层长什么样。
不连接真实数据库，用模拟数据演示。
"""

import json
from typing import Any


# ══════════════════════════════════════════════════════════
# 第一部分：Tool 的定义 — 给 LLM 看的"菜单"
# ══════════════════════════════════════════════════════════

def search_memory_schema() -> dict:
    """
    这个函数返回的 JSON 就是发给 LLM 的工具定义。
    LLM 看到这个定义后，就知道："哦，我可以调用 search_memory，
    需要传 query 参数，可选传 date_from 和 date_to。"

    这就是你在 OpenAI API 文档里看到的 tools[].function 结构。
    """
    return {
        "type": "function",
        "function": {
            "name": "search_memory",
            "description": "搜索工作记录记忆库。支持关键词搜索和日期范围过滤。当用户问到具体做了什么、工作时间安排、某个项目的进展时使用。",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "搜索关键词，例如 'debug'、'编码'、'会议'"
                    },
                    "date_from": {
                        "type": "string",
                        "description": "开始日期，格式 YYYY-MM-DD"
                    },
                    "date_to": {
                        "type": "string",
                        "description": "结束日期，格式 YYYY-MM-DD"
                    }
                },
                "required": ["query"]
            }
        }
    }


def analyze_intents_schema() -> dict:
    """分析工作意图分布（编码/会议/文档等）"""
    return {
        "type": "function",
        "function": {
            "name": "analyze_intents",
            "description": "分析指定日期范围内的工作意图分布。返回各意图类别（如编码开发、会议沟通、文档撰写等）的时间和占比。当用户问时间分布、时间占比、各类型工作时长时使用。",
            "parameters": {
                "type": "object",
                "properties": {
                    "date_from": {
                        "type": "string",
                        "description": "开始日期，格式 YYYY-MM-DD"
                    },
                    "date_to": {
                        "type": "string",
                        "description": "结束日期，格式 YYYY-MM-DD"
                    }
                },
                "required": ["date_from", "date_to"]
            }
        }
    }


# ══════════════════════════════════════════════════════════
# 第二部分：Tool 的执行 — 真正干活的代码
# ══════════════════════════════════════════════════════════

def search_memory_execute(args: dict) -> str:
    """
    这里是 Tool 的执行逻辑。
    在真实项目中，这里会调用你的 database.search_memory()。
    现在用模拟数据演示。

    注意：返回值是字符串（不是 dict），因为 LLM 只能读文字。
    你需要把结果格式化成 LLM 能理解的文字。
    """
    query = args.get("query", "")
    date_from = args.get("date_from", "未知")
    date_to = args.get("date_to", "未知")

    # 在真实项目中：results = db.search_memory(query, date_from, date_to, limit=8)
    # 现在用模拟数据
    mock_results = [
        {"date": "2026-06-05", "title": "修复登录超时bug", "app": "VS Code", "duration": 3600},
        {"date": "2026-06-04", "title": "数据库连接池优化", "app": "VS Code", "duration": 5400},
        {"date": "2026-06-03", "title": "debug session - 排查内存泄漏", "app": "Chrome DevTools", "duration": 7200},
    ]

    # 格式化成 LLM 能理解的文字
    lines = [f"搜索 '{query}' 在 {date_from} ~ {date_to} 的结果（共{len(mock_results)}条）："]
    for r in mock_results:
        hours = r["duration"] // 3600
        minutes = (r["duration"] % 3600) // 60
        lines.append(f"  - {r['date']} | {r['title']} | {r['app']} | {hours}h{minutes}m")

    return "\n".join(lines)


def analyze_intents_execute(args: dict) -> str:
    """分析工作意图分布"""
    date_from = args.get("date_from", "未知")
    date_to = args.get("date_to", "未知")

    # 在真实项目中：result = work_intelligence.analyze_intents(activities)
    mock_result = [
        {"label": "编码开发", "duration": 28800, "sessions": 12},
        {"label": "会议沟通", "duration": 7200, "sessions": 5},
        {"label": "文档撰写", "duration": 3600, "sessions": 3},
    ]

    total = sum(r["duration"] for r in mock_result)
    lines = [f"工作意图分布 ({date_from} ~ {date_to})："]
    for r in mock_result:
        hours = r["duration"] // 3600
        pct = r["duration"] / total * 100
        lines.append(f"  - {r['label']}: {hours}h ({pct:.0f}%) | {r['sessions']}个session")

    return "\n".join(lines)


# ══════════════════════════════════════════════════════════
# 第三部分：ToolRegistry — 工具注册中心
# ══════════════════════════════════════════════════════════

class ToolRegistry:
    """
    工具注册中心 — 把所有工具管理起来。
    职责：
    1. 注册工具（schema + execute 函数配对）
    2. 返回工具定义列表（给 LLM 看）
    3. 根据 LLM 的选择执行对应工具
    """

    def __init__(self):
        self._tools: dict[str, dict] = {}
        # _tools 结构: {
        #   "search_memory": {
        #     "schema": {...},       # 给 LLM 看的定义
        #     "execute": function,   # 真正执行的函数
        #   },
        #   ...
        # }

    def register(self, schema_fn, execute_fn):
        """注册一个工具：传入 schema 函数和 execute 函数"""
        schema = schema_fn()
        name = schema["function"]["name"]
        self._tools[name] = {
            "schema": schema,
            "execute": execute_fn,
        }
        print(f"  ✅ 注册工具: {name}")

    def get_all_schemas(self) -> list[dict]:
        """返回所有工具的定义 — 这个列表会发给 LLM"""
        return [t["schema"] for t in self._tools.values()]

    def execute(self, tool_name: str, arguments: dict) -> str:
        """执行指定的工具 — LLM 选了某个工具后调用这个"""
        if tool_name not in self._tools:
            return f"错误：未知的工具 '{tool_name}'"
        return self._tools[tool_name]["execute"](arguments)


# ══════════════════════════════════════════════════════════
# 演示：运行起来看看效果
# ══════════════════════════════════════════════════════════

if __name__ == "__main__":
    print("=" * 60)
    print("Stage 1 · Tool 层 Python 原型")
    print("=" * 60)

    # 1. 创建注册中心并注册工具
    print("\n📦 注册工具：")
    registry = ToolRegistry()
    registry.register(search_memory_schema, search_memory_execute)
    registry.register(analyze_intents_schema, analyze_intents_execute)

    # 2. 看看发给 LLM 的工具定义长什么样
    print('\n📋 发给 LLM 的工具定义（这就是 LLM 看到的「菜单」）：')
    print(json.dumps(registry.get_all_schemas(), indent=2, ensure_ascii=False))

    # 3. 模拟 LLM 选择了 search_memory 工具
    print("\n" + "=" * 60)
    print("🔄 模拟 LLM 的工具调用：")
    print("=" * 60)

    # 假设 LLM 看到用户问题后，决定调用 search_memory
    llm_tool_call = {
        "name": "search_memory",
        "arguments": {
            "query": "debug",
            "date_from": "2026-06-02",
            "date_to": "2026-06-08"
        }
    }
    print(f"\nLLM 选择: {llm_tool_call['name']}")
    print(f"LLM 传参: {json.dumps(llm_tool_call['arguments'], ensure_ascii=False)}")

    # 执行工具
    result = registry.execute(llm_tool_call["name"], llm_tool_call["arguments"])
    print(f"\n工具执行结果（这个结果会返回给 LLM）：")
    print(result)

    # 再模拟一次
    print("\n" + "-" * 40)
    llm_tool_call_2 = {
        "name": "analyze_intents",
        "arguments": {
            "date_from": "2026-06-02",
            "date_to": "2026-06-08"
        }
    }
    print(f"\nLLM 选择: {llm_tool_call_2['name']}")
    print(f"LLM 传参: {json.dumps(llm_tool_call_2['arguments'], ensure_ascii=False)}")
    result_2 = registry.execute(llm_tool_call_2["name"], llm_tool_call_2["arguments"])
    print(f"\n工具执行结果：")
    print(result_2)
