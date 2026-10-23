# Bundled fonts

Geist and Geist Mono come from the npm `geist` package and are distributed under the SIL Open Font
License 1.1. They are bundled rather than fetched: this is a desktop app with no network guarantee,
and a webfont that fails to load is a silently different-looking app.

Bundling them also removes a source of drift. The screenshot gallery is regenerated on Linux
during a release but usually by hand on macOS, and a system font stack would resolve to Roboto on
one and -apple-system on the other. Bundled files render the same way everywhere.

These are the same files as `v2/mobile/src/fonts/`, kept in both places so neither app depends on
the other's tree. If you update one, update both.

The desktop app has no in-app third-party notices screen yet (mobile shows its notices on
the More screen); adding one is open work.
