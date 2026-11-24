// CleanYourTV's lists as the fallback for apps the catalog has no record of.
//
// The shared rules (matching a TV to the lists that name it, which lines turn
// an app off, the author's note, the per-TV choice and the rows Optimize gets)
// run directly against the bundled catalog. Then the desktop Optimize tab is
// driven against the demo Shield: the list is only suggested until chosen,
// its rows start on Keep and offer Disable alone, they sit under their own
// heading, the choice is filed under the TV's hardware id, and Turn off
// removes them again. Browser simulation only.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { mkdir, readFile } from "node:fs/promises";
import { stripTypeScriptTypes } from "node:module";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
const source = await readFile(join(root, "shared/cleanyourtv-lists.ts"), "utf8");
const lists = await import(`data:text/javascript;base64,${Buffer.from(stripTypeScriptTypes(source)).toString("base64")}`);
const data = JSON.parse(await readFile(join(root, "catalogs/cleanyourtv/catalog.json"), "utf8"));
const profile = (id) => data.profiles.find((p) => p.profile === id);
const ids = (candidates) => candidates.map((c) => c.profile.profile);

// Matching: a codename names one TV; a model alone needs the same maker.
const shield = { manufacturer: "NVIDIA", brand: "NVIDIA", model: "SHIELD Android TV", device_codename: "mdarcy" };
assert.equal(ids(lists.candidateProfiles(data.profiles, shield))[0], "nvidia-shield-tv-pro-2019");
assert.ok(!ids(lists.candidateProfiles(data.profiles, shield)).includes("nvidia-shield-tv-2017"), "darcy is not mdarcy");
const chromecast = { manufacturer: "Google", brand: "google", model: "Chromecast", device_codename: "sabrina" };
assert.equal(ids(lists.candidateProfiles(data.profiles, chromecast))[0], "google-chromecast-gtv-4k");
assert.deepEqual(
  ids(lists.candidateProfiles(data.profiles, { manufacturer: "Acme", brand: "Acme", model: "Chromecast", device_codename: "" })),
  [],
  "a model name from another maker matches nothing",
);
assert.deepEqual(ids(lists.candidateProfiles(data.profiles, { manufacturer: "NVIDIA", brand: "NVIDIA" })), [], "brand alone matches nothing");
assert.deepEqual(ids(lists.candidateProfiles(data.profiles, {})), []);
assert.equal(
  lists.matchReasonText(lists.candidateProfiles(data.profiles, shield)[0], shield),
  "Names your TV's device mdarcy",
);

// Which lines turn an app off: active entries only, and never one the same list
// also keeps commented out.
const pro = profile("nvidia-shield-tv-pro-2019");
const listed = lists.listedForDisable(pro);
assert.ok(listed.length > 0);
const kept = new Set(pro.entries.filter((e) => e.upstream_form === "commented_out").map((e) => e.package));
for (const app of listed) {
  assert.ok(pro.entries.some((e) => e.package === app.package && e.line === app.line && e.upstream_form === "active_disable"));
  assert.ok(!kept.has(app.package));
}
assert.equal(new Set(listed.map((a) => a.package)).size, listed.length, "one row per package");
const conflicted = {
  ...pro,
  entries: [
    { package: "com.example.both", line: 3, upstream_form: "active_disable" },
    { package: "com.example.both", line: 9, upstream_form: "commented_out" },
    { package: "com.example.off", line: 4, upstream_form: "active_disable" },
    { package: "com.example.note", line: 5, upstream_form: "comment_reference" },
  ],
};
assert.deepEqual(lists.listedForDisable(conflicted), [{ package: "com.example.off", line: 4 }]);

// The author's note: the line's own comment, else the comment over its group;
// commented-out package lines are never notes.
const text = [
  "# Header",
  "",
  "# Preloaded apps",
  "com.example.a",
  "com.example.b     # the B app",
  "# The next two are modules: re-enable if",
  "# the Play Store misbehaves.",
  "com.example.c",
  "",
  "# Stock home",
  "# com.example.launcher",
  "com.example.d",
  "",
  "com.example.e",
].join("\n");
assert.equal(lists.sourceNote(text, 4), "Preloaded apps");
assert.equal(lists.sourceNote(text, 5), "the B app");
assert.equal(lists.sourceNote(text, 8), "The next two are modules: re-enable if the Play Store misbehaves.");
assert.equal(lists.sourceNote(text, 12), "Stock home");
assert.equal(lists.sourceNote(text, 14), "");
const proText = await readFile(join(root, "catalogs/cleanyourtv/upstream", pro.source), "utf8");
const autoinstall = listed.find((a) => a.package === "android.autoinstalls.config.nvidia");
assert.ok(autoinstall, "the demo's list row is on the Shield Pro list");
assert.equal(lists.sourceNote(proText, autoinstall.line), "Play auto-installs of partner apps");

// The choice is filed under a verified hardware id and nothing else.
const store = new Map();
const storage = {
  getItem: (k) => (store.has(k) ? store.get(k) : null),
  setItem: (k, v) => store.set(k, String(v)),
};
assert.equal(lists.savedListChoice(storage, "ABC"), null);
assert.equal(lists.saveListChoice(storage, "ABC", "nvidia-shield-tv-pro-2019"), true);
assert.equal(lists.savedListChoice(storage, " ABC "), "nvidia-shield-tv-pro-2019");
assert.equal(lists.savedListChoice(storage, "XYZ"), null, "another TV has its own choice");
assert.equal(lists.saveListChoice(storage, null, "x"), false, "a TV with no id is never remembered");
assert.equal(lists.saveListChoice(storage, "unknown", "x"), false);
assert.equal(lists.saveListChoice(storage, "ABC", lists.LIST_OFF), true);
assert.equal(lists.savedListChoice(storage, "ABC"), "off");
lists.saveListChoice(storage, "ABC", null);
assert.equal(lists.savedListChoice(storage, "ABC"), null);
const refusing = { getItem: () => "{}", setItem: () => { throw new Error("quota"); } };
assert.equal(lists.saveListChoice(refusing, "ABC", "x"), false);

// Rows: installed, not covered, enabled for Optimize and disabled for Restore,
// shaped as "remove if unused" entries that can only be disabled.
const installed = [
  { package: "android.autoinstalls.config.nvidia", enabled: true, name: null },
  { package: listed[0].package, enabled: false, name: "Disabled one" },
];
const urlFor = (line) => `https://example.test/list#L${line}`;
const optimizeRows = lists.listedPlanItems(pro, proText, urlFor, installed, new Set(), "optimize");
assert.deepEqual(optimizeRows.map((r) => r.entry.package), ["android.autoinstalls.config.nvidia"]);
const row = optimizeRows[0];
assert.deepEqual(row.action, { kind: "disable" });
assert.equal(row.entry.name, "android.autoinstalls.config.nvidia");
assert.equal(row.entry.method, "disable");
assert.equal(row.entry.review, true);
assert.equal(row.entry.default_optimize, false);
assert.equal(row.entry.play_store, false);
assert.equal(row.entry.optimize_description, "Play auto-installs of partner apps");
assert.deepEqual(row.entry.listed_by, {
  profile: "nvidia-shield-tv-pro-2019",
  tv: pro.upstream_context.tv,
  line: autoinstall.line,
  url: `https://example.test/list#L${autoinstall.line}`,
});
assert.deepEqual(
  lists.listedPlanItems(pro, proText, urlFor, installed, new Set(["android.autoinstalls.config.nvidia"]), "optimize"),
  [],
  "a package the catalog covers never gets a list row",
);
const restoreRows = lists.listedPlanItems(pro, proText, urlFor, installed, new Set(), "restore");
assert.deepEqual(restoreRows.map((r) => [r.entry.package, r.action.kind, r.entry.name]), [[listed[0].package, "enable", "Disabled one"]]);

// Desktop Optimize against the demo Shield.
const priorDemo = process.env.VITE_DEMO;
process.env.VITE_DEMO = "1";
const evidence = process.env.CLEANYOURTV_EVIDENCE;
let server, browser, context;
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  const { createServer } = await import("vite");
  const { chromium } = await import("playwright");
  server = await createServer({ root, server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false } });
  await server.listen();
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  const deviceUrl = `${base}/devices/${encodeURIComponent("192.168.1.42:5555")}`;
  browser = await chromium.launch();
  context = await browser.newContext({ viewport: { width: 1280, height: 900 }, colorScheme: "dark" });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route(/plugin-opener/, (r) => r.fulfill({ contentType: "application/javascript", body: "export async function openUrl(url){(window.openedSources??=[]).push(url)} export async function revealItemInDir(){}" }));
  await page.route(/plugin-updater/, (r) => r.fulfill({ contentType: "application/javascript", body: "export async function check(){return null}" }));
  await page.route(/plugin-process/, (r) => r.fulfill({ contentType: "application/javascript", body: "export async function relaunch(){}" }));
  await page.addInitScript(() => {
    if (!sessionStorage.getItem("cytv-test-started")) {
      localStorage.clear();
      sessionStorage.setItem("cytv-test-started", "1");
    }
  });
  await page.goto(deviceUrl, { waitUntil: "networkidle" });
  await page.locator("#tab-optimize").click();
  const opt = page.locator("#tabpanel-optimize");
  const card = opt.locator("[data-cleanyourtv-card]");
  await card.getByRole("button", { name: "Use this list" }).waitFor();
  assert.match(await card.innerText(), /NVIDIA SHIELD TV Pro/i, "the card names the matching list");
  assert.equal(await opt.locator("[data-cleanyourtv-rows]").count(), 0, "nothing from the list shows before it is chosen");
  if (evidence) await card.screenshot({ path: join(evidence, "desktop-card-suggested.png") });

  await card.getByRole("button", { name: "Use this list" }).click();
  const group = opt.locator("[data-cleanyourtv-rows]");
  await group.waitFor();
  assert.match(await group.locator(".listed-heading").innerText(), /^From the CleanYourTV list for NVIDIA SHIELD TV Pro/, "the heading names the list in normal case");
  const listedRow = group.locator("tr", { has: page.locator(".pkg-id", { hasText: "android.autoinstalls.config.nvidia" }) }).first();
  await listedRow.waitFor();
  const options = await listedRow.locator(".action-radio .radio-pill").evaluateAll((els) =>
    els.map((el) => ({ label: el.querySelector(".radio-label")?.textContent?.trim(), active: el.classList.contains("active") })),
  );
  assert.deepEqual(options.map((o) => o.label), ["Keep", "Disable"], "a list row offers Disable alone, never Uninstall");
  assert.deepEqual(options.filter((o) => o.active).map((o) => o.label), ["Keep"], "a list row starts on Keep");
  assert.match(await listedRow.innerText(), /Play auto-installs of partner apps/);
  assert.match(await listedRow.innerText(), /Unknown/i, "the verdict is still the app's own");
  await listedRow.getByRole("button", { name: /^Line \d+ of the list$/ }).click();
  const opened = await page.evaluate(() => window.openedSources ?? []);
  assert.match(opened.at(-1) ?? "", /^https:\/\/github\.com\/Gathin23\/cleanyourtv\/blob\/[0-9a-f]{40}\/examples\/nvidia-shield-tv-pro-2019\/debloat\.txt#L\d+$/);
  assert.match(await card.innerText(), /1 more app from/);
  const stored = await page.evaluate(() => JSON.parse(localStorage.getItem("atvopt.cleanyourtvList") ?? "{}"));
  assert.deepEqual(stored, { "0323220012345": "nvidia-shield-tv-pro-2019" }, "filed under the hardware id");
  if (evidence) {
    await card.screenshot({ path: join(evidence, "desktop-card-chosen.png") });
    await group.screenshot({ path: join(evidence, "desktop-list-rows.png") });
  }

  // The choice survives a reload; Restore has nothing from the list while the app is on.
  await page.reload({ waitUntil: "networkidle" });
  await page.locator("#tab-optimize").click();
  await opt.locator("[data-cleanyourtv-rows]").waitFor();
  await opt.getByRole("button", { name: "Restore", exact: true }).click();
  await opt.locator("[data-cleanyourtv-card]").waitFor();
  await page.waitForTimeout(500);
  assert.equal(await opt.locator("[data-cleanyourtv-rows]").count(), 0, "an enabled list app is not a Restore row");
  await opt.getByRole("button", { name: "Optimize", exact: true }).click();
  await opt.locator("[data-cleanyourtv-rows]").waitFor();

  await opt.locator("[data-cleanyourtv-card]").getByRole("button", { name: "Turn off" }).click();
  await page.waitForTimeout(300);
  assert.equal(await opt.locator("[data-cleanyourtv-rows]").count(), 0, "Turn off removes the list's rows");
  assert.match(await opt.locator("[data-cleanyourtv-card]").innerText(), /Off for this TV/);
  assert.deepEqual(await page.evaluate(() => JSON.parse(localStorage.getItem("atvopt.cleanyourtvList") ?? "{}")), { "0323220012345": "off" });

  // Choose a list by search when the suggestion isn't the one.
  await opt.locator("[data-cleanyourtv-card]").getByRole("button", { name: "Choose a list" }).click();
  await page.getByRole("searchbox", { name: "Search CleanYourTV lists" }).fill("shield tv 2017");
  await page.getByRole("radio", { name: /SHIELD TV 2017 \(darcy\)/i }).check();
  if (evidence) await opt.locator("[data-cleanyourtv-card]").screenshot({ path: join(evidence, "desktop-card-choosing.png") });
  await opt.locator("[data-cleanyourtv-card]").getByRole("button", { name: "Use this list" }).click();
  await page.waitForTimeout(500);
  assert.equal(
    (await page.evaluate(() => JSON.parse(localStorage.getItem("atvopt.cleanyourtvList") ?? "{}")))["0323220012345"],
    "nvidia-shield-tv-2017",
  );
  assert.deepEqual(errors, []);
  console.log("CleanYourTV lists: matching, list lines, notes, per-TV choice and the desktop Optimize rows passed. Browser simulation only.");
} finally {
  await context?.close();
  await browser?.close();
  await server?.close();
  if (priorDemo === undefined) delete process.env.VITE_DEMO;
  else process.env.VITE_DEMO = priorDemo;
}
