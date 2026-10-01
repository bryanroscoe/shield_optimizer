import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

const typescriptModule = process.env.TYPESCRIPT_MODULE || "typescript";
const ts = (await import(typescriptModule)).default;
const compilerOptions = {
  module: ts.ModuleKind.ES2022,
  target: ts.ScriptTarget.ES2022,
};

const source = await readFile(
  new URL("../src/lib/discoveryRows.ts", import.meta.url),
  "utf8",
);
const identitySource = await readFile(
  new URL("../src/lib/identity.ts", import.meta.url),
  "utf8",
);
const identityCompiled = ts.transpileModule(identitySource, { compilerOptions }).outputText;
const identityUrl = `data:text/javascript;base64,${Buffer.from(identityCompiled).toString("base64")}`;
const compiled = ts.transpileModule(
  source.replaceAll('"./identity"', `"${identityUrl}"`),
  { compilerOptions },
).outputText;
const { buildDiscoveryRows } = await import(
  `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`
);

const TLS_CONNECT = "_adb-tls-connect._tcp.";
const TLS_PAIRING = "_adb-tls-pairing._tcp.";
const LEGACY = "_adb._tcp.";

const advert = (host, port, service = TLS_CONNECT, name = "Living room") => ({
  name,
  host,
  port,
  service,
});

const saved = (overrides = {}) => ({
  host: "192.168.1.10",
  connectPort: 5555,
  name: "Living room",
  deviceType: "shield",
  hardwareId: "shield-a",
  lastUsed: "2026-09-01T00:00:00.000Z",
  ...overrides,
});

const offline = { connected: false, host: "", connectPort: 0 };
const liveAt = (host, connectPort, hardwareId) => ({ connected: true, host, connectPort, hardwareId });

const summarize = (rows) =>
  rows.map(({ key, name, status, connectPorts, pairingPorts }) => ({
    key,
    name,
    status,
    connectPorts,
    pairingPorts,
  }));

test("collapses repeated advertisements and separates saved from new hosts", () => {
  const rows = buildDiscoveryRows(
    [
      advert("192.168.1.10", 5555),
      advert("192.168.1.10", 5555),
      advert("192.168.1.10", 37099, TLS_PAIRING),
      advert("192.168.1.55", 41234, TLS_CONNECT, "Bedroom"),
    ],
    [saved()],
    offline,
  );

  assert.deepEqual(summarize(rows), [
    {
      key: "discovery:192.168.1.10",
      name: "Living room",
      status: "saved-address",
      connectPorts: [5555],
      pairingPorts: [37099],
    },
    {
      key: "discovery:192.168.1.55",
      name: "Bedroom",
      status: "found",
      connectPorts: [41234],
      pairingPorts: [],
    },
  ]);
});

test("the live endpoint is the only connected row", () => {
  const rows = buildDiscoveryRows(
    [
      advert("192.168.1.10", 5555),
      advert("192.168.1.55", 41234, TLS_CONNECT, "Bedroom"),
    ],
    [saved(), saved({ host: "192.168.1.77", hardwareId: "shield-b", name: "Den" })],
    liveAt("192.168.1.10", 5555),
  );

  assert.deepEqual(
    rows.map((row) => [row.key, row.status]),
    [
      ["discovery:192.168.1.10", "connected"],
      ["discovery:192.168.1.55", "found"],
      ["saved:hardware:shield-b", "saved-missing"],
    ],
  );
});

test("a connected TV that does not advertise stays one connected saved row", () => {
  // The Shield's Network debugging on :5555 publishes no mDNS record, so the
  // only evidence it exists is the live session plus its saved row.
  const rows = buildDiscoveryRows([], [saved()], liveAt("192.168.1.10", 5555, "shield-a"));

  assert.deepEqual(summarize(rows), [
    {
      key: "saved:hardware:shield-a",
      name: "Living room",
      status: "connected",
      connectPorts: [5555],
      pairingPorts: [],
    },
  ]);
  assert.equal(rows[0].savedTarget?.hardwareId, "shield-a");
});

test("only the saved TV whose id is live claims a shared live endpoint (#115)", () => {
  const shieldA = saved({ hardwareId: "shield-a", name: "Shield A" });
  const googleB = saved({ hardwareId: "google-b", name: "Google B", lastUsed: "2026-08-01T00:00:00.000Z" });

  const verified = buildDiscoveryRows([], [shieldA, googleB], liveAt("192.168.1.10", 5555, "google-b"));
  assert.deepEqual(
    verified.map((row) => [row.name, row.status]),
    [["Google B", "connected"], ["Shield A", "saved-address"]],
  );

  // With no live id there is no evidence for either identity, so neither
  // row claims the connection.
  for (const liveId of [undefined, "unknown"]) {
    const unverified = buildDiscoveryRows([], [shieldA, googleB], liveAt("192.168.1.10", 5555, liveId));
    assert.deepEqual(
      unverified.map((row) => [row.name, row.status]),
      [["Shield A", "saved-address"], ["Google B", "saved-address"]],
    );
  }

  // An id-less saved row is identified by its exact endpoint.
  const idless = buildDiscoveryRows(
    [],
    [saved({ hardwareId: undefined, name: "No id" })],
    liveAt("192.168.1.10", 5555),
  );
  assert.deepEqual(idless.map((row) => [row.name, row.status]), [["No id", "connected"]]);
});

test("saved TVs sharing an endpoint that answers unverified each keep their row (#115)", () => {
  const shieldA = saved({ hardwareId: "shield-a", name: "Shield A" });
  const googleB = saved({ hardwareId: "google-b", name: "Google B", lastUsed: "2026-08-01T00:00:00.000Z" });
  const rows = buildDiscoveryRows(
    [advert("192.168.1.10", 5555, LEGACY, "Android TV")],
    [shieldA, googleB],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.key, row.name, row.status]),
    [
      ["saved:hardware:shield-a", "Shield A", "saved-address"],
      ["saved:hardware:google-b", "Google B", "saved-address"],
      ["discovery:192.168.1.10", "Android TV", "saved-address"],
    ],
  );
  assert.equal(rows[0].savedTarget, shieldA);
  assert.equal(rows[1].savedTarget, googleB);
});

test("an address change reports a new host and a saved endpoint that is absent", () => {
  const rows = buildDiscoveryRows(
    [advert("192.168.1.42", 41234, TLS_CONNECT, "Android TV")],
    [saved()],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.key, row.name, row.status]),
    [
      // No identity evidence ties the new address to the saved TV, so the
      // cached name must not travel to it.
      ["discovery:192.168.1.42", "Android TV", "found"],
      ["saved:hardware:shield-a", "Living room", "saved-missing"],
    ],
  );
});

test("a rotated port keeps the saved endpoint truthful instead of calling it offline", () => {
  const rows = buildDiscoveryRows(
    [
      advert("192.168.1.10", 41234),
      advert("192.168.1.10", 37099, TLS_PAIRING),
    ],
    [saved()],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.key, row.status, row.connectPorts]),
    [
      ["discovery:192.168.1.10", "saved-address", [41234]],
      ["saved:hardware:shield-a", "saved-other-port", [5555]],
    ],
  );
  assert.equal(rows[1].savedTarget?.hardwareId, "shield-a");
});

test("a host advertising only a pairing service is not treated as answering", () => {
  const rows = buildDiscoveryRows(
    [advert("192.168.1.10", 37099, TLS_PAIRING)],
    [saved()],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.key, row.status]),
    [
      ["discovery:192.168.1.10", "saved-address"],
      ["saved:hardware:shield-a", "saved-missing"],
    ],
  );
});

test("two saved identities at one endpoint stay two real rows (#115)", () => {
  const livingRoom = saved({ name: "Living room", lastUsed: "2026-09-01T00:00:00.000Z" });
  const bedroom = saved({
    hardwareId: "google-b",
    name: "Bedroom",
    deviceType: "google_tv",
    lastUsed: "2026-09-06T00:00:00.000Z",
  });
  const rows = buildDiscoveryRows([], [livingRoom, bedroom], offline);

  // Each row carries the stored entry itself, so reconnect and Forget act on
  // a TV that exists rather than on a synthesized "Saved TV".
  assert.deepEqual(
    rows.map((row) => [row.key, row.name, row.status]),
    [
      ["saved:hardware:google-b", "Bedroom", "saved-missing"],
      ["saved:hardware:shield-a", "Living room", "saved-missing"],
    ],
  );
  assert.equal(rows[0].savedTarget, bedroom);
  assert.equal(rows[1].savedTarget, livingRoom);
});

test("a saved TV that shared an address with one that moved stays in the scan (#115)", () => {
  const shieldA = saved({ hardwareId: "shield-a", name: "Shield A" });
  const googleB = saved({ hardwareId: "google-b", name: "Google B", deviceType: "google_tv" });
  const rows = buildDiscoveryRows(
    [advert("192.168.5.5", 5555, LEGACY, "adb-shield-a")],
    [shieldA, googleB],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.key, row.name, row.status]),
    [
      ["discovery:192.168.5.5", "Shield A", "saved-verified"],
      ["saved:hardware:google-b", "Google B", "saved-missing"],
    ],
  );
  assert.equal(rows[1].savedTarget, googleB);
});

test("a stored placeholder id is not a wildcard for serial-less adbd (#116)", () => {
  const rows = buildDiscoveryRows(
    [advert("192.168.1.77", 5555, LEGACY, "adb-unknown-AbC123")],
    [saved({ host: "192.168.1.10", hardwareId: "unknown", name: "Living room" })],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.key, row.name, row.status]),
    [
      ["discovery:192.168.1.77", "Android TV", "found"],
      ["saved:idless:192.168.1.10:5555", "Living room", "saved-missing"],
    ],
  );
});

test("a saved id only matches its own advertised serial, not a longer one (#117)", () => {
  const rows = buildDiscoveryRows(
    [
      advert("192.168.1.80", 5555, LEGACY, "adb-shield-a"),
      advert("192.168.1.81", 5555, LEGACY, "adb-shield-a-jBeCEe"),
    ],
    [saved({ host: "192.168.1.10", hardwareId: "shield", name: "Living room" })],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.host, row.status]),
    [
      ["192.168.1.80", "found"],
      ["192.168.1.81", "found"],
      ["192.168.1.10", "saved-missing"],
    ],
  );

  const own = buildDiscoveryRows(
    [advert("192.168.1.82", 5555, LEGACY, "adb-shield-jBeCEe")],
    [saved({ host: "192.168.1.10", hardwareId: "shield", name: "Living room" })],
    offline,
  );
  assert.deepEqual(own.map((row) => [row.host, row.status]), [["192.168.1.82", "saved-verified"]]);
});

test("distinct saved endpoints on one host stay distinct rows", () => {
  const rows = buildDiscoveryRows(
    [],
    [saved(), saved({ connectPort: 41234, hardwareId: "google-b", name: "Bedroom" })],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.key, row.name]),
    [
      ["saved:hardware:shield-a", "Living room"],
      ["saved:hardware:google-b", "Bedroom"],
    ],
  );
});

test("malformed advertisements are dropped", () => {
  const rows = buildDiscoveryRows(
    [
      advert("   ", 5555),
      advert("192.168.1.10", 0),
      advert("192.168.1.10", 70000),
      advert("192.168.1.10", 5555.5),
      advert(" 192.168.1.10 ", 5555),
    ],
    [],
    offline,
  );

  assert.deepEqual(summarize(rows), [
    {
      key: "discovery:192.168.1.10",
      name: "Living room",
      status: "found",
      connectPorts: [5555],
      pairingPorts: [],
    },
  ]);
});

test("instance names are only used when they are unambiguous and not adb ids", () => {
  const adbInstance = buildDiscoveryRows(
    [advert("192.168.1.10", 5555, TLS_CONNECT, "adb-58040DLCH005YV-jBeCEe")],
    [],
    offline,
  );
  assert.equal(adbInstance[0].name, "Android TV");

  const conflicting = buildDiscoveryRows(
    [
      advert("192.168.1.10", 5555, TLS_CONNECT, "Living room"),
      advert("192.168.1.10", 37099, TLS_PAIRING, "Bedroom"),
    ],
    [],
    offline,
  );
  assert.equal(conflicting[0].name, "Android TV");
});

test("legacy connect ports are the only ones flagged as needing no pairing code", () => {
  const rows = buildDiscoveryRows(
    [
      advert("192.168.1.10", 5555, LEGACY),
      advert("192.168.1.10", 41234, TLS_CONNECT),
      advert("192.168.1.55", 41235, TLS_CONNECT, "Bedroom"),
      advert("192.168.1.55", 37099, TLS_PAIRING, "Bedroom"),
    ],
    [],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.host, row.connectPorts, row.legacyConnectPorts]),
    [
      ["192.168.1.10", [5555, 41234], [5555]],
      ["192.168.1.55", [41235], []],
    ],
  );
});

test("an advertised serial verifies a saved TV and names it", () => {
  const rows = buildDiscoveryRows(
    [advert("192.168.1.10", 5555, LEGACY, "adb-shield-a")],
    [saved()],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.key, row.name, row.status]),
    [["discovery:192.168.1.10", "Living room", "saved-verified"]],
  );
  assert.equal(rows[0].savedTarget?.hardwareId, "shield-a");
});

test("a verified serial recognizes a saved TV that moved to a new address", () => {
  const rows = buildDiscoveryRows(
    [advert("192.168.1.99", 5555, LEGACY, "adb-shield-a-jBeCEe")],
    [saved()],
    offline,
  );

  // The serial is identity evidence, so the name travels with the TV and the
  // stale saved endpoint does not also appear as a second row.
  assert.deepEqual(
    rows.map((row) => [row.key, row.name, row.status]),
    [["discovery:192.168.1.99", "Living room", "saved-verified"]],
  );
});

test("a serial that matches no saved TV leaves the row unclaimed", () => {
  const rows = buildDiscoveryRows(
    [advert("192.168.1.42", 5555, LEGACY, "adb-1324619053514")],
    [saved()],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.key, row.name, row.status]),
    [
      ["discovery:192.168.1.42", "Android TV", "found"],
      ["saved:hardware:shield-a", "Living room", "saved-missing"],
    ],
  );
});

test("two TVs advertising only adb ids are each named from their own serial", () => {
  const rows = buildDiscoveryRows(
    [
      advert("192.168.42.71", 5555, LEGACY, "adb-0323716101827"),
      advert("192.168.42.196", 5555, LEGACY, "adb-1324619053514"),
    ],
    [
      saved({ host: "192.168.42.71", hardwareId: "0323716101827", name: "Den" }),
      saved({
        host: "192.168.42.196",
        hardwareId: "1324619053514",
        name: "Bedroom",
      }),
    ],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.name, row.status]),
    [
      ["Den", "saved-verified"],
      ["Bedroom", "saved-verified"],
    ],
  );
});

test("rows are ordered by usability then recency, not by mDNS answer order", () => {
  const rows = buildDiscoveryRows(
    [
      advert("192.168.42.143", 5555, LEGACY, "adb-old-b"),
      advert("192.168.42.25", 5555, LEGACY, "adb-recent"),
      advert("192.168.42.99", 5555, LEGACY, "adb-stranger"),
      advert("192.168.42.71", 5555, LEGACY, "adb-old-a"),
    ],
    [
      saved({ host: "192.168.42.143", hardwareId: "old-b", name: "Media Room", lastUsed: "2026-09-07T00:00:00.000Z" }),
      saved({ host: "192.168.42.25", hardwareId: "recent", name: "Guest room", lastUsed: "2026-09-14T00:00:00.000Z" }),
      saved({ host: "192.168.42.71", hardwareId: "old-a", name: "Living Room", lastUsed: "2026-09-07T00:00:00.000Z" }),
    ],
    offline,
  );

  assert.deepEqual(
    rows.map((row) => [row.name, row.status]),
    [
      ["Guest room", "saved-verified"],
      // Same timestamp, so the address breaks the tie rather than the network.
      ["Living Room", "saved-verified"],
      ["Media Room", "saved-verified"],
      ["Android TV", "found"],
    ],
  );
});
