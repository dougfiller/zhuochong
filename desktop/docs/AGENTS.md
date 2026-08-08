# Documentation and evidence instructions

- Keep the English, Simplified Chinese, and Traditional Chinese user READMEs aligned when changing shared product behavior.
- Treat timestamped design records, baselines, release evidence, freeze records, hashes, signatures, approvals, and rollback reports as audit material. Do not rewrite history or hand-edit an outcome to pass.
- Missing real evidence remains `blocked`. Clearly label examples and synthetic fixtures; they are never production proof.
- Update screenshots only from a controlled local build and keep paths, locale, timezone, and capture tooling reproducible. Do not capture real user data.
- Preserve direct links from decisions to requirements, implementation paths, tests, and evidence.
- When user-visible README content changes, run the README-related Node tests and `npm run build` from `desktop/`.
