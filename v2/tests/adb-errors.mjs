// `adb pair` / `adb connect` failures are mapped to guidance a person can act
// on, with adb's own words kept for the Details disclosure. The inputs are the
// strings adb (platform-tools 37) actually prints.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const v2Root = dirname(dirname(fileURLToPath(import.meta.url)));
const source = readFileSync(join(v2Root, "src/lib/adb-errors.ts"), "utf8");
const { explainAdbFailure, subnetWarning, hostOf, localNetworkHint, isNoRoute, LOCAL_NETWORK_BLOCKED, LOCAL_NETWORK_POSSIBLE } = await import(
  "data:text/javascript," + encodeURIComponent(stripTypeScriptTypes(source, { mode: "strip" }))
);

// A mistyped address: 182.168… instead of 192.168…
{
  const raw =
    "adb pair: adb process failed (exit code Some(1)): error: protocol fault (couldn't read status message): Undefined error: 0";
  const r = explainAdbFailure("pair", "182.168.42.211:45439", raw);
  assert.equal(
    r.summary,
    "Couldn't reach 182.168.42.211:45439. Check the IP and pairing port match the pairing dialog, that it's still open, and that both are on the same Wi-Fi.",
  );
  assert.equal(r.raw, "error: protocol fault (couldn't read status message): Undefined error: 0");
}

for (const raw of [
  "failed to connect to 192.168.1.9:40000: Connection refused",
  "error: no route to host",
  "failed to connect to '10.0.0.4:5555': Operation timed out",
  "error: Network is unreachable",
]) {
  assert.match(explainAdbFailure("pair", "192.168.1.9:40000", raw).summary, /^Couldn't reach 192\.168\.1\.9:40000\. .*pairing dialog/, raw);
  assert.match(
    explainAdbFailure("connect", "192.168.1.9:40000", raw).summary,
    /^Couldn't reach 192\.168\.1\.9:40000\. .*main Wireless debugging screen \(not the pairing dialog\)/,
    raw,
  );
}

const shieldFailure = explainAdbFailure("connect", "192.0.2.1:5555", "No route to host");
assert.match(shieldFailure.summary, /On SHIELD, enable Network debugging/);
assert.match(shieldFailure.summary, /same local network/);
assert.equal(shieldFailure.raw, "No route to host");

// Wrong PIN: adb folds it together with a dropped connection, so it must be
// matched before the "dialog gone" rule sees "connection was dropped".
assert.match(
  explainAdbFailure("pair", "192.168.1.9:40000", "Failed: Wrong password or connection was dropped.").summary,
  /^The PIN didn't match/,
);

// The dialog closed or timed out on the device.
for (const raw of ["Failed: Unable to start pairing client.", "error: connection reset by peer"]) {
  assert.match(explainAdbFailure("pair", "192.168.1.9:40000", raw).summary, /^The pairing dialog closed or expired/, raw);
}

// Not recognised: no guess, raw text intact.
{
  const r = explainAdbFailure("pair", "192.168.1.9:40000", "error: something new and strange");
  assert.equal(r.summary, null);
  assert.equal(r.raw, "error: something new and strange");
}
// PIN-ish words only mean "wrong PIN" for pairing.
assert.equal(explainAdbFailure("connect", "1.2.3.4:5", "Failed: Wrong password").summary, null);

// Subnet warning.
assert.equal(
  subnetWarning("182.168.42.211", "192.168.42.17"),
  "This computer is on 192.168.42.x, and 182.168.42.211 isn't on it. Typo?",
);
assert.equal(subnetWarning("192.168.42.211", "192.168.42.17"), null);
assert.equal(subnetWarning("192.168.42.211", null), null, "unknown local address claims nothing");
assert.equal(subnetWarning("[fe80::1]", "192.168.42.17"), null);
assert.equal(subnetWarning("adb-X._adb-tls-pairing._tcp", "192.168.42.17"), null);
assert.equal(subnetWarning("192.168.42", "192.168.42.17"), null);

assert.equal(hostOf("192.168.1.9:40000"), "192.168.1.9");
assert.equal(hostOf("192.168.1.9"), "192.168.1.9");
assert.equal(hostOf("[fe80::1]:40000"), "[fe80::1]");

// The serial a Wireless debugging instance embeds, same rule as the Rust side.
const mdnsSource = readFileSync(join(v2Root, "src/lib/mdns.ts"), "utf8");
const { instanceSerial } = await import(
  "data:text/javascript," + encodeURIComponent(stripTypeScriptTypes(mdnsSource, { mode: "strip" }))
);
assert.equal(instanceSerial("adb-58040DLCH005YV-A1b2C3"), "58040DLCH005YV");
assert.equal(instanceSerial("adb-58040DLCH005YV-jBeCEe"), "58040DLCH005YV");
assert.equal(instanceSerial("adb-AB-12-x9"), "AB-12");
for (const bad of ["adb-unknown-x", "adb--x", "adb-1321920044953", "adb-ABC-", "Living Room", null, ""]) {
  assert.equal(instanceSerial(bad), null, String(bad));
}


// macOS "No route to host". What it points at follows the probe:
// "blocked" states the block; "allowed" means the app itself reaches the
// network, so the likely culprit is an adb server an earlier version left
// running (Restart ADB, no settings); "unknown" offers Restart ADB first, then
// Local Network as a possibility. Anything else adds nothing.
{
  const raw = "adb connect: adb process failed (exit code Some(1)): failed to connect to '192.168.1.9:5555': No route to host";
  const blocked = explainAdbFailure("connect", "192.168.1.9", raw, "blocked");
  assert.equal(blocked.localNetwork, "blocked");
  assert.equal(
    blocked.summary,
    "Couldn't reach 192.168.1.9. macOS is blocking ATV Optimizer from your local network. Turn ATV Optimizer on in System Settings → Privacy & Security → Local Network (off and on again if it already is), then Restart ADB.",
  );
  assert.ok(blocked.summary.includes(LOCAL_NETWORK_BLOCKED));

  const allowed = explainAdbFailure("connect", "192.168.1.9", raw, "allowed");
  assert.equal(allowed.localNetwork, "restart");
  assert.equal(
    allowed.summary,
    "Couldn't reach 192.168.1.9. Try Restart ADB first: after an update, macOS can cut off an ADB server the earlier version left running. If that doesn't help, check the TV's current IP and that both devices are on the same local network.",
  );
  assert.doesNotMatch(allowed.summary, /Local Network|Privacy|blocking/, "allowed never points at settings");

  const unknown = explainAdbFailure("connect", "192.168.1.9", raw, "unknown");
  assert.equal(unknown.localNetwork, "restart_or_settings");
  assert.equal(
    unknown.summary,
    "Couldn't reach 192.168.1.9. Try Restart ADB first: after an update, macOS can cut off an ADB server the earlier version left running. If that doesn't help, macOS may be blocking local network access for ATV Optimizer, or a VPN or firewall app may be in the way.",
  );
  assert.doesNotMatch(unknown.summary, /macOS is blocking/, "a possibility, never a certainty");
  assert.ok(unknown.summary.indexOf("Restart ADB") < unknown.summary.indexOf("may be blocking"), "restart first");

  const pairUnknown = explainAdbFailure("pair", "192.168.1.9:40000", "error: no route to host", "unknown");
  assert.equal(pairUnknown.localNetwork, "restart_or_settings");
  assert.match(pairUnknown.summary, /^Couldn't reach 192\.168\.1\.9:40000\. Try Restart ADB first/);

  for (const status of ["no_network", "not_applicable", null, undefined]) {
    const r = explainAdbFailure("connect", "192.168.1.9", raw, status);
    assert.equal(r.localNetwork, null, String(status));
    assert.doesNotMatch(r.summary, /macOS|Restart ADB/, String(status));
  }
  // The default (no status passed) is the old behaviour exactly.
  assert.equal(explainAdbFailure("connect", "192.0.2.1:5555", "No route to host").localNetwork, null);

  // Other failures never mention it, whatever the probe said.
  for (const other of ["Connection refused", "Operation timed out", "Network is unreachable", "something new"]) {
    for (const status of ["blocked", "allowed", "unknown"]) {
      assert.equal(explainAdbFailure("connect", "192.168.1.9", other, status).localNetwork, null, `${other} ${status}`);
    }
    assert.equal(localNetworkHint(other, "unknown"), null, other);
  }
  assert.equal(localNetworkHint("connect: Host is unreachable", "unknown"), "restart_or_settings");
  assert.equal(localNetworkHint("os error 65 (EHOSTUNREACH)", "blocked"), "blocked");
  assert.equal(localNetworkHint("", "blocked"), null);

  // Restart ADB's message carries adb's own words for each failed device.
  const restart =
    "ADB server restarted.\nCould not reconnect after 4 tries: 192.168.1.9:5555: failed to connect to '192.168.1.9:5555': No route to host — check the TV is awake";
  assert.ok(isNoRoute(restart));
  assert.equal(LOCAL_NETWORK_POSSIBLE, "macOS may be blocking local network access for ATV Optimizer, or a VPN or firewall app may be in the way.");
}

// Native Rust ADB connects from the app itself and has no adb server, so no
// remedy may point at Restart ADB or a stale server.
{
  const raw = "Couldn't connect to 192.168.1.9:5555: No route to host (os error 65)";
  const native = (status) => explainAdbFailure("connect", "192.168.1.9", raw, status, false);

  const blocked = native("blocked");
  assert.equal(blocked.localNetwork, "blocked");
  assert.equal(
    blocked.summary,
    `Couldn't reach 192.168.1.9. ${LOCAL_NETWORK_BLOCKED} Turn ATV Optimizer on in System Settings → Privacy & Security → Local Network (off and on again if it already is), then try again.`,
  );

  // The app itself reached the network: Local Network is not the cause.
  const allowed = native("allowed");
  assert.equal(allowed.localNetwork, null);
  assert.doesNotMatch(allowed.summary, /macOS|Restart ADB|server/);

  const unknown = native("unknown");
  assert.equal(unknown.localNetwork, "settings");
  assert.equal(unknown.summary, `Couldn't reach 192.168.1.9. ${LOCAL_NETWORK_POSSIBLE}`);

  const pair = explainAdbFailure("pair", "192.168.1.9:40000", raw, "unknown", false);
  assert.equal(pair.localNetwork, "settings");
  assert.doesNotMatch(pair.summary, /Restart ADB|server/);

  for (const status of ["no_network", "not_applicable", null]) {
    assert.equal(native(status).localNetwork, null, String(status));
  }
  assert.equal(localNetworkHint(raw, "allowed", false), null);
  assert.equal(localNetworkHint(raw, "unknown", false), "settings");
  assert.equal(localNetworkHint("Connection refused", "blocked", false), null);
}

console.log("adb errors passed: pair/connect failures map to guidance with raw details kept, unknown errors are not guessed at, the subnet typo warning only fires on readable IPv4, and a macOS No route to host points at Restart ADB or Local Network according to the probe, stating a block only on evidence; native ADB never points at a server restart.");
