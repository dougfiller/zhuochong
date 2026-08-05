## 冻结 Work Review 来源并建立正式产品源码副本（2026-08-05）

检测脚本：`kaifa/kaifa_test/verify_work_review_baseline.py`

该脚本不启动产品、不安装依赖；它验证步骤 1 的复制工件。`create` 会先以官方固定 commit 的 `git archive` 对本地参考进行内容级比对，只有比对一致才在临时同级目录完成复制与检测后原子创建 `desktop/`；`verify` 可以在后续重复执行。

| 验收项 | 命令/方法 | 实际结果 |
| --- | --- | --- |
| 脚本接口 | `python3 -B kaifa/kaifa_test/verify_work_review_baseline.py --help` | 通过；提供 `create`、`verify`、来源和目标参数。 |
| 固定上游身份 | 以 `git clone --depth 1 --branch v1.1.0` 检出官方仓库，验证 tag、commit 和 tree | 通过；`v1.1.0` 解析为 `500f9d2cb3027392cfcc32ad18395dfe348fb4a1`，tree 为 `ae807b665fe55e609dd7b81f25d4214ef9e9eae9`。 |
| 本地参考与官方归档 | 以官方 `git archive` 比较 `参考/Work-Review-main/` | 通过；已仅删除三个 README 中偏离固定提交的额外说明，580 文件、44,961,912 字节逐项一致。 |
| 正向来源证明 | 用当前参考执行 `create` 与 `verify` | 通过；创建和两次在线复核均报告固定 commit `500f9d2cb3027392cfcc32ad18395dfe348fb4a1` 与 tree `ae807b665fe55e609dd7b81f25d4214ef9e9eae9`。 |
| 前端门禁 | `npm ci && node --test && npm run build` | 通过；479/479 测试通过，Vite 生产构建成功。 |
| Rust 编译与 lint | `cargo check --workspace --all-targets`、`cargo clippy --workspace --all-targets -- -D warnings` | 通过；仅报告上游依赖 `block v0.1.6` 的 future-incompat 警告。 |
| Rust workspace 测试 | `cargo test --workspace` | 失败；372 通过、1 失败：`commands::system::tests::图标解析应忽略与app_name矛盾的executable_path` 断言实际首位图标为 `None`。该失败位于冻结上游业务代码，不在本次来源修复范围。 |
| 当前 `desktop/` 状态 | 复核 manifest 与官方归档 | 已验证为可信上游冻结基线；测试生成的 `node_modules/` 和 `dist/` 已移除，基线恢复为 580 个受控文件。 |

本次来源门禁已解除；但 Rust workspace 测试尚非全绿，后续如要解除完整发布门禁，应以独立任务诊断并修复上述上游测试失败，不能把它掩盖为来源冻结问题。

## Work Review 修改前回归基线与继承矩阵（2026-08-05）

检测脚本：`kaifa/kaifa_test/verify_work_review_regression_baseline.py`。脚本只读取 `desktop/` 的冻结 manifest、矩阵、结果 JSON 和脱敏摘要；不启动应用、不安装依赖、不联网，也不会读取聊天导出或真实截图。

| 验收项 | 命令/方法 | 实际结果 |
| --- | --- | --- |
| 自动化前端基线 | `cd desktop && node --test` | 通过；479/479。摘要见 `BASE-AUTO-FE-summary.txt`。 |
| 自动化前端构建 | `cd desktop && npm run build` | 通过；Vite 5.4.21，240 个模块转换完成。 |
| Rust 编译 | `cd desktop && cargo check --workspace --all-targets --quiet` | 通过。 |
| Rust lint | `cd desktop && cargo clippy --workspace --all-targets -- -D warnings` | 通过；保留上游依赖 `block v0.1.6` 的 future-incompat 警告，不误报零警告。 |
| Rust workspace 测试 | `cd desktop && cargo test --workspace --quiet` | 失败；372 通过、1 失败，固定登记为 `UPSTREAM-RUST-001`，未修改或跳过该测试。 |
| 基线工件一致性 | `python3 -B kaifa/kaifa_test/verify_work_review_regression_baseline.py --project-root .` | 应通过；校验 10 条 AC-BASE、5 条自动化支撑行、冻结 580 文件、摘要 SHA-256 和既有失败归因。 |
| 冻结来源复核 | `python3 -B kaifa/kaifa_test/verify_work_review_baseline.py verify --source '参考/Work-Review-main' --destination desktop` | 应通过本地快照；只有网络可用时才会重新验证官方归档。 |

Windows 11 x64 + WebView2 的 BASE-01--05、07--08 记录为 `blocked`，而无凭据模型/网络契约的 BASE-06、09--10 记录为 `not-run`；这些不是通过结果。后续在受控 Windows 机器上只能补同一行的 `before` 证据，任何微信/RAG 改动后只能补 `after` 证据。
