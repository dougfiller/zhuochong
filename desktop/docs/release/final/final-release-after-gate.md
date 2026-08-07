# M2 正式发布门禁

当前结论：`blocked`。

本文件只描述 task 27 的正式状态，不代表 Windows、性能、业务 UAT、签名、安装、updater、素材授权或回滚已经通过。当前 `release-freeze-v1.json` 的外部输入尚未获批，同批真实证据也不存在，因此发布工作流与 updater 必须继续保持禁用。

允许继续的工作只有：在用户确认的 Windows 测试根中收集同批 metadata-only 证据、运行严格 gate、修复明确失败项并重新生成不可变候选。只有 gate exit 0、全部 required section 为 `pass`、blockers 为空且发行负责人签字后，才允许发布。
