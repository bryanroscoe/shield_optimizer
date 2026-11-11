#!/usr/bin/env node
// Usage: node v2/scripts/version-newer.mjs CANDIDATE CURRENT
//
// Exits 0 when CANDIDATE is newer than CURRENT, 1 when it is not, 2 on bad
// usage. Uses isNewerVersion from src/lib/version.ts, the same rule as the
// app's update check, so the release workflow never publishes an updater
// manifest or Homebrew cask that installed copies would see as a downgrade.
// A leading `v2-` on either argument is ignored.
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
const [candidate, current] = process.argv.slice(2).map((v) => v?.replace(/^v2-/, ""));
if (!candidate || !current || !SEMVER.test(candidate) || !SEMVER.test(current)) {
  console.error("usage: version-newer.mjs CANDIDATE CURRENT (each X.Y.Z[-pre], optional v2- prefix)");
  process.exit(2);
}
const newer = isNewerVersion(candidate, current);
console.log(`${candidate} is ${newer ? "newer than" : "not newer than"} ${current}`);
process.exit(newer ? 0 : 1);
