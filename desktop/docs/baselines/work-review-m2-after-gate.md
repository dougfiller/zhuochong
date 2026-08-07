# M2 strict after-gate evidence

This step-26 after-gate is deliberately **blocked**. The machine-readable
document is `work-review-m2-after-gate.json`; it has no default-pass, waiver,
or authorized-default field.

## What the current implementation can prove

- The Rust M2 boundary can emit metadata-only retrieval, loopback embedding,
  and physical model-attempt events under one opaque request ID.
- A physical model-attempt event is durably appended before the exact frozen
  request reaches the provider transport. Audit failure is fail-closed and is
  not retried as a provider timeout.
- The schema has fixed capability keys and no field for chat/OCR/hit/reply
  text, credentials, provider bodies, endpoint strings, paths, or internal
  source identifiers.
- The strict verifier requires all fault and AC rows and rejects omitted
  counters, unknown evidence, non-zero forbidden capabilities, mismatched
  retry envelopes, and any default-pass field.

These are code and synthetic-test facts only. They are not Windows, real
model, package, performance, or licensing evidence.

## Current blockers

| Evidence category | Current state |
| --- | --- |
| Frozen Windows 11 host and WeChat profile | not-run |
| Real reply-model and local embedding batch | not-run |
| Candidate commit/source tree/NSIS/batch linkage | incomplete |
| Full M2 fault and AC evidence matrix | not-run |
| Package/update privacy scan | not-run |
| Windows pet/resource sentinel observation | not-run |
| Per-asset commercial authorization | blocked by existing `pending-verification` rows |

The JSON therefore keeps every required fault and AC row visible as
`not-run`, sets `verdict=blocked`, and lists explicit blockers. Step 27 must
not treat this document as a release pass.
