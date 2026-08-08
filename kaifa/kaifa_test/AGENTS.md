# Acceptance gate instructions

- Validators must be deterministic, read-only against product source/evidence, standard-library-only where currently established, and offline by default.
- Preserve the three outcomes: `0 = pass`, `1 = fail`, `2 = blocked`. Missing, unapproved, or non-production evidence is `blocked`, not a synthetic pass.
- Synthetic fixtures must use fictional values and stay explicitly labeled. They can test gate behavior but cannot satisfy a formal release requirement.
- Validate every evidence path against the declared project/test root. Reject traversal, symlinks, duplicates, missing files, batch mismatches, and hash mismatches.
- When a fixture payload changes, recompute all intentionally bound digests and update both positive and negative cases without weakening the assertion.
- Do not access networks, a real WeChat installation, user archives, credentials, or external signing services from unit fixtures.
- Run the changed verifier against pass, fail, and blocked cases, plus its `unittest` coverage when present.
