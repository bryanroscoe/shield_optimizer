// "Stop ADB when quitting" stops the adb server for every tool on the
// computer, so the checkbox must never read off while quitting would still
// stop it. It shows only what the native side confirmed: until then, and
// after a read fails, it reads unknown; a change that fails or loses a race
// leaves the box where the setting really is; and a late reply, even one
// asked for by a Devices page that has since closed, can't replace a newer
// one on screen or in the saved choice. Requests go out one at a time, and a
// reply to one made before a change can't confirm the setting. The
// native setting outlives a page change and a webview reload, starts off with
// every launch, and gets the saved choice back on the first read.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");
const SAVED = "shieldopt.stopAdbOnQuit";

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

async function exercise({ browser, base }) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 1000 } });
  await page.goto(base, { waitUntil: "networkidle" });

  const box = page.getByRole("checkbox", { name: "Stop ADB when quitting" });
  const tip = () => page.locator(".quit-toggle").getAttribute("data-tip");
  const problem = page.locator(".quit-error");
  const saved = () => page.evaluate((key) => localStorage.getItem(key), SAVED);
  const flag = (name, on) =>
    page.evaluate(
      ([key, value]) => (value ? localStorage.setItem(key, "1") : localStorage.removeItem(key)),
      [`shieldopt.demo.${name}`, on],
    );
  const shown = () =>
    box.evaluate((el) => ({ checked: el.checked, unknown: el.indeterminate, disabled: el.disabled }));
  const confirmed = async () => {
    await box.waitFor();
    await page.waitForFunction((el) => el && !el.disabled, await box.elementHandle());
  };
  // Every click waits for its reply before the next assertion.
  const click = async () => {
    await box.click();
    await confirmed();
  };
  const held = () => page.evaluate(() => window.__demoHeldReplies.count());
  const release = (which) => page.evaluate((w) => window.__demoHeldReplies.release(w), which);
  const native = () =>
    page.evaluate(() => JSON.parse(sessionStorage.getItem("shieldopt.demo.native.stopAdbOnQuit") ?? "null"));
  const landed = (enabled) =>
    page.waitForFunction(
      (on) => JSON.parse(sessionStorage.getItem("shieldopt.demo.native.stopAdbOnQuit") ?? "null")?.enabled === on,
      enabled,
    );
  const relaunch = async () => {
    await page.evaluate(() => sessionStorage.removeItem("shieldopt.demo.native.stopAdbOnQuit"));
    await page.reload({ waitUntil: "networkidle" });
    await confirmed();
  };
  const remount = async () => {
    await page.getByRole("link", { name: "Snapshots" }).click();
    await page.waitForURL("**/snapshots");
    await page.getByRole("link", { name: "Devices" }).click();
    await box.waitFor();
  };

  await confirmed();
  assert.equal(await box.isChecked(), false, "the stop is off until someone turns it on");
  assert.match(await tip(), /Android Studio, scrcpy/, "before it is turned on, it names who else loses adb");

  await click();
  assert.equal(await box.isChecked(), true, "a confirmed change ticks the box");
  assert.equal(await saved(), "true", "and the confirmed choice is saved");

  // A fresh launch: the native side starts off again, and the saved choice
  // is handed back to it.
  await relaunch();
  assert.equal(await box.isChecked(), true, "a saved choice survives a relaunch");

  // A change that fails leaves the box on, because the stop still is.
  await flag("stopAdbOnQuitFails", true);
  await click();
  assert.equal(await box.isChecked(), true, "a failed change cannot make the box read off");
  assert.match(await problem.innerText(), /Couldn't change Stop ADB when quitting/);
  assert.equal(await saved(), "true", "nothing unconfirmed is saved");
  await flag("stopAdbOnQuitFails", false);

  // Off always lands, even from an outdated view.
  await flag("stopAdbOnQuitRaced", true);
  await click();
  assert.equal(await box.isChecked(), false, "turning the stop off is never refused");
  assert.equal(await saved(), "false");

  // On from an outdated view is refused, and the box shows the refusal.
  await click();
  assert.equal(await box.isChecked(), false, "a refused change leaves the box where the setting is");
  assert.equal(await saved(), "false");
  await flag("stopAdbOnQuitRaced", false);

  await click();
  assert.equal(await box.isChecked(), true, "with the race gone, the change goes through");

  // The webview reloads while the stop is on. Until the read answers, the
  // box says it doesn't know rather than reading off.
  await flag("stopAdbOnQuitHeld", true);
  await page.reload({ waitUntil: "networkidle" });
  await box.waitFor();
  assert.deepEqual(await shown(), { checked: false, unknown: true, disabled: true }, "an unanswered read is unknown, not off");
  assert.match(await page.locator(".quit-toggle").innerText(), /checking/);
  await release("oldest");
  await confirmed();
  assert.deepEqual(await shown(), { checked: true, unknown: false, disabled: false }, "then shows the stop is on");
  await flag("stopAdbOnQuitHeld", false);

  // A read that fails after a reload stays unknown, with a way to ask again.
  await flag("stopAdbOnQuitReadFails", true);
  await page.reload({ waitUntil: "networkidle" });
  await problem.waitFor();
  assert.deepEqual(await shown(), { checked: false, unknown: true, disabled: true }, "a failed read never reads off");
  assert.match(await problem.innerText(), /Couldn't read Stop ADB when quitting, so it may still be on/);
  await flag("stopAdbOnQuitReadFails", false);
  await page.getByRole("button", { name: "Check again" }).click();
  await confirmed();
  assert.equal(await box.isChecked(), true, "asking again finds the stop still on");
  assert.equal(await problem.count(), 0, "and the failure is gone");

  // Coming back to Devices while the stop is on shows it straight away, even
  // before the new page's read is answered.
  await flag("stopAdbOnQuitHeld", true);
  await remount();
  assert.deepEqual(await shown(), { checked: true, unknown: false, disabled: false }, "a page change keeps the confirmed state");
  await release("oldest");
  assert.equal(await held(), 0);
  await flag("stopAdbOnQuitHeld", false);

  // Turn it off, then ask for On and leave the page before the reply comes.
  await click();
  assert.equal(await saved(), "false");
  await flag("stopAdbOnQuitHeld", true);
  await box.click();
  assert.equal(await held(), 1, "the On request has been answered natively but not delivered");
  await remount();
  assert.deepEqual(await shown(), { checked: false, unknown: true, disabled: true }, "a change on its way is unknown");
  assert.equal(await held(), 1, "and the new page's read waits behind it");
  // The old page's reply arrives after that page has closed, and still counts.
  await release("oldest");
  await confirmed();
  assert.equal(await box.isChecked(), true, "the On from the closed page lands");
  assert.equal(await saved(), "true");
  await page.waitForFunction(() => window.__demoHeldReplies.count() === 1);
  // Turned off on the new page before its read answers.
  await box.click();
  await release("oldest");
  await landed(false);
  assert.deepEqual(await shown(), { checked: false, unknown: true, disabled: true }, "the read's On is out of date");
  await release("oldest");
  await confirmed();
  assert.equal(await box.isChecked(), false);
  assert.equal(await saved(), "false", "and only the newest choice is saved");
  assert.equal(await held(), 0);
  await flag("stopAdbOnQuitHeld", false);
  await relaunch();
  assert.equal(await box.isChecked(), false, "so the next launch keeps the stop off");

  // A read asked for before a change answers after the change has landed. It
  // still says Off, but the change made that out of date.
  await flag("stopAdbOnQuitHeld", true);
  await remount();
  assert.equal(await held(), 1, "the new page's read is waiting");
  assert.deepEqual(await shown(), { checked: false, unknown: false, disabled: false });
  await box.click();
  await release("oldest");
  await landed(true);
  assert.deepEqual(await shown(), { checked: false, unknown: true, disabled: true }, "an older Off cannot confirm the setting");
  await release("oldest");
  await confirmed();
  assert.equal(await box.isChecked(), true, "the change's own reply does");
  assert.equal(await saved(), "true");

  // The same, but the change lands and its reply fails: the box asks again
  // after the change rather than trusting the older read.
  await remount();
  await flag("stopAdbOnQuitReplyFails", true);
  await box.click();
  await release("oldest");
  await landed(false);
  assert.equal((await shown()).unknown, true);
  await release("oldest");
  await page.waitForFunction(() => window.__demoHeldReplies.count() === 1);
  assert.deepEqual(await shown(), { checked: false, unknown: true, disabled: true }, "still unknown until the new read answers");
  assert.match(await problem.innerText(), /Couldn't change Stop ADB when quitting/);
  await release("oldest");
  await confirmed();
  assert.equal(await box.isChecked(), false, "which finds the Off that landed");
  assert.deepEqual(await native(), { enabled: false, generation: 2 });
  assert.equal(await saved(), "false");
  await flag("stopAdbOnQuitReplyFails", false);
  await flag("stopAdbOnQuitHeld", false);

  console.log(
    "Stop ADB when quitting passed: the box only shows confirmed changes and reads unknown until it has one, a failed or outdated change never reads off while the stop is on, a late reply from a closed page or a read older than a change can't bring back an older choice, and the saved choice comes back on relaunch.",
  );
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
