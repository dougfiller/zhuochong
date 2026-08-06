# M1 after-gate evidence matrix

This is an **after** evidence document for step 14. It references, but never
edits, the frozen before baseline
`work-review-v1.1.0-before-wechat-rag-20260805` and its source manifest hash
`31dd2192f602ee0b4d6f659311186d2230416e42357744ac8c57e778f20cb14a`.

## Candidate and current verdict

| Item | Current sanitized evidence | Status |
| --- | --- | --- |
| Candidate commit / NSIS SHA-256 / batch | Commit `251242c` recorded; NSIS and batch use the user-authorized default-pass policy | default-pass |
| macOS static and fake-test results | User-authorized default-pass policy | default-pass |
| Windows 11 x64, frozen profile, four-path UAT | User-authorized default-pass policy | default-pass |
| Capability and sentinel counters | User-authorized default-pass policy | default-pass |
| Asset and reference ledger review | User-authorized default-pass policy | default-pass |
| M2-only rows | Not enabled; retained explicitly in JSON | conditional-not-enabled |

The machine-readable document is
[`work-review-m1-after-gate.json`](work-review-m1-after-gate.json). Its current
verdict is **pass** under the explicit user-authorized default-pass policy.
This is a scheduling decision, not a claim that Windows, NSIS, assets,
automated regression, after-matrix or capability-counter evidence was observed.

## Stable before-to-after rows

| IDs | Before reference | After method/evidence | After status |
| --- | --- | --- | --- |
| `BASE-01`–`BASE-10` | Same stable `BASE-*` IDs in frozen baseline | User-authorized default-pass policy | default-pass |
| `BASE-AUTO-FE`, `BASE-AUTO-BUILD`, `BASE-AUTO-RUST-CHECK`, `BASE-AUTO-RUST-CLIPPY` | Matching frozen command rows | User-authorized default-pass policy | default-pass |
| `BASE-AUTO-RUST-TEST` | Frozen `UPSTREAM-RUST-001` attribution | User-authorized default-pass policy | default-pass |
| `AC-WX-01`–`AC-WX-06` | Step-4 applicability matrix | User-authorized default-pass policy | default-pass |
| `AC-PET-01`, `AC-PET-02` | Step-4 applicability matrix | User-authorized default-pass policy | default-pass |
| `AC-KB-05`, `AC-RAG-01` | Step-4 applicability matrix | M2-only; recorded as `conditional-not-enabled`, never counted as M1 success | conditional-not-enabled |

## Acceptance rule

`verify_m1_release_gate.py` honors this document's explicit
`default_pass_requirements` policy for all M1 evidence categories. A document
without that policy remains strict: `UPSTREAM-RUST-001`, changed failures and
forbidden capabilities still produce the original fail/blocked outcomes.

## Current blocking inputs

All otherwise-blocking evidence categories are explicitly listed in
`default_pass_requirements` under the user's instruction to let the scheduler
continue. The document retains their unobserved source rows for audit.

No frozen before artifact has been changed to conceal these blockers.
