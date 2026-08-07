# M2 observability-gate fixtures

All IDs, hashes, hosts, profiles and evidence values in this directory are
fabricated. The fixtures contain no chat text, screenshot, credential, endpoint,
local path, package, or user database content.

- `pass.json` exercises a complete strictly linked batch, including a successful
  request and a retrieval failure with explicit zero logical/physical model calls.
- `blocked-missing-evidence.json` proves absence stays blocked.
- `fail-capability.json` proves any forbidden capability is a failure.
- `fail-default-pass.json` proves policy bypass fields are rejected.
- `fail-hash-mismatch.json` proves an attempt must match its retrieval permit.
- `blocked-missing-ac.json` proves the required AC set is verifier-owned.
