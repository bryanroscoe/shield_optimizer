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
const { explainAdbFailure, subnetWarning, hostOf, localNetworkHint, localNetworkSentence, LOCAL_NETWORK_BLOCKED } = await import(
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


// macOS Local Network privacy. "No route to host" points at it only on the
// probe's evidence ("blocked": stated plainly) or when the probe could not
// tell ("unknown": a likely cause). Elsewhere it adds nothing.
{
  const raw = "adb connect: adb process failed (exit code Some(1)): failed to connect to '192.168.1.9:5555': No route to host";
  const blocked = explainAdbFailure("connect", "192.168.1.9", raw, "blocked");
  assert.equal(blocked.localNetwork, "blocked");
  assert.equal(
    blocked.summary,
    "Couldn't reach 192.168.1.9. macOS is blocking ATV Optimizer from your local network. Turn ATV Optimizer on in System Settings → Privacy & Security → Local Network (off and on again if it already is), then Restart ADB.",
  );
  assert.ok(blocked.summary.includes(LOCAL_NETWORK_BLOCKED));

  const possible = explainAdbFailure("connect", "192.168.1.9", raw, "unknown");
  assert.equal(possible.localNetwork, "possible");
  assert.match(possible.summary, /^Couldn't reach 192\.168\.1\.9\. Check the TV's current IP/);
  assert.match(possible.summary, /macOS may also be blocking local network access for ATV Optimizer, or a VPN or firewall app may be in the way\.$/);
  assert.doesNotMatch(possible.summary, /macOS is blocking/, "a likely cause, never a certainty");

  const pairPossible = explainAdbFailure("pair", "192.168.1.9:40000", "error: no route to host", "unknown");
  assert.equal(pairPossible.localNetwork, "possible");
  assert.match(pairPossible.summary, /pairing dialog.*macOS may also be blocking/);

  for (const status of ["allowed", "no_network", "not_applicable", null, undefined]) {
    const r = explainAdbFailure("connect", "192.168.1.9", raw, status);
    assert.equal(r.localNetwork, null, String(status));
    assert.doesNotMatch(r.summary, /macOS/, String(status));
  }
  // The default (no status passed) is the old behaviour exactly.
  assert.equal(explainAdbFailure("connect", "192.0.2.1:5555", "No route to host").localNetwork, null);

  // Other failures never mention it, whatever the probe said.
  for (const other of ["Connection refused", "Operation timed out", "Network is unreachable", "something new"]) {
    assert.equal(explainAdbFailure("connect", "192.168.1.9", other, "blocked").localNetwork, null, other);
    assert.equal(localNetworkHint(other, "unknown"), null, other);
  }
  assert.equal(localNetworkHint("connect: Host is unreachable", "unknown"), "possible");
  assert.equal(localNetworkHint("os error 65 (EHOSTUNREACH)", "blocked"), "blocked");
  assert.equal(localNetworkHint("", "blocked"), null);

  // Restart ADB's message carries adb's own words for each failed device.
  const restart =
    "ADB server restarted.\nCould not reconnect after 4 tries: 192.168.1.9:5555: failed to connect to '192.168.1.9:5555': No route to host — check the TV is awake";
  assert.equal(localNetworkHint(restart, "unknown"), "possible");
  assert.equal(localNetworkSentence("possible"), "macOS may also be blocking local network access for ATV Optimizer, or a VPN or firewall app may be in the way.");
  assert.equal(localNetworkSentence(null), null);
}

console.log("adb errors passed: pair/connect failures map to guidance with raw details kept, unknown errors are not guessed at, the subnet typo warning only fires on readable IPv4, and No route to host points at macOS Local Network only on probe evidence or as a stated possibility.");
