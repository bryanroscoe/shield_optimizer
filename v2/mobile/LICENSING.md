# ATV Optimizer licensing

ATV Optimizer is freemium: Free unlocks the read-only and low-risk tools, Pro
unlocks curated debloat, the optimize wizard, snapshots, launcher takeover,
tweaks writes, and the rest of the `Feature` list in
`crates/core/src/license.rs`.

Activation is **fully offline**. There is no license server, no phone-home, and
no account. A license key *is* the proof: it carries its own contents and an
Ed25519 signature over them, and the app verifies it against public keys
compiled into the binary.

## Key format

```
ATVOPT-<payload>-<signature>
```

Both halves are [Crockford base32](https://www.crockford.com/base32.html):
case-insensitive, and `I`/`l` decode as `1` while `O`/`o` decodes as `0`, so a
key read off a screen or typed on a TV keyboard can't be rejected for a
lookalike character. Whitespace anywhere is ignored. A typical key is ~150
characters — 64 of those bytes are the signature, which is the floor for
Ed25519.

The payload is compact binary, big-endian:

| offset | size | field |
|--------|------|-------|
| 0 | 1 | format version (currently 1) |
| 1 | 1 | plan (1 = Pro) |
| 2 | 1 | signing key id |
| 3 | 2 | issued, days since 1970-01-01 |
| 5 | 2 | expires, days since 1970-01-01 (0 = perpetual) |
| 7 | 1 | licensee byte length (1–40) |
| 8 | n | licensee, UTF-8 |

The signature covers those exact bytes. Verification rejects, with a
user-facing message each: malformed, unknown signing key, bad signature,
unsupported version, unknown plan, expired. `expires` is the last valid day,
inclusive, judged against the device's UTC date.

Nothing in the key is secret — the licensee string is visible to anyone who
decodes it, so put an email or a name in it, never anything sensitive.

## Where the signing keys live

**Private signing keys are never in this repo.** They live outside it, mode
`600`:

| key id | private key | used for |
|--------|-------------|----------|
| 0 | `~/.atvopt/license-signing-key.dev` | dev/test licenses; trusted by **debug builds only** |
| 1 | `~/.atvopt/license-signing-key.prod` | licenses sold to customers |

The public halves are compiled into `PUBLIC_KEYS` in
`crates/core/src/license.rs`. Key id 0 is behind `#[cfg(debug_assertions)]`, so
a dev-signed license does nothing in a shipped build.

`~/.atvopt/license-signing-key.prod` is the whole business. Back it up
somewhere durable and private (password manager / encrypted offline copy).
Losing it means every future license needs a new key id and an app update;
leaking it means anyone can mint Pro.

## The CLI

`tools/atvopt-license` links the app's own verification code, so what it says a
key means is exactly what the app will say.

```sh
# Generate a signing key pair. Writes the private key mode 600 and prints the
# public key plus the Rust literal to paste into PUBLIC_KEYS.
cargo run -p atvopt-license -- keygen --out ~/.atvopt/license-signing-key.prod

# Sell a perpetual Pro license.
cargo run -p atvopt-license -- issue \
  --key ~/.atvopt/license-signing-key.prod \
  --licensee "buyer@example.com"

# Or a one-year term (last valid day, inclusive).
cargo run -p atvopt-license -- issue \
  --key ~/.atvopt/license-signing-key.prod \
  --licensee "buyer@example.com" \
  --expires 2027-12-31

# Check a key exactly as the app would.
cargo run -p atvopt-license -- verify ATVOPT-…
```

`issue` defaults to `--key-id 1` (production) and to today's date for
`--issued`. Use `--key-id 0` with the dev key when you need a key for a debug
build.

## Debug builds

Debug builds additionally accept the documented test key `ATVOPT-PRO-2025`,
which reports as licensee "Debug build test key", perpetual, key id 0. Release
builds reject it. This is what lets the activation flow and every Pro gate be
exercised without issuing a real license.

## Key rotation

`PUBLIC_KEYS` is a list, and each license names the key that signed it, so
rotation never breaks keys already in customers' hands:

1. `keygen --out ~/.atvopt/license-signing-key.prod2`.
2. Add the printed public key to `PUBLIC_KEYS` under the next unused id
   (`(2, …)`), **leaving the old entry in place**.
3. Ship a build with both keys.
4. Start issuing with `--key-id 2`.

Only remove an old entry when a key is compromised and you have decided to
invalidate everything it signed — that is a customer-visible break, and the
affected buyers need re-issued keys (see below) before the build ships.

## Recovery, transfer, and refunds

Recommended policy, and what the format is built for:

- **One key per purchase.** The key is not device-locked and there is no
  activation count: a buyer can use it on their phone and their spare. That is
  deliberate — hardware locking needs a server, and the buyer who reinstalls at
  midnight is far more common than the buyer who shares a key.
- **Lost key → re-issue on request.** Confirm the purchase, then re-issue with
  the same licensee string. Nothing about issuing is stateful, so a re-issue is
  free and identical in effect. Keep a record (order id → licensee string) so
  you can answer "is this key ours?" without the buyer's key.
- **Prefer perpetual for one-off sales.** Only set `--expires` for a genuine
  subscription or a time-boxed evaluation. An expiry that arrives while the app
  is offline still deactivates Pro, and support cost lands on you.
- **Transfers** are a re-issue: mint a new key with the new licensee. The old
  key keeps working (there is no revocation without a server), so only transfer
  when you trust that the original is out of use.
- **Refunds** are the same caveat: you cannot revoke a key offline. Price and
  refund policy should assume that.
- **Evaluations**: issue a short `--expires` term rather than handing out the
  dev key, which does nothing in a release build anyway.

## Where activation happens

- Verification: `crates/core/src/license.rs` — `parse_license` (contents or a
  user-facing rejection reason) and `validate_license_key` (bool, kept for the
  existing call sites).
- The verified license is recorded in `AppState::set_license_info` and read back
  by the `license_info` command (`crates/core/src/commands/license.rs`).
- Persistence and the `activate_license` command live in
  `mobile/src-tauri/src/lib.rs`, which stores the key in
  `app_data_dir()/license.json` and re-verifies it on every launch — so an
  expired term deactivates on the next start, with no server involved.
- The UI is the Licensing card in `mobile/src/screens/More.svelte`.
