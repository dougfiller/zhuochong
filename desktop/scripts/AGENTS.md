# Desktop script instructions

- Keep scripts platform-specific and non-interactive where practical. Validate resolved paths before any install, uninstall, move, or overwrite operation.
- Preserve an explicit local-file path option for installer helpers; do not silently download or execute an unverified artifact.
- `capture-readme-pages.mjs` must restrict browser requests to the configured `README_CAPTURE_BASE_URL` origin and must never capture real user data.
- Generated icons and screenshots must come from licensed source assets. Preserve source and attribution records.
- Release/evidence scripts must fail closed on missing inputs, hash mismatches, unsafe paths, symlinks, or incomplete approvals.
- Add or update the adjacent script test when changing parsing, capture, packaging, or evidence behavior.
