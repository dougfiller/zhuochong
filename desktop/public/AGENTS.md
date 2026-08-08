# Packaged asset instructions

- Everything here may be shipped with the application. Never place secrets, user data, test evidence, local paths, or unapproved assets in this tree.
- Keep provider icons and third-party assets traceable to a source and compatible license; update `THIRD_PARTY_NOTICES.md` when required.
- Do not assume an image, font, logo, model, animation, or reference asset is redistributable because it is publicly viewable.
- Preserve stable filenames used by code and packaging, or update every caller and visual test in the same change.
- Optimize new assets for package size without silently degrading existing resolution, transparency, or platform icon requirements.
