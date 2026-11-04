import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { mock, test } from "node:test";

const typescriptModule = process.env.TYPESCRIPT_MODULE || "typescript";
const ts = (await import(typescriptModule)).default;
const source = await readFile(new URL("../src/lib/networkWatch.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ES2022, target: ts.ScriptTarget.ES2022 },
}).outputText;
const { NETWORK_CHANGED_EVENT, createNetworkProbe, networkProbeDelayMs } = await import(
  `data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`
);

test("a lost network gets longer to settle than a replacement", () => {
  assert.equal(networkProbeDelayMs("lost"), 4_000);
  for (const reason of ["changed", "addresses", undefined, "something-new"]) {
    assert.equal(networkProbeDelayMs(reason), 1_000, String(reason));
  }
});

test("a burst of network changes probes once, after the last", (t) => {
  mock.timers.enable({ apis: ["setTimeout"] });
  t.after(() => mock.timers.reset());
  let probes = 0;
  const probe = createNetworkProbe(() => probes++);
  probe.notify("lost");
  mock.timers.tick(3_000);
  probe.notify("changed");
  mock.timers.tick(999);
  assert.equal(probes, 0, "the replacement network restarted the wait");
  probe.notify("addresses");
  mock.timers.tick(999);
  assert.equal(probes, 0);
  mock.timers.tick(1);
  assert.equal(probes, 1);
  mock.timers.tick(10_000);
  assert.equal(probes, 1);
});

test("a lost network with no replacement still probes", (t) => {
  mock.timers.enable({ apis: ["setTimeout"] });
  t.after(() => mock.timers.reset());
  let probes = 0;
  createNetworkProbe(() => probes++).notify("lost");
  mock.timers.tick(3_999);
  assert.equal(probes, 0);
  mock.timers.tick(1);
  assert.equal(probes, 1);
});

test("cancel drops a pending probe", (t) => {
  mock.timers.enable({ apis: ["setTimeout"] });
  t.after(() => mock.timers.reset());
  let probes = 0;
  const probe = createNetworkProbe(() => probes++);
  probe.notify("changed");
  probe.cancel();
  mock.timers.tick(5_000);
  assert.equal(probes, 0);
});

test("the event name and reasons match what the plugin emits", async () => {
  const lib = await readFile(new URL("../tauri-plugin-atv-adb/src/lib.rs", import.meta.url), "utf8");
  assert.equal(lib.match(/NETWORK_CHANGED_EVENT: &str = "([^"]+)"/)?.[1], NETWORK_CHANGED_EVENT);
  const kotlin = await readFile(
    new URL(
      "../tauri-plugin-atv-adb/android/src/main/java/app/tauri/atvadb/NetworkChangeTracker.kt",
      import.meta.url,
    ),
    "utf8",
  );
  const reasons = [...kotlin.matchAll(/^\s+[A-Z]+\("([a-z]+)"\),$/gm)].map((m) => m[1]);
  assert.deepEqual(reasons, ["lost", "changed", "addresses"]);
});
