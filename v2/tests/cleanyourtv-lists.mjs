// CleanYourTV's lists, ranked after the catalog for apps it has no record of.
//
// The shared rules (matching a TV to the lists that name it, which lists are
// used, which lines turn an app off and which list decides, the author's note,
// the per-TV choice) run directly against the bundled catalog. Then the
// desktop Optimize tab is driven against the demo Shield: its codename's list
// is used without asking, its rows sit under the list's name after the
// catalog, start on Keep and offer Disable alone, and the choice to turn it
// off or pick another list is filed under the TV's hardware id. Browser
// simulation only.
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
const ids = (items) => items.map((item) => (item.profile ? item.profile.profile ?? item.profile : item));
const text = (p) => readFile(join(root, "catalogs/cleanyourtv/upstream", p.source), "utf8");

// Matching. A codename names one device; when any list names it, model-only
// matches (a sibling with another codename) are dropped.
const shield = { manufacturer: "NVIDIA", brand: "NVIDIA", model: "SHIELD Android TV", device_codename: "mdarcy" };
assert.deepEqual(ids(lists.candidateProfiles(data.profiles, shield)), ["nvidia-shield-tv-pro-2019"]);
const boreal = { manufacturer: "Google", brand: "google", model: "Chromecast", device_codename: "boreal" };
assert.deepEqual(ids(lists.candidateProfiles(data.profiles, boreal)), ["google-chromecast-gtv-hd"], "the 4K list's model name doesn't make it the HD's");
const sabrina = { ...boreal, device_codename: "sabrina" };
assert.deepEqual(ids(lists.candidateProfiles(data.profiles, sabrina)), ["google-chromecast-gtv-4k"]);
// A codename with an underscore is one word: BRAVIA_VH2 is not BRAVIA_VH21.
const vh2 = { manufacturer: "Sony", brand: "Sony", model: "BRAVIA 4K VH2", device_codename: "BRAVIA_VH2" };
const vh2Lists = ids(lists.activeLists(data.profiles, vh2, null));
assert.ok(vh2Lists.length > 0, "an underscore codename matches");
assert.ok(vh2Lists.includes("sony-bravia-4k-vh2"));
assert.ok(!vh2Lists.some((id) => id.includes("vh21")), `VH21 lists don't name a VH2: ${vh2Lists}`);
assert.ok(
  !ids(lists.candidateProfiles(data.profiles, { ...vh2, device_codename: "" })).some((id) => id.includes("vh21")),
  "a model name matches whole words only",
);
// With no codename match, model matches from the same maker are suggestions only.
const modelOnly = { manufacturer: "Google", brand: "google", model: "Chromecast", device_codename: "" };
assert.ok(lists.candidateProfiles(data.profiles, modelOnly).length >= 2);
assert.deepEqual(lists.activeLists(data.profiles, modelOnly, null), [], "a model match is never used without a choice");
assert.deepEqual(
  ids(lists.candidateProfiles(data.profiles, { manufacturer: "Acme", brand: "Acme", model: "Chromecast", device_codename: "" })),
  [],
  "a model name from another maker matches nothing",
);
assert.deepEqual(ids(lists.candidateProfiles(data.profiles, { manufacturer: "NVIDIA", brand: "NVIDIA" })), [], "brand alone matches nothing");
assert.deepEqual(ids(lists.candidateProfiles(data.profiles, {})), []);
assert.equal(lists.matchReasonText(lists.candidateProfiles(data.profiles, shield)[0], shield), "Names your TV's device mdarcy");

// Which lists are used: the codename's by default, the picked one alone, none when off.
assert.deepEqual(ids(lists.activeLists(data.profiles, shield, null)), ["nvidia-shield-tv-pro-2019"]);
assert.deepEqual(ids(lists.activeLists(data.profiles, shield, "nvidia-shield-tv-2017")), ["nvidia-shield-tv-2017"]);
assert.deepEqual(lists.activeLists(data.profiles, shield, lists.LIST_OFF), []);
assert.deepEqual(ids(lists.activeLists(data.profiles, shield, "no-such-list")), ["nvidia-shield-tv-pro-2019"], "a stale pick falls back to the suggestion");

// Which lines turn an app off: active entries only, and never one the same list
// also keeps commented out.
const pro = profile("nvidia-shield-tv-pro-2019");
const listed = lists.listedForDisable(pro);
assert.ok(listed.length > 0);
const kept = new Set(pro.entries.filter((e) => e.upstream_form === "commented_out").map((e) => e.package));
for (const app of listed) assert.ok(!kept.has(app.package));
assert.equal(new Set(listed.map((a) => a.package)).size, listed.length, "one row per package");

// The author's note: the line's own comment, else the comment over its group;
// commented-out package lines are never notes.
const sample = [
  "# Header", "", "# Preloaded apps", "com.example.a", "com.example.b     # the B app",
  "# The next two are modules: re-enable if", "# the Play Store misbehaves.", "com.example.c", "",
  "# Stock home", "# com.example.launcher", "com.example.d", "", "com.example.e",
].join("\n");
assert.equal(lists.sourceNote(sample, 4), "Preloaded apps");
assert.equal(lists.sourceNote(sample, 5), "the B app");
assert.equal(lists.sourceNote(sample, 8), "The next two are modules: re-enable if the Play Store misbehaves.");
assert.equal(lists.sourceNote(sample, 12), "Stock home");
assert.equal(lists.sourceNote(sample, 14), "");

// Ranking: the catalog first, then each list; the first list that mentions an
// app decides it, a disable line making a row and a kept line making none.
const top = {
  ...pro,
  profile: "top",
  upstream_context: { ...pro.upstream_context, tv: "Top TV" },
  entries: [
    { package: "com.example.kept-on-top", line: 1, upstream_form: "commented_out" },
    { package: "com.example.from-top", line: 2, upstream_form: "active_disable" },
    { package: "com.example.in-catalog", line: 3, upstream_form: "active_disable" },
    { package: "com.example.note-only", line: 4, upstream_form: "comment_reference" },
  ],
};
const second = {
  ...pro,
  profile: "second",
  upstream_context: { ...pro.upstream_context, tv: "Second TV" },
  entries: [
    { package: "com.example.kept-on-top", line: 1, upstream_form: "active_disable" },
    { package: "com.example.from-top", line: 2, upstream_form: "active_disable" },
    { package: "com.example.note-only", line: 3, upstream_form: "active_disable" },
    { package: "com.example.from-second", line: 4, upstream_form: "active_disable" },
  ],
};
const on = (pkg, enabled = true) => ({ package: pkg, enabled, name: null });
const installed = ["com.example.kept-on-top", "com.example.from-top", "com.example.in-catalog", "com.example.note-only", "com.example.from-second"].map((p) => on(p));
const ranked = lists.rankedPlanItems(
  [{ profile: top, text: "com.example.x\ncom.example.from-top # top's note", url: "u-top" }, { profile: second, text: "", url: "u-second" }],
  installed,
  new Set(["com.example.in-catalog"]),
  "optimize",
);
assert.deepEqual(
  ranked.map((r) => [r.entry.package, r.entry.listed_by.profile]),
  [["com.example.from-top", "top"], ["com.example.note-only", "second"], ["com.example.from-second", "second"]],
  "catalog wins, then the top list, and a note in a higher list doesn't decide",
);
const row = ranked[0];
assert.deepEqual(row.action, { kind: "disable" });
assert.equal(row.entry.method, "disable");
assert.equal(row.entry.review, true);
assert.equal(row.entry.default_optimize, false);
assert.equal(row.entry.play_store, false);
assert.equal(row.entry.optimize_description, "List author's note: top's note");
assert.deepEqual(row.entry.listed_by, { profile: "top", tv: "Top TV", url: "u-top" });
assert.equal(ranked[1].entry.optimize_description, "On the CleanYourTV list for Second TV.");
const restore = lists.rankedPlanItems([{ profile: second, text: "", url: "u" }], [on("com.example.from-second", false), on("com.example.note-only")], new Set(), "restore");
assert.deepEqual(restore.map((r) => [r.entry.package, r.action.kind]), [["com.example.from-second", "enable"]]);
const proText = await text(pro);
const autoinstall = listed.find((a) => a.package === "android.autoinstalls.config.nvidia");
assert.ok(autoinstall, "the demo's list row is on the Shield Pro list");
assert.equal(lists.sourceNote(proText, autoinstall.line), "Play auto-installs of partner apps");

// The choice is filed under a verified hardware id and nothing else.
const store = new Map();
const storage = { getItem: (k) => (store.has(k) ? store.get(k) : null), setItem: (k, v) => store.set(k, String(v)) };
assert.equal(lists.savedListChoice(storage, "ABC"), null);
assert.equal(lists.saveListChoice(storage, "ABC", "nvidia-shield-tv-2017"), true);
assert.equal(lists.savedListChoice(storage, " ABC "), "nvidia-shield-tv-2017");
assert.equal(lists.savedListChoice(storage, "XYZ"), null, "another TV has its own choice");
assert.equal(lists.saveListChoice(storage, null, "x"), false, "a TV with no id is never remembered");
assert.equal(lists.saveListChoice(storage, "unknown", "x"), false);
lists.saveListChoice(storage, "ABC", null);
assert.equal(lists.savedListChoice(storage, "ABC"), null);
assert.equal(lists.saveListChoice({ getItem: () => "{}", setItem: () => { throw new Error("quota"); } }, "ABC", "x"), false);

// Desktop Optimize against the demo Shield.
const priorDemo = process.env.VITE_DEMO;
process.env.VITE_DEMO = "1";
const evidence = process.env.CLEANYOURTV_EVIDENCE;
const STORE_KEY = "atvopt.cleanyourtvList";
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
  const stored = () => page.evaluate((key) => JSON.parse(localStorage.getItem(key) ?? "{}"), STORE_KEY);
  await page.goto(deviceUrl, { waitUntil: "networkidle" });
  await page.locator("#tab-optimize").click();
  const opt = page.locator("#tabpanel-optimize");
  const card = opt.locator("[data-cleanyourtv-card]");
  const group = opt.locator('[data-cleanyourtv-rows="nvidia-shield-tv-pro-2019"]');

  // The codename's list is used without asking, ranked after the catalog.
  await group.waitFor();
  assert.match(await card.innerText(), /Sources: ATV Optimizer catalog, then CleanYourTV's list for NVIDIA SHIELD TV Pro 2019/);
  assert.deepEqual(await stored(), {}, "nothing is remembered until the person changes something");
  const headings = await opt.locator(".listed-heading").allInnerTexts();
  assert.match(headings[0], /^ATV Optimizer catalog/, "the catalog comes first");
  assert.match(headings[1], /^CleanYourTV list for NVIDIA SHIELD TV Pro 2019/);
  assert.match(headings[1], /Community list, not reviewed by ATV Optimizer · Disable only/);
  const listedRow = group.locator("tr", { has: page.locator(".pkg-id", { hasText: "android.autoinstalls.config.nvidia" }) }).first();
  await listedRow.waitFor();
  const options = await listedRow.locator(".action-radio .radio-pill").evaluateAll((els) =>
    els.map((el) => ({ label: el.querySelector(".radio-label")?.textContent?.trim(), active: el.classList.contains("active") })),
  );
  assert.deepEqual(options.map((o) => o.label), ["Keep", "Disable"], "a list row offers Disable alone, never Uninstall");
  assert.deepEqual(options.filter((o) => o.active).map((o) => o.label), ["Keep"], "a list row starts on Keep");
  assert.match(await listedRow.innerText(), /List author's note: Play auto-installs of partner apps/);
  assert.match(await listedRow.innerText(), /Unknown/i, "the verdict is still the app's own");
  assert.equal(await listedRow.getByRole("button", { name: /^Line / }).count(), 0, "no per-line link");
  await group.getByRole("button", { name: "View list" }).click();
  const opened = await page.evaluate(() => window.openedSources ?? []);
  assert.match(opened.at(-1) ?? "", /^https:\/\/github\.com\/Gathin23\/cleanyourtv\/blob\/[0-9a-f]{40}\/examples\/nvidia-shield-tv-pro-2019\/debloat\.txt$/);
  if (evidence) {
    await card.screenshot({ path: join(evidence, "desktop-sources.png") });
    await group.screenshot({ path: join(evidence, "desktop-list-rows.png") });
  }

  // Restore has nothing from the list while the app is on.
  await opt.getByRole("button", { name: "Restore", exact: true }).click();
  await card.waitFor();
  await page.waitForTimeout(500);
  assert.equal(await opt.locator("[data-cleanyourtv-rows]").count(), 0, "an enabled list app is not a Restore row");
  await opt.getByRole("button", { name: "Optimize", exact: true }).click();
  await group.waitFor();

  // Turn off is remembered for this TV and survives a reload.
  await card.getByRole("button", { name: "Turn off" }).click();
  await page.waitForTimeout(300);
  assert.equal(await opt.locator("[data-cleanyourtv-rows]").count(), 0, "Turn off removes the list's rows");
  assert.equal(await opt.locator(".listed-heading").count(), 0, "no source headings with the catalog alone");
  assert.match(await card.innerText(), /CleanYourTV lists are off for this TV/);
  assert.deepEqual(await stored(), { "0323220012345": "off" });
  await page.reload({ waitUntil: "networkidle" });
  await page.locator("#tab-optimize").click();
  await card.getByRole("button", { name: "Turn on CleanYourTV" }).waitFor();
  await card.getByRole("button", { name: "Turn on CleanYourTV" }).click();
  await group.waitFor();
  assert.deepEqual(await stored(), {});

  // Another list by search; Use suggested goes back.
  await card.getByRole("button", { name: "Change list" }).click();
  await page.getByRole("searchbox", { name: "Search CleanYourTV lists" }).fill("shield 2017");
  await page.getByRole("radio", { name: /SHIELD TV 2017 \(darcy\)/i }).check();
  if (evidence) await card.screenshot({ path: join(evidence, "desktop-sources-choosing.png") });
  await card.getByRole("button", { name: "Use this list" }).click();
  const group2017 = opt.locator('[data-cleanyourtv-rows="nvidia-shield-tv-2017"]');
  await group2017.waitFor();
  assert.equal(await group.count(), 0, "the replaced list's group is gone");
  const row2017 = group2017.locator("tr", { has: page.locator(".pkg-id", { hasText: "android.autoinstalls.config.nvidia" }) }).first();
  await row2017.waitFor();
  assert.deepEqual(
    await row2017.locator(".action-radio .radio-pill.active .radio-label").allInnerTexts(),
    ["Keep"],
    "a row that moves to another list starts on Keep again",
  );
  assert.deepEqual(await stored(), { "0323220012345": "nvidia-shield-tv-2017" });
  assert.match(await card.innerText(), /CleanYourTV's list for NVIDIA SHIELD TV 2017/);
  await card.getByRole("button", { name: "Use suggested" }).click();
  await group.waitFor();
  assert.deepEqual(await stored(), {});
  assert.deepEqual(errors, []);
  console.log("CleanYourTV lists: matching, ranking, notes, per-TV choice and the desktop Optimize rows passed. Browser simulation only.");
} finally {
  await context?.close();
  await browser?.close();
  await server?.close();
  if (priorDemo === undefined) delete process.env.VITE_DEMO;
  else process.env.VITE_DEMO = priorDemo;
}
