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
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { readdir, readFile, realpath } from "node:fs/promises";
import { join, resolve } from "node:path";

process.env.VITE_DEMO = "1";
delete process.env.TAURI_DEV_HOST;
const { createServer } = await import("vite");
const { chromium } = await import("playwright");
const root = resolve(import.meta.dirname, "..");

const RECORDED = ["reboot_device", "delete_snapshot", "save_snapshot", "disable_stock_launcher"];

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

let server, browser;
try {
  await sourceUsesNoBrowserDialogs();
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
    assert.match(asked.message, /^Reboot the device\?/);
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
    await page.getByText("The snapshot wasn't saved, so the stock launcher was left alone.").waitFor();
    assert.equal((await calls("disable_stock_launcher")).length, 0);

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

  console.log("Native dialogs passed: native Cancel stops reboot and snapshot delete, OK runs once per dialog, snapshots are named in-app (Cancel, Escape, Enter, empty name), Save snapshot first saves before the takeover, an async false cancels.");
} finally {
  await browser?.close();
  await server?.close();
}
