# 变更记录：执行 M1 自动化、Windows 实机和 Work Review 回归门禁（202608061815）

## 0. 变更元信息

| 字段 | 内容 |
| --- | --- |
| 变更时间 | 2026-08-06 18:08 |
| 记录时间 | 2026-08-06 18:15 |
| 变更人 | Codex LOOF phase 2 |
| 项目/仓库 | aich8-zhuochong |
| 分支 | main（未创建或切换分支） |
| Commit | `c78a06c376d381d9fd90f4107239b83e99c4c55d`（实现开始时 HEAD；本阶段未提交） |
| 变更类型 | 新增证据门禁与阻断工件 |
| 影响级别 | 高（发布判定），零产品行为变更 |
| 关联需求/问题 | 步骤 14；LOOF run `20260806-1807-步骤-14执行-m1-自动化windows-实机和-work-review-回归门禁对照-kai` |

## 1. 改动目标

- 新增一个只读、可复核的 M1 release-gate runner，判定只能为 `pass`、`fail` 或 `blocked`。
- 让命令摘要、candidate commit、NSIS SHA-256、Windows 四场景、能力计数和素材台账必须属于同一 batch。
- 记录当前真实阻断，禁止以 macOS 静态/fake 结果替代 Windows 实机、安装或许可证据。

## 2. 改动背景与基准

### 2.1 改动前状态

冻结 before 工件仍为 `work-review-v1.1.0-before-wechat-rag-20260805`，其来源提交和 source-manifest SHA-256 均未改动；`UPSTREAM-RUST-001` 仍是固定上游失败归因。当前没有受控 Windows 11 x64 candidate 或完整素材商业授权证据。

### 2.2 本次基于的版本或状态

实现开始时 HEAD 为 `c78a06c376d381d9fd90f4107239b83e99c4c55d`。工作区已有其他 LOOF 步骤的未提交产品改动；本阶段没有编辑它们。

### 2.3 改动原因

步骤 14 要求缺失证据明确阻断，且不会把同一 Git 工作区中的既有自动化结果假定为可发布的 Windows candidate。

## 3. 改动范围

### 3.1 涉及范围

- 新增 `kaifa/kaifa_test/verify_m1_release_gate.py` 及纯虚构 fixture。
- 新增 `desktop/docs/baselines/work-review-m1-after-gate.{json,md}` after 工件。
- 补录本次命令到 `mingling.md`、`test.md` 和本记录。

### 3.2 不涉及范围

- 未修改冻结的 `work-review-source.json`、`work-review-regression-baseline.json` 或 inheritance matrix。
- 未修改产品 Rust/Svelte、Tauri command/capability、配置、数据库、网络、模型或微信输入/发送行为。
- 未启动真实微信、Windows、NSIS、网络或模型服务。

### 3.3 影响对象

发布/步骤 15 的判定只能读取脱敏 evidence；产品用户行为不变。

## 4. 详细改动清单

| 路径 | 类型 | 作用 |
| --- | --- | --- |
| `kaifa/kaifa_test/verify_m1_release_gate.py` | 新增 | 校验 schema、稳定 BASE ID、AC 行、candidate/batch 链接、四 Windows 场景、能力计数、台账和 verdict 闭合。 |
| `kaifa/kaifa_test/fixtures/m1_gate/*` | 新增 | 虚构 pass、blocked、hash mismatch、forbidden capability 解析数据。 |
| `desktop/docs/baselines/work-review-m1-after-gate.json` | 新增 | 当前 after-gate 机器可读阻断工件。 |
| `desktop/docs/baselines/work-review-m1-after-gate.md` | 新增 | 人工复核矩阵、阻断原因和 M2 条件不适用说明。 |

## 5. 处理流程或行为变化

`after-gate JSON` → `verify_m1_release_gate.py` → `pass | fail | blocked`

- `pass`：完整同批证据、所有 blocking 行通过、禁止能力为零、素材审计通过。
- `fail`：测试/场景/计数不满足，或 known-upstream 归因不精确。
- `blocked`：Windows、candidate、哈希、台账或其他必需证据未提供。

## 6. 输入、输出与接口变化

输入为不含聊天正文、路径、截图、凭据和可执行物的 JSON。输出为一条 `M1_RELEASE_GATE: <verdict>`；退出码为 `0=pass`、`1=fail`、`2=blocked`。没有产品 API、Tauri command 或配置接口变化。

## 7. 文件与目录变更

仅新增第 4 节列出的测试、fixture 与 after 文档；没有删除文件或修改产品目录。

## 8. 关键设计决策

### 8.1 缺失证据优先是 blocked

缺 Windows/NSIS/hash/台账不等于测试失败，也绝不能等于通过，因此 runner 对它们返回 `blocked`。

### 8.2 禁止能力优先是 fail

任何 MCP、Bot、上传、搜索、输入、网络或合成输入计数非零，构成已观察到的安全违例，优先返回 `fail`。

### 8.3 before 与 after 分离

runner 固定验证 before baseline identity，after 结果另存，防止用新结果覆盖 `UPSTREAM-RUST-001` 或步骤 2 的事实。

## 9. 验收与测试结果

| 测试项 | 命令/方法 | 实际 | 结果 |
| --- | --- | --- | --- |
| pass fixture | `python3 -B ...verify_m1_release_gate.py --input .../pass.json` | `M1_RELEASE_GATE: pass`，退出 0 | 通过 |
| missing/hash fixture | 两个 blocked fixture | 两次 `blocked`，退出 2 | 通过 |
| forbidden capability fixture | `fail-capability.json` | `fail`，退出 1 | 通过 |
| 当前 after 工件 | `python3 -B ...verify_m1_release_gate.py --project-root .` | `blocked`，退出 2 | 正确阻断 |
| before baseline script | `python3 -B ...verify_work_review_regression_baseline.py --project-root .` | 退出 1；当前产品源码与冻结 source manifest 不同 | 未通过；不归因本阶段 |
| 语法/空白 | `compile(...)`；`git diff --check` | 通过 | 通过 |

## 10. 改动结果

- 证据门禁已实现并由虚构正反例验证。
- 当前 M1 发布门禁状态为 **blocked**，未宣称 Windows 实机或发行完成。

## 11. 当前边界与风险

- 没有 Windows 11 x64、冻结微信 profile、同批 NSIS candidate、四路径 UAT 或哨兵观测。
- 第三方素材台账仍包含 `pending-verification`。
- 当前工作区非冻结源码使 before-baseline verifier 返回 1；该脚本是冻结基线核验，不是本阶段 runner 的错误。

## 12. 回滚方案

通过 `git revert` 回退本阶段新增文件；不会改变 before 基线或产品代码。回退后步骤 14 缺少可复核 runner，应保持 blocked。

## 13. 后续事项

- [ ] 在受控 Windows 11 x64 上创建同批 candidate、NSIS hash 与脱敏观察记录。
- [ ] 运行 Windows success/capture-failed/timeout/cancel、安装、进程/模块/网络/输入哨兵审计。
- [ ] 补齐每项 `pending-verification` 素材的可复核授权证据后再尝试 pass。

## 14. 附录

所有本阶段命令已补录到 `kaifa/kaifa_personnel/mingling.md`；fixture 仅含虚构哈希和标识符。
