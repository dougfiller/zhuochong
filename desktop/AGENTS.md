# Desktop application instructions

- This directory is the runnable product root: Tauri 2, Rust 2021, Svelte 4, Vite 5, and SQLite.
- Run npm and workspace Cargo commands here. Use `npm ci` for reproducible installs and keep both npm and Cargo lockfiles synchronized with manifest changes.
- Do not edit or commit `node_modules/`, `.venv/`, `dist/`, `target/`, `src-tauri/target/`, `src-tauri/gen/`, local databases, screenshots, or generated packages.
- Keep the inherited Work Review application and the WeChat/knowledge extensions in one process and lifecycle. Do not add a second desktop shell, tray, updater, or sidecar GUI.
- Preserve cross-platform behavior. Guard platform-specific Rust code with explicit `cfg` blocks and test the supported target rather than making another platform silently compile a stub that claims success.
- A frontend `invoke()` contract change must update the Rust command, serialized DTOs, handler registration, UI caller, and relevant tests together.
- Behavior changes require adjacent tests. At minimum run the relevant `node --test` selection or Rust test plus `npm run build`; run workspace checks for shared changes.
- Generated release evidence and product status must stay truthful. A build completing locally is not proof that M2 or a formal release passed.
