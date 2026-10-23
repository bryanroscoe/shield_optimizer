import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { runInNewContext } from "node:vm";

const v2Root = dirname(dirname(fileURLToPath(import.meta.url)));
const catalog = JSON.parse(
  readFileSync(join(v2Root, "src/lib/app-files-catalog.json"), "utf8"),
);

const ids = new Set();
for (const entry of catalog) {
  const label = entry.id ?? entry.package ?? "Catalog entry";
  // `id` keys the UI rows and the results map. It is deliberately separate
  // from `package`, which must be the installed package id — not a string from
  // the app's backup *filename* (SmartTube's is org.smarttube.stable) — or
  // anything that filters this catalog by installed packages silently drops
  // the entry.
  assert.ok(
    typeof entry.id === "string" && entry.id.trim() !== "",
    `${label} must have an id`,
  );
  assert.ok(!ids.has(entry.id), `duplicate catalog id: ${entry.id}`);
  ids.add(entry.id);
  assert.ok(
    typeof entry.package === "string" && /^[a-z][a-z0-9_]*(\.[a-z0-9_]+)+$/i.test(entry.package),
    `${label} must carry a real installed package id, got ${entry.package}`,
  );
  assert.ok(
    Array.isArray(entry.search_dirs) && entry.search_dirs.length > 0,
    `${label} must have at least one search directory`,
  );
  assert.ok(
    entry.search_dirs.every(
      (directory) => typeof directory === "string" && directory.startsWith("/sdcard"),
    ),
    // find_files refuses anything outside /sdcard, so one dir elsewhere fails
    // the entry's whole search.
    `${label} has a search directory outside /sdcard`,
  );
  assert.ok(
    typeof entry.pattern === "string" && entry.pattern.trim() !== "",
    `${label} must have a search pattern`,
  );
}

const smartTube = catalog.find((entry) => entry.id === "smarttube");
assert.ok(smartTube, "SmartTube entry is present");
assert.equal(
  smartTube.package,
  "com.teamsmart.videomanager.tv",
  "SmartTube Stable's installed package id",
);

// #86 reports the backup at /storage/emulated/0/Documents/SmartTubeBackup,
// which is /sdcard/Documents/SmartTubeBackup. The legacy app-data location
// stays so older builds keep working.
const expectedDirs = [
  "/sdcard/Documents/SmartTubeBackup",
  "/sdcard/SmartTubeBackup",
  "/sdcard/Android/data/com.teamsmart.videomanager.tv",
];
assert.deepEqual(smartTube.search_dirs, expectedDirs);
assert.equal(smartTube.pattern, "*.zip");

const component = readFileSync(
  join(v2Root, "src/lib/components/FilesTab.svelte"),
  "utf8",
);
const handlerStart = component.indexOf("  async function findAppFiles(");
const handlerEnd = component.indexOf(
  "  async function downloadFoundFile(",
  handlerStart,
);
assert.ok(
  handlerStart >= 0 && handlerEnd > handlerStart,
  "Exact findAppFiles handler source boundaries found",
);
const handler = stripTypeScriptTypes(
  component.slice(handlerStart, handlerEnd),
  { mode: "strip" },
);

const resultPath =
  "/sdcard/Documents/SmartTubeBackup/org.smarttube.stable_20260908.zip";
const calls = [];
const api = {
  findFiles: async (...args) => {
    calls.push(args);
    assert.equal(args[0], "synthetic-tv");
    assert.deepEqual(args[1], expectedDirs);
    assert.equal(args[2], "*.zip");
    return { hits: [resultPath], unsearched: [] };
  },
};

const exercise = runInNewContext(
  `(async entry => {
    const serial = "synthetic-tv";
    let appFilesBusy = null;
    let filesMessage = "";
    const appFilesResults = {};
    ${handler}
    await findAppFiles(entry);
    return { appFilesBusy, filesMessage, found: appFilesResults[entry.id] };
  })`,
  { api },
);
const result = await exercise(smartTube);

assert.equal(calls.length, 1);
assert.deepEqual(result.found, { hits: [resultPath], unsearched: [] });
assert.equal(result.appFilesBusy, null);
assert.equal(result.filesMessage, "");

// A search that could not run must stay distinguishable from one that ran and
// found nothing — the UI says different things about them.
const failing = {
  findFiles: async () => ({ hits: [], unsearched: expectedDirs.slice(0, 1) }),
};
const exerciseFailure = runInNewContext(
  `(async entry => {
    const serial = "synthetic-tv";
    let appFilesBusy = null;
    let filesMessage = "";
    const appFilesResults = {};
    ${handler}
    await findAppFiles(entry);
    return appFilesResults[entry.id];
  })`,
  { api: failing },
);
const failed = await exerciseFailure(smartTube);
assert.deepEqual(failed.hits, []);
assert.deepEqual(failed.unsearched, ["/sdcard/Documents/SmartTubeBackup"]);

console.log(
  "App-files catalog passed: ids are unique, packages are real package ids, SmartTube forwards every backup directory with *.zip, and an unsearchable directory stays distinct from no matches.",
);
