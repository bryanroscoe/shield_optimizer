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
const compiled = ts.transpileModule(source, { compilerOptions }).outputText;
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
const liveAt = (host, connectPort) => ({ connected: true, host, connectPort });

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
      ["saved:192.168.1.77:5555", "saved-missing"],
    ],
  );
});

test("a connected TV that does not advertise stays one connected saved row", () => {
  // The Shield's Network debugging on :5555 publishes no mDNS record, so the
  // only evidence it exists is the live session plus its saved row.
  const rows = buildDiscoveryRows([], [saved()], liveAt("192.168.1.10", 5555));

  assert.deepEqual(summarize(rows), [
    {
      key: "saved:192.168.1.10:5555",
      name: "Living room",
      status: "connected",
      connectPorts: [5555],
      pairingPorts: [],
    },
  ]);
  assert.equal(rows[0].savedTarget?.hardwareId, "shield-a");
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
      ["saved:192.168.1.10:5555", "Living room", "saved-missing"],
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
      ["saved:192.168.1.10:5555", "saved-other-port", [5555]],
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
      ["saved:192.168.1.10:5555", "saved-missing"],
    ],
  );
});

test("two saved identities at one endpoint collapse without borrowing a name", () => {
  const rows = buildDiscoveryRows(
    [],
    [
      saved({ name: "Living room", lastUsed: "2026-09-01T00:00:00.000Z" }),
      saved({
        hardwareId: "google-b",
        name: "Bedroom",
        deviceType: "google_tv",
        lastUsed: "2026-09-06T00:00:00.000Z",
      }),
    ],
    offline,
  );

  assert.equal(rows.length, 1);
  assert.equal(rows[0].key, "saved:192.168.1.10:5555");
  assert.equal(rows[0].name, "Saved TV");
  assert.deepEqual(rows[0].savedTarget, {
    host: "192.168.1.10",
    connectPort: 5555,
    name: "Saved TV",
    deviceType: "unknown",
    lastUsed: "2026-09-06T00:00:00.000Z",
  });
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
      ["saved:192.168.1.10:5555", "Living room"],
      ["saved:192.168.1.10:41234", "Bedroom"],
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
