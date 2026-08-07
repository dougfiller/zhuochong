# 变更记录：实现唯一 knowledge_retrieve 混合检索门面（202608072035）

## 0. 变更元信息

| 字段 | 内容 |
| --- | --- |
| 变更时间 | 2026-08-07 20:24—20:35 |
| 记录时间 | 2026-08-07 20:35 |
| 变更人 | Codex |
| 项目/仓库 | aich8-zhuochong |
| 分支 | main |
| 基准 Commit | 4aa1944 |
| 变更类型 | 新增功能、测试与记录补录 |
| 影响级别 | 中 |
| 关联需求/问题 | run_id `20260807-2013-步骤-23实现唯一-knowledge_retrieve-混合检索门面dispatch_idai` |

## 1. 改动目标

- 建立唯一 `KnowledgeStore::knowledge_retrieve(request)`，从严格 active-ready catalog 执行授权 FTS + 本地向量 + 共享 RRF。
- 只允许私有 `success/no_hit` 构造 `RetrievedReply`；Store、scope、SQLite、索引、代际或组装错误均不产生回复。
- 保持只读导出、独立 `knowledge.sqlite`、明确用户选择和手动检查/复制/粘贴/发送边界。

## 2. 改动背景与基准

### 2.1 改动前状态

`knowledge/types.rs` 只有接收人工 `KnowledgeRetrieveResult` 的步骤 4 占位入口；既有 Store/embedding 已具备 active loader、FTS5、严格 BLOB Top-K、loopback fingerprint 和共享 RRF，但没有统一的 scope token、正文重取和完整结果绑定。

### 2.2 本次基于的版本或状态

基于 `main` 的 `4aa1944` 及步骤 22 schema v5/catalog 激活实现。工作区中步骤 22 run 状态与 `kaifa/定时任务.md` 的既有 dirty 改动未被本阶段触碰。

### 2.3 改动原因

步骤 23 要求在步骤 24/25 接线之前冻结一个可审计、fail-closed、无回复模型依赖的唯一知识检索门面。

## 3. 改动范围

### 3.1 涉及范围

- 新增 `knowledge/retrieve.rs`：request/error/status/mode、私有 hit/reply、检索编排、预算/hash/trace 与行为测试。
- 修改 `store.rs`：冻结 retrieval budget、类型化 scope token、授权 FTS/向量候选、受控正文/来源重取和代际重验。
- 修改 `embedding.rs`：将连接/timeout 暂不可达分类为显式 fallback，policy/fingerprint/payload/dimension 错误保持 fail-closed。
- 收敛 `types.rs`/`mod.rs`：移除旧占位入口并导出唯一契约。
- 新增静态门禁并补录 `mingling.md`、`test.md`。

### 3.2 不涉及范围

- 不新增/修改 migration、Tauri command、设置 UI、步骤 24 会话绑定或步骤 25 模型编排。
- 不新增微信协议/数据库访问、注入、UIA、键鼠模拟、自动发送、上传、MCP、Bot、search 或 Agent。

### 3.3 影响对象

仅影响知识模块内部 M2 预备契约；当前不注册用户入口，也不执行真实微信或回复模型调用。

## 4. 详细改动清单

- `desktop/src-tauri/src/knowledge/retrieve.rs`：唯一异步门面、确定性 query 规范化、三 scope、RRF/boost、512/总预算 UTF-8 截断、冻结 hash、私有 trace 和 13 个行为测试。
- `desktop/src-tauri/src/knowledge/store.rs`：`FrozenRetrievalRead`/`AuthorizedScopeToken`/候选/payload DTO；统一 active mapping、denial、每消息 active provenance SQL；generation 前后重验。
- `desktop/src-tauri/src/knowledge/embedding.rs`：`QueryVectorAttempt::{Available,Unavailable}` 与一次 query embedding。
- `desktop/src-tauri/src/knowledge/types.rs`：只保留精确 scope wire 与契约 re-export；旧 `knowledge_retrieve(result)` 删除。
- `desktop/src-tauri/src/knowledge/mod.rs`：注册 retriever 并导出窄契约。
- `kaifa/kaifa_test/verify_knowledge_retrieval_facade.py`：37 项纯静态边界门禁。
- `kaifa/kaifa_personnel/mingling.md`、`kaifa/kaifa_test/test.md`：实际命令、结果和 not-run 边界。

## 5. 处理流程或行为变化

`规范化/校验 request → 冻结 active catalog/index/scope → 授权 FTS → 一次本地 query embedding → 授权向量扫描或 FTS fallback → 共享 RRF → 候选内 boost → 再授权重取正文/来源 → token 预算 → 代际重验 → 私有 success/no_hit`。

任何阶段的非 transport-unavailable 错误直接返回三种稳定 `KB_*` 错误之一，不返回部分 hit。

## 6. 输入、输出与接口变化

- 请求新增并绑定 `request_id/query_text/binding_generation/bound_conversation_id/scope/top_k/token_budget/token_counter_version/same_conversation_boost`，未知字段拒绝。
- 结果私有绑定 catalog/index/snapshot/result hash、status、mode、完整本地 hit 与 elapsed；不实现前端反序列化。
- `KnowledgeError` 只对外稳定序列化为 `KB_NOT_READY`、`KB_SCOPE_UNRESOLVED`、`KB_RETRIEVAL_FAILED`。

## 7. 文件与目录变更

| 路径 | 类型 | 作用 |
| --- | --- | --- |
| `desktop/src-tauri/src/knowledge/retrieve.rs` | 新增 | 唯一检索门面、私有回复与行为测试 |
| `desktop/src-tauri/src/knowledge/store.rs` | 修改 | 授权候选、正文重取、冻结代际 |
| `desktop/src-tauri/src/knowledge/embedding.rs` | 修改 | 向量可用性分类与 query primitive |
| `desktop/src-tauri/src/knowledge/types.rs` | 修改 | 删除旧占位入口、保留 scope/re-export |
| `desktop/src-tauri/src/knowledge/mod.rs` | 修改 | 注册/导出唯一门面契约 |
| `kaifa/kaifa_test/verify_knowledge_retrieval_facade.py` | 新增 | 步骤 23 静态门禁 |
| `kaifa/kaifa_personnel/mingling.md` | 修改 | 命令与安全说明补录 |
| `kaifa/kaifa_test/test.md` | 修改 | 实际测试结果与 not-run 边界 |

## 8. 关键设计决策

### 8.1 授权先于排序且重取时再次授权

原因：禁止全库召回后在 UI/Rust 展示层过滤；FTS、向量和 payload 都复用 active mapping、denial 与每消息 active provenance 谓词。

### 8.2 仅 transport unavailable 允许 FTS fallback

原因：fingerprint、endpoint policy、payload/dimension/finite 与 BLOB 损坏属于完整性错误，降级会掩盖索引问题。

### 8.3 版本化保守向量门槛

`retrieval-policy-v1` 固定 `MIN_VECTOR_SCORE_V1=0.20`。当前只有合成行为证据，真实中文质量保持 not-run。

## 9. 验收与测试结果

| 测试项 | 命令/方法 | 实际 | 结果 |
| --- | --- | --- | --- |
| 门面行为 | `cargo test ... 'knowledge::retrieve::tests' ...` | 13/13 passed（获准本机 loopback） | 通过 |
| knowledge 回归 | `cargo test ... 'knowledge::' ...` | 81 passed、0 failed、1 ignored | 通过 |
| M2 编译 | `cargo check ... 'wechat-contract-check,wechat-m2'` | exit 0，仅既有/预留 warning | 通过 |
| 静态门禁 | `python3 -B kaifa/kaifa_test/verify_knowledge_retrieval_facade.py` | `status=passed checks=37` | 通过 |
| 私有构造 | private-constructor feature compile probe | 预期 exit 101，两个私有 struct literal 均被拒绝 | 通过（预期失败） |
| 格式/空白 | scoped `rustfmt --check` + `git diff --check` | exit 0 | 通过 |
| 真实质量/Windows | ignored probe 与受控目标机 | not-run | 未运行 |

## 10. 改动结果

- 唯一门面、三 scope、hybrid/fallback/no-hit、受控重取、完整响应、冻结 hash 与 fail-closed 错误分流已实现并通过定向/模块回归。
- 当前状态：phase 2 实现与记录补录完成，等待独立 phase 3 审查。

## 11. 当前边界与风险

- 真实中文 Recall@5、真实大规模库与 Windows 性能未运行；`0.20` 门槛不能表述为真实质量已验证。
- 步骤 23 不接 UI/模型；`knowledge_retrieve` 在步骤 25 前预期没有生产调用点，因此非测试编译会有预留 dead-code warning。
- 默认沙箱禁止临时 loopback bind；行为测试需在获准环境运行，已明确区分环境限制与产品结果。

## 12. 回滚方案

- 回滚本记录列出的步骤 23 Rust/测试/文档文件；恢复 `types.rs` 旧契约只用于回退代码，不修改或删除用户 `knowledge.sqlite`。
- 无 migration/数据回滚动作；回滚后复跑步骤 22 knowledge 回归确认 active catalog 不变。

## 13. 后续事项

- [ ] phase 3 独立审查 Store SQL、错误映射、并发代际与私有 getter。
- [ ] 步骤 24/25 仅通过受信任绑定适配器接入，不在 UI/模型层重建 scope 或原始 hit。
- [ ] 由显式本地模型/脱敏质量集单独校准下一版本向量门槛；当前不冒充通过。

## 14. 附录

- 方案：`kaifa/kaifa_plan/2026年08月07日20点16分-实现唯一knowledge_retrieve混合检索门面.md`
- run_id：`20260807-2013-步骤-23实现唯一-knowledge_retrieve-混合检索门面dispatch_idai`
- 全部 fixture 均为虚构数据；未读取真实微信、用户导出或外网。
