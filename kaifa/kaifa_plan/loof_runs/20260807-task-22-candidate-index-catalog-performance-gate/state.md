# LOOF Run 20260807-task-22-candidate-index-catalog-performance-gate

- project_root: `/Users/sky/aich8-zhuochong`
- feature: 步骤 22：校验候选索引、原子切换 catalog 并冻结性能门禁；dispatch_id=aich8-zhuochong-desktop-pet-rag-27-steps-20260805-task-22。只接续步骤20持久化的 building index generation 与 candidate import mapping，重验冻结配置并确认步骤20 chunks/FTS和步骤21 vectors均属同一 indexGenerationId；校验计数、BLOB维度、外键、source conflict、denial及chunk-message映射。成功后才标记ready/completed_at，并在单一BEGIN IMMEDIATE事务内完成ready_candidate→active、旧active→superseded、会话active import pointers、snapshot hash、active index、activated_at及严格递增catalog sequence；任一失败整体回滚并保持旧完整索引服务。building/failed不可见，无完整active组合或无法安全deny过滤时返回KB_NOT_READY。补齐崩溃、维度变化、计数不一致、数据库忙、原子切换与旧索引持续服务测试。目标Windows真实性能门禁须记录真实硬件、冻结exportId/规模、首次索引耗时、数据库/索引体积、峰值内存、查询p50/p95、调度漂移及UI响应，并写knowledge-performance-gate-v1.json；未取得真实证据不得标为通过，超限不得判通过。若BLOB流式扫描超限，仅替换KnowledgeStore内部向量索引实现，不改变knowledge_retrieve()契约。保持只读聊天导出、独立knowledge.sqlite、当前明确选择的前台微信与M2 fail-closed边界，禁止注入、协议/微信数据库、UIA输入、键鼠、自动粘贴/发送、未选聊天以及MCP/Bot/search/upload/Agent调用。
- current_phase: 5
- current_phase_name: 推送同步
- current_thread_id: 019fdc0b-402c-7923-89ec-b04b2c8d6538
- next_poll_at: 2026-08-07T11:48:53+00:00
- status: active

## Artifacts
- phase_1: 2026年08月07日18点35分-校验候选索引原子切换Catalog并冻结性能门禁.md
- phase_2: 2026年08月07日18时56分-校验候选索引原子切换Catalog并冻结性能门禁.md
- phase_3: 2026年08月07日19时30分-候选激活来源一致性与原子性测试缺口.md
- phase_4: 
- phase_5: 

## Threads
- phase_1: 019fdbc9-8ebd-7220-a2db-6ed8e651030d
- phase_2: 019fdbd2-63f6-7060-9068-c67000d7f849
- phase_3: 019fdbf8-e852-7d30-b5e9-56a983ed097c
- phase_4: 019fdc01-884d-7403-83cd-2ae89384ccb6
- phase_5: 019fdc0b-402c-7923-89ec-b04b2c8d6538

## Notes
- 2026-08-07T11:01:10+00:00 phase 2 gate failed: done status is not done: blocked
- 2026-08-07T11:04:47+00:00 phase 2 gate failed: done status is not done: blocked
- 2026-08-07T11:11:50+00:00 phase 2 gate failed: done status is not done: blocked
- 2026-08-07T11:18:32+00:00 phase 2 gate failed: done status is not done: blocked
