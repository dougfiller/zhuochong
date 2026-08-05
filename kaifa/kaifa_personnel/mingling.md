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
