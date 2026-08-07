# M2 发布、重建与阶段感知回滚 Runbook

本 runbook 只适用于发行负责人冻结的 Windows 主机、微信 profile、数据集、模型/loopback embedding 和不可变 M2 候选。它不会授权自动操作微信，也不会把当前 blocked 状态改成通过。

## 1. 冻结与候选

1. 发行负责人先完整填写并签字确认 `release-freeze-v1.json`；不得使用空值、临时值或历史默认通过政策。
2. 固定 clean `main` commit/tree 与 lockfile hash。构建必须显式只启用 `custom-protocol,wechat-m2`；默认、M1 或双 feature 包立即丢弃。
3. 在签名候选生成后计算 NSIS、updater archive、`.sig`、可执行文件及依赖清单 SHA-256。后续测试只使用该候选，不得重编。

## 2. Windows 证据采集

1. 用户显式选择独立 evidence root；不得选择用户目录、应用 data root 或原始导出目录。
2. 运行 `collect-final-evidence.ps1`，并在脚本打开的 Windows 原生目录选择器中选择独立证据根。脚本不接受调用者传入证据根路径，只读取明确传入的候选与 metadata JSON，不做键鼠/UIA 输入、粘贴、发送、微信数据库/协议访问或网络上传。
3. 在人工操作下完成八类题集、前台 fail-closed、手动生成/审阅/复制、性能、安装升级卸载、单实例与自启动。真实问题、OCR、命中和回复正文只留在受控私有根，不进入 release manifest。

## 3. rebuild 与 recovery

1. rebuild root 必须来自本次 native picker receipt。私有 rebuild manifest 只留在应用受管目录，绝不复制进 evidence/package/upload。
2. 缺失、损坏或 hash 漂移时停止重建并提示重新选择；禁止扫描常用目录或猜 recent path。
3. pre-upgrade 与 pre-rollback 分别创建独立 recovery bundle。config、`.bak` 和核心 `workreview.db` 必须 hash、SQLite integrity/reopen 验证成功。
4. knowledge 派生集合可按兼容矩阵隔离并重建；用户原始 JSON/JSONL 永不进入隔离、删除或上传集合。

## 4. 严格门禁

```powershell
python -B kaifa/kaifa_test/verify_final_m2_release_gate.py `
  --project-root . `
  --freeze desktop/docs/release/final/release-freeze-v1.json `
  --manifest D:\approved-evidence\final-release-manifest.json
```

退出码固定为 `0=pass`、`1=fail`、`2=blocked`。只有 exit 0、全部 required section 为 pass、blockers 为空且签字存在时才允许 publish。

## 5. 阶段感知回滚

1. 先生成并验证新的 pre-rollback recovery bundle；失败即停止。
2. 从受保护且已签名的 release index 精确选取最近 `m2Contract=forced-rag`、final gate pass、签名和 schema 兼容的 LKG。任何 M1、无 feature、未签名或 index 缺失目标均拒绝。
3. knowledge schema 兼容时复验后复用；不兼容时隔离完整派生集合并用已验证私有 manifest 重建。不能恢复时保持 `KB_NOT_READY`，不得触碰原始导出。
4. 回滚后先验证核心 config/DB/FTS 与普通 Work Review 功能，再验证 M2 同 request 先 retrieval 后 model permit；任何失败停止，不降级 M1。
