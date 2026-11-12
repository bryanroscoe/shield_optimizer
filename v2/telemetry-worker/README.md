# ATV Optimizer telemetry Worker

A Cloudflare Worker with one D1 database. It does two jobs:

1. Takes anonymous telemetry from the desktop and Android apps (`POST /v1/events`), so we can see
   which TVs people actually use and where actions fail. This is how the supported-device list
   gets built.
2. Serves the support-tier list (`GET /v1/support-tiers`), so a TV can move from beta to supported
   without an app release.
3. Takes the logs a desktop user chooses to send (`POST /v1/diagnostics`), and lets the owner find
   everything stored for one install by the support ID the user quotes
   (`scripts/support-lookup.sh`).

Telemetry is on by default in the apps with an opt-out. The app side lives in the desktop and mobile
code; this directory is only the backend. It runs on the Cloudflare free plan.

## What is collected

The Android privacy policy is served at `GET /privacy` (also `HEAD`). Set the public
`PRIVACY_CONTACT_EMAIL` Worker variable to the owner's approved support/privacy email before
publication. The endpoint returns 503 with no caching until a valid email is configured; it
never publishes a placeholder or reads request location. The mobile app links this page in
More → Privacy and exposes the current full installation Support ID for deletion requests.
The policy describes Android only: desktop's opt-in Send logs upload is a separate data flow.

Each event is one of `device_seen`, `action_result`, `error` or (schema 2) `diagnostic`, with:

| Field | Example | Notes |
| --- | --- | --- |
| `install_id` | random UUID v4 | Made on the client, not derived from anything. A new one is made whenever the user turns telemetry back on, so old and new data are not linked. |
| `app`, `app_version`, `os` | `desktop`, `2.3.1`, `macos 15.1` | `os` is the computer or phone running our app, not the TV. |
| `ts` | RFC 3339 | Client clock. The server also records when it received the event. |
| `device` | `NVIDIA` / `SHIELD Android TV` / `mdarcy`, Android `11`, SDK `30`, profile, tier | The TV's public build properties: manufacturer, brand, model, device codename, product, Android release, SDK level. No serial. |
| `action`, `ok`, `error_code` | `disable`, `false`, `pm_denied` | Which kind of action ran and whether it worked. Not which app it ran on. |
| `message` | `pm disable failed on [ip]` | Short error text, redacted on the client and again here. 500 characters at most. |
| `diag` | `{"sweep":{"probed":254,"open":0,...},"mdns":{"check":"ok","services":0,...}}` | Schema 2. What the app saw when a scan or TV found nothing: counts, durations, fixed enums, version numbers and error codes. Rebuilt key by key from `DIAG_SPEC` in `src/diag.ts`; anything else is dropped. No free text. |
| `log_excerpt` | `2026-11-02T14:00:02Z INFO ...::devices: connect finished address=<redacted> ...` | Schema 2, desktop only. Up to 60 lines / 8 KiB of the app's adb and connection log, redacted on the client and again here. Never command output (`stdout=`/`stderr=` values), TV names or Private DNS hosts. Cleared after 30 days. |

`diagnostic` events carry the reason as `error_code`: `scan_empty`, `scan_connect_failed`,
`discover_empty` or `device_details_empty`. Schema 1 events are still accepted unchanged; a schema
1 event cannot be a `diagnostic` and any `diag` or `log_excerpt` on it is ignored.

### Send logs uploads

A test build (a native test package marked with `ATVOPT_TEST_BUILD`) can send logs too. Its upload
carries `app_version` as `<version>-dev`, which the version check here already accepts, and its text
starts with `Test build <commit>`: the commit that was built (a PR's merge commit), followed by
`(merge commit; PR head <head>)` when the PR head differs. Test builds never send automatic reports, so they add nothing to
`events` or the device-support aggregates.

`diag_uploads` holds what a desktop user sent with **Send logs to developer**: a summary (support
ID, app and OS, TVs by state and public build properties, `adb devices -l` and a `diag` object)
and up to 256 KiB of the app's own log, redacted line by line on the client
(`engine/log_redact.rs`), with a second pass here that drops command output (`stdout=`/`stderr=`
values and their continuation lines) and the value of any `device_name` or
`private_dns_specifier`, and removes addresses, MACs, emails, adb mDNS names, `*.local` names,
digit- or hyphen-labelled hosts and licence keys. That pass works token by token with bounded
patterns, skips tokens that can't match and drops any token over 512 characters whole, so a
256 KiB upload costs a few milliseconds of CPU whatever it contains (tested against pathological
input). It is keyed by the install id, sent only when the user presses
the button (even with automatic reports off), and deleted after 30 days.

## What is never collected or stored

- TV serial numbers, `ro.serialno`, adb transport ids or adb mDNS service names (they embed the serial).
- IP addresses: neither the TV's nor the client's. The Worker never reads the request IP, has
  [Workers Logs](https://developers.cloudflare.com/workers/observability/logs/workers-logs/) turned
  off in `wrangler.toml`, and contains no `console.log`.
- Location of any kind. The Worker never reads `request.cf`, so Cloudflare's country, region,
  city, coordinates, colo, ASN and timezone for the request are not looked at, let alone stored.
  Until 2026-11-01 it kept the two-letter country per raw event; migration
  `0002_drop_country.sql` deleted those values and dropped the column (owner decision, so the
  apps' Play Data safety form has no location to declare).
- TV names or labels, the user's package list, file paths, emails, licence or licensee data.
- Logs from the phone app. Its Play Data safety form doesn't declare them, so `log_excerpt` from
  `app: "mobile"` is dropped and a mobile `POST /v1/diagnostics` is refused.

How that is enforced server-side, so a client bug cannot break it:

- Every event is rebuilt field by field from the contract. Any field the contract does not name
  (a `serial`, an `ip`, a `packages` array, a `tv_name`...) is dropped before anything is written.
- `kind`, `app`, `support_tier` and `schema` (1 or 2) must be known values or the event is rejected. An
  unknown `profile` is stored as `unknown` and an unknown `action` as `other`, so a newer app
  does not lose data against an older Worker.
- `install_id` must be a UUID v4. `app_version` must be `X.Y.Z`, optionally with an
  `-alpha|beta|rc|preview|pre|dev[.N]` tag. `ts` must be RFC 3339 with an offset. `error_code`
  must be `short_snake_case` (64 chars max); any such code is accepted, there is no fixed list
  (`panic`, `uncaught_error`, `pm_denied`...). A well-formed code with more than four digits or a
  license word is stored as `redacted_code` instead, since codes are kept forever in the
  aggregates. Codes should be constants such as `pm_denied` or
  `adb_exit_255`; a name folded into a code cannot be detected, so that stays the client's job.
- Fields that do not belong to the event's kind are dropped (`action`/`ok`/`error_code` on
  `device_seen`, `error_code` on a successful action, `ok` on an error).
- Short fields (manufacturer, model, os...) are clamped to 64 characters (`os` to 40). If one looks
  like an IP, MAC, host:port, email, path, URL or (except `model`) a serial number, the whole
  value becomes `[redacted]` (the serial shapes are the same ones used for messages, below).
  A short field holding an `ATVOPT-` license key is always replaced. `model` skips the serial
  check because real model numbers such as `55UQ7590PUB` have the same shape and the device
  report needs them.
- `message` is redacted again:
  - IPv4 and IPv6 (with ports), `host:port`, MAC addresses, emails, URLs, `*.local` names and
    adb mDNS names.
  - Windows, UNC, POSIX, home and relative paths; quoted ones are replaced whole, spaces included.
  - Serial-looking tokens: 10+ letters and digits with at least 4 digits; 8+ upper-case letters
    and digits with at least one of each (`R5CRABCDEFZ`); 10+ in any case (`r5crabcdefz`); and
    hyphen/underscore-joined ones with a run of 4+ digits (`emulator-5554`).
  - One package name is kept because it is useful for debugging a failed disable; three or more
    (including two-part ids like `tv.baz`) are treated as a package list and all are replaced.
  - If any `/` or `\` is left after all that, the message may still hold part of a path the
    patterns missed (an unquoted relative path with spaces, say), so the whole message becomes
    `[message removed: contained a path]`. A short field containing either is `[redacted]`.
  - A message that contains an ATV Optimizer license key (`ATVOPT-...`) or mentions a license or
    licensee is replaced whole with `[license-related message removed]`; `error_code` still says
    what failed.
  - These are heuristics. A serial made only of letters cannot be told from a word, and the
    server cannot recognise a TV's friendly name inside free text, so the client's own redaction
    stays the first line.

## Retention

- Raw events (`events` table) are deleted 30 days after they were received, by a daily Cron
  Trigger at 03:17 UTC. Change `RAW_RETENTION_DAYS` in `wrangler.toml` to adjust (until this is
  deployed production still keeps 90 days; see [Production](#production)). It governs `events` rows and nothing else: the aggregate tables below, and
  so the device report and its distinct-install counts, are not shortened by it.
- Log text is kept 30 days (`LOG_RETENTION_DAYS`): the same cron deletes older `diag_uploads`
  rows and clears `log_excerpt` on older events. With both settings at 30 the excerpt goes with
  its event; if raw retention is ever set longer, the event and its `diag` stay without it.
- Aggregates are kept: per-day counts per device/app version (`agg_device_daily`), per-day error
  code counts (`agg_error_daily`) and the install ids seen per device group
  (`agg_device_installs`, used to count distinct installs). These hold no free text.
- Rate-limit counters are deleted after two days.
- If stored data ever nears the database's size limit, the same cron frees space early, oldest
  first: Send logs uploads, then log excerpts, then raw events, and only then the oldest aggregate
  days. See [Storage budget](#storage-budget).
- To honour a deletion request, the user can read their support ID from the app (look it up with
  `scripts/support-lookup.sh` to get the full id); delete it with
  `DELETE FROM events WHERE install_id = ?` and the same on `diag_uploads`,
  `agg_device_installs` and `rate_daily` (`key = <install id>` and `key = 'upload:' || <install id>`).
  The per-day counts cannot be attributed to an install.

## API

### `POST /v1/events`

Body `{ "events": [Event, ...] }`, `Content-Type: application/json`, at most 50 events and 64 KB.
The event shape is the schema 1 contract the apps implement.

| Status | Meaning |
| --- | --- |
| `204` | Stored. `X-Events-Accepted` / `X-Events-Rejected` say how many events in the batch passed validation; invalid events in an otherwise good batch are dropped, not stored. |
| `400` | Bad JSON, wrong envelope, more than 50 events, or no valid event at all (`{"error":"no_valid_events","rejected":{"unknown_kind":1}}`). |
| `413` | Body over 64 KB. |
| `415` | Not `application/json`. |
| `429` | Daily cap reached (`install_daily_cap`, or `global_daily_cap` for the global event count or stored-bytes budget). `Retry-After` is the seconds to the next UTC midnight. Clients should drop or hold the batch, not retry in a loop. |
| `503` | D1 unavailable (`storage_unavailable`). If the batch had been admitted, its per-install quota is given back; the global count is not, so a storage outage cannot turn into unbounded writes. |

Clients should treat anything other than `204` as "drop this batch". Telemetry must never block or
slow the app.

### `POST /v1/diagnostics`

Desktop Send logs. Body `{ "schema": 1, "install_id", "app": "desktop", "app_version", "os", "ts",
"reports_enabled", "text" }`, `Content-Type: application/json`, `text` at most 300 KiB (body
616 KiB, room for JSON escaping). `install_id`, `app_version`, `os` and `ts` follow the event rules.

| Status | Meaning |
| --- | --- |
| `201` | Stored: `{ "ok": true, "id": 12, "bytes": 123456 }`. |
| `400` | Bad JSON or shape, `unsupported_app` (anything but `desktop`), `bad_text`, `text_too_large`. |
| `413` / `415` | Body too large / not `application/json`. |
| `429` | `upload_daily_cap`: `PER_INSTALL_DAILY_UPLOADS` (5) for this install, checked first so a capped install never spends the shared budget. `upload_global_cap`: `GLOBAL_DAILY_UPLOADS` (200) or `GLOBAL_DAILY_UPLOAD_BYTES` (6 MiB) across everyone. Per UTC day; `Retry-After` as for events. |
| `503` | D1 unavailable. |

`OPTIONS` returns `204` but never sends `Access-Control-Allow-Origin`. Both apps post from native
code (Rust on desktop, the native layer on Android), where CORS does not apply; keeping it closed
means a web page cannot make a visitor's browser post here.

### `GET /v1/support-tiers`

Public, `Cache-Control: public, max-age=3600`.

```json
{
  "schema": 1,
  "updated": "2026-10-31T00:00:00Z",
  "rules": [
    { "match": { "manufacturer": "NVIDIA", "model_prefix": "SHIELD" }, "tier": "fully_supported", "note": "..." },
    { "match": { "manufacturer": "Amazon", "model_prefix": "AFT" }, "tier": "not_supported", "note": "..." },
    { "match": { "manufacturer": "TCL" }, "tier": "beta", "note": "..." }
  ]
}
```

Matching: every key present in `match` must match the TV (case-insensitive; `model_prefix` is a
prefix test on `ro.product.model`, the others are exact). Rules are checked in order and the first
match wins. No match means `unknown`. `matchTier` in `src/tiers.ts` is the reference.

**The tier list is informational only.** The apps use it to tell the user how well their TV is
supported. It never relaxes a safety gate: the do-not-disable list, profile detection and every
other check in the apps behave the same whatever tier the server returns, and a TV the server calls
`fully_supported` gets no extra permissions. Anyone who can change this list can change a label,
nothing more.

The list's source of truth is the copy the apps embed, `v2/crates/core/data/device-support.json`.
The [Support tiers sync](../../.github/workflows/support-tiers-sync.yml) workflow `PUT`s it here on
every merge to `main` that changes it and fails unless the served rules then equal the file's; the
seed in `migrations/0001_init.sql` is only the starting state of a fresh database. Don't change the
live list any other way. The apps ignore a served document whose `updated` is older than their
embedded copy; `updated` is an RFC 3339 timestamp that every `PUT` moves strictly forward. The
process for changing a tier is [`docs/DEVICE-SUPPORT.md`](../../docs/DEVICE-SUPPORT.md). SHIELD strings come from the scrubbed capture in
`v2/crates/core/tests/fixtures/devices/shield-tv-pro/getprop.txt`. The repo has no TCL getprop
capture; the TCL rule matches manufacturer `TCL` as reported for the TCL QM7L Pro (Android 14) in
issue #88. Tighten it once real TCL rows show up in the device report.

### Admin

All need `Authorization: Bearer <ADMIN_TOKEN>`. With no token set (or one shorter than 16
characters) the admin routes answer `503`.

- `GET /v1/admin/support/<support-id>[?limit=N]`: everything stored for one install. The id is the
  full UUID or the short form the app shows (`7D0B-0B9E`, dash optional, any case), which matches
  every install whose id starts with those eight hex digits (up to 5 are returned, newest first;
  `matches` and `truncated` say how many there were). Each install has first/last seen, event
  counts by kind, apps, versions and OSes, its TVs, its top error codes, the `limit` most recent
  events (default 50, max 200) with `diag` parsed and `log_excerpt`, and its uploads.
- `GET /v1/admin/uploads/<id>`: one Send logs upload as `text/plain`, with `x-install-id`.

From a terminal, `scripts/support-lookup.sh` does both and prints a readable report:

```sh
export ATVOPT_ADMIN_TOKEN="$(op read 'op://Bryan Code/ATV Optimizer telemetry ADMIN_TOKEN/credential' --account my.1password.com)"
scripts/support-lookup.sh 7D0B-0B9E           # or the full UUID; --json for the raw response
scripts/support-lookup.sh --upload 12         # the text of upload #12 from that report
```

The token reaches `curl` as a header on stdin, so it is not in the process list, and the script
never prints it. `ATV_TELEMETRY_URL` overrides the production URL.


- `GET /v1/admin/devices[?days=N][&limit=N][&group=manufacturer,model,device,profile,app,app_version]`:
  one row per group with distinct `installs`, `events`, `seen` (device_seen count), `action_ok`,
  `action_fail`, `errors`, `top_errors` (top 5 codes from errors and failed actions) and
  `last_seen`. `group` defaults to all six columns; pick fewer to roll up (for example
  `group=manufacturer,model` to merge app versions without double counting installs). `days`
  (default 90, max 3650) is the window in UTC days and `limit` (default 500, max 5000) keeps the
  busiest groups by event count; `truncated: true` means the limit was reached. The defaults keep the
  response bounded even if a hostile client sends junk device names. For all-time numbers use
  `sql/device-report.sql`.
- `PUT /v1/admin/support-tiers` with `{ "rules": [ { "match": {...}, "tier": "...", "note": "..." } ] }`:
  replaces the whole list atomically and bumps `updated`. Each rule needs at least one non-blank
  match key and a note (200 chars max); up to 200 rules. Other top-level keys are ignored, so the
  embedded file itself is a valid body. Only the sync workflow should call this.

## Production

Live since 2026-11-01 on the owner's Cloudflare account (free plan). The deploy was reported by
[PR #269](https://github.com/bryanroscoe/shield_optimizer/pull/269); that does not by itself show
that a real app build has sent events.

Diagnostics deployment verified 2026-11-03: migration `0003_diagnostics.sql` is applied and
Worker version `7e0652bf-703c-4896-9a51-7c524943ff4d` serves the code from main
`f52a73e00822e246f9a69d2e73ba9115f12ee035`. A synthetic desktop upload returned `201`,
was retrievable through both admin support routes, and had its test address redacted.
The support-tier endpoint remained available. This checks production storage and retrieval;
it does not substitute for a native app or physical TV test.

| | |
| --- | --- |
| URL | https://atv-optimizer-telemetry.bryanroscoe.workers.dev |
| Worker | `atv-optimizer-telemetry` |
| D1 | `telemetry`, id `e86ca0a5-fc1d-4692-9602-01407920ce61` (the committed binding in `wrangler.toml`) |
| Cron | `17 3 * * *` (03:17 UTC daily): purge raw events, log excerpts and uploads past 30 days, and old rate counters; then enforce the [storage budget](#storage-budget) with what the purge left of the run's row-write allowance |
| Repo variable | `ATVOPT_TELEMETRY_URL` = the URL above. Release builds embed it, and so do the PR's native test packages (`v2-tests.yml`), which also set `ATVOPT_TEST_BUILD` to the merge commit they build and the PR head; the sync and report workflows call it. |
| Repo secret | `TELEMETRY_ADMIN_TOKEN` = the Worker's `ADMIN_TOKEN`. Used by the sync and report workflows. |

Secrets live in the owner's personal 1Password (`my.1password.com`, vault **Bryan Code**), never
in the repo, a shell history or a log:

- **ATV Optimizer telemetry ADMIN_TOKEN** (field `credential`): the admin bearer token. The same
  value is the Worker secret `ADMIN_TOKEN` and the repo secret `TELEMETRY_ADMIN_TOKEN`.
- **cloudflare -admin api token** (field `credential`): a Cloudflare API token for `wrangler`, used
  as `CLOUDFLARE_API_TOKEN` instead of `wrangler login`.

Read them with `op read` into an environment variable or a pipe. Never echo them.

What runs against production on its own:

- [Support tiers sync](../../.github/workflows/support-tiers-sync.yml): pushes
  `device-support.json` on every merge to `main` that changes it, then verifies the served list.
  This is the only way the live tier list changes; don't `PUT` it by hand. Re-run it from the
  Actions tab if the live list is ever in doubt.
- [Device support report](../../.github/workflows/device-support-report.yml): Mondays, rewrites the
  "Device support report" issue. See [`docs/DEVICE-SUPPORT.md`](../../docs/DEVICE-SUPPORT.md).

Both skip with a notice if the secret or variable is missing.

Storage budget deployed 2026-11-12: Worker version `c2a36241-c0a0-41bf-91f2-66a9ae1a25d4` serves
the code from main `76b2e029d95197a7594e51b6223b6872f3d0b5d9`, with the storage budget
(`GLOBAL_DAILY_EVENT_BYTES`, `STORAGE_BUDGET_BYTES`, `STORAGE_TRIM_MAX_ROW_WRITES`, the lower
upload caps and the cron's oldest-first trim), the 30-day raw retention and the `400` for a
malformed support id. No migration was needed. After the deploy `/privacy` showed 30 days,
`/v1/support-tiers` answered `200`, and events kept arriving (26 in the 20 minutes before the
check). The database was 25 MB.

Before changing `GLOBAL_DAILY_EVENT_BYTES` (10 MiB/day), check that it clears real traffic with
room to spare; events over it are refused with `429 global_daily_cap` for the rest
of the UTC day. This read-only query (`sql/event-volume.sql`) lists the last 14 days' event counts,
their average stored size and each day's total as the cap counts it (string bytes plus 256 per
row):

```sh
npx wrangler d1 execute telemetry --remote --file sql/event-volume.sql
```

Size the cap from the busiest day's `day_bytes`: keep the cap at least three times that, and
check that `GLOBAL_DAILY_EVENTS` × `avg_bytes` is not far below it, or the count cap binds first.
After raising either, redo the "Storage budget" arithmetic below: raw events are kept 30 days, so
each extra MiB/day is about 31 MB at the worst case.

The check was run read-only against production on 2026-11-11. Since desktop 2.5.0 shipped:
1,900-2,250 events a day from about 130-170 desktop installs, `avg_bytes` 1,250-1,600 and
`day_bytes` 2.7-3.5 MB, the busiest 3,487,846 on 2026-11-10. About 70% of the bytes are
`log_excerpt` (2.2-2.8 KB on errors, failed actions and diagnostics). The 1.5 MiB cap first
merged would have refused more than half of each day's events, so the cap is now 10 MiB
(three times the busiest day), and raw retention went from 90 to 30 days to keep the worst case
inside the Free plan. 7,000 events at that average is 8.3-10.7 MiB, so the two caps bind at about
the same point (see [Free-tier limits](#free-tier-limits-and-abuse-protection)). Rerun the check
before changing either cap.

The first cron after the deploy deletes every raw event older than 30 days in one statement, at
three row writes each. The production database was created on 2026-11-01, so a deploy before
2026-12-01 has nothing extra to delete. A later one should first count
`SELECT COUNT(*) FROM events WHERE received_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-30 days')`:
over about 10,000 rows, that day's purge alone takes a large share of the 100,000 daily writes.

### Redeploy

Production deploys and remote migrations need the owner's approval; the commands below are not
that approval. A routine redeploy **keeps the committed database binding and the existing admin
secret**: don't create another database, paste a replacement id, or generate or reset the admin
token as part of it. If the binding or access looks wrong, stop and reconcile it with the owner
instead of provisioning replacements.

```sh
cd v2/telemetry-worker
npm ci
npm test
npm run typecheck
export CLOUDFLARE_API_TOKEN="$(op read 'op://Bryan Code/cloudflare -admin api token/credential' --account my.1password.com)"
```

Before the remote steps, check the selected account (`npx wrangler whoami`) and the committed
`[[d1_databases]]` binding, and review any new `migrations/000N_*.sql`. Apply only the intended
pending migrations to the existing database, before the code that needs them, then deploy:

```sh
npx wrangler d1 migrations apply telemetry --remote   # only when migrations/ changed
npx wrangler deploy
curl -fsS https://atv-optimizer-telemetry.bryanroscoe.workers.dev/v1/support-tiers
rm -rf node_modules .wrangler
```

If `wrangler` asks which account to use, set `CLOUDFLARE_ACCOUNT_ID` from the Cloudflare
dashboard. The `curl` is a read-only smoke check: it doesn't validate event ingestion, admin
access or a real app's release configuration, so check those separately, and never send
fabricated production telemetry just to test a deploy. Admin access is proven by a green
**Support tiers sync** run.

### Rotating ADMIN_TOKEN

An owner-authorized operation, separate from deploys. Three copies must change together, or the
workflows start failing with `401`:

```sh
# 1. New value, straight into 1Password (edit the item's credential field).
op item edit "ATV Optimizer telemetry ADMIN_TOKEN" --vault "Bryan Code" --account my.1password.com \
  "credential=$(openssl rand -base64 32)" > /dev/null
# 2. The Worker secret.
cd v2/telemetry-worker
export CLOUDFLARE_API_TOKEN="$(op read 'op://Bryan Code/cloudflare -admin api token/credential' --account my.1password.com)"
op read "op://Bryan Code/ATV Optimizer telemetry ADMIN_TOKEN/credential" --account my.1password.com \
  | npx wrangler secret put ADMIN_TOKEN
# 3. The repo secret.
op read "op://Bryan Code/ATV Optimizer telemetry ADMIN_TOKEN/credential" --account my.1password.com \
  | gh secret set TELEMETRY_ADMIN_TOKEN -R bryanroscoe/shield_optimizer
```

Then run **Support tiers sync** from the Actions tab: a green run proves the new token works end to
end. The apps never see this token; rotating it changes nothing for users.

### Schema changes

Later schema changes go in a new `migrations/000N_*.sql`, reviewed and applied with the same
`migrations apply --remote` command before deploying the code that needs them. A migration that
removes something the live Worker still writes (as `0002_drop_country.sql` did) is the other way
round: deploy the Worker that no longer writes it first, then apply the migration, or ingestion
fails with `503` in between. Never create a replacement database to apply a schema change.

## First-time setup for a fork

Not the production procedure above. Use the fork owner's own account and explicitly approved
resource and credential setup; the committed database id belongs to upstream production and must
not be reused.

```sh
cd v2/telemetry-worker
npm ci
npx wrangler login                       # only when the owner authorizes sign-in
npx wrangler d1 create telemetry         # a new, independent database
# Set the fork's own database_id in wrangler.toml -> [[d1_databases]].
# Verify the intended account, Worker name and database before any remote change.
npx wrangler d1 migrations apply telemetry --remote
npx wrangler deploy
```

Provision `ADMIN_TOKEN` through the owner's secret manager and `wrangler secret put ADMIN_TOKEN`;
never commit or print it. `wrangler deploy` prints the URL; apps post to `<url>/v1/events` and
read `<url>/v1/support-tiers`. Set it as the repo variable `ATVOPT_TELEMETRY_URL` and the token as
the repo secret `TELEMETRY_ADMIN_TOKEN`, then run **Support tiers sync** once so the live list
matches the repo.

## The device report

The weekly issue is the usual way to read it (see [`docs/DEVICE-SUPPORT.md`](../../docs/DEVICE-SUPPORT.md)).
From the admin endpoint, as a table:

```sh
export ATV_TELEMETRY_URL=https://atv-optimizer-telemetry.bryanroscoe.workers.dev
export ATV_TELEMETRY_ADMIN_TOKEN="$(op read 'op://Bryan Code/ATV Optimizer telemetry ADMIN_TOKEN/credential' --account my.1password.com)"
npm run report -- --days 90
npm run report -- --issue      # the weekly issue body, with current and suggested tiers
```

Or straight from D1 without the token (uses your `wrangler login`):

```sh
npx wrangler d1 execute telemetry --remote --file sql/device-report.sql
```

Both read the aggregate tables, so they include devices whose raw events have already been purged.

## Free-tier limits and abuse protection

Cloudflare's free plan (check the current numbers on the
[Workers](https://developers.cloudflare.com/workers/platform/limits/) and
[D1](https://developers.cloudflare.com/d1/platform/limits/) limits pages):

- Workers: 100,000 requests a day, 10 ms CPU per request. A 50-event batch validates well inside that.
- D1: 5 million rows read and 100,000 rows written a day, 50 queries per Worker call, and
  **500 MB per database** (5 GB is the total across all of an account's databases, not one
  database's limit). Each request runs at most six queries however many events it carries (all
  writes go through `json_each` over one JSON parameter). Row writes per accepted event are lowest
  in big batches (aggregates are merged per batch) and highest for a one-event error batch: about
  nine (three quota rows, the raw row and its two index entries, three aggregate rows). A Send
  logs upload is about six (three quota rows, the row and its two index entries). A support
  lookup reads through the `events_install` index and runs at most 1 + 7 × 5 queries. Over the
  daily limits, D1 refuses queries until the daily reset; the apps just lose that day's telemetry.
  Over the size limit, every write fails until space is freed, which is what the storage budget
  below prevents.
- Cron Triggers: this Worker uses one.

Abuse protection:

- 64 KB body cap, enforced while reading the stream, and at most 50 events per request.
- Per-install cap: `PER_INSTALL_DAILY_EVENTS` (500) events per install id per UTC day.
- Global caps: `GLOBAL_DAILY_EVENTS` (7,000) events and `GLOBAL_DAILY_EVENT_BYTES` (10 MiB) of
  stored event data per UTC day across everyone (`rate_daily` keys `*` and `event-bytes:*`).
  10 MiB / 7,000 is about 1,500 bytes an event, close to the measured average (1,250-1,600), so
  in normal use the count cap binds first on days of small events and the byte cap on
  error-heavy days; a flood of minimal events (~330 bytes each) meets the count cap at about
  2.2 MiB. The write budget is about nine row writes per event on arrival in the worst case (a
  one-event error batch under a made-up install id) plus about four for the cron's deletes (its
  quota row after two days, the raw row and its two index entries after 30). Under a sustained
  flood that is roughly thirteen writes per event per day: 7,000 events is about 63,000 writes on
  arrival and 28,000 in the cron's retention purge. Uploads add at most about 1,200 on arrival
  (200 a day at about six writes each) and 600 when they expire. The cron run as a whole, purge
  and storage trim together, spends at most `STORAGE_TRIM_MAX_ROW_WRITES` (30,000): the purge
  first, the trim what is left. So the worst case is about 63,000 + 1,200 + 30,000 ≈ 94,000 of the
  100,000 free writes a day. Raise them only together with a paid plan.
- Uploads: `PER_INSTALL_DAILY_UPLOADS` (5), `GLOBAL_DAILY_UPLOADS` (200) and
  `GLOBAL_DAILY_UPLOAD_BYTES` (6 MiB, 24 full-size uploads) per UTC day, counted in
  `rate_daily` under `upload:<install id>`, `upload:*` and `upload-bytes:*`. The install's own cap
  is read first, so one person retrying can't spend everyone's budget.

Both caps are counters in one D1 table, `rate_daily` (a row per install id per day, plus key `*`
for the global total). Admission and increment are a single statement: it checks every counter
the batch would touch against the state before the statement and either bumps them all or none.
D1 runs one write at a time, so concurrent requests cannot both slip under a cap, and a batch
that would cross a cap is refused whole. A refused request writes nothing, so retry loops and
floods of made-up install ids cost row reads (5 million a day) rather than the much smaller
write quota.

### Storage budget

Production is on the Workers Free plan, where one D1 database stops accepting writes at 500 MB.
After that every ingest, upload and support-tier sync answers `503` until rows are deleted. The
caps bound the worst case, a flood of made-up install ids every day, and the storage budget below
holds what they leave over. Each event costs
its stored string bytes plus 256 for the row and its index entries (`eventBytes` in
`src/ingest.ts`):

| Data | Worst case |
| --- | --- |
| Raw events: 10 MiB/day × 30 days (`RAW_RETENTION_DAYS`) | 10,485,760 × 30 ≈ 315 MB |
| Uploads: 6 MiB/day × 30 days (`LOG_RETENTION_DAYS`) | 6,291,456 × 30 ≈ 189 MB |
| Upload rows: 200/day × 30 days × ~400 bytes | ≈ 2.4 MB |
| `rate_daily`: 3 days × ~7,000 keys × ~100 bytes | ≈ 2.1 MB |
| **Total data** | **≈ 508 MB** |

That is far more than the 300 MB storage budget (and more than the database's 500 MB), so under a
sustained worst-case flood the budget, not the caps, sets the size: the trim below keeps the
estimate at or under 300 MB by deleting the oldest Send logs uploads first, then raw events.
SQLite pages are not packed full, so the file runs larger than the data: at a generous 1.4× the
budget is about 420 MB of the 500 MB. Real traffic is far below this: at the measured 3.5 MB a
day, 30 days of raw events is about 105 MB, so the trim does not run.

The upload byte cap is deliberately larger than the events': a few made-up install ids can spend
it and lock "Send logs" out for everyone until the next UTC midnight, so it buys 24 full-size
uploads a day rather than 8, and the budget trim is the backstop for the space.

The aggregate tables are kept forever and are not capped by bytes. Normal use upserts the same few
device rows, but a flood of made-up device names adds up to about three rows per event, so they
are the one thing that can grow without limit. The cron therefore also enforces
`STORAGE_BUDGET_BYTES` (300 MB of estimated data): when the estimate passes it, it frees space
oldest first, towards 80% of the budget, in this order: Send logs uploads, log excerpts, raw
events, then the oldest days of `agg_error_daily`, `agg_device_daily` and `agg_device_installs`.
The support-tier list is never touched.

The trim must not cause the outage it prevents, so its cost is bounded in rows, not statements:

- **Reads.** The estimate reads every stored row once per cron run, about 0.5 to 2 million rows
  in the worst case above, of the 5 million a day. Each tier then reads at most as many of its
  oldest rows as it may delete.
- **Writes.** D1 counts every row and index entry a statement changes. Deleting an upload or a raw
  event costs three row writes (the row and its two index entries), clearing an excerpt or
  deleting an aggregate row one. One cron run spends at most `STORAGE_TRIM_MAX_ROW_WRITES`
  (30,000) on its deletes. The retention purge runs first and its writes count against that; the
  trim gets the rest, in capped oldest-first batches
  (`… WHERE rowid IN (SELECT rowid … ORDER BY received_at LIMIT n)`). That leaves room for the
  ingest caps' ~64,000 inside the 100,000 a day even on a day when the purge and the trim both
  run, which a trim with its own separate allowance would not.
- **Convergence.** A run that reaches the cap stops and the next day's run carries on while the
  estimate is still over the budget, so a large overshoot is worked off over several days. Under a
  worst-case flood the purge's quota-row deletes take about 7,000 writes, leaving the trim about
  23,000: all of a day's uploads (at most 600 writes) and about 7,400 raw events, more than the
  7,000 a day can add. What it cannot also keep up with is a flood of made-up device names, which
  adds up to three aggregate rows per event (about 21,000 rows, roughly 4 MB a day); those are
  freed only after uploads, excerpts and raw events, and the daily write quota leaves no room to
  delete them as fast as such a flood adds them, so one sustained for weeks would eventually
  outgrow the budget. That was already so with the earlier caps, more slowly. Between two runs the caps add at most ~16 MiB of raw data
  plus aggregate rows, inside the 80 MB of headroom.

On a paid plan (10 GB per database) the caps and the budget can be raised together; keep the
budget under the plan's per-database limit divided by about 1.4.

Why D1 and not the alternatives: KV's free tier allows only 1,000 writes a day, far too few to
count requests; the Workers Rate Limiting binding only supports 10- and 60-second windows, counts
per Cloudflare location, and the useful key for it against made-up install ids would be the
client IP, which this Worker does not use. D1 counters are global, consistent, work for daily
windows, and cost one row write per install id plus one for the global row per accepted request,
and none per refused one.

## Development

```sh
npm ci
npm test            # vitest inside workerd via @cloudflare/vitest-plugin, D1 migrations applied
npm run typecheck   # the Worker, then the Node scripts (tsconfig.scripts.json)
npm run dev         # wrangler dev on a local D1; run `npm run migrate:local` first
```

For local admin calls, put `ADMIN_TOKEN="some-local-token-16+chars"` in `.dev.vars` (gitignored).
Local state lives in `.wrangler/` (gitignored).
