// The release workflow refuses to publish an updater manifest or Homebrew cask
// that is strictly older than the one already live (two close tags could
// otherwise roll every installed copy back), but republishes an equal one: a
// re-dispatched tag re-signs every asset and the manifest must carry the new
// signatures. This checks the CLI it calls.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const v2Root = dirname(dirname(fileURLToPath(import.meta.url)));
const script = join(v2Root, "scripts/version-newer.mjs");
const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: "utf8" }).status;

assert.equal(run("2.5.1", "2.5.0"), 0);
assert.equal(run("v2-2.6.0", "2.5.0"), 0, "a tag name is accepted");
assert.equal(run("2.5.10", "2.5.9"), 0, "numeric, not lexical");
assert.equal(run("2.6.0", "2.6.0-rc.2"), 0, "stable beats its own pre-release");
assert.equal(run("2.6.0-beta.10", "2.6.0-beta.9"), 0);

assert.equal(run("2.5.0", "2.5.0"), 1, "the same version is not newer");
assert.equal(run("2.5.0", "2.5.1"), 1, "a rollback is refused");
assert.equal(run("2.6.0-rc.1", "2.6.0"), 1);

// The mode the workflow uses: only a strictly older candidate is refused.
assert.equal(run("--or-equal", "2.5.0", "2.5.0"), 0, "re-dispatching the live tag republishes");
assert.equal(run("--or-equal", "v2-2.5.0", "2.5.0"), 0);
assert.equal(run("--or-equal", "2.5.1", "2.5.0"), 0);
assert.equal(run("--or-equal", "2.5.0", "2.5.1"), 1, "a rollback is still refused");
assert.equal(run("--or-equal", "2.6.0-rc.1", "2.6.0"), 1);
assert.equal(run("--or-equal", "2.5.0"), 2);

assert.equal(run("2.5.0"), 2);
assert.equal(run("latest", "2.5.0"), 2);
assert.equal(run("2.5.0", ""), 2);
assert.equal(run("2.5.0", "2.5.0", "2.5.0"), 2);

// Both publishing jobs gate on it in the non-refusing-equal mode.
const workflow = readFileSync(join(v2Root, "../.github/workflows/v2-release.yml"), "utf8");
const gates = workflow.match(/version-newer\.mjs"? --or-equal "/g) ?? [];
assert.equal(gates.length, 2, "merge-updater and bump-tap both gate with --or-equal");
assert.ok(!/version-newer\.mjs"? "\$/.test(workflow), "no gate refuses an equal version");

console.log("release version gate: ok");
