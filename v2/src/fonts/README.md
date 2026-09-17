# Bundled fonts

Geist and Geist Mono come from the npm `geist` package and are distributed under the SIL Open Font
License 1.1. They are bundled rather than fetched: this is a desktop app with no network guarantee,
and a webfont that fails to load is a silently different-looking app.

Bundling them also removes a real source of drift. The screenshot gallery is regenerated on Linux
during a release but usually by hand on macOS, and the previous system font stack resolved to
Roboto on one and -apple-system on the other. The same files now render the same way everywhere.

These are the same files as `v2/mobile/src/fonts/`, kept in both places so neither app depends on
the other's tree. If you update one, update both.

A consolidated in-app third-party notices surface is still required before release.
