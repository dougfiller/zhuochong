# LOOF Run 20260807-1703-步骤-21复用嵌入向量rrf-primitive-并强制本地-loopbackdispatch_

- project_root: `/Users/sky/aich8-zhuochong`
- feature: 步骤 21：复用嵌入/向量/RRF primitive 并强制本地 loopback；dispatch_id=aich8-zhuochong-desktop-pet-rag-27-steps-20260805-task-21。复用或抽取 Work Review 的 embedding 编解码、归一化、流式余弦 Top-K 和通用 RRF 单一受测实现，保留屏幕语义记忆 API 与配置兼容；抽取最小 embedding HTTP payload/响应 primitive。聊天使用独立 KnowledgeEmbeddingConfig 与步骤 20 冻结的 indexGeneration：构建只读 building 配置、查询只读 catalog active 配置，配置变更只建候选代际。聊天 provider 仅允许经严格验证并 pin 实际连接的本地 loopback Ollama-compatible/等价实现，拒绝 userinfo、非法 scheme、fragment、普通私网、云端、DNS rebind 和 redirect 逃逸，reqwest 强制 no_proxy 与 redirect none，不读取 Work Review 云 API key/fallback。知识库 embedding BLOB 必须严格等于 dimension*4，尾字节或维度不符按索引损坏失败。增加不记录正文的 endpoint 分类/调用次数 spy、真实中文语义质量与维度/批量/超时/服务不可用探针，并回归 index_semantic_memory/search_semantic_memory_inner；聊天 RRF 去重使用稳定 chunk key。保持只读聊天导出、独立 knowledge.sqlite、当前明确选择前台微信和 M2 fail-closed 产品边界，禁止注入、协议/微信数据库、UIA 输入、键鼠、自动粘贴/发送、未选聊天以及 MCP/Bot/search/upload/Agent 调用。
- current_phase: 5
- current_phase_name: 推送同步
- current_thread_id: 019fdbb8-081a-7141-8d91-e212d2031f36
- next_poll_at: 2026-08-07T10:18:05+00:00
- status: active

## Artifacts
- phase_1: 2026年08月07日17点06分-复用嵌入向量RRF并强制本地Loopback.md
- phase_2: 2026年08月07日17时34分-复用嵌入向量RRF并强制本地Loopback.md
- phase_3: 2026年08月07日17时48分-复用嵌入向量RRF与Loopback审查问题.md
- phase_4: 
- phase_5: 

## Threads
- phase_1: 019fdb77-08da-7b92-a601-3def89948a3e
- phase_2: 019fdb82-1fc5-7641-b57a-f0e3acc2b1fd
- phase_3: 019fdb9b-a59c-72c1-8e9c-3c03783bdafd
- phase_4: 019fdba2-4cd4-7493-bc37-722007f66da5
- phase_5: 019fdbb8-081a-7141-8d91-e212d2031f36

## Notes
- 2026-08-07T09:09:34+00:00 phase 1 gate failed: done file is missing
- 2026-08-07T09:20:24+00:00 phase 2 gate failed: done file is missing
- 2026-08-07T09:23:53+00:00 phase 2 gate failed: done file is missing
- 2026-08-07T09:30:26+00:00 phase 2 gate failed: done file is missing
- 2026-08-07T09:33:50+00:00 phase 2 gate failed: done file is missing
- 2026-08-07T09:37:25+00:00 phase 2 gate failed: done file is missing
- 2026-08-07T09:47:20+00:00 phase 3 gate failed: done file is missing
- 2026-08-07T09:57:52+00:00 phase 4 gate failed: done file is missing
- 2026-08-07T10:01:23+00:00 phase 4 gate failed: done file is missing
- 2026-08-07T10:04:53+00:00 phase 4 gate failed: done file is missing
- 2026-08-07T10:11:22+00:00 phase 4 gate failed: done file is missing
