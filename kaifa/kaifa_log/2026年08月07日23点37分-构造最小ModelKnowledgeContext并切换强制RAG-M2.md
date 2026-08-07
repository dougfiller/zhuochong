# 变更记录：构造最小 ModelKnowledgeContext 并切换强制 RAG M2（202608072337）

## 0. 变更元信息

| 字段 | 内容 |
| --- | --- |
| 变更时间 | 2026-08-07 23:11–23:37 |
| 记录时间 | 2026-08-07 23:37 |
| 变更人 | Codex |
| 项目/仓库 | aich8-zhuochong |
| 分支 | main |
| Commit | 未提交；基准 `2e9b9d6` |
| 变更类型 | 新增功能/安全合约收紧 |
| 影响级别 | 高 |
| 关联需求/问题 | LOOF `20260807-task-25-minimal-model-context-m2-rag`；步骤 25 方案 |

## 1. 改动目标

- 把 M2 微信建议收紧为 `knowledge_retrieve -> build_model_context -> revalidate -> generate_rag_reply`的必经链路。
- 只允许同 requestId 的冻结 `RetrievedReply` 由私有适配器构造最小 `ModelKnowledgeContext`，以 permit 绑定 request/binding/context hash 后才可调用模型。
- 为用户提供显式加载历史、显式点击的本次来源查看，从当前 Store 重取安全片段，不在 trace/前端 DTO 暴露内部 ID、路径或分数。

## 2. 改动背景与基准

### 2.1 改动前状态

步骤 23 已有唯一 `knowledge_retrieve`门面，步骤 24 已有会话 scope binding，但 M2 模型输入的最小化、二次预算、permit 授权、M1 不可达证据和来源重取尚未完整接线。

### 2.2 本次基于的版本或状态

基于 `main@2e9b9d6`。工作区已有步骤 24 运行状态和定时任务文件的无关 dirty 变更；本阶段保留并未归因、未清理这些文件。

### 2.3 改动原因

方案要求每次 M2 都必须使用同 requestId 的授权检索结果，数据边界、预算、重试和来源查看均 fail-closed，不能在检索/context 失败时回退 M1。

## 3. 改动范围

### 3.1 涉及范围

- Rust knowledge Store/retrieval 安全上下文行与 active-source 重取。
- Rust WeChat model contract/client/runtime/state machine/reply flow/trace/commands 和 Tauri command 注册。
- 知识设置页的历史摘要与来源显式查看，四语言文案与定向 Node 断言。
- 步骤 25 静态验证脚本、命令/测试说明和本记录。

### 3.2 不涉及范围

- 不修改原始微信导出，不直读微信数据库/协议，不添加注入、UIA 输入、键鼠模拟、自动粘贴或发送。
- 不添加 Agent/tools/search/upload/MCP/Bot 路径，不执行真实模型请求。
- 不修改 heartbeat automation，不执行发布/推送，不伪造 Windows、性能或素材合规证据。

### 3.3 影响对象

- `wechat-m2` 的用户显式触发回复链路和本地可追溯性。
- `wechat-m1` 仍可单独编译，但与 M2 互斥，M2 构建不存在 M1 可达路径。

## 4. 详细改动清单

- `knowledge/store.rs`：在单一 Store reader 授权边界内读取入选 chunk 的安全时间/方向/文本行；新增从当前 active Store 重取片段并在来源变化时返回不可用。
- `knowledge/retrieve.rs`/`types.rs`/`mod.rs`：把授权 hit 转为结构化安全 context parts，冻结 token counter/budget，不向编排层开放私有构造。
- `wechat/model_contract.rs`：实现唯一 `build_model_context(RetrievedReply)`；构造系统规则/不可信历史/当前文字的 canonical payload，按实际序列化字节计数，仅从尾部整 hit 裁减，生成 context hash 和安全 excerpt hash。
- `wechat/model_client.rs`/`runtime.rs`/`state_machine.rs`：仅接受绑定完整的 context+permit，生成阶段与 request/binding/hash/model 完全匹配后调用；可重试错误仅重放完全相同请求一次。
- `wechat/reply_flow.rs`/`commands.rs`/`mod.rs`/`main.rs`：M2 路径在 OCR 后检索、冻结 context、再次验证窗口/header/binding、授权并生成建议；M1/M2 command 编译分支互斥，注册本次来源查看 command。
- `wechat/trace.rs`：冻结 M2 context/token/来源安全摘要，前端 DTO 只返回数量、是否可查看和 context hash 短前缀；内部 receipt 保留重取所需 hit token。
- `SettingsKnowledge.svelte` 与 locale/test：新增用户显式加载 M2 记录和显式查看本次来源，不自动重取正文；移除与强制 M2 现状冲突的过时提示。
- `verify_task25_m2_rag.py`：新增 fail-closed 源码边界检查。

## 5. 处理流程或行为变化

`explicit trigger -> capture/OCR -> begin binding request -> knowledge_retrieve -> build frozen minimal context -> revalidate window/header/binding -> issue ModelCallPermit -> generate_rag_reply -> trace safe receipt -> publish suggestion`

- `success` 和 `no_hit` 可进入模型；`KB_NOT_READY`/`KB_SCOPE_UNRESOLVED`/`KB_RETRIEVAL_FAILED`/context 超限或授权不匹配均在模型网络调用前终止。
- 只在用户明确触发时运行；最终建议仅供人工审阅/复制/粘贴/发送。

## 6. 输入、输出与接口变化

- 模型入口收紧为 `generate_rag_reply(&ModelKnowledgeContext, &ModelCallPermit)`；请求消息由 contract 内部生成，无 tools/Agent 字段。
- `get_wechat_reply_sources({ requestId })` 新增；前端输出仅有 ordinal、availability、时间/角色/文本行，不返回 hit/chunk/message/conversation ID、路径、provenance 或分数。
- trace 的 M2 前端摘要新增 `sourceCount`/`hasSourceDetails`/`contextHashPrefix`；内部 receipt 不直接序列化到前端。

## 7. 文件与目录变更

| 路径 | 类型 | 作用 |
| --- | --- | --- |
| `desktop/src-tauri/src/knowledge/{store,retrieve,types,mod}.rs` | 修改 | 安全 context parts、冻结预算与 Store 重取 |
| `desktop/src-tauri/src/wechat/{model_contract,model_client,runtime,state_machine,reply_flow,trace,commands,mod}.rs` | 修改 | 强制 RAG M2 合约、编排、permit、trace 与来源命令 |
| `desktop/src-tauri/src/main.rs` | 修改 | 注册来源查看 command |
| `desktop/src/routes/settings/components/SettingsKnowledge.svelte` | 修改 | 显式历史/来源 UI |
| `desktop/src/routes/settings/SettingsWechatKnowledge.test.js` | 修改 | 显式调用与脱敏静态断言 |
| `desktop/src/lib/i18n/locales/{ar,en,zh-CN,zh-TW}.js` | 修改 | 来源查看四语言文案 |
| `kaifa/kaifa_test/verify_task25_m2_rag.py` | 新增 | 步骤 25 静态 fail-closed 验证 |
| `kaifa/kaifa_personnel/mingling.md` | 修改 | 新命令/脚本补录 |
| `kaifa/kaifa_test/test.md` | 修改 | 检测方法、结果与边界 |
| 本文件 | 新增 | 阶段 2 代码改动记录 |

## 8. 关键设计决策

### 8.1 以私有类型和 permit 证明授权，不依赖调用约定

`RetrievedReply`/`ModelKnowledgeContext` 的成功构造受限，permit 只能在生成阶段且 request/binding/hash/model 全部一致时发放，使绕过检索或替换 context 在编译期/运行时 fail-closed。

### 8.2 对实际 canonical payload 二次计数

预算覆盖真正序列化后的 system/user 字节，超限时只从尾部移除整 hit，不切断 UTF-8、不变更顺序；固定部分仍超限则不调模型。

### 8.3 历史知识统一标记为不可信文本

历史中的命令、URL、工具或上传要求只能作为人类聊天参考，不会生成工具调用或动作通道。

### 8.4 来源详情按当前 active Store 重取

trace 只保留安全摘要和后端内部 receipt，不保存可长期暴露的正文快照。退役、拒绝、删除或代际改变后不返回旧正文。

## 9. 验收与测试结果

| 测试项 | 命令/方法 | 预期 | 实际 | 结果 |
| --- | --- | --- | --- | --- |
| 步骤 25 静态门禁 | `python3 -B kaifa/kaifa_test/verify_task25_m2_rag.py` | 安全边界全部存在 | `TASK25_M2_RAG_STATIC_OK` | 通过 |
| WeChat M2 回归 | `cargo test ... 'wechat::' ... --no-fail-fast` | 合约/运行时/spy/trace 通过 | 75 passed、0 failed | 通过 |
| Retrieval 回归 | `cargo test ... 'knowledge::retrieve::tests' ...` | 入选 context 行与既有检索回归通过 | 获准的本地 loopback 环境 23 passed、0 failed | 通过 |
| M2/M1 正向编译 | 分别 `cargo check ... wechat-m2`、`wechat-m1` | 均 exit 0 | 均 exit 0 | 通过 |
| 四组编译负向探针 | M2→M1、私有构造、零 release feature、双 release feature | 均 exit 101 并命中目标边界 | 均预期 exit 101 | 通过（预期失败） |
| 设置页定向 | `node --test ...SettingsWechatKnowledge.test.js` | 显式加载/查看，不自动正文 | 1/1 passed | 通过 |
| 前端生产构建 | `npm run build` | 构建成功 | 246 modules、exit 0 | 通过 |
| Rust 格式 | `cargo fmt ... -- --check` | 无差异 | exit 0 | 通过 |
| 范围空白 | scoped `git diff --check` | 无 whitespace error | exit 0 | 通过 |
| Windows/真实模型/性能/发布 | 受控目标环境 | 只有证据才可声称通过 | 未运行 | not-run |

## 10. 改动结果

- 步骤 25 计划内的实现、必要合成/行为/编译测试、新脚本、命令补录、测试说明和改动记录已完成。
- M2 路径的检索授权、最小 context、二次预算、permit、冻结重试和来源重取已接线；M1 不作为失败回退。
- 当前状态：阶段 2 实现完成，等待 LOOF phase 3 审查。

## 11. 当前边界与风险

- Windows 前台微信、真实 OCR/header/窗口切换、冻结 production profile 和真实模型请求未运行；现有证据不替代真机 UAT。
- 真实规模聊天的 context 质量、延迟、峰值内存和 UI 响应未测；本阶段不声称性能通过。
- 获准环境中的 loopback 测试仅使用虚构数据验证行为，不是真实数据合规证据。
- 工作区的步骤 24/定时任务 dirty 文件为既有并行状态，不在本 run 范围。

## 12. 回滚方案

- 由后续阶段按本记录第 7 节的明确文件集进行针对性 revert；不使用 `git reset --hard`，不删除无关 dirty 文件。
- 回滚后必须重跑 M1/M2 feature 编译、WeChat/retrieval 定向回归、设置页测试与生产构建，并确认旧 M1 手工安全边界未受破坏。

## 13. 后续事项

- [ ] LOOF phase 3 仅审查本 run 的实现、测试和补录证据。
- [ ] 在受控 Windows 11/冻结 profile/用户已验证文本模型环境执行纵向 UAT，且不放宽手工发送安全边界。
- [ ] 在不保存正文/内部 ID 证据的前提下补充真实规模性能与可用性验证。

## 14. 附录

- 方案：`kaifa/kaifa_plan/2026年08月07日23点06分-构造最小ModelKnowledgeContext并切换强制RAG-M2.md`
- 新验证器：`kaifa/kaifa_test/verify_task25_m2_rag.py`
- 命令清单：`kaifa/kaifa_personnel/mingling.md`
- 测试结果：`kaifa/kaifa_test/test.md`
- 本阶段未创建提交、未推送、未修改 heartbeat automation。
