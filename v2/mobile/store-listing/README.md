# Google Play listing assets

Run `npm run store-assets` from `v2/mobile/`. This reuses the real Svelte screens and the existing
demo fixture layer, producing six 1236×2196 portrait PNGs (approximately 9:16), a 512×512 app icon,
and a 1024×500 feature graphic. No new image model, external logos or device frames are used.
`listing.json` contains the English listing copy, screenshot order and alt text.

Screenshots are **simulated data rendered by the app's actual UI in Chromium**, not photographs
or evidence from a phone or TV. The fixture is gated behind `VITE_DEMO=1`; never set that flag
for a shipping build. These assets do not establish hardware compatibility or billing acceptance.
The feature graphic uses the app's existing SVG icon and bundled Geist font.

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
