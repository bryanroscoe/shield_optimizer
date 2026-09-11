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

for (const entry of catalog) {
  assert.ok(
    Array.isArray(entry.search_dirs) && entry.search_dirs.length > 0,
    `${entry.package ?? "Catalog entry"} must have at least one search directory`,
  );
  assert.ok(
    entry.search_dirs.every(
      (directory) => typeof directory === "string" && directory.trim() !== "",
    ),
    `${entry.package ?? "Catalog entry"} has an empty search directory`,
  );
  assert.ok(
    typeof entry.pattern === "string" && entry.pattern.trim() !== "",
    `${entry.package ?? "Catalog entry"} must have a search pattern`,
  );
}

const smartTube = catalog.find(
  (entry) => entry.package === "org.smarttube.stable",
);
assert.ok(smartTube, "Current SmartTube package is present");

const expectedDirs = [
  "/sdcard/Documents/SmartTubeBackup",
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
    return [resultPath];
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
    return { appFilesBusy, filesMessage, found: appFilesResults[entry.package] };
  })`,
  { api },
);
const result = await exercise(smartTube);

assert.equal(calls.length, 1);
assert.deepEqual(result.found, [resultPath]);
assert.equal(result.appFilesBusy, null);
assert.equal(result.filesMessage, "");

console.log(
  "App-files catalog passed: every entry is searchable and SmartTube forwards both backup directories with *.zip.",
);
