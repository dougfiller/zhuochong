"""
Stage 2 · Lesson 2.2: Model 层 Python 原型

运行方式:
  无 API Key: python learning/02_model.py
  有 API Key: OPENAI_API_KEY=sk-xxx python learning/02_model.py

这个脚本展示 Model 层做了什么：
1. 把统一的消息格式翻译成各家 API 的请求格式
2. 把各家 API 的响应翻译回统一格式
3. Agent Loop 只需要和统一格式打交道
"""

import json
import os
import sys

# ══════════════════════════════════════════════════════════
# 第一部分：统一的消息格式 — Agent 内部只用这个
# ══════════════════════════════════════════════════════════

class Role:
    USER = "user"
    ASSISTANT = "assistant"
    TOOL_RESULT = "tool"


class StopReason:
    STOP = "stop"            # LLM 直接回答了
    TOOL_CALL = "tool_call"  # LLM 要调工具
    MAX_TOKENS = "max_tokens"  # Token 用完了


class ToolCall:
    """统一的工具调用格式 — 不管底层是 OpenAI/Claude/Gemini"""
    def __init__(self, id: str, name: str, arguments: dict):
        self.id = id
        self.name = name
        self.arguments = arguments

    def __repr__(self):
        return f"ToolCall(id={self.id!r}, name={self.name!r}, args={self.arguments})"


class Message:
    """统一的消息格式"""
    def __init__(self, role: str, content: str = None, tool_calls: list[ToolCall] = None,
                 tool_call_id: str = None):
        self.role = role
        self.content = content
        self.tool_calls = tool_calls
        self.tool_call_id = tool_call_id  # 仅 ToolResult 消息需要

    def __repr__(self):
        if self.role == Role.TOOL_RESULT:
            return f"ToolResult(id={self.tool_call_id}, content={self.content[:50]}...)"
        if self.tool_calls:
            return f"Assistant(tool_calls={self.tool_calls})"
        return f"{self.role}: {self.content[:50] if self.content else 'None'}..."


class LlmResponse:
    """LLM 的统一响应格式"""
    def __init__(self, content: str = None, tool_calls: list[ToolCall] = None,
                 stop_reason: str = StopReason.STOP):
        self.content = content
        self.tool_calls = tool_calls
        self.stop_reason = stop_reason


# ══════════════════════════════════════════════════════════
# 第二部分：Mock Provider — 没有 API Key 也能看到完整流程
# ══════════════════════════════════════════════════════════

class MockProvider:
    """
    模拟 LLM 的行为，不需要真实 API。
    用来理解 Model 层在做什么。

    面试关键：Model 层的接口是 chat(messages, tools) → LlmResponse
    不管底层是 Mock/OpenAI/Claude/Gemini，接口不变。
    """

    def chat(self, messages: list[Message], tools: list[dict]) -> LlmResponse:
        """
        核心方法：发送消息 + 工具定义，返回统一格式的响应。

        参数：
          messages: 对话历史（统一格式）
          tools: 工具定义列表（Stage 1 输出的 JSON Schema）

        返回：
          LlmResponse（统一格式）
        """
        user_msg = ""
        for m in messages:
            if m.role == Role.USER:
                user_msg = m.content or ""

        # 模拟 LLM 的"思考过程"：
        # 根据用户问题内容决定是直接回答还是调用工具
        user_lower = user_msg.lower()

        if "对比" in user_msg or "比较" in user_msg:
            # 复杂查询 → LLM 决定调两次工具
            # 检查是否已经拿到过数据（通过看历史消息）
            has_data = any(m.role == Role.TOOL_RESULT for m in messages)

            if not has_data:
                # 第一次：查上月数据
                return LlmResponse(
                    tool_calls=[ToolCall(
                        id="mock_call_1",
                        name="analyze_intents",
                        arguments={"date_from": "2026-05-01", "date_to": "2026-05-31"}
                    )],
                    stop_reason=StopReason.TOOL_CALL
                )
            else:
                # 第二次：查本月数据（模拟已拿到上月数据的场景）
                tool_results = [m for m in messages if m.role == Role.TOOL_RESULT]
                has_two_results = len(tool_results) >= 2

                if not has_two_results:
                    return LlmResponse(
                        tool_calls=[ToolCall(
                            id="mock_call_2",
                            name="analyze_intents",
                            arguments={"date_from": "2026-06-01", "date_to": "2026-06-09"}
                        )],
                        stop_reason=StopReason.TOOL_CALL
                    )

            # 两次数据都有了 → 生成最终回答
            return LlmResponse(
                content="对比分析结果：\n\n"
                        "5月总工时 165h：编码 120h(73%)、会议 30h(18%)、文档 15h(9%)\n"
                        "6月总工时 65h：编码 45h(69%)、会议 12h(19%)、文档 8h(12%)\n\n"
                        "编码时间占比从73%下降到69%，会议和文档占比略有上升。",
                stop_reason=StopReason.STOP
            )

        elif "时间分布" in user_msg or "时间占比" in user_msg or "做了什么" in user_msg:
            # 时间相关问题 → 调用 analyze_intents
            return LlmResponse(
                tool_calls=[ToolCall(
                    id="mock_call_3",
                    name="analyze_intents",
                    arguments={"date_from": "2026-06-02", "date_to": "2026-06-09"}
                )],
                stop_reason=StopReason.TOOL_CALL
            )

        else:
            # 简单问题 → 直接回答
            return LlmResponse(
                content=f"你好！我是你的工作助手。关于「{user_msg}」，我可以帮你分析工作时间、查看工作记录等。请问你想了解什么？",
                stop_reason=StopReason.STOP
            )


# ══════════════════════════════════════════════════════════
# 第三部分：OpenAI Provider — 真实 API 调用（有 Key 时启用）
# ══════════════════════════════════════════════════════════

class OpenAIProvider:
    """
    真实的 OpenAI API 调用。
    展示 Model 层如何把统一格式翻译成 OpenAI 的请求格式，
    又把 OpenAI 的响应翻译回统一格式。

    面试核心：你在这里看到的就是 Model 层的全部职责——格式翻译。
    """

    def __init__(self, api_key: str, model: str = "gpt-4o-mini", base_url: str = None):
        try:
            from openai import OpenAI
            kwargs = {"api_key": api_key}
            if base_url:
                kwargs["base_url"] = base_url
            self.client = OpenAI(**kwargs)
            self.model = model
        except ImportError:
            print("需要安装: pip install openai")
            sys.exit(1)

    def chat(self, messages: list[Message], tools: list[dict]) -> LlmResponse:
        # ① 把统一格式翻译成 OpenAI 的请求格式
        openai_messages = self._to_openai_messages(messages)

        # ② 调用 API
        response = self.client.chat.completions.create(
            model=self.model,
            messages=openai_messages,
            tools=tools,
            max_tokens=1600,
            temperature=0.2,
        )

        # ③ 把 OpenAI 响应翻译回统一格式
        choice = response.choices[0]
        msg = choice.message

        tool_calls = None
        if msg.tool_calls:
            tool_calls = [
                ToolCall(
                    id=tc.id,
                    name=tc.function.name,
                    arguments=json.loads(tc.function.arguments)  # OpenAI 返回字符串，要解析
                )
                for tc in msg.tool_calls
            ]

        stop_reason = StopReason.STOP
        if choice.finish_reason == "tool_calls":
            stop_reason = StopReason.TOOL_CALL
        elif choice.finish_reason == "length":
            stop_reason = StopReason.MAX_TOKENS

        return LlmResponse(
            content=msg.content,
            tool_calls=tool_calls,
            stop_reason=stop_reason,
        )

    def _to_openai_messages(self, messages: list[Message]) -> list[dict]:
        """
        把统一格式翻译成 OpenAI 格式。
        这就是 Model 层做的"格式翻译"工作。
        """
        result = []
        for m in messages:
            if m.role == Role.USER:
                result.append({"role": "user", "content": m.content})

            elif m.role == Role.ASSISTANT:
                msg = {"role": "assistant"}
                if m.content:
                    msg["content"] = m.content
                if m.tool_calls:
                    msg["tool_calls"] = [
                        {
                            "id": tc.id,
                            "type": "function",
                            "function": {
                                "name": tc.name,
                                "arguments": json.dumps(tc.arguments, ensure_ascii=False)
                                # ↑ OpenAI 要求 arguments 是字符串，不是 object！
                            }
                        }
                        for tc in m.tool_calls
                    ]
                result.append(msg)

            elif m.role == Role.TOOL_RESULT:
                result.append({
                    "role": "tool",
                    "tool_call_id": m.tool_call_id,
                    "content": m.content,
                })
        return result


# ══════════════════════════════════════════════════════════
# 第四部分：Provider 工厂 — 根据配置选择实现
# ══════════════════════════════════════════════════════════

def create_provider() -> MockProvider | OpenAIProvider:
    """
    面试要点：工厂模式。
    Agent Loop 不关心用哪个提供商，它只调用 provider.chat()。
    这个函数根据环境决定具体实现。
    """
    api_key = os.environ.get("OPENAI_API_KEY")
    if api_key:
        base_url = os.environ.get("OPENAI_BASE_URL")  # 支持自定义端点
        model = os.environ.get("OPENAI_MODEL", "gpt-4o-mini")
        print(f"🔗 使用 OpenAI Provider (model={model})")
        return OpenAIProvider(api_key, model, base_url)
    else:
        print("🔗 使用 Mock Provider（无 API Key，模拟 LLM 行为）")
        return MockProvider()


# ══════════════════════════════════════════════════════════
# 演示：Model 层怎么工作
# ══════════════════════════════════════════════════════════

if __name__ == "__main__":
    print("=" * 60)
    print("Stage 2 · Model 层 Python 原型")
    print("=" * 60)

    provider = create_provider()

    # 从 Stage 1 导入工具定义
    from importlib.util import spec_from_file_location, module_from_spec
    spec = spec_from_file_location("tool_module",
                                    os.path.join(os.path.dirname(__file__), "01_tool.py"))
    tool_module = module_from_spec(spec)
    spec.loader.exec_module(tool_module)

    registry = tool_module.ToolRegistry()
    registry.register(tool_module.search_memory_schema, tool_module.search_memory_execute)
    registry.register(tool_module.analyze_intents_schema, tool_module.analyze_intents_execute)

    tools = registry.get_all_schemas()

    # ── 场景 1：简单问题 → LLM 直接回答 ──
    print("\n" + "=" * 60)
    print("场景 1：简单问题（LLM 直接回答，不调工具）")
    print("=" * 60)

    messages = [Message(role=Role.USER, content="你好")]
    response = provider.chat(messages, tools)

    print(f"\n用户: 你好")
    print(f"LLM stop_reason: {response.stop_reason}")
    print(f"LLM tool_calls: {response.tool_calls}")
    print(f"LLM 回答: {response.content}")

    # ── 场景 2：需要工具的问题 → LLM 选择调工具 ──
    print("\n" + "=" * 60)
    print("场景 2：时间分布问题（LLM 选择调用 analyze_intents）")
    print("=" * 60)

    messages = [Message(role=Role.USER, content="这周的时间分布怎么样")]
    response = provider.chat(messages, tools)

    print(f"\n用户: 这周的时间分布怎么样")
    print(f"LLM stop_reason: {response.stop_reason}")

    if response.tool_calls:
        tc = response.tool_calls[0]
        print(f"LLM 选择工具: {tc.name}")
        print(f"LLM 传入参数: {json.dumps(tc.arguments, ensure_ascii=False)}")

        # 执行工具
        result = registry.execute(tc.name, tc.arguments)
        print(f"\n工具执行结果:\n{result}")

    # ── 场景 3：对比问题 → LLM 调两次工具 ──
    print("\n" + "=" * 60)
    print("场景 3：对比问题（LLM 需要调两次工具）")
    print("=" * 60)
    print("这就是 Agent Loop 的雏形 — LLM 自主决定调几次工具")
    print()

    messages = [Message(role=Role.USER, content="对比上个月和这个月的工作时间分布")]
    iteration = 0
    max_iterations = 5

    while iteration < max_iterations:
        iteration += 1
        response = provider.chat(messages, tools)

        print(f"── 第 {iteration} 轮 ──")

        if response.stop_reason == StopReason.STOP:
            # LLM 给出最终回答
            print(f"LLM 最终回答:\n{response.content}")
            break

        elif response.stop_reason == StopReason.TOOL_CALL:
            # LLM 要调工具
            for tc in response.tool_calls:
                print(f"LLM 选择: {tc.name}({json.dumps(tc.arguments, ensure_ascii=False)})")

                # 1. 记录 assistant 的工具调用
                messages.append(Message(
                    role=Role.ASSISTANT,
                    content=response.content,
                    tool_calls=response.tool_calls
                ))

                # 2. 执行工具
                result = registry.execute(tc.name, tc.arguments)
                print(f"工具结果: {result[:80]}...")

                # 3. 把结果追加到对话历史
                messages.append(Message(
                    role=Role.TOOL_RESULT,
                    content=result,
                    tool_call_id=tc.id
                ))

        print()

    print(f"总共 {iteration} 轮完成")
