// The release workflow refuses to publish an updater manifest or Homebrew cask
// that is not newer than the one already live (two close tags could otherwise
// roll every installed copy back). This checks the CLI it calls.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const script = join(dirname(dirname(fileURLToPath(import.meta.url))), "scripts/version-newer.mjs");
const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: "utf8" }).status;

assert.equal(run("2.5.1", "2.5.0"), 0);
assert.equal(run("v2-2.6.0", "2.5.0"), 0, "a tag name is accepted");
assert.equal(run("2.5.10", "2.5.9"), 0, "numeric, not lexical");
assert.equal(run("2.6.0", "2.6.0-rc.2"), 0, "stable beats its own pre-release");
assert.equal(run("2.6.0-beta.10", "2.6.0-beta.9"), 0);

assert.equal(run("2.5.0", "2.5.0"), 1, "the same version is not newer");
assert.equal(run("2.5.0", "2.5.1"), 1, "a rollback is refused");
assert.equal(run("2.6.0-rc.1", "2.6.0"), 1);

assert.equal(run("2.5.0"), 2);
assert.equal(run("latest", "2.5.0"), 2);
assert.equal(run("2.5.0", ""), 2);

console.log("release version gate: ok");
