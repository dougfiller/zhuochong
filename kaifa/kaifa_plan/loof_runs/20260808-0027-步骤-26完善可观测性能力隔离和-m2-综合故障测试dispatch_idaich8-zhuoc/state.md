# LOOF Run 20260808-0027-步骤-26完善可观测性能力隔离和-m2-综合故障测试dispatch_idaich8-zhuoc

- project_root: `/Users/sky/aich8-zhuochong`
- feature: 步骤 26：完善可观测性、能力隔离和 M2 综合故障测试；dispatch_id=aich8-zhuochong-desktop-pet-rag-27-steps-20260805-task-26。用可复核证据证明先检索后模型、数据不越界、旧功能不退化并覆盖真实故障和并发：汇总 reply trace、model transport spy、embedding spy、上传队列 spy 与 capability 计数，按 requestId 串联且不记录正文/API key；分别计数允许的 replyModelNonLoopback、knowledgeEmbeddingLoopback、条件 ocrBackendLocalProcess 以及禁止的 MCP/Bot/Localhost API/upload/search/action，只有微信 OCR 编排器可启动获准本地 OCR，桌宠/资源配置启动计数为 0；以 stageSeq/合法迁移证明 retrieval_completed(success|no_hit) 先于 model_transport_started。故障注入覆盖 KB_NOT_READY、scope unresolved、embedding 不可用、FTS fallback、SQLite busy/损坏、context 组装失败、binding 晚失效；模型超时、有限重试、空输出、tool call、provider 错误、晚到结果；导入/索引崩溃、候选计数错、维度变化、source conflict、删除/deny、原子切换。验证模型重试 context hash/hits 不变且检索不重复；上传队列即使全局开启上传/MCP/Bot 也无微信/知识库路径、正文、对象 ID、派生文件；内容留存默认关闭/开启/到期/手动删除/来源重取失败/不入知识库。用无害哨兵 DLL、脚本、命令清单、URL 声明观测桌宠/资源配置文件访问、模块加载、子进程、网络、合成输入，调用全部为 0。重跑当前适用 AC-BASE、AC-WX、AC-PET 及全部 AC-KB、AC-RAG；无真实凭据的 Work Review 原网络能力保留 schema/mock/队列/错误路径；扫描源码夹具、trace、截图、安装包、更新包无真实聊天数据；复核参考/素材台账来源、commit、复制结论、显著修改、LICENSE/NOTICE/归属及逐素材商业授权，任一缺口阻止发布。参考 Work-Review JS/Rust 测试、CI、agent/remote_upload/localhost_api/Bot、errorDisplay 与 Settings/Avatar 测试；M2 stageSeq/spy/故障矩阵/哨兵观测原创。验收：stageSeq 与真实 transport 证明每次 M2 先检索后模型；所有检索错误模型调用数 0；禁止 capability 和微信/知识上传队列项恒 0；全部适用 AC 通过。保持仅用户明确选择前台微信、手动审阅复制粘贴发送；禁止注入、微信协议/数据库、UIA 输入、键鼠模拟、自动发送、未选聊天与 MCP/Bot/search/upload/Agent 工具调用；原始聊天导出只读，运行时仅独立 knowledge.sqlite；没有真实凭据/Windows/素材授权证据时不得伪造真机、性能、合规或正式发布通过。
- current_phase: 5
- current_phase_name: 推送同步
- current_thread_id: 019fdd40-561f-7333-95f8-b9c5b82136ba
- next_poll_at: 2026-08-07T17:26:34+00:00
- status: active

## Artifacts
- phase_1: 2026年08月08日00点29分-完善可观测性能力隔离和M2综合故障测试.md
- phase_2: 2026年08月08日00时50分-完善M2可观测性能力隔离和严格故障门禁.md
- phase_3: 2026年08月08日01时01分-M2严格门禁证据链缺失审查.md
- phase_4: 
- phase_5: 

## Threads
- phase_1: 019fdd0d-5fcd-7273-ae24-ed107a7ee10b
- phase_2: 019fdd16-351e-75c3-87ec-2113d3c79bb7
- phase_3: 019fdd29-686c-7751-9376-67f6a1c267e3
- phase_4: 019fdd2e-ea69-7dc0-8a08-3830caff2e78
- phase_5: 019fdd40-561f-7333-95f8-b9c5b82136ba

## Notes
- 2026-08-07T16:28:14+00:00 phase 1 gate failed: done file is missing
- 2026-08-07T16:33:51+00:00 phase 1 gate failed: done file is missing
- 2026-08-07T16:44:15+00:00 phase 2 gate failed: done file is missing
- 2026-08-07T16:47:45+00:00 phase 2 gate failed: done file is missing
- 2026-08-07T16:51:15+00:00 phase 2 gate failed: done file is missing
- 2026-08-07T16:54:44+00:00 phase 2 gate failed: done file is missing
- 2026-08-07T17:10:50+00:00 phase 4 gate failed: done file is missing
- 2026-08-07T17:17:18+00:00 phase 4 gate failed: done file is missing
