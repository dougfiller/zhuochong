# Tauri backend instructions

- Keep commands thin and place domain behavior in the existing modules. Register every new command in the appropriate module and in `main.rs`'s `tauri::generate_handler!` list.
- Use explicit `cfg` guards for Windows, macOS, and Linux APIs. Do not weaken a supported platform merely to satisfy another platform's compiler.
- Maintain local-first, least-privilege behavior. Secrets and private content must not appear in logs, URLs, traces, fixtures, or error strings.
- The WeChat flow is a closed capability domain: explicit user trigger, window validation, local capture/OCR, required retrieval, one configured reply model, bounded suggestion, and user-initiated copy only.
- Never add automatic message watching, UI Automation input, mouse/keyboard control, paste, send, provider fallback, or access from the WeChat flow to MCP, bots, localhost API, upload, search, or general agent tools.
- Under M2, every model-bound request must first pass `knowledge_retrieve()`. `KB_NOT_READY`, `KB_SCOPE_UNRESOLVED`, and `KB_RETRIEVAL_FAILED` fail closed and must never fall back to M1.
- Original chat exports stay read-only. `src/knowledge/store.rs` is the sole owner of `knowledge.sqlite` connections and knowledge SQL; other modules use its typed APIs.
- Keep schema migrations append-only and transactional. Update migration, rollback/recovery behavior, fixtures, and validation tests together.
- Preserve `wechat-m1`, `wechat-m2`, and `wechat-contract-check` feature isolation. Default builds do not prove an M2 feature build works.
- Run `cargo fmt --all -- --check` and targeted tests; use workspace check, clippy, and tests for shared or release-facing changes.
