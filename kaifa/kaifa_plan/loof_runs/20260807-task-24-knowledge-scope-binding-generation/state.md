# LOOF Run 20260807-task-24-knowledge-scope-binding-generation

- project_root: `/Users/sky/aich8-zhuochong`
- feature: 步骤 24：实现会话范围选择、重名消歧和 bindingGeneration（dispatch_id=aich8-zhuochong-desktop-pet-rag-27-steps-20260805-task-24）。要求：SettingsKnowledge.svelte 与微信气泡入口提供“绑定一个会话”“选择多个会话”“确认全库”三个显式操作且默认不选；KnowledgeStore 仅列本地会话 displayName、群标记、时间范围、消息数和稳定键，不用正文猜测；单会话必须点选确认，同名仍不可区分即拒绝；复用步骤7/8同帧 headerIdentityRoi 与本地 OcrService/OcrBackend，将可验证 header 线索交由用户确认，不得用微信通用窗口标题冒充聊天身份。新增仅进程内 KnowledgeScopeBinding：session nonce、内部窗口 token/PID、header 线索、layout profile、单调 bindingGeneration、已选稳定 conversation keys 或不透明编码；不得持久化窗口 token/PID/header 授权/bindingGeneration。仅可持久化上次稳定 keys 作为提示；重启新 nonce、unbound、首次 M2 必须重绑，不持久化 knowledge_conversations.id。每次请求按稳定键解析 active row；解析失败、前台窗口/header/矩形/profile/用户 scope/会话变化均递增 generation。generation 变化要使 capture、OCR、retrieval、模型结果、待显示建议失效并清泡。检索前及模型传输前重读前台窗口，通过共享捕获协调器重取核对 header 与 generation；header-only 观察只增 bindingObservationVersion，不替换主 captureVersion。header 不可靠时每次请求都需用户重确认 scope，否则 KB_SCOPE_UNRESOLVED，禁止自动识别切换或自动全库。群聊可选作知识范围但实时回复验收仍限支持 profile 的单聊。sameConversationBoost 只能在授权集合内重排。覆盖重名、同标题不同 HWND、header 变化、窗口切换、resize、重启、数据库重建、scope 修改、晚到结果、全库确认。验收：无明确 scope 不得 M2；重名不自动选；重建不误绑；重启 unbound；变化后旧建议不可复制；scope 不静默扩大。保持安全边界：只处理用户当前明确选择的前台微信；禁止微信注入/协议/数据库读取/UI Automation 输入/键鼠模拟/自动粘贴发送/未选聊天处理，以及 MCP/Bot/search/upload/Agent 工具调用；原始导出只读、运行时只用独立 knowledge.sqlite；M2 每次模型前同 requestId 完成 knowledge_retrieve，失败不得降级 M1。前置步骤 19、23；复用现有真实代码，最小改动。
- current_phase: 5
- current_phase_name: 推送同步
- current_thread_id: 019fdcab-0161-7d02-a6af-37ac409c2d2d
- next_poll_at: 2026-08-07T14:43:27+00:00
- status: active

## Artifacts
- phase_1: 2026年08月07日21点48分-会话范围绑定与代际失效.md
- phase_2: 2026年08月07日22点15分-会话范围绑定与代际失效.md
- phase_3: 2026年08月07日22点22分-会话绑定阶段复核与失效补偿缺口.md
- phase_4: 
- phase_5: 

## Threads
- phase_1: 019fdc77-bdf3-7690-b0f7-be80619f72dc
- phase_2: 019fdc80-f8a4-7fa2-8650-41e75b4f23d1
- phase_3: 019fdc96-8776-76f0-96df-4b478c4a99b8
- phase_4: 019fdc9e-b840-7bf1-a638-2c88ab5f97f3
- phase_5: 019fdcab-0161-7d02-a6af-37ac409c2d2d

## Notes
