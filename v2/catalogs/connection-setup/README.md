# Connection setup by brand

`connection-setup.json` is the one source for the brand-specific "how to turn
on debugging" help on desktop (Devices › Pair PIN › Help with pairing, and
Connection help) and Android (Find your TV › How to turn on debugging). Both
apps read it through `v2/shared/connection-setup.ts`.

Rules for editing it:

- State only a path a source gives, and list that source in the variant's
  `sources` (a path in CleanYourTV at the pinned commit, or another MIT source
  with its own notice). Where no source gives the unlock step, leave `unlock`
  null and the generic steps show instead. Don't guess.
- `connect` is `port` (plain ADB on 5555: Add by IP / Connect (no code)),
  `pair` (Wireless debugging pairing code) or `either`.
- Set `generic_unlock: false` only where the generic Android TV steps would be
  wrong (Fire OS).

`tests/connection-setup.mjs` (desktop) and `mobile/tests/connection-setup.test.mjs`
check the schema, that every variant has a source, and the help in both apps.
