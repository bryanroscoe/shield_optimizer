// The desktop app must not run a confirmed action unless the person said OK,
// and must be able to name a snapshot, under the dialogs the real app has.
//
// In the Tauri app, tauri-plugin-dialog replaces window.confirm with an async
// function, so `if (!confirm(...))` saw a Promise and ran the action; and
// macOS's web view returns null from prompt() at once, so snapshot Save did
// nothing. Playwright's Chromium has real blocking dialogs, which is why the
// other tests never saw either.
//
// This drives the demo UI with that environment simulated: `isTauri` set, an
// async window.confirm that would say yes, a prompt() that returns null, and
// the native dialog answered through `plugin:dialog|message`. A second run
// without `isTauri` checks that an async confirm resolving false still cancels.
// The native dialog itself can't be shown here.
//
// Every confirmation is checked by answering Cancel and asserting nothing ran,
// which is what a missing `await` on confirmAction() gets wrong; the source
// scan alone would not see it. Those that act on a TV also hold the dialog
// open while the page moves to another TV, answer OK, and assert nothing ran.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { readdir, readFile, realpath } from "node:fs/promises";
import { stripTypeScriptTypes } from "node:module";
import { join, resolve } from "node:path";
import { mock } from "node:test";

process.env.VITE_DEMO = "1";
delete process.env.TAURI_DEV_HOST;
const { createServer } = await import("vite");
const { chromium } = await import("playwright");
const root = resolve(import.meta.dirname, "..");

const RECORDED = [
  "reboot_device",
  "delete_snapshot",
  "save_snapshot",
  "disable_stock_launcher",
  "delete_path",
  "set_display_scaling",
  "clone_app",
];
const SHIELD = "192.168.1.42:5555";
const OTHER_TV = "0323220054321";

function simulateDesktop({ tauri, recorded }) {
  const test = { calls: [], answers: [], hold: false, release: null };
  window.__DIALOG_TEST__ = test;
  if (tauri) Object.defineProperty(window, "isTauri", { configurable: true, value: true });
  // tauri-plugin-dialog's init script on desktop: confirm is async.
  window.confirm = async (message) => {
    test.calls.push({ cmd: "window.confirm", message });
    return tauri ? true : (test.answers.shift() ?? false);
  };
  window.alert = (message) => {
    test.calls.push({ cmd: "window.alert", message });
  };
  // WKWebView on macOS has no text-input panel.
  window.prompt = (message) => {
    test.calls.push({ cmd: "window.prompt", message });
    return null;
  };
  let bridge;
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    get: () => bridge,
    set(value) {
      const original = value.invoke.bind(value);
      value.invoke = async (cmd, args = {}, options) => {
        if (cmd === "plugin:dialog|message") {
          test.calls.push({ cmd, message: args.message, buttons: args.buttons });
          if (test.hold) await new Promise((r) => (test.release = r));
          return test.answers.shift() ?? "Cancel";
        }
        if (recorded.includes(cmd)) {
          test.calls.push({ cmd, args });
          if (cmd === "reboot_device") return { ok: true, message: "Reboot sent (simulated)" };
        }
        return original(cmd, args, options);
      };
      bridge = value;
    },
  });
}

async function sourceUsesNoBrowserDialogs() {
  const offenders = [];
  // Bare calls, and calls through window / globalThis / self in any spacing.
  function browserDialogCalls(code) {
    const found = [];
    for (const m of code.matchAll(/(?<![\w$])(confirm|prompt|alert)\s*\(/g)) {
      const before = code.slice(0, m.index).trimEnd();
      if (!before.endsWith(".")) {
        if (!/\bfunction\s*$/.test(before)) found.push(m[0]);
      } else if (/(?<![\w$.])(window|globalThis|self)$/.test(before.slice(0, -1).trimEnd())) {
        found.push(m[0]);
      }
    }
    return found;
  }
  assert.deepEqual(
    ["confirm(", "window .confirm (", "globalThis.prompt(", "self.alert(", "api.confirm(", "confirmAction(", "x(confirm(1))"].map((c) => browserDialogCalls(c).length),
    [1, 1, 1, 1, 0, 0, 1],
    "the source scan recognises every spelling",
  );
  async function walk(dir) {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (/\.(svelte|ts)$/.test(entry.name) && !/(^|\/)(confirm|demo-mock)\.ts$/.test(path)) {
        const lines = (await readFile(path, "utf8")).split("\n");
        lines.forEach((line, i) => {
          const code = line.trim();
          if (code.startsWith("//") || code.startsWith("*")) return;
          if (browserDialogCalls(code).length) offenders.push(`${path}:${i + 1}: ${code}`);
        });
      }
    }
  }
  await walk(join(root, "src"));
  await walk(join(root, "shared"));
  assert.deepEqual(offenders, [], "use confirmAction() from $lib/confirm, not the browser's dialogs");
}

// confirm.ts, run on its own with the Tauri imports stubbed: a dialog call
// that never settles must not hold the one-question guard forever.
async function confirmActionRecovers() {
  const source = stripTypeScriptTypes(await readFile(join(root, "src/lib/confirm.ts"), "utf8"), { mode: "strip" })
    .replace(/^import \{ isTauri \} from "@tauri-apps\/api\/core";$/m, "const { isTauri } = globalThis.__CONFIRM_STUBS__;")
    .replace(/^import \{ confirm as nativeConfirm \} from "@tauri-apps\/plugin-dialog";$/m, "const { nativeConfirm } = globalThis.__CONFIRM_STUBS__;");
  assert.doesNotMatch(source, /^import /m, "the test stubs every import of confirm.ts");
  const asked = [];
  let ask = () => new Promise((answer) => asked.push(answer));
  globalThis.__CONFIRM_STUBS__ = { isTauri: () => true, nativeConfirm: (...args) => ask(...args) };
  const { confirmAction, CONFIRM_TIMEOUT_MS } = await import(`data:text/javascript,${encodeURIComponent(source)}`);
  assert.ok(CONFIRM_TIMEOUT_MS >= 60_000, "the timeout is long enough to read a warning");
  const flush = () => new Promise((r) => setImmediate(r));
  const quiet = console.error;
  console.error = () => {};
  mock.timers.enable({ apis: ["setTimeout"] });
  try {
    let stuckAnswer = null;
    const stuck = confirmAction("never answered").then((v) => (stuckAnswer = v));
    assert.equal(await confirmAction("double click"), false, "a second question while one is open is refused");
    mock.timers.tick(CONFIRM_TIMEOUT_MS - 1);
    await flush();
    assert.equal(stuckAnswer, null, "a slow answer is still waited for");
    assert.equal(await confirmAction("still open"), false);
    mock.timers.tick(1);
    await stuck;
    assert.equal(stuckAnswer, false, "an unanswered dialog counts as Cancel");

    const next = confirmAction("asked after the timeout");
    await flush();
    assert.equal(asked.length, 2, "the guard is released, so the next question is shown");
    asked[0](true);
    await flush();
    assert.equal(stuckAnswer, false, "a late OK from the stuck dialog is ignored");
    asked[1](false);
    assert.equal(await next, false, "the late OK doesn't answer the next question");

    const ok = confirmAction("answered");
    await flush();
    asked[2](true);
    assert.equal(await ok, true, "an explicit OK confirms");

    ask = () => Promise.reject(new Error("no dialog"));
    assert.equal(await confirmAction("broken"), false, "a dialog that can't be shown is Cancel");
    ask = () => Promise.resolve("yes");
    assert.equal(await confirmAction("odd answer"), false, "only true is a yes");
  } finally {
    mock.timers.reset();
    console.error = quiet;
    delete globalThis.__CONFIRM_STUBS__;
  }
}

let server, browser;
try {
  await sourceUsesNoBrowserDialogs();
  await confirmActionRecovers();
  server = await createServer({
    root,
    cacheDir: resolve(root, "node_modules/.vite-native-dialogs"),
    server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false, fs: { allow: [root, await realpath(resolve(root, "node_modules"))] } },
  });
  await server.listen();
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();

  async function open(tauri) {
    const context = await browser.newContext({ viewport: { width: 1280, height: 1000 } });
    await context.addInitScript(simulateDesktop, { tauri, recorded: RECORDED });
    const page = await context.newPage();
    page.setDefaultTimeout(10000);
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    // A real browser dialog here would mean the app bypassed the simulation.
    page.on("dialog", (d) => {
      errors.push(`unexpected browser ${d.type()}: ${d.message()}`);
      void d.dismiss();
    });
    await page.goto(base, { waitUntil: "networkidle" });
    await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
    await page.locator(".reboot-btn").waitFor();
    const calls = (cmd) => page.evaluate((c) => window.__DIALOG_TEST__.calls.filter((x) => x.cmd === c), cmd);
    const answer = (...values) => page.evaluate((v) => window.__DIALOG_TEST__.answers.push(...v), values);
    const settle = () => page.waitForTimeout(300);
    return { context, page, errors, calls, answer, settle };
  }

  // A route change under the live device page, as when the device list
  // collapses a transport alias into its row while a dialog is open.
  async function switchTo(page, serial) {
    await page.evaluate((s) => {
      const link = document.createElement("a");
      link.href = `/devices/${encodeURIComponent(s)}`;
      link.textContent = "switch TV";
      link.id = "test-switch-tv";
      link.style.cssText = "position:fixed;top:0;left:0;z-index:99999";
      document.body.append(link);
    }, serial);
    await page.locator("#test-switch-tv").click();
    await page.evaluate(() => document.getElementById("test-switch-tv")?.remove());
    await page.waitForURL((url) => decodeURIComponent(url.pathname).endsWith(`/devices/${serial}`));
    await page.locator(".reboot-btn").waitFor();
  }

  /// Run `start`, which opens a confirmation, and keep that dialog open.
  async function holdDialog(page, start) {
    await page.evaluate(() => {
      window.__DIALOG_TEST__.hold = true;
      window.__DIALOG_TEST__.release = null;
    });
    await start();
    await page.waitForFunction(() => window.__DIALOG_TEST__.release !== null);
  }

  async function answerHeld(page, value) {
    await page.evaluate((v) => {
      const test = window.__DIALOG_TEST__;
      test.answers.push(v);
      test.hold = false;
      const release = test.release;
      test.release = null;
      release();
    }, value);
  }

  async function rebootNormal(page) {
    if (!(await page.getByRole("menuitem", { name: "Normal" }).isVisible())) await page.locator(".reboot-btn").click();
    await page.getByRole("menuitem", { name: "Normal" }).click();
  }

  // Desktop app: the native dialog decides.
  {
    const { context, page, errors, calls, answer, settle } = await open(true);

    await answer("Cancel");
    await rebootNormal(page);
    await page.waitForFunction(() => window.__DIALOG_TEST__.calls.some((c) => c.cmd === "plugin:dialog|message"));
    await settle();
    const [asked] = await calls("plugin:dialog|message");
    assert.match(asked.message, /^Reboot this TV\?/);
    assert.equal(asked.buttons, "OkCancel");
    assert.equal((await calls("reboot_device")).length, 0, "Cancel in the native dialog doesn't reboot");

    await answer("Ok");
    await rebootNormal(page);
    await page.getByText("Reboot sent (simulated)").waitFor();
    assert.equal((await calls("reboot_device")).length, 1, "OK reboots");

    // A second click while the dialog is open doesn't queue a second reboot.
    await page.evaluate(() => (window.__DIALOG_TEST__.hold = true));
    await rebootNormal(page);
    await page.waitForFunction(() => window.__DIALOG_TEST__.release !== null);
    await rebootNormal(page);
    await settle();
    assert.equal((await calls("plugin:dialog|message")).length, 3, "one dialog for two clicks");
    await answer("Ok");
    await page.evaluate(() => {
      window.__DIALOG_TEST__.hold = false;
      window.__DIALOG_TEST__.release();
    });
    await page.waitForFunction(() => window.__DIALOG_TEST__.calls.filter((c) => c.cmd === "reboot_device").length === 2);
    await settle();
    assert.equal((await calls("reboot_device")).length, 2, "the held confirmation ran once");

    // Snapshot delete: Cancel keeps the file.
    await page.locator("#tab-snapshot").click();
    const deletes = page.getByRole("button", { name: /^Delete / });
    await deletes.first().waitFor();
    const before = await deletes.count();
    await answer("Cancel");
    await deletes.first().click();
    await settle();
    assert.equal((await calls("delete_snapshot")).length, 0, "Cancel keeps the snapshot");
    assert.equal(await deletes.count(), before);

    // Naming a snapshot: the app's own dialog, never prompt().
    const save = page.locator("#tabpanel-snapshot").getByRole("button", { name: "Save snapshot", exact: true });
    const nameDialog = page.getByRole("dialog", { name: "Save snapshot" });
    const nameField = nameDialog.getByLabel("Name (optional)");
    await save.click();
    await nameDialog.waitFor();
    assert.ok(await nameField.evaluate((el) => el === document.activeElement), "the name field has focus");
    await nameDialog.getByRole("button", { name: "Cancel" }).click();
    await nameDialog.waitFor({ state: "detached" });
    await save.evaluate((el) => new Promise((resolve, reject) => {
      const start = Date.now();
      (function check() {
        if (el === document.activeElement) resolve();
        else if (Date.now() - start > 2000) reject(new Error("focus didn't return to Save snapshot"));
        else requestAnimationFrame(check);
      })();
    }));
    await save.click();
    await nameField.fill("Kept by Escape?");
    await page.keyboard.press("Escape");
    await nameDialog.waitFor({ state: "detached" });
    assert.equal((await calls("save_snapshot")).length, 0, "Cancel and Escape save nothing");

    await save.click();
    await nameField.fill("  Before the test  ");
    await page.keyboard.press("Enter");
    await nameDialog.waitFor({ state: "detached" });
    await page.getByText(/^Saved Before the test/).waitFor();
    assert.deepEqual((await calls("save_snapshot")).map((c) => c.args.label), ["Before the test"]);
    await save.click();
    await nameDialog.getByRole("button", { name: "Save snapshot", exact: true }).click();
    await nameDialog.waitFor({ state: "detached" });
    await page.waitForFunction(() => window.__DIALOG_TEST__.calls.filter((c) => c.cmd === "save_snapshot").length === 2);
    assert.equal((await calls("save_snapshot"))[1].args.label, null, "an empty name saves without a label");

    // Launcher "Save snapshot first" saves, then hands Home over; Cancel hands nothing over.
    await page.getByRole("tab", { name: "Launcher" }).click();
    await page.locator("details.home-picker summary").click();
    const picker = page.getByLabel("App to set as Home");
    await page.waitForFunction(
      () => [...document.querySelectorAll("details.home-picker option")].some((o) => o.value === "com.spocky.projengmenu"),
    );
    await picker.selectOption("com.spocky.projengmenu");
    const stockConfirm = page.getByRole("alertdialog", { name: "Confirm disabling the stock launcher" });
    await page.getByRole("button", { name: "Disable stock launcher" }).click();
    await stockConfirm.getByRole("button", { name: "Save snapshot first" }).click();
    await nameDialog.getByRole("button", { name: "Cancel" }).click();
    await page.getByText("The snapshot wasn't saved, so the stock launcher wasn't changed.").waitFor();
    assert.equal((await calls("disable_stock_launcher")).length, 0);

    // Moving to another TV while the snapshot is being named releases the
    // picker: it isn't left on "Saving snapshot…" for the next TV or this one.
    async function pickProjectivy() {
      await page.getByRole("tab", { name: "Launcher" }).click();
      await page.locator("details.home-picker summary").click();
      await page.waitForFunction(
        () => [...document.querySelectorAll("details.home-picker option")].some((o) => o.value === "com.spocky.projengmenu"),
      );
      await picker.selectOption("com.spocky.projengmenu");
    }
    await page.getByRole("button", { name: "Disable stock launcher" }).click();
    await stockConfirm.getByRole("button", { name: "Save snapshot first" }).click();
    await nameDialog.waitFor();
    await switchTo(page, OTHER_TV);
    await nameDialog.waitFor({ state: "detached" });
    await pickProjectivy();
    assert.ok(await page.getByRole("button", { name: "Disable stock launcher" }).isEnabled(), "the other TV's picker is free");
    await switchTo(page, SHIELD);
    await pickProjectivy();
    assert.ok(await page.getByRole("button", { name: "Disable stock launcher" }).isEnabled(), "this TV's picker is free again");
    assert.equal((await calls("save_snapshot")).length, 2, "the abandoned name dialog saved nothing");
    assert.equal((await calls("disable_stock_launcher")).length, 0, "the abandoned takeover changed nothing");

    await page.getByRole("button", { name: "Disable stock launcher" }).click();
    await stockConfirm.getByRole("button", { name: "Save snapshot first" }).click();
    assert.ok(
      await page.getByRole("button", { name: "Saving snapshot…" }).isDisabled(),
      "the takeover can't be started again while the snapshot is named and saved",
    );
    await nameField.fill("Before stock takeover");
    await nameDialog.getByRole("button", { name: "Save snapshot", exact: true }).click();
    await page.waitForFunction(() => window.__DIALOG_TEST__.calls.some((c) => c.cmd === "disable_stock_launcher"));
    const order = await page.evaluate(() =>
      window.__DIALOG_TEST__.calls.filter((c) => ["save_snapshot", "disable_stock_launcher"].includes(c.cmd)).map((c) => c.args.label ?? c.cmd),
    );
    assert.deepEqual(order.slice(-2), ["Before stock takeover", "disable_stock_launcher"], "the snapshot is saved before stock is disabled");
    // Focus lands on the Launcher tab once its confirm is gone.
    await page.waitForFunction(() => document.activeElement?.id === "tabpanel-launcher", null, { timeout: 3000 });

    // Files delete: Cancel keeps the file, OK deletes it on this TV, and a
    // TV change while the dialog is open deletes nothing anywhere.
    await page.locator("#tab-files").click();
    const deleteFile = page.getByRole("button", { name: "Delete device-report.txt from the TV" });
    await deleteFile.waitFor();
    await answer("Cancel");
    await deleteFile.click();
    await settle();
    assert.equal((await calls("delete_path")).length, 0, "Cancel deletes nothing");
    await answer("Ok");
    await deleteFile.click();
    await page.waitForFunction(() => window.__DIALOG_TEST__.calls.some((c) => c.cmd === "delete_path"));
    assert.deepEqual(
      (await calls("delete_path")).map((c) => [c.args.serial, c.args.path, c.args.allowSystem]),
      [[SHIELD, "/sdcard/device-report.txt", false]],
    );
    await settle();
    await holdDialog(page, () => deleteFile.click());
    await switchTo(page, OTHER_TV);
    await answerHeld(page, "Ok");
    await settle();
    assert.equal((await calls("delete_path")).length, 1, "OK after a TV change deletes nothing");
    await switchTo(page, SHIELD);

    // Display scaling.
    await page.locator("#tab-tweaks").click();
    const scale1080 = page.locator("#tabpanel-tweaks").getByRole("button", { name: /^1080p/ });
    await scale1080.waitFor();
    await answer("Cancel");
    await scale1080.click();
    await settle();
    assert.equal((await calls("set_display_scaling")).length, 0, "Cancel leaves the display alone");
    await answer("Ok");
    await scale1080.click();
    await page.locator("#tabpanel-tweaks").getByText("Applied.", { exact: true }).waitFor();
    assert.deepEqual((await calls("set_display_scaling")).map((c) => [c.args.serial, c.args.preset]), [[SHIELD, "fhd_1080p"]]);
    await holdDialog(page, () => scale1080.click());
    await switchTo(page, OTHER_TV);
    await answerHeld(page, "Ok");
    await settle();
    assert.equal((await calls("set_display_scaling")).length, 1, "OK after a TV change rescales nothing");
    await switchTo(page, SHIELD);

    // Clone to another TV.
    async function openClone(targetLabel) {
      await page.locator("#tab-apps").click();
      await page.getByRole("button", { name: /^Copy .+ to another TV$/ }).first().click();
      const target = page.locator(".clone-panel").getByRole("button", { name: targetLabel });
      await target.waitFor();
      return target;
    }
    const toOtherTv = `Workshop Shield (${OTHER_TV})`;
    let cloneTarget = await openClone(toOtherTv);
    await answer("Cancel");
    await cloneTarget.click();
    await settle();
    assert.equal((await calls("clone_app")).length, 0, "Cancel copies nothing");
    await answer("Ok");
    await cloneTarget.click();
    await page.waitForFunction(() => window.__DIALOG_TEST__.calls.some((c) => c.cmd === "clone_app"));
    assert.deepEqual((await calls("clone_app")).map((c) => [c.args.sourceSerial, c.args.targetSerial]), [[SHIELD, OTHER_TV]]);
    await settle();
    cloneTarget = await openClone(toOtherTv);
    await holdDialog(page, () => cloneTarget.click());
    await switchTo(page, OTHER_TV);
    // The same app's copy picker open again on the new TV must not let the
    // old question copy from it.
    await openClone(`NVIDIA SHIELD Android TV (${SHIELD})`);
    await answerHeld(page, "Ok");
    await settle();
    assert.equal((await calls("clone_app")).length, 1, "OK after a TV change copies nothing");

    assert.deepEqual(await calls("window.confirm"), [], "the plugin's async window.confirm is never used");
    assert.deepEqual(await calls("window.prompt"), [], "prompt() is never used");
    assert.deepEqual(await calls("window.alert"), [], "alert() is never used");
    assert.deepEqual(errors, []);
    await context.close();
  }

  // An async window.confirm that resolves false still cancels.
  {
    const { context, page, errors, calls, answer, settle } = await open(false);
    await answer(false);
    await rebootNormal(page);
    await page.waitForFunction(() => window.__DIALOG_TEST__.calls.some((c) => c.cmd === "window.confirm"));
    await settle();
    assert.equal((await calls("reboot_device")).length, 0, "a Promise of false is not a yes");
    await answer(true);
    await rebootNormal(page);
    await page.getByText("Reboot sent (simulated)").waitFor();
    assert.equal((await calls("reboot_device")).length, 1);
    assert.deepEqual(await calls("plugin:dialog|message"), [], "outside Tauri the browser's confirm answers");
    assert.deepEqual(errors, []);
    await context.close();
  }

  console.log("Native dialogs passed: a stuck dialog times out as Cancel and frees the guard; native Cancel stops reboot, snapshot delete, Files delete, display scaling and Clone; OK runs once per dialog on the TV it was asked for, and not after a TV change; snapshots are named in-app (Cancel, Escape, Enter, empty name); Save snapshot first saves before the takeover and frees the picker on a TV change; an async false cancels.");
} finally {
  await browser?.close();
  await server?.close();
}
