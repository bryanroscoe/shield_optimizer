# ATV Optimizer telemetry Worker

A Cloudflare Worker with one D1 database. It does two jobs:

1. Takes anonymous telemetry from the desktop and Android apps (`POST /v1/events`), so we can see
   which TVs people actually use and where actions fail. This is how the supported-device list
   gets built.
2. Serves the support-tier list (`GET /v1/support-tiers`), so a TV can move from beta to supported
   without an app release.

Telemetry is on by default in the apps with an opt-out. The app side lives in the desktop and mobile
code; this directory is only the backend. It runs on the Cloudflare free plan.

## What is collected

Each event is one of `device_seen`, `action_result` or `error`, with:

| Field | Example | Notes |
| --- | --- | --- |
| `install_id` | random UUID v4 | Made on the client, not derived from anything. A new one is made whenever the user turns telemetry back on, so old and new data are not linked. |
| `app`, `app_version`, `os` | `desktop`, `2.3.1`, `macos 15.1` | `os` is the computer or phone running our app, not the TV. |
| `ts` | RFC 3339 | Client clock. The server also records when it received the event. |
| `device` | `NVIDIA` / `SHIELD Android TV` / `mdarcy`, Android `11`, SDK `30`, profile, tier | The TV's public build properties: manufacturer, brand, model, device codename, product, Android release, SDK level. No serial. |
| `action`, `ok`, `error_code` | `disable`, `false`, `pm_denied` | Which kind of action ran and whether it worked. Not which app it ran on. |
| `message` | `pm disable failed on [ip]` | Short error text, redacted on the client and again here. 500 characters at most. |
| country | `US` | From Cloudflare's `request.cf.country`. Nothing finer: no city, region, coordinates, ASN or timezone. |

## What is never collected or stored

- TV serial numbers, `ro.serialno`, adb transport ids or adb mDNS service names (they embed the serial).
- IP addresses: neither the TV's nor the client's. The Worker never reads the request IP, has
  [Workers Logs](https://developers.cloudflare.com/workers/observability/logs/workers-logs/) turned
  off in `wrangler.toml`, and contains no `console.log`.
- TV names or labels, the user's package list, file paths, emails, licence or licensee data.

How that is enforced server-side, so a client bug cannot break it:

- Every event is rebuilt field by field from the contract. Any field the contract does not name
  (a `serial`, an `ip`, a `packages` array, a `tv_name`...) is dropped before anything is written.
- `kind`, `app`, `support_tier` and `schema` must be known values or the event is rejected. An
  unknown `profile` is stored as `unknown` and an unknown `action` as `other`, so a newer app
  does not lose data against an older Worker.
- `install_id` must be a UUID v4, `app_version` must look like a version, `ts` must be RFC 3339
  with an offset, and `error_code` must be `short_snake_case` (64 chars max) with at most four digits and no
  license words, since codes are kept forever in the aggregates. Codes should be constants such as
  `pm_denied` or `adb_exit_255`; a name folded into a code cannot be detected, so that stays the
  client's job.
- Short fields (manufacturer, model, os...) are clamped to 64 characters (`os` to 40). If one looks
  like an IP, MAC, host:port, email, path, URL or (except `model`) a serial number, the whole
  value becomes `[redacted]` (the serial shapes are the same ones used for messages, below).
  A short field holding an `ATVOPT-` license key is always replaced. `model` skips the serial
  check because real model numbers such as `55UQ7590PUB` have the same shape and the device
  report needs them.
- `message` is redacted again: IPv4 and IPv6 (with ports), `host:port`, MAC addresses, emails,
  URLs, Windows/UNC/POSIX/home paths (quoted ones whole, spaces included), `*.local` names, adb mDNS names and serial-looking tokens
  (10+ letters and digits with at least 4 digits, or 8+ upper-case letters and digits with at
  least one of each, such as `R5CRABCDEFZ`, or 10+ in any case, such as `r5crabcdefz`, or hyphen/underscore-joined
  ones with a run of 4+ digits, such as `emulator-5554`). These are heuristics: a serial made only of letters
  cannot be told from a word, so the client's own redaction stays the first line. One package name is kept because it is useful
  for debugging a failed disable; three or more (including two-part ids like `tv.baz`) are treated
  as a package list and all are replaced.
  A message that contains an ATV Optimizer license key (`ATVOPT-...`) or mentions a license or
  licensee is replaced whole with `[license-related message removed]`; the `error_code` still says
  what failed. The server cannot recognise a TV's friendly name inside free text, so that stays
  the client's job.

## Retention

- Raw events (`events` table) are deleted 90 days after they were received, by a daily Cron
  Trigger at 03:17 UTC. Change `RAW_RETENTION_DAYS` in `wrangler.toml` to adjust.
- Aggregates are kept: per-day counts per device/app version (`agg_device_daily`), per-day error
  code counts (`agg_error_daily`) and the install ids seen per device group
  (`agg_device_installs`, used to count distinct installs). These hold no free text.
- Rate-limit counters are deleted after two days.
- To honour a deletion request, the user can read their install id from the app; delete it with
  `DELETE FROM events WHERE install_id = ?` and the same on `agg_device_installs` and
  `rate_daily` (`key = <install id>`). The per-day counts cannot be attributed to an install.

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
| `429` | Daily cap reached (`install_daily_cap` or `global_daily_cap`). `Retry-After` is the seconds to the next UTC midnight. Clients should drop or hold the batch, not retry in a loop. |
| `503` | D1 unavailable (`storage_unavailable`). If the batch had been admitted, its per-install quota is given back; the global count is not, so a storage outage cannot turn into unbounded writes. |

Clients should treat anything other than `204` as "drop this batch". Telemetry must never block or
slow the app.

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
    { "match": { "manufacturer": "TCL" }, "tier": "beta", "note": "..." },
    { "match": { "manufacturer": "Amazon", "model_prefix": "AFT" }, "tier": "not_supported", "note": "..." }
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

The seed rules: SHIELD strings come from the scrubbed capture in
`v2/crates/core/tests/fixtures/devices/shield-tv-pro/getprop.txt`. The repo has no TCL getprop
capture; the TCL rule matches manufacturer `TCL` as reported for the TCL QM7L Pro (Android 14) in
issue #88. Tighten it once real TCL rows show up in the device report.

### Admin

Both need `Authorization: Bearer <ADMIN_TOKEN>`. With no token set (or one shorter than 16
characters) the admin routes answer `503`.

- `GET /v1/admin/devices[?days=N][&group=manufacturer,model,device,profile,app,app_version]`:
  one row per group with distinct `installs`, `events`, `seen` (device_seen count), `action_ok`,
  `action_fail`, `errors`, `top_errors` (top 5 codes from errors and failed actions) and
  `last_seen`. `group` defaults to all six columns; pick fewer to roll up (for example
  `group=manufacturer,model` to merge app versions without double counting installs). `days`
  limits to the last N UTC days.
- `PUT /v1/admin/support-tiers` with `{ "rules": [ { "match": {...}, "tier": "...", "note": "..." } ] }`:
  replaces the whole list atomically and bumps `updated`. Each rule needs at least one non-blank
  match key and a note (200 chars max); up to 200 rules.

## Deploy (owner)

Nothing here has been deployed. You need a Cloudflare account (free plan is fine) and Node 22.18+.

```sh
cd v2/telemetry-worker
npm ci

npx wrangler login                       # opens a browser
npx wrangler d1 create telemetry         # prints a database_id
# paste that id into wrangler.toml -> [[d1_databases]] database_id

npx wrangler d1 migrations apply telemetry --remote
npx wrangler deploy

openssl rand -base64 32                  # copy this into your password manager first
npx wrangler secret put ADMIN_TOKEN      # paste it at the prompt; Cloudflare never shows it again
```

`wrangler deploy` prints the URL, `https://atv-optimizer-telemetry.<your-subdomain>.workers.dev`.
That base URL is what the apps need: they post to `<url>/v1/events` and read
`<url>/v1/support-tiers`. Check it with:

```sh
curl -s https://atv-optimizer-telemetry.<your-subdomain>.workers.dev/v1/support-tiers
```

Later schema changes go in a new `migrations/000N_*.sql` and are applied with the same
`migrations apply --remote` command before deploying the code that needs them.

To change support tiers:

```sh
curl -X PUT "$URL/v1/admin/support-tiers" \
  -H "authorization: Bearer $ADMIN_TOKEN" -H "content-type: application/json" \
  --data @tiers.json
```

## The device report

From the admin endpoint, as a table:

```sh
ATV_TELEMETRY_URL=https://atv-optimizer-telemetry.<subdomain>.workers.dev \
ATV_TELEMETRY_ADMIN_TOKEN=... \
npm run report -- --days 90
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
- D1: 5 million rows read and 100,000 rows written a day, 5 GB storage, 50 queries per Worker call.
  Each request runs at most six queries however many events it carries (all writes go through
  `json_each` over one JSON parameter). Row writes per accepted event are lowest in big batches
  (aggregates are merged per batch) and highest for a one-event error batch: about seven (two
  quota rows, the raw row and its index, three aggregate rows). Over the limit, D1 refuses queries until the
  daily reset; the apps just lose that day's telemetry.
- Cron Triggers: this Worker uses one.

Abuse protection:

- 64 KB body cap, enforced while reading the stream, and at most 50 events per request.
- Per-install cap: `PER_INSTALL_DAILY_EVENTS` (500) events per install id per UTC day.
- Global cap: `GLOBAL_DAILY_EVENTS` (7,000) events per UTC day across everyone. The write budget
  is about seven row writes per event on arrival in the worst case (a one-event error batch) plus
  about three for the cron's deletes (its quota row after two days, the raw row and its index
  entry after 90).
  Under a sustained flood that is roughly ten writes per event per day, so 7,000 events is about
  70,000 of the 100,000 free writes. Raise it only together with a paid plan.

Both caps are counters in one D1 table, `rate_daily` (a row per install id per day, plus key `*`
for the global total). Admission and increment are a single statement: it checks every counter
the batch would touch against the state before the statement and either bumps them all or none.
D1 runs one write at a time, so concurrent requests cannot both slip under a cap, and a batch
that would cross a cap is refused whole. A refused request writes nothing, so retry loops and
floods of made-up install ids cost row reads (5 million a day) rather than the much smaller
write quota.

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
npm run typecheck
npm run dev         # wrangler dev on a local D1; run `npm run migrate:local` first
```

For local admin calls, put `ADMIN_TOKEN="some-local-token-16+chars"` in `.dev.vars` (gitignored).
Local state lives in `.wrangler/` (gitignored).
