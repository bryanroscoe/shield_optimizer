// "Report this app" (#100). What has to hold:
//   - the preview is exactly what Copy, Save and the GitHub form carry, and it
//     never names the device's serial, its address, or any other package;
//   - the optional installed/enabled state and measurements stay out until the
//     user ticks the box;
//   - the saved file is the record shape tools/registry-triage reads, the same
//     shape as its fixture, which the triage tool's own Rust test parses;
//   - nothing is sent: the only exits are the clipboard, a file the user
//     picked, and a URL the user opens and still has to submit.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");
const FIXTURE = JSON.parse(
  readFileSync(
    join(V2, "tools/registry-triage/tests/fixtures/app-report/desktop-app-report.json"),
    "utf8",
  ),
);

// The demo SHIELD: its adb serial is an address, and its ro.serialno is set.
const DEVICE_SERIAL = "192.168.1.42:5555";
const HARDWARE_SERIAL = "0323220012345";
const UNCATALOGUED = "org.fdroid.fdroid";
const CATALOGUED = "com.netflix.ninja";

function serverURL(server) {
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("no address");
  const host = address.address.includes(":") ? `[${address.address}]` : address.address;
  return `http://${host}:${address.port}`;
}

function setHarnessEnvironment() {
  const keys = ["VITE_DEMO", "TAURI_DEV_HOST"];
  const previous = new Map(keys.map((k) => [k, { present: Object.hasOwn(process.env, k), value: process.env[k] }]));
  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;
  return () => {
    for (const [k, prior] of previous) {
      if (prior.present) process.env[k] = prior.value;
      else delete process.env[k];
    }
  };
}

const stub = (body) => ({ status: 200, contentType: "application/javascript", body });

/// registry-triage's `mobile_record` rules, so a report this test accepts is
/// one the tool would count rather than reject.
const REASONS = new Set([
  "uncatalogued_package",
  "unknown_safety_classification",
  "safety_lookup_unavailable",
  "user_report_not_listed",
  "user_report_wrong_verdict",
  "user_report_wrong_description",
  "user_report_other",
]);
const FAMILIES = new Set(["shield", "google_tv", "android_tv", "unknown"]);
function assertTriageShape(report) {
  assert.equal(report.schema_version, 1);
  assert.ok(Array.isArray(report.records) && report.records.length === 1, "one record");
  const r = report.records[0];
  assert.equal(r.kind, "installed_package");
  assert.match(r.token, /^[A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z][A-Za-z0-9_]*)+$/);
  assert.ok(REASONS.has(r.reason), r.reason);
  assert.match(r.app_version, /^[0-9A-Za-z][0-9A-Za-z.+_-]{0,31}$/);
  assert.ok(r.registry_version === null);
  assert.ok(Object.hasOwn(r, "device_family") && (r.device_family === null || FAMILIES.has(r.device_family)));
  assert.ok(Object.hasOwn(r, "device_os") && (r.device_os === null || /^[0-9A-Za-z][0-9A-Za-z._ -]{0,31}$/.test(r.device_os)));
  assert.ok(Number.isFinite(Date.parse(r.first_seen)) && Number.isFinite(Date.parse(r.last_seen)));
  assert.equal(r.count, 1);
  // Same fields as the fixture the Rust test parses, plus only the opt-in state.
  const expected = Object.keys(FIXTURE.records[0]).sort();
  const actual = Object.keys(r).filter((k) => k !== "state").sort();
  assert.deepEqual(actual, expected, "record fields match the triage fixture");
  assert.deepEqual(Object.keys(report).sort(), Object.keys(FIXTURE).sort());
}

function assertNothingIdentifying(text, label) {
  assert.ok(!text.includes(DEVICE_SERIAL), `${label} names the adb serial`);
  assert.ok(!text.includes(HARDWARE_SERIAL), `${label} names ro.serialno`);
  assert.doesNotMatch(text, /\b\d{1,3}(?:\.\d{1,3}){3}\b/, `${label} carries an IPv4 address`);
  assert.doesNotMatch(text, /(?:[0-9a-f]{2}:){5}[0-9a-f]{2}/i, `${label} carries a MAC address`);
  for (const other of ["com.teamsmart.videomanager.tv", "ca.devmesh.overseerrtv", "com.android.vending", "com.spocky.projengmenu"]) {
    assert.ok(!text.includes(other), `${label} names another package (${other})`);
  }
}

async function openReport(page, pkg) {
  const row = page.locator("tr", { has: page.locator(".pkg-id", { hasText: pkg }) }).first();
  await row.scrollIntoViewIfNeeded();
  await row.locator(".row-caret").click();
  await page.getByRole("button", { name: "Report this app" }).click();
  const dialog = page.getByRole("dialog", { name: "Report this app" });
  await dialog.waitFor();
  // The app version arrives a tick later; wait until the preview carries it.
  await page.waitForFunction(() => {
    const v = document.querySelector('textarea[aria-label="Report preview"]')?.value ?? "";
    return v.includes('"app_version"') && !v.includes('"app_version": "unknown"');
  });
  return dialog;
}

async function preview(dialog) {
  return dialog.getByLabel("Report preview").inputValue();
}

async function exercise({ browser, base }) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 1000 } });
  page.on("pageerror", (e) => console.error("pageerror:", e.message));
  page.on("console", (m) => m.type() === "error" && console.error("console:", m.text()));
  await page.route(/plugin-updater/, (r) => r.fulfill(stub(`export async function check() { return null; }`)));
  await page.route(/plugin-opener/, (r) =>
    r.fulfill(stub(`export async function openUrl(url) { (window.__OPENED__ ??= []).push(url); } export async function revealItemInDir() {} export async function openPath() {}`)),
  );
  await page.route(/plugin-process/, (r) => r.fulfill(stub(`export async function relaunch() {}`)));
  await page.addInitScript(() => {
    localStorage.clear();
    window.__OPENED__ = [];
    window.__COPIED__ = [];
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: async (t) => void window.__COPIED__.push(t) },
    });
  });

  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.locator("#tab-apps").click();
  await page.locator(".pkg-id", { hasText: UNCATALOGUED }).first().waitFor();

  // An uncatalogued app: unknown, from no list, reported as missing.
  let dialog = await openReport(page, UNCATALOGUED);
  await dialog.getByText("Nothing is sent automatically", { exact: false }).waitFor();
  const checkbox = dialog.getByRole("checkbox");
  assert.equal(await checkbox.isChecked(), false, "optional state is unchecked by default");
  let text = await preview(dialog);
  assertNothingIdentifying(text, "the default preview");
  let report = JSON.parse(text);
  assertTriageShape(report);
  let record = report.records[0];
  assert.equal(record.token, UNCATALOGUED);
  assert.equal(record.app_name, "F-Droid");
  assert.equal(record.reason, "user_report_not_listed");
  assert.equal(record.device_family, "shield");
  assert.equal(record.device_os, "11");
  assert.equal(record.current_verdict, "unknown");
  assert.equal(record.verdict_source, "no_record");
  assert.ok(!Object.hasOwn(record, "state"), "state is excluded by default");

  // Ticking the box adds the state, and still nothing identifying.
  await checkbox.check();
  // F-Droid has no diskstats row in the demo, so storage is the APK fallback
  // the panel shows, and no meminfo process means a measured "not running".
  await page.waitForFunction(() =>
    (document.querySelector('textarea[aria-label="Report preview"]')?.value ?? "").includes('"apk_files"'),
  );
  text = await preview(dialog);
  {
    const st = JSON.parse(text).records[0].state;
    assert.equal(st.running, false, "a successful read with no process is not running");
    assert.equal(st.ram_mb, null);
    assert.deepEqual(st.storage, { source: "apk_files", app_bytes: 11534336, data_bytes: null, cache_bytes: null });
  }
  assertNothingIdentifying(text, "the preview with state");
  record = JSON.parse(text).records[0];
  assert.equal(record.state.installed, true);
  assert.equal(record.state.enabled, true);
  await checkbox.uncheck();
  assert.ok(!Object.hasOwn(JSON.parse(await preview(dialog)).records[0], "state"));

  await dialog.getByLabel("Wrong description").check();
  await dialog.getByLabel("Note (optional)").fill("The description says it is a launcher.");
  text = await preview(dialog);
  record = JSON.parse(text).records[0];
  assert.equal(record.reason, "user_report_wrong_description");
  assert.equal(record.note, "The description says it is a launcher.");

  // A pasted log line cannot smuggle the identifiers back in through the note.
  await dialog
    .getByLabel("Note (optional)")
    .fill(`seen on ${DEVICE_SERIAL} (${HARDWARE_SERIAL}) mac aa:bb:cc:dd:ee:ff via fe80::1 at 20:10:00`);
  const redacted = JSON.parse(await preview(dialog)).records[0].note;
  assertNothingIdentifying(redacted, "a pasted note");
  assert.ok(!redacted.includes("fe80::1"), redacted);
  assert.match(redacted, /\[redacted\]/);
  assert.match(redacted, /at 20:10:00$/, "ordinary text is left alone");
  await dialog.getByLabel("Note (optional)").fill("The description says it is a launcher.");

  // Copy and Save carry the preview byte for byte.
  await dialog.getByRole("button", { name: "Copy" }).click();
  await page.waitForFunction(() => window.__COPIED__.length > 0);
  assert.equal(await page.evaluate(() => window.__COPIED__.at(-1)), text);
  await dialog.getByRole("button", { name: "Save as file" }).click();
  await page.waitForFunction(() => (window.__SAVED_APP_REPORTS__ ?? []).length > 0);
  const saved = await page.evaluate(() => window.__SAVED_APP_REPORTS__.at(-1));
  assert.match(String(saved.path), /\.json$/);
  assert.equal(saved.contents, text);
  assertTriageShape(JSON.parse(saved.contents));

  // The GitHub form: prefilled from the same text, opened, never submitted.
  await dialog.getByRole("button", { name: "Open GitHub issue" }).click();
  await page.waitForFunction(() => window.__OPENED__.length > 0);
  let url = new URL(await page.evaluate(() => window.__OPENED__.at(-1)));
  assert.equal(url.origin + url.pathname, "https://github.com/bryanroscoe/shield_optimizer/issues/new");
  assert.equal(url.searchParams.get("template"), "app_report.yml");
  assert.equal(url.searchParams.get("package"), UNCATALOGUED);
  assert.equal(url.searchParams.get("reason"), "Wrong description");
  assert.equal(url.searchParams.get("report"), text);
  assertNothingIdentifying(url.toString(), "the issue URL");

  // Too long to prefill: the form opens without the report and asks for a paste.
  await dialog.getByLabel("Note (optional)").fill("€".repeat(1000));
  await dialog.getByRole("button", { name: "Open GitHub issue" }).click();
  await page.waitForFunction(() => window.__OPENED__.length > 1);
  url = new URL(await page.evaluate(() => window.__OPENED__.at(-1)));
  assert.equal(url.searchParams.get("report"), null, "an over-long report is not put in the URL");
  assert.equal(url.searchParams.get("package"), UNCATALOGUED);
  await dialog.getByText("paste it into the Report field", { exact: false }).waitFor();
  await dialog.getByRole("button", { name: "Close" }).click();
  await dialog.waitFor({ state: "detached" });

  // A catalogued app: reported against its verdict, which the report quotes
  // and never changes.
  dialog = await openReport(page, CATALOGUED);
  text = await preview(dialog);
  assertNothingIdentifying(text, "the catalogued preview");
  report = JSON.parse(text);
  assertTriageShape(report);
  record = report.records[0];
  assert.equal(record.reason, "user_report_wrong_verdict");
  assert.equal(record.verdict_source, "reviewed_catalog");
  assert.equal(record.current_verdict, "safe");
  await page.keyboard.press("Escape");
  await dialog.waitFor({ state: "detached" });

  await page.close();
  console.log(
    "App report passed: the preview is what is shared, carries no serial, address or other package, keeps state out by default, and saves in the registry-triage shape.",
  );
}

/// A box whose ro.serialno reads "unknown" has no hardware id. Redacting that
/// placeholder would rewrite the user's own words, so it must pass through.
async function placeholderSerials(server) {
  const { redactNote } = await server.ssrLoadModule("/src/lib/app-report.ts");
  const note = "the verdict is unknown, and Unknown elsewhere";
  for (const placeholder of ["unknown", "UNKNOWN", " unknown "]) {
    assert.equal(redactNote(note, [placeholder]), note, `placeholder ${JSON.stringify(placeholder)}`);
  }
  assert.equal(
    redactNote("the verdict is unknown on 0323220012345", ["unknown", "0323220012345"]),
    "the verdict is unknown on [redacted]",
    "a real serial beside a placeholder is still redacted",
  );
  console.log("App report passed (placeholder serial): a ro.serialno of \"unknown\" leaves the word in the note.");
}

/// #143: a serial pasted in another casing, a serial shorter than four
/// characters, and package ids other than the reported one never survive.
async function noteRedactionGaps(server) {
  const { redactNote, buildAppReport } = await server.ssrLoadModule("/src/lib/app-report.ts");
  assert.equal(
    redactNote("box serial 0323ab0012345 and 0323AB0012345", ["0323AB0012345"]),
    "box serial [redacted] and [redacted]",
    "serials match whatever their casing",
  );
  assert.equal(redactNote("serial x7 on x7b, X7.", ["x7"]), "serial [redacted] on x7b, [redacted].");
  assert.equal(
    redactNote("conflicts with com.android.vending and Com.Example.Report:remote, e.g. on boot", [], "com.example.report"),
    "conflicts with [redacted] and Com.Example.Report:remote, e.g. on boot",
    "other package ids go, the reported one and e.g. stay",
  );
  const report = buildAppReport({
    package: "org.fdroid.fdroid",
    appName: "F-Droid",
    reason: "other",
    note: "also see ca.devmesh.overseerrtv and org.fdroid.fdroid",
    appVersion: "2.3.0",
    device: { family: "shield", androidVersion: "11", redact: [] },
    verdict: null,
    includeState: false,
    state: { status: null, running: null, ramMb: null, storage: null },
    now: new Date("2026-09-30T00:00:00Z"),
  });
  assert.equal(report.records[0].note, "also see [redacted] and org.fdroid.fdroid");
  console.log("App report passed (note gaps): case-varied and short serials and other package ids are redacted.");
}

/// #138: while the page re-reads package states after a run, a report has no
/// state to give for a catalog row.
async function resyncingState(server) {
  const { reportLiveState } = await server.ssrLoadModule("/src/lib/app-details.ts");
  assert.equal(reportLiveState(true, "enabled", true), null, "a resync in flight reports unknown");
  assert.equal(reportLiveState(true, "disabled", false), "disabled");
  assert.equal(reportLiveState(true, undefined, false), null);
  assert.equal(reportLiveState(false, "enabled", true), undefined, "a non-catalog row defers to its own state");
  assert.equal(reportLiveState(false, undefined, false, true), null, "a run with no inventory to re-read leaves every row unknown");
  assert.equal(reportLiveState(true, "enabled", false, true), null);
  console.log("App report passed (resync): state read before an Optimize run is never reported during the re-read.");
}

async function main() {
  const restore = setHarnessEnvironment();
  let server, browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
    await server.listen();
    browser = await chromium.launch();
    await placeholderSerials(server);
    await noteRedactionGaps(server);
    await resyncingState(server);
    await exercise({ browser, base: serverURL(server) });
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    restore();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
