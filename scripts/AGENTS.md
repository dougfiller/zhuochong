# Root script instructions

- These scripts are repository orchestration/probe infrastructure, not product runtime entry points.
- `loof_sequence_probe.py` currently depends on an external macOS path and strict receipt parsing. Do not present it as portable on Windows without replacing that dependency and adding tests.
- Preserve read-only probing and fail-closed parsing. Ambiguous, duplicated, or incomplete receipts must not be interpreted as success.
- Do not let orchestration scripts mutate product evidence, private archives, reference projects, or release state as a side effect of inspection.
- Use only Python standard-library dependencies unless a new dependency is explicitly declared and justified.
