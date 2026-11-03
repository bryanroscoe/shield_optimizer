import assert from "node:assert/strict";
import { mkdir, realpath, rename } from "node:fs/promises";
import { resolve } from "node:path";

process.env.VITE_DEMO = "1";
const { createServer } = await import("vite");
const { chromium } = await import("playwright");
const root = resolve(import.meta.dirname, "..");
const evidence = process.env.SHIELD_SNAPSHOT_EVIDENCE;
if (evidence) await mkdir(evidence, { recursive: true });
let server, browser;

try {
  server = await createServer({
    root,
    cacheDir: resolve(root, "node_modules/.vite-snapshot-delete"),
    server: { host: "127.0.0.1", port: 0, hmr: false, fs: { allow: [root, await realpath(resolve(root, "node_modules"))] } },
  });
  await server.listen();
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();
  const context = await browser.newContext({
    viewport: { width: 1280, height: 1000 },
    ...(evidence ? { recordVideo: { dir: evidence, size: { width: 1280, height: 1000 } } } : {}),
  });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(base, { waitUntil: "networkidle" });
  await page.evaluate(async () => {
    const original = window.__TAURI_INTERNALS__.invoke;
    const snapshots = structuredClone(await original("list_snapshots"));
    window.__SNAPSHOT_TEST__ = { snapshots, calls: [], fail: false, hold: false, release: null };
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      const test = window.__SNAPSHOT_TEST__;
      if (command === "list_snapshots") return structuredClone(test.snapshots);
      if (["apply_snapshot", "run_shell"].includes(command)) {
        test.calls.push({ command, args });
        throw new Error("Unexpected TV mutation in local snapshot deletion test");
      }
      if (command === "delete_snapshot") {
        test.calls.push({ command, args });
        if (test.fail) throw new Error("simulated file permission denial");
        if (test.hold) await new Promise(resolve => { test.release = resolve; });
        const index = test.snapshots.findIndex(snapshot => snapshot.path === args.snapshotPath);
        if (index < 0) throw new Error("snapshot file is missing");
        test.snapshots.splice(index, 1);
        return;
      }
      return original(command, args);
    };
    const label = document.createElement("div");
    label.textContent = "SIMULATION — local snapshot fixtures; no TV changes";
    label.style.cssText = "position:fixed;bottom:0;left:0;background:#111;color:white;padding:8px;z-index:99999;pointer-events:none";
    document.body.append(label);
  });
  const fixtures = await page.evaluate(() => window.__SNAPSHOT_TEST__.snapshots);
  const remove = snapshot => page.getByRole("button", { name: `Delete ${snapshot.filename}`, exact: true });
  const calls = () => page.evaluate(() => window.__SNAPSHOT_TEST__.calls);
  const pause = () => evidence ? page.waitForTimeout(800) : Promise.resolve();
  async function confirmDelete(snapshot, accepted) {
    const dialog = page.waitForEvent("dialog");
    const click = remove(snapshot).click();
    const prompt = await dialog;
    assert.equal(prompt.type(), "confirm");
    assert.ok(prompt.message().includes(snapshot.label ?? snapshot.device_name));
    assert.ok(prompt.message().includes(snapshot.filename));
    assert.match(prompt.message(), /permanently deletes the saved file from this computer/);
    assert.match(prompt.message(), /does not change the TV/);
    if (accepted) await prompt.accept();
    else await prompt.dismiss();
    await click;
  }

  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.locator("#tab-snapshot").click();
  await remove(fixtures[0]).waitFor();
  await pause();
  assert.equal(await page.getByRole("button", { name: /^Delete / }).count(), 2);
  await confirmDelete(fixtures[0], false);
  assert.equal((await calls()).length, 0, "cancel never calls the backend");
  await page.locator(".snap-list li").first().getByRole("button", { name: "Preview restore", exact: true }).click();
  await page.getByRole("button", { name: "Restore this snapshot", exact: true }).waitFor();
  await page.evaluate(() => { window.__SNAPSHOT_TEST__.fail = true; });
  await confirmDelete(fixtures[0], true);
  await page.getByRole("alert").getByText(/simulated file permission denial/).waitFor();
  assert.equal(await remove(fixtures[0]).count(), 1, "failed delete retains the row");
  assert.equal(await page.getByRole("button", { name: "Restore this snapshot", exact: true }).count(), 1, "failed delete retains the preview");
  if (evidence) await page.screenshot({ path: resolve(evidence, "snapshot-delete-error-simulated.png"), fullPage: true });
  await pause();
  await page.evaluate(() => { window.__SNAPSHOT_TEST__.fail = false; window.__SNAPSHOT_TEST__.hold = true; });
  await confirmDelete(fixtures[0], true);
  await page.getByRole("button", { name: "Restore this snapshot", exact: true }).waitFor({ state: "visible" });
  assert.ok(await remove(fixtures[0]).isDisabled());
  assert.ok(await remove(fixtures[1]).isDisabled(), "a second delete is blocked while one is pending");
  assert.ok(await page.getByRole("button", { name: "Restore this snapshot", exact: true }).isDisabled());
  assert.ok(await page.getByRole("button", { name: "Save snapshot", exact: false }).isDisabled());
  await pause();
  await page.evaluate(() => window.__SNAPSHOT_TEST__.release());
  await remove(fixtures[0]).waitFor({ state: "detached" });
  assert.equal(await page.getByRole("button", { name: "Restore this snapshot", exact: true }).count(), 0, "deleting the selected row clears its preview");
  await page.getByRole("status").getByText(/Deleted After debloat/).waitFor();
  await pause();

  await page.getByRole("link", { name: "Snapshots", exact: true }).click();
  await remove(fixtures[1]).waitFor();
  await pause();
  assert.equal(await remove(fixtures[0]).count(), 0, "global list reflects the same deleted file");
  const beforeGlobalCancel = (await calls()).length;
  await confirmDelete(fixtures[1], false);
  assert.equal((await calls()).length, beforeGlobalCancel);
  await page.getByRole("combobox", { name: /Restore SHIELD \(Bedroom\) to a device/ }).selectOption({ label: "Preview restore → NVIDIA SHIELD Android TV" });
  await page.getByRole("button", { name: "Restore snapshot", exact: true }).waitFor();
  await page.evaluate(() => { window.__SNAPSHOT_TEST__.fail = true; window.__SNAPSHOT_TEST__.hold = false; });
  await confirmDelete(fixtures[1], true);
  await page.getByText(/Delete failed:.*simulated file permission denial/).waitFor();
  assert.equal(await remove(fixtures[1]).count(), 1);
  assert.equal(await page.getByRole("button", { name: "Restore snapshot", exact: true }).count(), 1);
  await page.evaluate(() => { window.__SNAPSHOT_TEST__.fail = false; window.__SNAPSHOT_TEST__.hold = true; });
  await confirmDelete(fixtures[1], true);
  assert.ok(await remove(fixtures[1]).isDisabled());
  assert.ok(await page.getByRole("button", { name: "Restore snapshot", exact: true }).isDisabled());
  assert.ok(await page.getByRole("button", { name: "Refresh", exact: true }).isDisabled());
  if (evidence) await page.screenshot({ path: resolve(evidence, "snapshot-delete-global-simulated.png"), fullPage: true });
  await pause();
  await page.evaluate(() => window.__SNAPSHOT_TEST__.release());
  await page.getByRole("heading", { name: "No snapshots yet.", exact: true }).waitFor();
  await pause();
  assert.equal(await page.getByRole("button", { name: "Restore snapshot", exact: true }).count(), 0);
  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  await page.getByRole("heading", { name: "No snapshots yet.", exact: true }).waitFor();
  const completed = await calls();
  assert.equal(completed.length, 4);
  assert.ok(completed.every(call => call.command === "delete_snapshot"));
  assert.deepEqual(completed.map(call => call.args), [fixtures[0], fixtures[0], fixtures[1], fixtures[1]].map(snapshot => ({ snapshotPath: snapshot.path })), "deletion sends only the selected local path, never a TV serial");
  assert.deepEqual(errors, []);
  const video = page.video();
  const videoPath = evidence ? await video.path() : null;
  await context.close();
  if (videoPath) await rename(videoPath, resolve(evidence, "snapshot-deletion-simulated.webm"));
  console.log("PASS: device/global Delete controls; labeled confirmation; cancel; file errors; pending-operation guards; selected preview cleanup; empty/refresh state; only local delete calls and zero TV mutations.");
} finally {
  await browser?.close();
  await server?.close();
}
