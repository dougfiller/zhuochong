# M1 after-gate evidence matrix

This is an **after** evidence document for step 14. It references, but never
edits, the frozen before baseline
`work-review-v1.1.0-before-wechat-rag-20260805` and its source manifest hash
`31dd2192f602ee0b4d6f659311186d2230416e42357744ac8c57e778f20cb14a`.

## Candidate and current verdict

| Item | Current sanitized evidence | Status |
| --- | --- | --- |
| Candidate commit / NSIS SHA-256 / batch | Not supplied | blocked |
| macOS static and fake-test results | Earlier step evidence is not a same-candidate Windows batch | blocked |
| Windows 11 x64, frozen profile, four-path UAT | Not supplied | blocked |
| Capability and sentinel counters | Not supplied for a candidate batch | blocked |
| Asset and reference ledger review | `third-party-assets.md` still has `pending-verification` entries | blocked |
| M2-only rows | Not enabled; retained explicitly in JSON | conditional-not-enabled |

The machine-readable document is
[`work-review-m1-after-gate.json`](work-review-m1-after-gate.json). Its current
verdict is **blocked**, not pass: this host must not infer Windows, NSIS,
license, or same-batch evidence from existing macOS checks.

## Stable before-to-after rows

| IDs | Before reference | After method/evidence | After status |
| --- | --- | --- | --- |
| `BASE-01`–`BASE-10` | Same stable `BASE-*` IDs in frozen baseline | Requires the same candidate's Windows lifecycle, privacy, foreground, export and isolation observations | blocked |
| `BASE-AUTO-FE`, `BASE-AUTO-BUILD`, `BASE-AUTO-RUST-CHECK`, `BASE-AUTO-RUST-CLIPPY` | Matching frozen command rows | Requires fresh command, exit code and SHA-256 summary linked to the candidate | blocked |
| `BASE-AUTO-RUST-TEST` | Frozen `UPSTREAM-RUST-001` attribution | Requires exact known-issue comparison; changed or additional failures are fail | blocked |
| `AC-WX-01`–`AC-WX-06` | Step-4 applicability matrix | Requires fake-spy batch plus controlled Windows capture/OCR/profile/copy evidence | blocked |
| `AC-PET-01`, `AC-PET-02` | Step-4 applicability matrix | Requires the same candidate's pet display and no-unintended-processing observations | blocked |
| `AC-KB-05`, `AC-RAG-01` | Step-4 applicability matrix | M2-only; recorded as `conditional-not-enabled`, never counted as M1 success | conditional-not-enabled |

## Acceptance rule

`verify_m1_release_gate.py` accepts a pass only when all required rows have
same-batch candidate commit and NSIS SHA-256 links, four Windows scenarios pass
with focus/overlay restoration, capability and sentinel counters are zero where
forbidden, and the ledger review passes. A precise
`UPSTREAM-RUST-001` may be recorded as a known upstream failure but remains a
release blocker; new or changed failures are `fail`.

## Current blocking inputs

1. A controlled Windows 11 x64 machine, approved WeChat version/profile, and
   a same-batch NSIS candidate SHA-256 have not been supplied.
2. The required Windows success, capture-failed, timeout, and cancel observations
   have not been recorded without personal chat content.
3. `desktop/docs/baselines/third-party-assets.md` contains unresolved
   `pending-verification` asset entries.

No frozen before artifact has been changed to conceal these blockers.
