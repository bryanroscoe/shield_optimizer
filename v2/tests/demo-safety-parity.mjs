// The demo layer behind the README screenshots gives every package on the
// app's protected and caution lists the verdict the app gives it, because it
// reads those tables from crates/core/src/engine/safety.rs. A hand-kept copy
// had two of each and showed Google Feedback as safe to remove. Browser
// simulation only.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { stripTypeScriptTypes } from "node:module";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
const source = await readFile(join(root, "src/lib/rust-safety-tables.ts"), "utf8");
const { rustSafetyTable } = await import(`data:text/javascript;base64,${Buffer.from(stripTypeScriptTypes(source)).toString("base64")}`);
const rust = await readFile(join(root, "crates/core/src/engine/safety.rs"), "utf8");

// Every entry is read, counted independently of the parser.
const block = (name) => rust.slice(rust.indexOf(`const ${name}:`), rust.indexOf("\n];", rust.indexOf(`const ${name}:`)));
const entries = (name) => block(name).match(/\(\n\s+"/g).length;
const protectedList = rustSafetyTable(rust, "NEVER_DISABLE");
const cautionList = rustSafetyTable(rust, "CAUTION");
assert.equal(Object.keys(protectedList).length, entries("NEVER_DISABLE"), "every protected entry is read");
assert.equal(Object.keys(cautionList).length, entries("CAUTION"), "every caution entry is read");
assert.ok(protectedList["com.android.systemui"]);
assert.match(cautionList["com.google.android.feedback"], /crash reports/);
assert.throws(() => rustSafetyTable("", "CAUTION"), /no CAUTION table/);
// Rust string escapes are read as Rust reads them.
const escaped = rustSafetyTable(
  'const CAUTION: &[(&str, &str)] = &[\n    (\n        "a.b",\n        "Say \\"no\\" \\u{2014} twice.\\nThen \\\n         stop.",\n    ),\n];',
  "CAUTION",
);
assert.deepEqual(escaped, { "a.b": 'Say "no" — twice.\nThen stop.' });

const priorDemo = process.env.VITE_DEMO;
process.env.VITE_DEMO = "1";
let server, browser;
try {
  const { createServer } = await import("vite");
  const { chromium } = await import("playwright");
  server = await createServer({ root, server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false } });
  await server.listen();
  browser = await chromium.launch();
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/`, { waitUntil: "networkidle" });
  const verdicts = await page.evaluate(async ([guarded, cautioned]) => {
    const { handle } = await import("/src/lib/demo-mock.ts");
    const out = {};
    for (const pkg of [...guarded, ...cautioned]) out[pkg] = handle("safety_info", { package: pkg });
    out.process = handle("process_safety_info", { process: "com.google.android.feedback:remote" });
    out.catalogProcess = handle("process_safety_info", { process: "com.android.gallery3d" });
    return out;
  }, [Object.keys(protectedList), Object.keys(cautionList)]);
  for (const [pkg, reason] of Object.entries(protectedList)) {
    assert.deepEqual(verdicts[pkg], { kind: "never_disable", reason, source: "protected_list" }, pkg);
  }
  for (const [pkg, reason] of Object.entries(cautionList)) {
    if (protectedList[pkg]) continue;
    assert.deepEqual(verdicts[pkg], { kind: "caution", reason, source: "caution_list" }, pkg);
  }
  assert.equal(verdicts.process.source, "caution_list", "a process gets the rule tables, before its :suffix");
  assert.equal(verdicts.catalogProcess.source, "no_record", "a process never inherits a catalog verdict");
  console.log(`Demo safety parity: ${Object.keys(protectedList).length} protected and ${Object.keys(cautionList).length} caution packages match the app. Browser simulation only.`);
} finally {
  await browser?.close();
  await server?.close();
  if (priorDemo === undefined) delete process.env.VITE_DEMO;
  else process.env.VITE_DEMO = priorDemo;
}
