# End-to-end harness

The real app, minus the TV. Playwright drives the real Svelte UI; every
`invoke()` goes to a dev-only server that runs the **real Rust command layer**
(the same `invoke_handler` table the desktop app registers) against
`SimulatedAdb`, a stateful model of the adb server and the devices behind it.
Only the adb driver is swapped. Each scenario screenshots every step and then
asserts on both the UI and the simulated device's state.

Demo mode (`VITE_DEMO=1`, `src/lib/demo-mock.ts`) is still the right tool for
the marketing screenshots. It answers commands in TypeScript, so it cannot
catch a backend bug. This harness can.

## Run it

```sh
cd v2
npx playwright install chromium   # once
npm run e2e                       # every scenario
npm run e2e -- launcher snapshot  # scenarios whose name contains a filter
```

`npm run e2e` builds `e2e_server` (`cargo build --features e2e --bin e2e_server`),
starts it on a free port, starts Vite with `VITE_E2E=1` on another free port,
and runs `e2e/scenarios/*.mjs` in headless Chromium. Set `E2E_SKIP_BUILD=1` to
reuse an existing build, and `CARGO_TARGET_DIR` to use a non-default target directory.

**Screenshots** land in `e2e/artifacts/<scenario>/NN-step.png`, which is gitignored.
A failed scenario also gets `NN-failed.png` and `failure.json`, which holds the
error, console errors, the full simulated state and every invoke. The
`visual-smoke-dark` and `visual-smoke-light` scenarios visit every tab in both
themes, so a reviewer can look over the whole UI from one folder.

Run it before every release. CI runs it on Ubuntu (the `e2e` job in
`.github/workflows/v2-tests.yml`) and uploads the screenshots when it fails.

## Pieces

| Piece | Where |
|---|---|
| `SimulatedAdb` (devices, transports, mDNS, pairing, packages, settings, Home, shell interpreter, faults, replay) | `crates/core/src/adb/sim/`, behind the core `test-support` feature, never in a release build |
| Device profiles (real, scrubbed, read-only captures) | `crates/core/tests/fixtures/devices/<name>/` |
| E2E server | `src-tauri/src/bin/e2e_server.rs`, behind the `e2e` feature |
| Frontend bridge | `src/lib/e2e-bridge.ts`, installed from `src/routes/+layout.ts` when `VITE_E2E=1` |
| Runner, harness, scenarios | `e2e/run.mjs`, `e2e/lib/`, `e2e/scenarios/` |

The bridge forwards app commands verbatim. Plugin calls (opener, dialog,
updater, process, events) and `navigator.clipboard` get recording stubs instead,
because they would otherwise reach the host desktop. A scenario reads them
from `window.__E2E__.calls` or `.clipboard`, and can preset
`window.__E2E__.dialogResult` for a file picker. Three commands are answered by the server
instead of dispatched, because they would reach the internet or the desktop:
`check_for_update`, `install_adb` and `open_log_dir`.

The subnet sweep in Scan LAN is answered from the simulated network
(`SWEEP_OVERRIDE` in `src-tauri/src/adb/scan.rs`, which compiles only with `e2e`), on
`192.0.2.0/24`.

## Writing a scenario

A scenario file exports `scenarios = [{ name, run(ctx) }]`. `ctx` gives you:

- `page` (Playwright), `step(label)` (full-page screenshot), `open(path)`, `openDevice(key, tab)`, `tab(id)`
- `reset(world)` starts a fresh simulated world (see below), `state()` returns it, `device(hwSerial)` returns one device's
  packages (`enabled` / `disabled` / `uninstalled`), settings, Home (`resolved`, `handlers`, …), input log
- `fault(rule)` / `clearFaults()`, `shell(hwSerial, cmd)` (setup, unlogged, fault-free), `deviceControl(hwSerial, patch)`,
  `attach(key, hwSerial, state)` / `detach(key)`
- `log(since)` returns every adb call the app made with its answer, `invokes()` returns every command the frontend sent
- `invoke(cmd, args)` calls the real command directly, for asserting on a backend guard
- `assert` (node:assert/strict), `waitFor(fn)`

A world is a list of devices built from profiles plus overrides:

```js
await ctx.reset({
  devices: [{
    profile: "shield-tv-pro",          // a directory under tests/fixtures/devices
    serial: "0323716101827",            // replace ro.serialno (a second box)
    ip: "192.0.2.71",
    authorized: false,                  // never approved this computer
    leanback: null,                     // pm has-feature gives no answer
    props: { "ro.build.characteristics": "nosdcard" },
    transports: [],                     // not attached (default: as captured)
    legacy_port: null,                  // no network debugging on :5555
    wireless: { connect_port: 37011, connect_instance: "adb-SERIAL-CoNNeC",
                pairing: ["adb-SERIAL-PaIrNg", 41999, "246810"] },  // open pairing dialog
    paired: true, auto_attach: true, reattach_after_polls: 2,     // adb re-attaches after a disconnect
    home_policy: "stock_overrides",     // an enabled stock launcher wins every preference
    setup: ["pm enable com.google.android.tvlauncher"],
  }],
  faults: [ /* see below */ ],
});
```

Worlds used by several scenarios live in `e2e/lib/worlds.mjs`.

## Injecting faults

A rule matches one *simple* device command by substring, so a fault inside a
batched read fires on that section alone. With `scope: "adb"` it matches the
joined host-side arguments instead (`connect …`, `pair …`):

```js
{ matches: "settings put secure screensaver_enabled",      // refuse a setter
  effect: { type: "fail", stderr: "java.lang.SecurityException: …", exit_code: 255 } }
{ matches: "set-home-activity", effect: { type: "ignore", stdout: "Success\n" } }  // accept but ignore
{ matches: "resolve-activity", effect: { type: "output", stdout: "…\ncom.google.android.tvlauncher/.MainActivity\n" } }
{ matches: "pm has-feature", effect: { type: "output", stdout: "false\n", exit_code: 1 } }
{ matches: "dumpsys meminfo", effect: { type: "timeout" } }  // the whole adb call times out
{ matches: "resolve-activity", effect: { type: "delay", ms: 1500 } }  // a slow resolver
{ scope: "adb", matches: "connect 192.0.2.50", effect: { type: "fail", stdout: "failed to connect …", exit_code: 1 }, times: 1 }
```

`serial` limits a rule to one hardware serial, `times` caps how often it fires,
and `after` lets the first N matches through. `state().faults[i].fired` says
whether a rule was ever hit.

Behaviours that need state rather than one answer are device settings instead
of rules: an unauthorized device is `authorized: false`. A device that comes back after a
disconnect is `auto_attach` plus `paired`. Different pairing and connect suffixes
come from the `wireless` block. Setup Wraith holding Home in passing is
`deviceControl(serial, { transient_home: { holder: "com.google.android.tungsten.setupwraith/.ui.MainActivity", polls: 3 } })`.

## Gaps are loud

A command the simulator has no model for fails with
`simulated device has no handler for: <command>` (exit 127), and it is recorded in
`state().gaps`. The runner prints any gaps after each scenario, and the
visual smoke pass fails on them. To close a gap, add the command to
`crates/core/src/adb/sim/device.rs` (device side) or `world.rs` (`adb …` host side).

## Adding a device profile

Profiles are captured from a real device with **read-only** commands only:

```sh
e2e/capture-device.sh <adb-transport> <profile-name> --leanback true|false|unknown [--notes TEXT]
#   --anonymize-third-party [--keep-package PKG]   for a personal phone
```

It uses the bundled adb at `~/Library/Application Support/ShieldOptimizer/platform-tools/adb`
(override with `ADB=`), and refuses to touch a transport that is not already
`device`, so it never connects anything. It runs only `getprop`, `pm list packages` in every
variant, `settings list`, `dumpsys meminfo`/`diskstats`/`package <home app>`,
`cmd package query-activities`/`resolve-activity`, `wm size`/`density`,
`top -b -n 1`, `cmd role get-role-holders`, `adb devices -l` and `adb mdns services`.

Everything is scrubbed before it is written: IPs are mapped into
`192.0.2.0/24`, MACs into `02:00:00:00:00:xx`, SSIDs become `TestSSID`, emails become `user@example.com`,
user-set device names become `Test <model>`, and IDs and tokens are redacted. Serials and models are kept. The
script then fails if any private IP, MAC or email is left. Add more strings to strip with
`CAPTURE_EXTRA_REDACT` (one per line). Read the output before committing it.

Anything the capture may not run (`dumpsys display`, `/proc`, `pm has-feature`)
is synthesized by `crates/core/src/adb/sim/profile.rs` from generic templates.

Captured so far: `shield-tv-pro` (NVIDIA SHIELD TV Pro, Android 11, network
debugging) and `pixel-10-pro` (Pixel 10 Pro over Wireless debugging; third-party
packages renamed `com.example.appN`).

## Recorded sessions

With **Debug logging** on, the desktop app records every adb call it makes as one JSON line,
together with UI breadcrumbs (routes, tabs and button labels, never anything typed) and info summaries for
pair, connect, forget, scan and launcher switches. Debug logging is on by default in dev builds (`tauri dev`) and
can be toggled in Report a bug. The files are written to:

| OS | Session files |
|---|---|
| macOS | `~/Library/Application Support/ShieldOptimizer/logs/sessions/` |
| Windows | `%LOCALAPPDATA%\ShieldOptimizer\logs\sessions\` |
| Linux | `~/.local/share/ShieldOptimizer/logs/sessions/` |

Each run gets one file, `<startup UTC time>-<pid>.jsonl`. A file stops at 20 MB, and
the newest 10 files are kept. The pairing PIN, typed Remote text and
expert-shell commands and output are redacted. Package lists are kept in full,
which is why these files **never leave the machine**: nothing uploads them,
and the Report-a-bug bundle only reads the log files directly in `logs/`,
never `logs/sessions/`.

Replay one:

```sh
npm run e2e:replay -- ~/Library/Application\ Support/ShieldOptimizer/logs/sessions/2026-09-30T14-03-22.123Z-4242.jsonl
```

The server answers every adb call with what the device said at the time, in order. The
breadcrumbs are driven through the UI where they map to an action (route →
navigate, tab → click the tab, click → click the button with that label), with a
screenshot per step in `e2e/artifacts/replay-<name>/`. At the end it prints every
**divergence**: a command the app sent that the recording never saw
(`unrecorded`), one it saw in a different order (`out_of_order`), or recorded calls
the app never made (`not_replayed`). They are also saved to `divergences.json`, so a
behaviour change between versions shows up as a diff. A command that was never
recorded falls back to a profile built from the session. Pass `--no-profile` to
turn that off. Pollers such as `adb devices` are matched by arguments only.

Turn a session into a profile:

```sh
"$CARGO_TARGET_DIR"/debug/e2e_server profile-from-session <session.jsonl> crates/core/tests/fixtures/devices/<name> --name <name>
e2e/capture-device.sh --scrub-only crates/core/tests/fixtures/devices/<name>
```

The same read-only and scrubbing rules apply as for a capture. Read the result before
committing it.
