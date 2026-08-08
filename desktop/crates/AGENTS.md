# Rust workspace crate instructions

- `core` owns reusable domain, configuration, policy, privacy, analysis, and database logic. Keep it independent of Tauri UI/runtime types.
- `mcp-server` is a stdio adapter over approved core capabilities. Its `WORK_REVIEW_DB_PATH` and `WORK_REVIEW_CONFIG_PATH` overrides must fail safely and must never expose secrets or raw private content in protocol errors or logs.
- `skills-engine` may depend on `core`; avoid reverse dependencies or coupling core types to a single adapter.
- Put shared behavior in the lowest appropriate crate instead of copying it into `src-tauri` and an adapter.
- Treat public types, serialized fields, database contracts, and error variants as cross-crate APIs. Search all workspace consumers before changing them.
- Dependency changes require the relevant `Cargo.toml` and root `Cargo.lock` update together.
- Run the changed crate's tests plus `cargo check --workspace --all-targets`; use workspace clippy/tests for public API or database changes.
