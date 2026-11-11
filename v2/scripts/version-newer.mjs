#!/usr/bin/env node
// Usage: node v2/scripts/version-newer.mjs [--or-equal] CANDIDATE CURRENT
//
// Exits 0 when CANDIDATE is newer than CURRENT, 1 when it is not, 2 on bad
// usage. With --or-equal, an equal version also exits 0, so only a strictly
// older CANDIDATE exits 1. Uses isNewerVersion from src/lib/version.ts, the
// same rule as the app's update check.
//
// The release workflow calls it with --or-equal before committing the updater
// manifest and before bumping the Homebrew cask: a strictly older version
// (two close tags finishing out of order) is refused, so installed copies are
// never moved backwards. An equal version must go through: re-dispatching the
// current tag re-uploads every asset with fresh signatures, and the manifest
// has to carry them. A leading `v2-` on either argument is ignored.
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const v2Root = dirname(dirname(fileURLToPath(import.meta.url)));
const source = readFileSync(join(v2Root, "src/lib/version.ts"), "utf8");
const { isNewerVersion } = await import(
  "data:text/javascript," + encodeURIComponent(stripTypeScriptTypes(source, { mode: "strip" }))
);

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;
let args = process.argv.slice(2);
const orEqual = args[0] === "--or-equal";
if (orEqual) args = args.slice(1);
const [candidate, current] = args.map((v) => v?.replace(/^v2-/, ""));
if (args.length !== 2 || !candidate || !current || !SEMVER.test(candidate) || !SEMVER.test(current)) {
  console.error(
    "usage: version-newer.mjs [--or-equal] CANDIDATE CURRENT (each X.Y.Z[-pre], optional v2- prefix)",
  );
  process.exit(2);
}
const newer = isNewerVersion(candidate, current);
const older = isNewerVersion(current, candidate);
const ok = newer || (orEqual && !older);
const relation = newer ? "newer than" : older ? "older than" : "the same as";
console.log(`${candidate} is ${relation} ${current}`);
process.exit(ok ? 0 : 1);
