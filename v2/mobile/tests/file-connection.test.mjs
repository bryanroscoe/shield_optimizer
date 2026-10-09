import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server;
let matches;
before(async () => {
  ({ server } = await startViteServer());
  ({ fileConnectionMatches: matches } = await server.ssrLoadModule("/src/lib/appFiles.ts"));
});
after(async () => server?.close());

const target = { serial: "A:5555", generation: 10 };

test("file results belong to the live connection that started them", () => {
  assert.equal(matches(target, { ...target, isConnected: true }), true);
  assert.equal(matches(target, { ...target, serial: "B:5555", isConnected: true }), false);
  assert.equal(matches(target, { ...target, serial: null, isConnected: false }), false);
});

test("reconnecting to the same endpoint invalidates an old file action", () => {
  assert.equal(matches(target, { ...target, generation: 11, isConnected: true }), false);
});

test("a lost connection cannot accept a file result or delete confirmation", () => {
  assert.equal(matches(target, { ...target, isConnected: false }), false);
});
