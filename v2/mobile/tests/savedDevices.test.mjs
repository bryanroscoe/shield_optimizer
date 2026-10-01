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
const identitySource = await readFile(
  new URL("../src/lib/identity.ts", import.meta.url),
  "utf8",
);
const identityCompiled = ts.transpileModule(identitySource, { compilerOptions }).outputText;
const identityUrl = `data:text/javascript;base64,${Buffer.from(identityCompiled).toString("base64")}`;
const savedDevicesSource = await readFile(
  new URL("../src/lib/savedDevices.ts", import.meta.url),
  "utf8",
);
const savedDevicesCompiled = ts.transpileModule(
  savedDevicesSource
    .replaceAll('"./types"', `"${typesUrl}"`)
    .replaceAll('"./identity"', `"${identityUrl}"`),
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

  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", 5555), null);
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", 5555, "shield-a"), "Shield");
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", 5555, "google-b"), "Google TV");
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", 5555, "other-id"), null);

  seed([saved({ hardwareId: undefined, name: "No id" })]);
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", 5555, "shield-a"), null);
});

test("a reused address never lends an identified TV's name to a TV that reports no id (#117)", () => {
  seed([saved({ hardwareId: "shield-a", name: "Living room" })]);

  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", 5555), null);
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", 5555, "unknown"), null);
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", 5555, " "), null);
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", 5555, "shield-a"), "Living room");

  // An id-less row is identified by its exact endpoint and nothing looser.
  seed([saved({ hardwareId: undefined, name: "No id" })]);
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", 5555), "No id");
  assert.equal(savedDevices.cachedDeviceName("192.168.1.10", 41234), null);
});

test("a stored placeholder id becomes an id-less row with its own stable key, never merged with another (#116, #146)", () => {
  seed([
    saved({ hardwareId: undefined, name: "Real", lastUsed: "2026-09-02T00:00:00.000Z" }),
    // Shares "Real"'s exact host:port once its placeholder id normalizes
    // away -- a genuinely different TV, not a duplicate of "Real".
    saved({ hardwareId: "unknown", name: "Ghost", lastUsed: "2026-09-01T00:00:00.000Z" }),
    saved({ host: "192.168.1.20", hardwareId: " UNKNOWN ", name: "Other", lastUsed: "2026-08-01T00:00:00.000Z" }),
  ]);

  const rows = savedDevices.listSavedDevices();
  const keys = rows.map(savedDevices.savedDeviceKey);
  assert.equal(new Set(keys).size, keys.length);
  // All three survive -- the placeholder migration must not discard a TV
  // just because it now shares an address with another id-less row.
  assert.deepEqual(
    new Set(rows.map((row) => `${row.name}:${row.hardwareId}`)),
    new Set(["Real:undefined", "Ghost:undefined", "Other:undefined"]),
  );
  // The placeholder is migrated out of storage, not just hidden on read.
  assert.equal(rawRows().some((row) => "hardwareId" in row), false);
  // Each row's freshly-minted local key is itself persisted, so it stays
  // stable across reads instead of being re-rolled every time.
  assert.deepEqual(savedDevices.listSavedDevices().map(savedDevices.savedDeviceKey), keys);

  const ghost = rows.find((row) => row.name === "Ghost");
  savedDevices.forgetSavedDevice(ghost);
  assert.deepEqual(
    new Set(savedDevices.listSavedDevices().map((row) => row.name)),
    new Set(["Real", "Other"]),
  );
});

test("reconnecting a TV that reports a placeholder id refreshes its row instead of appending (#116)", () => {
  seed([saved({ hardwareId: "unknown", name: "Ghost" })]);

  savedDevices.rememberDevice("192.168.1.10", 5555, device("unknown"));
  savedDevices.rememberDevice("192.168.1.10", 5555, device("unknown"));

  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 1);
  assert.equal(rows[0].hardwareId, undefined);
  assert.equal(rows[0].name, "Ghost");
});

test("never guesses which of two id-less TVs at one endpoint just reconnected (#146)", () => {
  seed([
    saved({ hardwareId: undefined, localId: "first-tv", name: "First TV", lastUsed: "2026-09-01T00:00:00.000Z" }),
    saved({ hardwareId: undefined, localId: "second-tv", name: "Second TV", lastUsed: "2026-09-02T00:00:00.000Z" }),
  ]);

  // An id-less connection lands on the one shared endpoint of two already
  // distinct saved TVs. Which one it is cannot be told from the address
  // alone, so neither existing row is claimed (and so overwritten with a
  // possibly-wrong identity) -- and nothing new is persisted either, since a
  // fresh unidentified row on every repeat reconnect would eventually evict a
  // genuine saved TV once MAX is reached. The connection is simply not
  // recorded against any saved identity.
  savedDevices.rememberDevice("192.168.1.10", 5555, device(undefined, {
    name: "Just reported",
    properties: { friendly_name: null, serial_number: undefined },
  }));

  // Repeating the ambiguous reconnect many times still does not grow storage.
  for (let i = 0; i < 20; i++) {
    savedDevices.rememberDevice("192.168.1.10", 5555, device(undefined, {
      name: "Just reported",
      properties: { friendly_name: null, serial_number: undefined },
    }));
  }

  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 2);
  assert.deepEqual(
    new Set(rows.map((row) => row.name)),
    new Set(["First TV", "Second TV"]),
  );
  // Both original rows are untouched -- neither lost its name nor its key.
  assert.equal(rows.some((row) => row.name === "First TV" && savedDevices.savedDeviceKey(row) === "local:first-tv"), true);
  assert.equal(rows.some((row) => row.name === "Second TV" && savedDevices.savedDeviceKey(row) === "local:second-tv"), true);
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

test("only an unambiguous match counts as the live connection (#146)", () => {
  const shieldA = saved({ hardwareId: "shield-a" });
  const lone = saved({ hardwareId: undefined, localId: "lone-tv" });
  const first = saved({ hardwareId: undefined, localId: "first-tv" });
  const second = saved({ hardwareId: undefined, localId: "second-tv", name: "Second TV" });

  // A verified hardware id is unambiguous regardless of what else is saved.
  assert.equal(
    savedDevices.savedDeviceIsLiveConnection(shieldA, [shieldA, first, second], "192.168.1.10", 5555, "shield-a"),
    true,
  );
  // A single id-less row at the endpoint is as good as this app's identity
  // story gets, so it counts.
  assert.equal(
    savedDevices.savedDeviceIsLiveConnection(lone, [lone], "192.168.1.10", 5555, undefined),
    true,
  );
  // Two id-less rows sharing the endpoint: neither may claim the connection.
  assert.equal(
    savedDevices.savedDeviceIsLiveConnection(first, [first, second], "192.168.1.10", 5555, undefined),
    false,
  );
  assert.equal(
    savedDevices.savedDeviceIsLiveConnection(second, [first, second], "192.168.1.10", 5555, undefined),
    false,
  );
  // A row that does not even match the endpoint is never live, ambiguous or not.
  assert.equal(
    savedDevices.savedDeviceIsLiveConnection(first, [first, second], "192.168.1.99", 5555, undefined),
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

// #154: a lone id-less match used to be trusted on endpoint alone, so a
// *different* id-less TV that later answers at the same address silently
// inherited the saved row's name instead of being recognized as distinct.

test("a lone id-less match stores a fingerprint from the live device's properties (#154)", () => {
  seed([saved({ hardwareId: undefined, name: "Living room TV" })]);

  savedDevices.rememberDevice(
    "192.168.1.10",
    5555,
    device(undefined, {
      properties: {
        friendly_name: null,
        serial_number: undefined,
        model: "Shield TV Pro",
        manufacturer: "NVIDIA",
        device_codename: "mdarcy",
      },
    }),
  );

  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 1);
  assert.deepEqual(rows[0].fingerprint, {
    model: "Shield TV Pro",
    manufacturer: "NVIDIA",
    deviceCodename: "mdarcy",
  });
});

test("a different model or manufacturer at a saved id-less row's address is not silently inherited (#154)", () => {
  seed([
    saved({
      hardwareId: undefined,
      name: "Living room TV",
      fingerprint: { model: "Shield TV Pro", manufacturer: "NVIDIA" },
    }),
  ]);

  const result = savedDevices.rememberDevice(
    "192.168.1.10",
    5555,
    device(undefined, {
      name: "Reported device",
      properties: {
        friendly_name: null,
        serial_number: undefined,
        model: "Chromecast with Google TV",
        manufacturer: "Google",
      },
    }),
  );

  assert.equal(result.mismatch, true);
  const rows = savedDevices.listSavedDevices();
  // The old row survives untouched -- it is not renamed or repurposed.
  assert.equal(rows.length, 2);
  const old = rows.find((row) => row.name === "Living room TV");
  const fresh = rows.find((row) => row.name !== "Living room TV");
  assert.deepEqual(old.fingerprint, { model: "Shield TV Pro", manufacturer: "NVIDIA" });
  assert.equal(old.hardwareId, undefined);
  assert.notEqual(fresh, undefined);
  assert.deepEqual(fresh.fingerprint, { model: "Chromecast with Google TV", manufacturer: "Google" });

  // Neither row can now be told apart as "the" live connection -- the address
  // is ambiguous between two distinct saved TVs, so the UI must not claim
  // either one is connected or let a reconnect silently refresh either.
  assert.equal(
    savedDevices.savedDeviceIsLiveConnection(old, rows, "192.168.1.10", 5555, undefined),
    false,
  );
  assert.equal(
    savedDevices.savedDeviceIsLiveConnection(fresh, rows, "192.168.1.10", 5555, undefined),
    false,
  );
});

test("a manufacturer-only disagreement also counts as a mismatch (#154)", () => {
  seed([
    saved({
      hardwareId: undefined,
      name: "Living room TV",
      fingerprint: { model: "ATV1000", manufacturer: "NVIDIA" },
    }),
  ]);

  const result = savedDevices.rememberDevice(
    "192.168.1.10",
    5555,
    device(undefined, {
      properties: {
        friendly_name: null,
        serial_number: undefined,
        model: "ATV1000",
        manufacturer: "Some Other Vendor",
      },
    }),
  );

  assert.equal(result.mismatch, true);
  assert.equal(savedDevices.listSavedDevices().length, 2);
});

test("an empty saved fingerprint (an older row, or a device that reported nothing) never blocks a match (#154)", () => {
  seed([saved({ hardwareId: undefined, name: "Living room TV" })]);

  // No fingerprint stored yet (row predates this feature) and the live
  // device reports no model/manufacturer either -- unknown vs unknown must
  // not be treated as a disagreement.
  const result = savedDevices.rememberDevice(
    "192.168.1.10",
    5555,
    device(undefined, {
      name: "Generic report",
      properties: { friendly_name: null, serial_number: undefined },
    }),
  );

  assert.equal(result.mismatch, false);
  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 1);
  assert.equal(rows[0].name, "Living room TV");
});

test("a matching fingerprint still refreshes the row normally, and is never treated as proof (#154)", () => {
  seed([
    saved({
      hardwareId: undefined,
      name: "Living room TV",
      fingerprint: { model: "Shield TV Pro", manufacturer: "NVIDIA" },
    }),
  ]);

  const result = savedDevices.rememberDevice(
    "192.168.1.10",
    5555,
    device(undefined, {
      name: "Generic report",
      properties: {
        friendly_name: "Renamed on the TV",
        serial_number: undefined,
        model: "Shield TV Pro",
        manufacturer: "NVIDIA",
      },
    }),
  );

  assert.equal(result.mismatch, false);
  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 1);
  // The row refreshed in place (still id-less, no hardwareId appeared out of
  // a mere model/manufacturer agreement) and picked up the new friendly name.
  assert.equal(rows[0].hardwareId, undefined);
  assert.equal(rows[0].name, "Renamed on the TV");
});

test("a hardware-identified reconnect is never second-guessed by fingerprint (#154)", () => {
  seed([saved({ fingerprint: undefined })]);

  const result = savedDevices.rememberDevice(
    "192.168.1.10",
    5555,
    device("shield-a", {
      properties: {
        friendly_name: null,
        serial_number: "shield-a",
        model: "Completely Different Model",
        manufacturer: "Completely Different Vendor",
      },
    }),
  );

  assert.equal(result.mismatch, false);
  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 1);
  assert.equal(rows[0].hardwareId, "shield-a");
  // Hardware-identified rows don't carry a fingerprint -- the id is already
  // verified identity, so there is nothing for it to add.
  assert.equal(rows[0].fingerprint, undefined);
});

test("a stored fingerprint survives a read/normalize round trip and sanitizes stray fields", () => {
  seed([
    saved({
      hardwareId: undefined,
      fingerprint: {
        model: " Shield TV Pro ",
        manufacturer: "NVIDIA",
        deviceCodename: "",
        extra: "should be dropped",
      },
    }),
  ]);

  const rows = savedDevices.listSavedDevices();
  assert.equal(rows.length, 1);
  assert.deepEqual(rows[0].fingerprint, {
    model: "Shield TV Pro",
    manufacturer: "NVIDIA",
  });
});
