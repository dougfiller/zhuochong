# Svelte frontend instructions

- Use Svelte 4 and plain JavaScript/ES modules already established in this tree. Reuse components, stores, utilities, and `$lib` imports before adding another abstraction.
- Keep route-level UI in `routes/`, reusable UI in `lib/components/`, state in `lib/stores/`, and shared helpers in `lib/utils/`.
- Co-locate behavioral tests as `*.test.js` and use the existing Node test runner; do not introduce a second frontend test framework.
- User-facing text must go through i18n. Update `zh-CN.js`, `zh-TW.js`, `en.js`, and `ar.js` together, including RTL layout and interpolation behavior.
- Preserve keyboard access, focus handling, reduced-motion expectations, responsive layouts, and Arabic RTL behavior.
- Sanitize rendered Markdown/HTML with the existing DOMPurify path. Never inject model, OCR, report, or chat text as unsanitized HTML.
- Keep Tauri command names and payload casing synchronized with Rust commands and `generate_handler!` registration.
- The WeChat UI may request a suggestion and let the user copy or close it. It must not focus WeChat, synthesize input, paste, or send.
- Validate relevant tests with `node --test`, then run `npm run build` from `desktop/`.
