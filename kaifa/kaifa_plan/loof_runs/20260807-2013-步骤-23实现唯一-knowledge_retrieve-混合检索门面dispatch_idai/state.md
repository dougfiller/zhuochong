# LOOF Run 20260807-2013-步骤-23实现唯一-knowledge_retrieve-混合检索门面dispatch_idai

- project_root: `/Users/sky/aich8-zhuochong`
- feature: 步骤 23：实现唯一 knowledge_retrieve() 混合检索门面；dispatch_id=aich8-zhuochong-desktop-pet-rag-27-steps-20260805-task-23。仅在 knowledge/retrieve.rs 暴露 knowledge_retrieve(request)->Result<RetrievedReply,KnowledgeError>，RetrievedReply 只能由同模块私有构造器从 success/no_hit 构造，错误不得产生回复。请求与响应完整绑定 request_id、规范化 query、binding/index generation、精确三种 serde scope、top_k、版本化 token budget、冻结 hash、status/mode/hits/elapsed；读取 active ready catalog 与冻结 embedding 配置，无组合返回 KB_NOT_READY。本地向量不可用时显式 FTS fallback，执行 FTS5+向量召回、共享 RRF 去重排序；所有 active generation、denial、source state/provenance、scope 与 knowledge_chunk_messages 每消息至少一个 active 来源的授权必须在 KnowledgeStore/SQL 候选层完成，禁止展示层全库后过滤。只在授权候选内 sameConversationBoost，开关前后 conversation ID 集合相同。按 topK/单 hit 上限/token budget 产出有界且字段完整的本地 hit；无命中为 no_hit，SQLite/索引/scope/组装错误 fail closed 且模型调用数为 0。默认 trace 仅 IDs/scores，正文按权限从 store 重取；retriever 只依赖 KnowledgeStore 类型化查询与 local embedding，禁止回复模型、UI、上传、MCP、Bot、search/Agent。测试三种 scope 精确 wire、越界会话、boost 集合不变、no_hit、FTS fallback、索引损坏、DB busy、token 截断、响应字段完整、模型调用数 0。保持当前明确选择的前台微信、手动检查复制粘贴发送、只读聊天导出与独立 knowledge.sqlite 边界；不得注入、微信协议/数据库、UIA 输入、键鼠模拟、自动发送、未选聊天或外部工具。只处理本步骤与本 run，复用参考/Work-Review-main/src-tauri/src/commands/semantic_memory.rs search_semantic_memory_inner() 及 crates/core/src/database.rs，原创门面/私有构造/scope SQL/no_hit 分流。
- current_phase: 5
- current_phase_name: 推送同步
- current_thread_id: 019fdc5b-2b36-7643-b8d2-d0063513812d
- next_poll_at: 
- status: done

## Artifacts
- phase_1: 2026年08月07日20点16分-实现唯一knowledge_retrieve混合检索门面.md
- phase_2: 2026年08月07日20点35分-实现唯一knowledge_retrieve混合检索门面.md
- phase_3: 2026年08月07日20点53分-检索授权竞态与冻结哈希不完整.md
- phase_4: 
- phase_5: 

## Threads
- phase_1: 019fdc25-7bae-7793-a2ba-d7c4d64696f6
- phase_2: 019fdc2e-53e2-7f02-9b46-35954ebdfe6f
- phase_3: 019fdc44-b181-75f0-8478-e48ef8373bbf
- phase_4: 019fdc4e-59f8-7871-84ea-b0a456c4f0d1
- phase_5: 019fdc5b-2b36-7643-b8d2-d0063513812d

## Notes
- 2026-08-07T12:38:12+00:00 phase 2 gate failed: done file is missing
