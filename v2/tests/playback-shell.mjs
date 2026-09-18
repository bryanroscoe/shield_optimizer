import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");

async function exerciseFeatures(browser, base) {
  const page = await browser.newPage();
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));
  await page.addInitScript(() => {
    localStorage.clear();
    window.__REGRESSION__ = {
      calls: [], pending: {}, hold: {},
      settings: { encoded_surround_output: "3", encoded_surround_output_enabled_formats: "5,26,99" },
      shellResult: {
        stdout: '<img src=x onerror="window.__INJECTED__=true">', stderr: "command failed",
        exit_code: 7, blocked: false, blocked_reason: null, termination: "completed",
      },
    };
    const timer = setInterval(() => {
      const bridge = window.__TAURI_INTERNALS__;
      if (!bridge?.invoke) return;
      clearInterval(timer);
      const original = bridge.invoke.bind(bridge);
      bridge.invoke = async (command, args = {}) => {
        const state = window.__REGRESSION__;
        state.calls.push({ command, args });
        const mediaTitle = command === "media_report" ? state.mediaTitles?.shift() : null;
        if (state.hold[command]) {
          await new Promise((resolve, reject) => {
            (state.pending[command] ??= []).push({ resolve, reject });
          });
        }
        if (command === "run_shell") return { ...state.shellResult };
        if (command === "write_setting") {
          state.settings[args.key] = args.value;
          if (state.failAfterWrite) {
            state.failAfterWrite = false;
            throw "transport closed after write";
          }
          return { ok: true, message: "Updated" };
        }
        if (command === "get_tweaks") return { ...await original(command, args), ...state.settings };
        if (mediaTitle) return {
          ...await original(command, args),
          verdicts: [{ title: mediaTitle, detail: "Regression fixture", level: "info" }],
        };
        return original(command, args);
      };
    }, 0);
  });
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();

  const calls = (command) => page.evaluate((name) =>
    window.__REGRESSION__.calls.filter((call) => call.command === name), command);
  const hold = (command, value = true) => page.evaluate(({ command, value }) => {
    window.__REGRESSION__.hold[command] = value;
  }, { command, value });
  const settle = (command, error = null) => page.evaluate(({ command, error }) => {
    const pending = window.__REGRESSION__.pending[command]?.shift();
    if (!pending) throw new Error(`No pending ${command}`);
    if (error) pending.reject(error);
    else pending.resolve();
  }, { command, error });
  const waitPending = (command) => page.waitForFunction((name) =>
    window.__REGRESSION__.pending[name]?.length > 0, command);

  await page.getByRole("tab", { name: "Shell", exact: true }).click();
  const shell = page.locator("#tabpanel-shell");
  const editor = shell.getByRole("textbox", { name: "Shell command" });
  const run = shell.getByRole("button", { name: "Run", exact: true });
  await shell.getByRole("button", { name: "Uptime", exact: true }).click();
  assert.equal(await editor.inputValue(), "uptime");
  assert.equal(await run.isDisabled(), true, "expert acknowledgment is required");
  await editor.press("Control+Enter");
  assert.equal((await calls("run_shell")).length, 0, "keyboard shortcut must respect acknowledgment");
  await shell.getByRole("checkbox", { name: /I understand these risks/ }).check();
  await shell.getByRole("textbox", { name: "Bookmark name" }).fill("Saved uptime");
  await shell.getByRole("button", { name: "Bookmark current command", exact: true }).click();
  await editor.fill("different command");
  await shell.getByRole("button", { name: "Saved uptime", exact: true }).click();
  assert.equal(await editor.inputValue(), "uptime");
  assert.equal((await calls("run_shell")).length, 0, "presets and bookmarks must never execute");
  await run.click();
  await shell.getByText("exit 7", { exact: true }).waitFor();
  assert.equal((await calls("run_shell")).length, 1);
  assert.equal(await shell.locator("pre").first().textContent(), '<img src=x onerror="window.__INJECTED__=true">');
  assert.equal(await shell.locator("img").count(), 0, "output is text, not markup");
  assert.equal(await page.evaluate(() => window.__INJECTED__), undefined);

  for (const [termination, warning] of [["timeout", "Stopped after 30 seconds."], ["output_limit", "Stopped at the output limit."]]) {
    await page.evaluate((termination) => {
      window.__REGRESSION__.shellResult = {
        stdout: "partial output", stderr: "", exit_code: null,
        blocked: false, blocked_reason: null, termination,
      };
    }, termination);
    await run.click();
    await shell.getByText(warning, { exact: false }).waitFor();
    assert.equal(await shell.locator("pre").textContent(), "partial output");
  }
  await page.getByRole("tab", { name: "Overview", exact: true }).click();
  await page.getByRole("tab", { name: "Shell", exact: true }).click();
  assert.equal(await shell.getByRole("checkbox", { name: /I understand these risks/ }).isChecked(), true,
    "acknowledgment lasts for the current device-page session");

  await page.evaluate(() => {
    window.__REGRESSION__.mediaTitles = ["Stale playback before audio", "Fresh playback after audio"];
  });
  await hold("media_report");
  await page.getByRole("tab", { name: "Playback", exact: true }).click();
  await page.locator("#tabpanel-media").getByRole("heading", { name: "Playback", exact: true }).waitFor();
  await page.waitForFunction(() => window.__REGRESSION__.calls.some((call) => call.command === "media_report"));
  const mediaCalls = (await calls("media_report")).length;
  await page.getByRole("tab", { name: "Tweaks", exact: true }).click();
  const tweaks = page.locator("#tabpanel-tweaks");
  const digital = tweaks.getByRole("button", { name: "Dolby Digital", exact: true });
  await digital.waitFor();
  assert.equal(await tweaks.getByRole("button", { name: "DTS:X", exact: true }).count(), 0);

  await hold("write_setting");
  await digital.click();
  await waitPending("write_setting");
  let writes = await calls("write_setting");
  assert.equal(writes.length, 1);
  assert.equal(writes[0].args.key, "encoded_surround_output_enabled_formats");
  assert.deepEqual(writes[0].args.value.split(",").sort(), ["26", "99"], "unmodeled encodings survive toggles");
  const trueHD = tweaks.getByRole("button", { name: "Dolby TrueHD", exact: true });
  const auto = tweaks.getByRole("button", { name: "Auto", exact: true });
  assert.equal(await trueHD.isDisabled(), true);
  assert.equal(await auto.isDisabled(), true, "mode writes cannot race a format write");
  await hold("get_tweaks");
  await settle("write_setting");
  await waitPending("get_tweaks");
  assert.equal(await trueHD.isDisabled(), true, "write lock extends through readback");
  await hold("write_setting", false);
  await hold("get_tweaks", false);
  await settle("get_tweaks");
  await page.waitForFunction(() => !Array.from(document.querySelectorAll("#tabpanel-tweaks button"))
    .find((button) => button.textContent.trim() === "Dolby TrueHD")?.disabled);
  await trueHD.click();
  await page.waitForFunction(() => window.__REGRESSION__.calls.filter((call) => call.command === "write_setting").length === 2);
  writes = await calls("write_setting");
  assert.deepEqual(writes[1].args.value.split(",").sort(), ["14", "26", "99"], "next toggle uses completed readback");
  await page.waitForFunction(() => !Array.from(document.querySelectorAll("#tabpanel-tweaks button"))
    .find((button) => button.textContent.trim() === "Dolby TrueHD")?.disabled);
  await page.evaluate(() => { window.__REGRESSION__.failAfterWrite = true; });
  await hold("get_tweaks");
  await trueHD.click();
  await waitPending("get_tweaks");
  await tweaks.getByText("transport closed after write", { exact: false }).waitFor();
  assert.equal(await digital.isDisabled(), true, "a transport failure must still lock toggles through readback");
  assert.equal(await auto.isDisabled(), true, "a transport failure must still lock mode changes through readback");
  writes = await calls("write_setting");
  assert.equal(writes.length, 3);
  assert.deepEqual(writes[2].args.value.split(",").sort(), ["26", "99"]);
  await hold("get_tweaks", false);
  await settle("get_tweaks");
  await page.waitForFunction(() => !Array.from(document.querySelectorAll("#tabpanel-tweaks button"))
    .find((button) => button.textContent.trim() === "Dolby Digital")?.disabled);
  await digital.click();
  await page.waitForFunction(() => window.__REGRESSION__.calls.filter((call) => call.command === "write_setting").length === 4);
  writes = await calls("write_setting");
  assert.deepEqual(writes[3].args.value.split(",").sort(), ["26", "5", "99"],
    "toggle after an ambiguous write must use readback, not restore the removed format from stale state");
  await page.waitForFunction(() => !Array.from(document.querySelectorAll("#tabpanel-tweaks button"))
    .find((button) => button.textContent.trim() === "Dolby Digital")?.disabled);
  await page.getByRole("tab", { name: "Playback", exact: true }).click();
  await page.waitForFunction((count) => window.__REGRESSION__.calls.filter((call) => call.command === "media_report").length > count, mediaCalls);
  await page.waitForFunction(() => window.__REGRESSION__.pending.media_report?.length === 2);
  await settle("media_report");
  const playback = page.locator("#tabpanel-media");
  assert.equal(await playback.getByText("Stale playback before audio", { exact: true }).count(), 0,
    "pre-mutation playback result must not overwrite the reset report");
  assert.equal(await playback.getByRole("button", { name: "Reading…", exact: true }).isDisabled(), true,
    "stale playback completion cannot unlock the new request");
  await hold("media_report", false);
  await settle("media_report");
  await playback.getByText("Fresh playback after audio", { exact: true }).waitFor();

  await hold("resource_sample");
  await page.getByRole("tab", { name: "Health", exact: true }).click();
  await waitPending("resource_sample");
  const health = page.locator("#tabpanel-health");
  await health.getByRole("heading", { name: "Vitals", exact: true }).waitFor();
  const sampleCount = (await calls("resource_sample")).length;
  await health.getByRole("button", { name: "Refresh", exact: true }).click();
  await health.getByRole("button", { name: "Refresh", exact: true }).waitFor();
  assert.equal((await calls("resource_sample")).length, sampleCount, "health refresh cannot overlap resource samples");
  await settle("resource_sample", "sample unavailable");
  await health.getByText("Resource sample: sample unavailable", { exact: true }).waitFor();
  assert.equal(await health.getByRole("heading", { name: "Vitals", exact: true }).isVisible(), true);
  assert.equal(await health.getByText("Temperature", { exact: true }).isVisible(), true, "resource error preserves health report");
  await hold("resource_sample", false);
  await health.getByRole("button", { name: "Refresh", exact: true }).click();
  await page.waitForFunction((count) => window.__REGRESSION__.calls.filter((call) => call.command === "resource_sample").length > count, sampleCount);
  await health.getByText("Resource sample: sample unavailable", { exact: true }).waitFor({ state: "hidden" });
  await health.getByRole("button", { name: "Refresh", exact: true }).waitFor();
  await hold("resource_sample");
  await health.getByRole("button", { name: "Refresh", exact: true }).click();
  await waitPending("resource_sample");
  await page.getByRole("link", { name: "Devices", exact: true }).click();
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "Shell", exact: true }).click();
  assert.equal(await shell.getByRole("checkbox", { name: /I understand these risks/ }).isChecked(), false,
    "expert acknowledgment must not leak into a new device-page session");
  await page.getByRole("tab", { name: "Health", exact: true }).click();
  await page.waitForFunction(() => window.__REGRESSION__.pending.resource_sample?.length === 2);
  await settle("resource_sample", "stale sample failure");
  const newSampleCount = (await calls("resource_sample")).length;
  await health.getByRole("button", { name: "Refresh", exact: true }).click();
  await health.getByRole("button", { name: "Refresh", exact: true }).waitFor();
  assert.equal((await calls("resource_sample")).length, newSampleCount, "old completion cannot unlock a new sample");
  assert.equal(await health.getByText("Resource sample: stale sample failure", { exact: true }).count(), 0);
  await hold("resource_sample", false);
  await settle("resource_sample");
  assert.deepEqual(pageErrors, [], "feature flows must not throw browser errors");
  await page.close();
  console.log("Playback/shell regressions passed: explicit expert execution, escaped partial output, serialized audio readback, playback invalidation, and isolated resource errors.");
}

async function main() {
  const previous = new Map(["VITE_DEMO", "TAURI_DEV_HOST"].map((key) => [key, process.env[key]]));
  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;
  let server;
  let browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
    await server.listen();
    const address = server.httpServer?.address();
    if (!address || typeof address === "string") throw new Error("Vite did not expose its TCP address");
    browser = await chromium.launch();
    await exerciseFeatures(browser, `http://127.0.0.1:${address.port}`);
  } finally {
    await browser?.close().catch((error) => console.error("browser cleanup failed", error));
    await server?.close().catch((error) => console.error("Vite cleanup failed", error));
    for (const [key, value] of previous) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
