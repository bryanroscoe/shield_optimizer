import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";

const typescriptModule = process.env.TYPESCRIPT_MODULE || "typescript";
const ts = (await import(typescriptModule)).default;
const source = await readFile(
  new URL("../src/lib/unknownDiagnostics.ts", import.meta.url),
  "utf8",
);
const compiled = ts.transpileModule(source, {
  compilerOptions: {
    module: ts.ModuleKind.ES2022,
    target: ts.ScriptTarget.ES2022,
  },
}).outputText;
const diagnostics = await import(
  `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`
);

class MemoryStorage {
  constructor(raw = null) {
    this.raw = raw;
    this.failSet = false;
    this.failRemove = false;
  }
  getItem() {
    return this.raw;
  }
  setItem(_key, value) {
    if (this.failSet) throw new Error("full");
    this.raw = value;
  }
  removeItem() {
    if (!this.failRemove) this.raw = null;
  }
}

const base = (overrides = {}) => ({
  kind: "installed_package",
  token: "com.example.unknown",
  reason: "uncatalogued_package",
  appVersion: "0.1.0",
  registryVersion: null,
  deviceFamily: "shield",
  deviceOs: "14",
  ...overrides,
});

test("records uncatalogued packages and unresolved native processes distinctly", () => {
  const store = new MemoryStorage();
  const collector = new diagnostics.UnknownDiagnostics(store, {
    now: () => new Date("2026-09-08T12:00:00.000Z"),
  });

  assert.equal(collector.observe(base()).ok, true);
  assert.equal(
    collector.observe(
      base({
        kind: "unresolved_process",
        token: "system",
        reason: "process_not_resolved",
      }),
    ).ok,
    true,
  );
  assert.equal(
    collector.observe(
      base({
        token: "com.example.unclassified",
        reason: "unknown_safety_classification",
      }),
    ).ok,
    true,
  );

  const report = collector.snapshot();
  assert.deepEqual(
    report.records.map(({ kind, token, reason }) => ({ kind, token, reason })),
    [
      {
        kind: "installed_package",
        token: "com.example.unknown",
        reason: "uncatalogued_package",
      },
      {
        kind: "unresolved_process",
        token: "system",
        reason: "process_not_resolved",
      },
      {
        kind: "installed_package",
        token: "com.example.unclassified",
        reason: "unknown_safety_classification",
      },
    ],
  );
});

test("deduplicates repeat polls, saturates counts, and separates device context", () => {
  const store = new MemoryStorage();
  const collector = new diagnostics.UnknownDiagnostics(store, { maxCount: 2 });

  collector.observe(base());
  collector.observe(base());
  collector.observe(base());
  collector.observe(base({ deviceFamily: "google_tv" }));

  const records = collector.snapshot().records;
  assert.equal(records.length, 2);
  assert.equal(records.find((r) => r.device_family === "shield").count, 2);
  assert.equal(records.find((r) => r.device_family === "google_tv").count, 1);
});

test("drops stale scheduled observations after a device changes", () => {
  const tasks = [];
  const collector = new diagnostics.UnknownDiagnostics(new MemoryStorage(), {
    schedule: (task) => tasks.push(task),
  });
  let current = true;

  collector.schedule(base(), () => current);
  current = false;
  tasks.shift()();

  assert.equal(collector.snapshot().records.length, 0);
});

test("rejects paths, addresses, control strings, and malformed package tokens", () => {
  const collector = new diagnostics.UnknownDiagnostics(new MemoryStorage());
  const rejected = [
    "192.168.1.9",
    "192.168.1.9:5555",
    "aa:bb:cc:dd:ee:ff",
    "/data/app/private",
    "com.example.bad\nlicense=secret",
    "not-a-package",
  ];
  for (const token of rejected) {
    assert.equal(collector.observe(base({ token })).ok, false, token);
  }
  assert.equal(
    collector.observe(base({ reason: "process_not_resolved" })).ok,
    false,
  );
  assert.equal(
    collector.observe(
      base({
        kind: "unresolved_process",
        token: "com.example.app:renderer",
        reason: "process_not_resolved",
      }),
    ).ok,
    true,
  );
  assert.equal(collector.snapshot().records.length, 1);
});

test("bounds record count and serialized byte size while marking truncation", () => {
  const store = new MemoryStorage();
  const collector = new diagnostics.UnknownDiagnostics(store, {
    maxRecords: 3,
    maxBytes: 900,
  });
  for (let i = 0; i < 8; ++i) {
    collector.observe(base({ token: `com.example.unknown${i}` }));
  }

  const report = collector.snapshot();
  assert.ok(report.records.length <= 3);
  assert.equal(report.truncated, true);
  assert.ok(Buffer.byteLength(store.raw, "utf8") <= 900);
  assert.equal(report.records.at(-1).token, "com.example.unknown7");
});

test("fails harmlessly for corrupt or full storage", () => {
  const corrupt = new MemoryStorage("{not-json");
  const corruptCollector = new diagnostics.UnknownDiagnostics(corrupt);
  assert.deepEqual(corruptCollector.snapshot().records, []);
  assert.match(corruptCollector.lastError, /Couldn't load/);
  assert.equal(corruptCollector.observe(base()).ok, true);

  const full = new MemoryStorage();
  full.failSet = true;
  const fullCollector = new diagnostics.UnknownDiagnostics(full);
  const result = fullCollector.observe(base());
  assert.equal(result.ok, false);
  assert.equal(result.message, "Couldn't save diagnostics on this device.");
  assert.deepEqual(fullCollector.snapshot().records, []);
});

test("exports an immutable exact snapshot with allowlisted fields only", () => {
  const collector = new diagnostics.UnknownDiagnostics(new MemoryStorage(), {
    now: () => new Date("2026-09-08T12:00:00.000Z"),
  });
  collector.observe(
    base({
      serial: "SECRET-SERIAL",
      host: "192.168.1.9",
      licenseKey: "ATVOPT-SECRET",
      rawInventory: ["com.private.one"],
    }),
  );
  const snapshot = collector.snapshot();
  const exported = collector.exportSnapshot(snapshot);
  collector.observe(base({ token: "com.example.later" }));

  assert.equal(snapshot.records.length, 1);
  assert.deepEqual(JSON.parse(exported), snapshot);
  assert.doesNotMatch(exported, /SECRET|192\.168|rawInventory|licenseKey|serial|host/);
  assert.deepEqual(Object.keys(snapshot.records[0]).sort(), [
    "app_version",
    "count",
    "device_family",
    "device_os",
    "first_seen",
    "kind",
    "last_seen",
    "reason",
    "registry_version",
    "token",
  ]);
});

test("clear prevents a queued old save from resurrecting records", () => {
  const tasks = [];
  const store = new MemoryStorage();
  const collector = new diagnostics.UnknownDiagnostics(store, {
    schedule: (task) => tasks.push(task),
  });
  collector.schedule(base());
  assert.equal(collector.clear().ok, true);
  tasks.shift()();
  assert.equal(store.raw, null);
  assert.deepEqual(collector.snapshot().records, []);
});

test("clear failures remain visible and do not claim deletion", () => {
  const store = new MemoryStorage();
  const collector = new diagnostics.UnknownDiagnostics(store);
  collector.observe(base());
  store.failRemove = true;

  const result = collector.clear();
  assert.equal(result.ok, false);
  assert.match(result.message, /Couldn't clear/);
  assert.equal(collector.snapshot().records.length, 1);
});
