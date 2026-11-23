# Google Play listing assets

Run `npm run store-assets` from `v2/mobile/` (offline, about a minute). `listing.json` drives
everything: the listing text, the screenshot order, each screenshot's caption and alt text, and the
feature graphic's tagline. The command writes:

| File | Size | What it is |
| --- | --- | --- |
| `screenshots/01-….png` … `08-….png` | 1242×2208 (9:16), 24-bit PNG, no alpha | A caption over the real app screen, shown as a rounded card that runs off the bottom edge |
| `feature-graphic.png` | 1024×500, 24-bit PNG, no alpha | App name, small icon mark and tagline beside two real app screens |
| `icon.png` | 512×512 RGBA, at most 1024 KB | Copied from `src-tauri/icons/icon.png` |
| `preview.png` | review sheet | Feature graphic, title, short description, the eight screenshots and the first paragraphs of the full description, laid out roughly like the Play page. For review only; don't upload it |

Each screenshot entry in `listing.json` is
`{ "file", "screen", "headline", "subline"?, "accent"?, "alt" }`. `screen` names an entry in
`../screenshots/screens.mjs` (the same list `npm run screenshots` uses, plus store-only variants
such as `diagnostics-playback` and `remote-text`). `accent` is an optional word or phrase from the
headline drawn in the lime accent color. Headlines wrap to at most two lines; `subline` is
optional.

Screens are **simulated data rendered by the app's actual UI in Chromium**, not photographs or
evidence from a phone or TV. The fixture is gated behind `VITE_DEMO=1`; never set that flag for a
shipping build. These assets do not establish hardware compatibility or billing acceptance. The
captured UI has no Android status bar, and none is faked. No image model, device frames or
third-party logos are used; type is the bundled Geist font.

The command fails before writing if a rule breaks:

- Captions: Google asks that taglines take no more than **20% of the image**. The caption block
  (headline, subline and padding) is 440 px of the 2208 px canvas; the script measures it and
  fails if it grows past 20%, if a headline wraps to three lines, or if the text reaches the card.
- Wording: no "Best", "#1", "Top", "New", "Free", "Discount", "Sale", "Million Downloads" or calls
  to action such as "Install now" in the title, short description, captions or tagline, and no
  "Pro" or prices in captions or the tagline.
- Limits: title ≤ 30 characters, short description ≤ 80 (letters, digits, spaces and `, . ' & -`
  only), full description ≤ 4000, alt text ≤ 140, 2-8 screenshots.
- Images: exact pixel sizes, no alpha in screenshots or the feature graphic, screenshots ≤ 8 MB
  with the long side at most 2.3× the short side, icon ≤ 1024 KB.

`tests/store-listing.test.mjs` (part of `npm test`) runs the same checks on the committed files.

Before uploading:

- Confirm the final signed artifact matches these screens and the feature claims in `listing.json`.
- Set the support email and check the publicly served privacy-policy URL. The Worker returns 503
  until its `PRIVACY_CONTACT_EMAIL` is configured; do not submit an unavailable policy.
- Confirm actual listing behavior against the release's ABI and device evidence. Do not advertise
  Google TV code pairing as verified until the physical pairing matrix has passed.
- Keep the Console's $4.99 US one-time Pro product price separate from listing metadata; localized
  prices come from Play. No purchase or product activation is performed by the asset command.

The mobile release track is separate from desktop `v2-*` releases. See `../RELEASE.md` and
`../PLAY-CONSOLE-WALKTHROUGH.md` for artifact and Console gates.
