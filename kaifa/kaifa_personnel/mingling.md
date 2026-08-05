## 冻结 Work Review 来源并建立正式产品源码副本（2026-08-05）

### 基线复制与复核

```bash
python3 -B kaifa/kaifa_test/verify_work_review_baseline.py create \
  --source '参考/Work-Review-main' \
  --destination desktop

python3 -B kaifa/kaifa_test/verify_work_review_baseline.py verify \
  --source '参考/Work-Review-main' \
  --destination desktop
```

- `create` 仅在 `desktop/` 不存在时执行：先从官方 Git 仓库检出固定 tag，并以 `git archive` 获取固定 commit 的文件集。只有该文件集与本地参考逐文件 SHA-256 一致时，才会在同级临时目录复制、写入三份基线元数据并原子形成 `desktop/`；任何不一致或上游不可获取都会失败且不创建目标目录。
- `verify` 先复核来源清单、目标路径/字节数/SHA-256、必需工程文件、许可证、第三方声明和禁止路径；可联网时还会重新取得官方归档并复核 tree 与文件清单。网络不可用时它只报告“本地快照已复核”，不将上游身份标记为已验证。
- 固定来源身份为 `https://github.com/wm94i/Work-Review` 的 `v1.1.0`，解析提交 `500f9d2cb3027392cfcc32ad18395dfe348fb4a1`。`-B` 禁止 Python 在本机缓存目录写入 `.pyc`，保持本步骤仅修改项目内产物。

## 建立 Work Review 修改前回归基线与继承矩阵（2026-08-05）

```bash
# 仅校验结构、冻结源码、脱敏证据哈希和已知失败；不启动应用、不安装依赖、不联网。
python3 -B kaifa/kaifa_test/verify_work_review_regression_baseline.py --project-root .

# 复核步骤 1 的冻结来源；网络不可用时会只确认本地快照，不会伪报上游已重新验证。
python3 -B kaifa/kaifa_test/verify_work_review_baseline.py verify \
  --source '参考/Work-Review-main' \
  --destination desktop
```

- 基线记录在 `desktop/docs/baselines/work-review-inheritance-matrix.md` 与 `work-review-regression-baseline.json`；自动化证据只提交脱敏摘要和 SHA-256，严禁放入截图正文、活动/OCR 文本、凭据、Cookie、模型 payload 或用户绝对路径。
- 状态语义固定为：`pass` 为已有可复核证据的真实通过；`conditional-pass` 只表示 schema/mock/错误路径通过；`fail`、`blocked`、`not-run` 均关闭相应发布门禁。后续只追加 `after` 证据，不能改写 `before`。
- `verify_work_review_baseline.py` 现在将本步骤明确列出的矩阵、结果 JSON 与五份摘要视为可审计的 `docs/baselines/` 元数据；仍拒绝该目录中的任何其他文件或目录。
