# LOOF Run 20260806-task-15-wechat-json-archive-importer

- project_root: `/Users/sky/aich8-zhuochong`
- feature: 步骤 15：冻结聊天导出 schema、只读规则和脱敏导入夹具；dispatch_id=aich8-zhuochong-desktop-pet-rag-27-steps-20260805-task-15。仅只读读取根 manifest.json、report.json、_integrity/ 已声明完整性信息、会话 meta.json/messages.json 结构；记录 schemaVersion/exportId/account/exportedAt/scope-filters/快照类型/统计。当前冻结样本核对721会话、402673消息并记录探针时间及 manifest hash，但这些数字非永久协议；scope=selected 仅为 filtered/selected coverage，禁止缺失即删除或授予 full snapshot。制作各消息类型完全脱敏、小型 wechat_archive_v1 fixture，重写 account/conversation/message ID 和正文，禁止真实正文进入源码、测试输出和普通日志。定义 WechatJsonArchiveImporter schema 路由；未知版本返回 KB_SOURCE_UNSUPPORTED、不得猜字段。完整性仅消费 manifest/JSON/_integrity 已给元数据，禁止为校验打开、重算 hash 或遍历媒体正文。只读 guard：importer 只持读句柄，状态日志数据库写产品 dataDir，不在源目录创建文件。最大 messages.json 约151MiB/108447条消息，作为流式内存验收样本；探针仅遍历 JSON/manifest 所需路径，不遍历/stat/hash/打开媒体。导入前后比较 manifest/JSON/_integrity 的 size、mtime、声明hash，记录校验范围/缺失/completeness_verdict 到派生库，不把未查媒体误写完整通过。用 account stable ID/export ID/schema/manifest-content hash/coverage signature 判定未变化源包，完全匹配走可审计快速核验，不重新流式全解析。确认私有源和 fixture 中间文件均被 Git/安装包忽略。保持产品边界：仅当前选择的前台微信，无注入、协议/数据库读取、UI Automation输入、键鼠模拟、自动粘贴/发送、未选聊天、MCP/Bot/search/upload/Agent。M2 每次调用前同 requestId knowledge_retrieve，检索/scope/context失败不得降级 M1。
- current_phase: 5
- current_phase_name: 推送同步
- current_thread_id: 019fd706-6d67-7a21-a5f4-119853528e63
- next_poll_at:
- status: active

## Artifacts
- phase_1: 2026年08月06日19点18分-冻结微信聊天导出Schema只读规则和脱敏导入夹具.md
- phase_2: 2026年08月06日19点26分-微信聊天导出Schema只读导入夹具.md
- phase_3: 2026年08月06日19点35分-微信聊天导出Schema审查阻断问题.md
- phase_4:
- phase_5:

## Threads
- phase_1: 019fd6ca-c786-7641-9136-8f098cbcfc59
- phase_2: 019fd6ce-0183-7800-8d65-f8abf29c143b
- phase_3: 019fd6d9-c43b-7152-9a94-cfb95adbbc8d
- phase_4: 019fd702-4527-7932-8e10-1c149125830c
- phase_5: 019fd706-6d67-7a21-a5f4-119853528e63

## Notes
- 2026-08-06T11:24:17+00:00 phase 2 gate failed: done file is missing
- 2026-08-06T11:27:35+00:00 phase 2 gate failed: done file is missing
