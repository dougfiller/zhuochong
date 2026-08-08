# Learning example instructions

- This directory contains isolated Python teaching prototypes, not production application code.
- Keep its third-party dependencies in `requirements.txt`; do not make the Tauri application or release depend on these examples.
- Examples may use mock behavior when `OPENAI_API_KEY` is unset. Never hard-code or print real keys, private prompts, or user records.
- Keep examples small, readable, and runnable independently from `desktop/`.
- If an idea graduates into the product, implement and test it in the appropriate Rust/Svelte module rather than importing the prototype at runtime.
