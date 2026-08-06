# M1 release-gate fixtures

All fixture values are fabricated identifiers and hashes. They contain no chat,
profile, screenshot, credential, executable, or local-path data.

- `pass.json` exercises a fully linked evidence batch.
- `fail-capability.json` proves a forbidden capability is a failure, even when
  other required Windows evidence is absent.
- `blocked-missing-evidence.json` proves missing candidate, Windows, and ledger
  evidence is never inferred as a pass.
- `blocked-hash-mismatch.json` proves a mismatched candidate hash is blocked.
