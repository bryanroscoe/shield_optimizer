import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { beforeEach, test } from "node:test";

const typescriptModule = process.env.TYPESCRIPT_MODULE || "typescript";
const ts = (await import(typescriptModule)).default;
const compilerOptions = {
  module: ts.ModuleKind.ES2022,
  target: ts.ScriptTarget.ES2022,
};

const typesSource = await readFile(
  new URL("../src/lib/types.ts", import.meta.url),
  "utf8",
);
const typesCompiled = ts.transpileModule(typesSource, { compilerOptions }).outputText;
const typesUrl = `data:text/javascript;base64,${Buffer.from(typesCompiled).toString("base64")}`;
const savedDevicesSource = await readFile(
  new URL("../src/lib/savedDevices.ts", import.meta.url),
  "utf8",
);
const savedDevicesCompiled = ts.transpileModule(
  savedDevicesSource.replaceAll('"./types"', `"${typesUrl}"`),
  { compilerOptions },
).outputText;
const savedDevices = await import(
  `data:text/javascript;base64,${Buffer.from(savedDevicesCompiled).toString("base64")}`
);

const KEY = "atv.savedDevices.v1";

class MemoryStorage {
  values = new Map();

  getItem(key) {
    return this.values.get(key) ?? null;
  }

  setItem(key, value) {
    this.values.set(key, value);
  }
}

let storage;

beforeEach(() => {
  storage = new MemoryStorage();
  globalThis.localStorage = storage;
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

const device = (hardwareId, overrides = {}) => ({
  id: 1,
  serial: "192.168.1.10:5555",
  name: "Reported device",
  model: "Android TV",
  device_type: "shield",
  status: "device",
  connection: "network",
  properties: {
    friendly_name: null,
    serial_number: hardwareId,
  },
  ...overrides,
});

function seed(rows) {
  storage.setItem(KEY, JSON.stringify(rows));
}

function rawRows() {
  return JSON.parse(storage.getItem(KEY));
}

test("keeps mixed saved and new TVs and deduplicates repeated advertisements", () => {
  seed([
    saved(),
    saved({
      host: "192.168.1.20",
      name: "Bedroom",
      hardwareId: undefined,
    }),
  ]);

  savedDevices.rememberDevice(
    "192.168.1.30",
    5555,
    device("google-b", {
      device_type: "google_tv",
      properties: {
        friendly_name: "Office",
        serial_number: "google-b",
      },
    }),
  );
  savedDevices.rememberDevice(
    "192.168.1.30",
    5555,
    device("google-b", {
      device_type: "google_tv",
      properties: {
        friendly_name: "Office",
        serial_number: "google-b",
      },
    }),
  );

  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 3);
  assert.equal(rows.filter((row) => row.hardwareId === "google-b").length, 1);
  assert.deepEqual(
    new Set(rows.map((row) => row.name)),
    new Set(["Living room", "Bedroom", "Office"]),
  );
});

test("moves one matching hardware identity across an address change", () => {
  seed([saved()]);

  savedDevices.rememberDevice(
    "192.168.1.99",
    5555,
    device("shield-a", { name: "Generic report" }),
  );

  assert.deepEqual(savedDevices.listSavedDevices().map(({ host, connectPort, name, hardwareId }) => ({
    host,
    connectPort,
    name,
    hardwareId,
  })), [
    {
      host: "192.168.1.99",
      connectPort: 5555,
      name: "Living room",
      hardwareId: "shield-a",
    },
  ]);
});

test("moves one matching hardware identity across a port change", () => {
  seed([saved()]);

  savedDevices.rememberDevice("192.168.1.10", 42137, device("shield-a"));

  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 1);
  assert.equal(rows[0].connectPort, 42137);
  assert.equal(rows[0].name, "Living room");
});

test("keeps different hardware identities that claim the same address", () => {
  seed([saved()]);

  savedDevices.rememberDevice(
    "192.168.1.10",
    5555,
    device("google-b", {
      device_type: "google_tv",
      properties: {
        friendly_name: "New TV",
        serial_number: "google-b",
      },
    }),
  );

  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 2);
  assert.deepEqual(
    new Set(rows.map((row) => `${row.hardwareId}:${row.name}:${row.host}`)),
    new Set([
      "shield-a:Living room:192.168.1.10",
      "google-b:New TV:192.168.1.10",
    ]),
  );
});

test("does not merge a saved id-less row into a live identified TV", () => {
  seed([
    saved({
      hardwareId: undefined,
      name: "Old endpoint name",
      deviceType: "unknown",
    }),
  ]);

  savedDevices.rememberDevice(
    "192.168.1.10",
    5555,
    device("shield-a", {
      properties: {
        friendly_name: "Identified TV",
        serial_number: "shield-a",
      },
    }),
  );

  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 2);
  const idless = rows.find((row) => !row.hardwareId);
  const identified = rows.find((row) => row.hardwareId === "shield-a");
  assert.deepEqual(
    { name: idless.name, deviceType: idless.deviceType },
    { name: "Old endpoint name", deviceType: "unknown" },
  );
  assert.deepEqual(
    { name: identified.name, deviceType: identified.deviceType },
    { name: "Identified TV", deviceType: "shield" },
  );
});

test("does not merge an id-less live TV into a saved identified row", () => {
  seed([saved()]);

  savedDevices.rememberDevice(
    "192.168.1.10",
    5555,
    device(undefined, {
      device_type: "unknown",
      properties: {
        friendly_name: "Unidentified TV",
        serial_number: undefined,
      },
    }),
  );

  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 2);
  const identified = rows.find((row) => row.hardwareId === "shield-a");
  const idless = rows.find((row) => !row.hardwareId);
  assert.deepEqual(
    { name: identified.name, deviceType: identified.deviceType },
    { name: "Living room", deviceType: "shield" },
  );
  assert.deepEqual(
    { name: idless.name, deviceType: idless.deviceType },
    { name: "Unidentified TV", deviceType: "unknown" },
  );
});

test("matches id-less rows only at the exact endpoint", () => {
  seed([saved({ hardwareId: undefined, name: "Endpoint TV" })]);

  savedDevices.rememberDevice(
    "192.168.1.10",
    5555,
    device(undefined, {
      properties: { friendly_name: null, serial_number: undefined },
    }),
  );
  assert.equal(savedDevices.listSavedDevices().length, 1);
  assert.equal(savedDevices.listSavedDevices()[0].name, "Endpoint TV");

  savedDevices.rememberDevice(
    "192.168.1.10",
    5556,
    device(undefined, {
      properties: { friendly_name: "Other port", serial_number: undefined },
    }),
  );
  assert.equal(savedDevices.listSavedDevices().length, 2);
});

test("forgets identified TVs by id and id-less TVs by exact endpoint", () => {
  seed([
    saved({ host: "192.168.1.20", lastUsed: "2026-09-04T00:00:00.000Z" }),
    saved({ lastUsed: "2026-09-03T00:00:00.000Z" }),
    saved({ hardwareId: "google-b", name: "Other TV", lastUsed: "2026-09-02T00:00:00.000Z" }),
    saved({
      host: "192.168.1.30",
      hardwareId: undefined,
      name: "No id",
      lastUsed: "2026-09-05T00:00:00.000Z",
    }),
    saved({
      host: "192.168.1.30",
      hardwareId: "shield-c",
      name: "Identified at same endpoint",
      lastUsed: "2026-09-01T00:00:00.000Z",
    }),
  ]);

  savedDevices.forgetDevice("192.168.1.20", 5555);
  assert.equal(savedDevices.listSavedDevices().some((row) => row.hardwareId === "shield-a"), false);
  assert.equal(savedDevices.listSavedDevices().some((row) => row.hardwareId === "google-b"), true);

  savedDevices.forgetDevice("192.168.1.30", 5555);
  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.some((row) => !row.hardwareId), false);
  assert.equal(rows.some((row) => row.hardwareId === "shield-c"), true);
});

test("returns cached names only for an unambiguous or matching identity", () => {
  seed([
    saved({ hardwareId: "shield-a", name: "Shield" }),
    saved({
      hardwareId: "google-b",
      name: "Google TV",
      lastUsed: "2026-09-02T00:00:00.000Z",
    }),
  ]);

  assert.equal(savedDevices.cachedDeviceName("192.168.1.10"), null);
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", "shield-a"), "Shield");
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", "google-b"), "Google TV");
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", "other-id"), null);

  seed([saved({ hardwareId: undefined, name: "No id" })]);
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", "shield-a"), null);
});

test("sorts by lastUsed and truncates the oldest saved rows", () => {
  seed(Array.from({ length: 18 }, (_, index) => saved({
    host: `192.168.1.${index + 1}`,
    hardwareId: `tv-${index}`,
    name: `TV ${index}`,
    lastUsed: new Date(Date.UTC(2020, 0, index + 1)).toISOString(),
  })).reverse());

  savedDevices.rememberDevice("192.168.2.1", 5555, device("new-tv"));

  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 16);
  assert.equal(rows[0].hardwareId, "new-tv");
  assert.deepEqual(
    rows.map((row) => row.lastUsed),
    [...rows].map((row) => row.lastUsed).sort().reverse(),
  );
  assert.equal(rows.some((row) => row.hardwareId === "tv-0"), false);
  assert.equal(rows.some((row) => row.hardwareId === "tv-1"), false);
  assert.equal(rows.some((row) => row.hardwareId === "tv-2"), false);
  assert.equal(rawRows().length, 16);
});

test("ignores corrupt records and normalizes recoverable stored values", () => {
  storage.setItem(KEY, "{not-json");
  assert.deepEqual(savedDevices.listSavedDevices(), []);

  seed([
    null,
    { host: "", connectPort: 5555 },
    { host: "bad-port", connectPort: 0 },
    saved({
      host: " 192.168.1.40 ",
      name: " ",
      deviceType: "googletv",
      hardwareId: " serial-40 ",
      lastUsed: "not-a-date",
    }),
  ]);

  assert.deepEqual(savedDevices.listSavedDevices(), [
    {
      host: "192.168.1.40",
      connectPort: 5555,
      name: "192.168.1.40",
      deviceType: "google_tv",
      hardwareId: "serial-40",
      lastUsed: "1970-01-01T00:00:00.000Z",
    },
  ]);
});

test("builds stable identity keys for host collisions and identical endpoints", () => {
  const identifiedA = saved({ hardwareId: "shield-a" });
  const identifiedB = saved({ hardwareId: "google-b", connectPort: 42137 });
  const idless = saved({ hardwareId: undefined });

  assert.equal(savedDevices.savedDeviceKey(identifiedA), "hardware:shield-a");
  assert.equal(savedDevices.savedDeviceKey(identifiedB), "hardware:google-b");
  assert.equal(
    savedDevices.savedDeviceKey(idless),
    "idless:192.168.1.10:5555",
  );
  assert.equal(
    new Set([identifiedA, identifiedB, idless].map(savedDevices.savedDeviceKey)).size,
    3,
  );
  assert.equal(
    savedDevices.savedHostHasMultipleIdentities(
      [identifiedA, identifiedB],
      "192.168.1.10",
    ),
    true,
  );
  assert.equal(
    savedDevices.savedHostHasMultipleIdentities(
      [identifiedA, saved({ hardwareId: "shield-a", connectPort: 42137 })],
      "192.168.1.10",
    ),
    false,
  );
});

test("a selected reconnect token activates only its saved row", () => {
  const rows = [
    saved({ hardwareId: "shield-a" }),
    saved({ hardwareId: "google-b", name: "Google TV" }),
    saved({ hardwareId: undefined, name: "No id" }),
  ];
  const selectedToken = savedDevices.savedDeviceKey(rows[1]);

  assert.deepEqual(
    rows.map((row) => savedDevices.savedDeviceKey(row) === selectedToken),
    [false, true, false],
  );
});

test("matches the current TV by verified id or an exact id-less endpoint", () => {
  const identified = saved({ hardwareId: "shield-a" });
  const otherIdentity = saved({ hardwareId: "google-b" });
  const idless = saved({ hardwareId: undefined });

  assert.equal(
    savedDevices.savedDeviceMatchesConnection(
      identified,
      "192.168.1.99",
      42137,
      "shield-a",
    ),
    true,
  );
  assert.equal(
    savedDevices.savedDeviceMatchesConnection(
      otherIdentity,
      "192.168.1.10",
      5555,
      "shield-a",
    ),
    false,
  );
  assert.equal(
    savedDevices.savedDeviceMatchesConnection(
      idless,
      "192.168.1.10",
      5555,
      "shield-a",
    ),
    false,
  );
  assert.equal(
    savedDevices.savedDeviceMatchesConnection(
      idless,
      "192.168.1.10",
      5555,
    ),
    true,
  );
  assert.equal(
    savedDevices.savedDeviceMatchesConnection(
      idless,
      "192.168.1.10",
      42137,
    ),
    false,
  );
  assert.equal(
    savedDevices.savedDeviceMatchesConnection(
      identified,
      "192.168.1.10",
      5555,
    ),
    false,
  );
});

test("forgets the exact selected identity at an identical endpoint", () => {
  seed([
    saved({ hardwareId: "shield-a", lastUsed: "2026-09-03T00:00:00.000Z" }),
    saved({ hardwareId: "google-b", name: "Google TV", lastUsed: "2026-09-02T00:00:00.000Z" }),
    saved({ hardwareId: undefined, name: "No id", lastUsed: "2026-09-01T00:00:00.000Z" }),
  ]);

  const google = savedDevices.listSavedDevices().find(
    (row) => row.hardwareId === "google-b",
  );
  savedDevices.forgetSavedDevice(google);
  assert.deepEqual(
    savedDevices.listSavedDevices().map((row) => row.hardwareId),
    ["shield-a", undefined],
  );

  const idless = savedDevices.listSavedDevices().find((row) => !row.hardwareId);
  savedDevices.forgetSavedDevice(idless);
  assert.deepEqual(
    savedDevices.listSavedDevices().map((row) => row.hardwareId),
    ["shield-a"],
  );
});

test("auto-dials only when the saved list truly has one entry", () => {
  const one = [saved()];
  const twoAtOneEndpoint = [
    saved({ hardwareId: "shield-a" }),
    saved({ hardwareId: "google-b" }),
  ];

  assert.equal(savedDevices.shouldAutoDialSavedDevices([], true), false);
  assert.equal(savedDevices.shouldAutoDialSavedDevices(one, false), false);
  assert.equal(savedDevices.shouldAutoDialSavedDevices(one, true), true);
  assert.equal(
    savedDevices.shouldAutoDialSavedDevices(twoAtOneEndpoint, true),
    false,
  );
});
