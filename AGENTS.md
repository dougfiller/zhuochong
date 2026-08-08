# Repository instructions

## Scope and precedence

- These rules apply to the whole repository. A nearer `AGENTS.md` adds or overrides rules for its subtree.
- The runnable product lives in `desktop/`. Treat the repository root as the coordination, requirements, evidence, and reference layer.
- `kaifa/最终需求文档.md` is the authoritative product and safety baseline. Do not weaken it to make an implementation or test pass.

## Non-negotiable product boundaries

- Preserve the inherited Work Review features while extending the same Tauri application; do not introduce a second desktop shell or runtime.
- The WeChat path may capture only after an explicit user action, perform local OCR and retrieval, and return a suggestion for review and copy. Never add automatic monitoring, input, paste, send, mouse/keyboard control, or a fallback that bypasses required M2 retrieval.
- Keep original chat exports read-only. Never commit user chats, screenshots, OCR text, databases, tokens, API keys, local paths containing private data, or generated release credentials.
- Keep WeChat retrieval/model code isolated from MCP, bots, localhost APIs, upload, web search, and general action tools.
- Preserve `desktop/LICENSE`, upstream attribution, and `desktop/THIRD_PARTY_NOTICES.md`. Reference code or assets are not automatically licensed for redistribution.

## Repository workflow

- Run Node and Cargo commands from `desktop/` unless a command explicitly uses `--manifest-path` or `--project-root`.
- Keep dependency declarations and lockfiles paired: `package.json` with `package-lock.json`, and Cargo manifests with `Cargo.lock`.
- Do not edit generated or local-only outputs such as `node_modules/`, `.venv/`, `dist/`, `target/`, `desktop/src-tauri/gen/`, databases, or screenshots.
- Make the smallest change that satisfies the requirement. Add or update adjacent tests for behavior changes.
- Treat `pass`, `fail`, and `blocked` as different release-gate outcomes. Missing real evidence must remain `blocked`; synthetic fixtures can never stand in for production evidence.

## Validation

- Frontend or shared UI: `node --test` and `npm run build` from `desktop/`.
- Rust: `cargo fmt --all -- --check`, a relevant targeted test, and when practical `cargo check --workspace --all-targets` from `desktop/`.
- Cross-boundary or release work: run the applicable scripts under `kaifa/kaifa_test/` and preserve their strict evidence semantics.
- Documentation-only changes should at least verify links, paths, commands, and that no secret or machine-specific value was introduced.
