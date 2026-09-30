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
const { explainAdbFailure, subnetWarning, hostOf } = await import(
  "data:text/javascript," + encodeURIComponent(stripTypeScriptTypes(source, { mode: "strip" }))
);

// The owner's typo: 182.168… instead of 192.168…
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

console.log("adb errors passed: pair/connect failures map to guidance with raw details kept, unknown errors are not guessed at, and the subnet typo warning only fires on readable IPv4.");
