import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import ts from "typescript";
import test from "node:test";
const source = readFileSync(new URL("../src/lib/log-poller.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
}).outputText;
const { LogPoller } = await import("data:text/javascript," + encodeURIComponent(compiled));
const flush = async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); };
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };

function harness(t, read) {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  let context = "tv:1", calls = 0, latest;
  const poller = new LogPoller(() => { ++calls; return read(); }, () => context, next => { latest = next; });
  t.after(() => poller.dispose());
  return { poller, get calls() { return calls; }, get latest() { return latest; }, setContext(value) { context = value; } };
}

test("reads only on demand and never overlaps a pending read", async t => {
  const pending = deferred(); const h = harness(t, () => pending.promise);
  assert.equal(h.calls, 0);
  const first = h.poller.once(); h.poller.once(); h.poller.start();
  assert.equal(h.calls, 1);
  pending.resolve("snapshot"); await first;
  assert.equal(h.latest.value, "snapshot");
  assert.equal(h.latest.busy, false);
  t.mock.timers.tick(9000); assert.equal(h.calls, 1);
});

test("refresh delay starts after completion, then stop cancels the timer", async t => {
  const pending = deferred(); const h = harness(t, () => pending.promise);
  h.poller.start(); t.mock.timers.tick(9000); assert.equal(h.calls, 1);
  pending.resolve("one"); await flush();
  t.mock.timers.tick(2999); assert.equal(h.calls, 1);
  t.mock.timers.tick(1); await flush(); assert.equal(h.calls, 2);
  h.poller.stop(); t.mock.timers.tick(9000); assert.equal(h.calls, 2);
});

test("stop drops an in-flight result and prevents automatic restart", async t => {
  const pending = deferred(); const h = harness(t, () => pending.promise);
  h.poller.start(); h.poller.stop(); pending.resolve("late private logs"); await flush();
  assert.equal(h.latest.value, null); assert.equal(h.latest.running, false);
  assert.equal(h.latest.busy, false); t.mock.timers.tick(9000); assert.equal(h.calls, 1);
});

test("device generation changes drop late data even before UI cleanup", async t => {
  const pending = deferred(); const h = harness(t, () => pending.promise);
  h.poller.start(); h.setContext("tv:2"); pending.resolve("old TV logs"); await flush();
  assert.equal(h.latest.value, null); assert.equal(h.latest.running, false);
  t.mock.timers.tick(9000); assert.equal(h.calls, 1);
});

test("a hidden view cannot start or continue polling", async t => {
  const h = harness(t, () => Promise.resolve("one"));
  h.setContext(null); h.poller.start(); await h.poller.once(); assert.equal(h.calls, 0);
  h.setContext("tv:1"); h.poller.start(); await flush();
  h.setContext(null); t.mock.timers.tick(3000); await flush();
  assert.equal(h.calls, 1); assert.equal(h.latest.running, false);
});

test("an unsupported or failed read stops retries and shows the error", async t => {
  const h = harness(t, () => Promise.reject(new Error("not supported")));
  h.poller.start(); await flush(); assert.match(h.latest.error, /not supported/);
  assert.equal(h.latest.running, false); t.mock.timers.tick(9000); assert.equal(h.calls, 1);
});

test("destroying the view prevents both late publication and future reads", async t => {
  const pending = deferred(); const h = harness(t, () => pending.promise);
  h.poller.start(); const before = h.latest;
  h.poller.dispose(); pending.resolve("private"); await flush();
  assert.equal(h.latest, before); h.poller.start(); await h.poller.once();
  t.mock.timers.tick(9000); assert.equal(h.calls, 1);
});
