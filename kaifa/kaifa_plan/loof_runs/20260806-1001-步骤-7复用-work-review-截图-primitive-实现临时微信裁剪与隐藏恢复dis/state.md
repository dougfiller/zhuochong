# LOOF Run 20260806-1001-步骤-7复用-work-review-截图-primitive-实现临时微信裁剪与隐藏恢复dis

- project_root: `/Users/sky/aich8-zhuochong`
- feature: 步骤 7：复用 Work Review 截图 primitive 实现临时微信裁剪与隐藏恢复（dispatch_id=aich8-zhuochong-desktop-pet-rag-27-steps-20260805-task-07）。复用现有 GDI BitBlt/WGC 像素采集为无落盘 ephemeral 微信帧，新增 CaptureCoordinator 与 WechatCaptureGuard；以 capture origin、DPI 和窗口相对 ROI 正确裁剪 chat/header，所有成功/失败/超时/取消路径恢复覆盖层且不抢焦点。严格保持前台单一微信、无注入/协议/数据库读取/UI 自动化/键鼠模拟/自动发送等产品边界；普通 Work Review 截图行为不变。真实 Windows 成功、失败、超时、取消验证只能如实标注，不能伪造通过。
- current_phase: 5
- current_phase_name: 推送同步
- current_thread_id: 019fd4f3-ecbe-7533-99c9-b7367891b026
- next_poll_at: 
- status: done

## Artifacts
- phase_1: 2026年08月06日10点02分-复用Work-Review截图primitive实现临时微信裁剪与隐藏恢复.md
- phase_2: 2026年08月06日10点07分-临时微信截图裁剪与隐藏恢复.md
- phase_3: 2026年08月06日10点12分-截图恢复验收缺口.md
- phase_4: 
- phase_5: 

## Threads
- phase_1: 019fd4cd-f65f-7f30-b256-aed646716547
- phase_2: 019fd4d0-ceeb-7f40-be95-4772b207431f
- phase_3: 019fd4d6-bda5-7e10-aaab-2336f131d5de
- phase_4: 019fd4d9-ef8e-7031-b2a2-406ce938af70
- phase_5: 019fd4f3-ecbe-7533-99c9-b7367891b026

## Notes
- 2026-08-06T02:07:31+00:00 phase 2 gate failed: done file is missing
- 2026-08-06T02:17:31+00:00 phase 4 gate failed: done file is missing
- 2026-08-06T02:21:01+00:00 phase 4 gate failed: done status is not done: blocked
- 2026-08-06T02:24:04+00:00 phase 4 gate failed: done status is not done: blocked
- 2026-08-06T02:27:07+00:00 phase 4 gate failed: done status is not done: blocked
- 2026-08-06T02:33:40+00:00 phase 4 gate failed: done status is not done: blocked
