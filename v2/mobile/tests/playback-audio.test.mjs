import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { playbackFixture, resourceFixture } from "./helpers/report-fixtures.mjs";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server;
let browser;
let origin;
before(async () => {
  ({ server, origin } = await startViteServer());
  browser = await chromium.launch({ headless: true });
});
after(async () => {
  await browser?.close();
  await server?.close();
});

async function open(t, screen = "diagnostics", pro = false, options = {}) {
  const page = await browser.newPage({ viewport: { width: 384, height: 812 }, ...options });
  t.after(() => page.close());
  await page.addInitScript(({ pro }) => {
    localStorage.clear();
    window.calls = [];
    window.handlers = {};
    window.activeHost = "A";
    window.tweaks = {
      encoded_surround_output: "3",
      encoded_surround_output_enabled_formats: "5,26,27,999",
    };
    window.media = {
      video: [{ label: "AV1", mime: "video/av01", advertised: true, software: false, acceleration_unknown: true }],
      hdr_types: [], modes: [], audio: { mode: "unset", enabled_formats: ["MPEG-H LC L4", "DTS UHD P1"], raw_formats: "26,27" },
      match_content_frame_rate: null, verdicts: [],
    };
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.calls.push({ command, args });
      if (window.handlers[command]) return window.handlers[command](args);
      switch (command) {
        case "get_entitlement": return pro ? "pro" : "free";
        case "wireless_connect": window.activeHost = args.host; return { ok: true };
        case "wireless_disconnect": return { ok: true };
        case "wireless_status": return { connected: true };
        case "list_devices": return [{ id: 1, serial: `${window.activeHost}:5555`, name: window.activeHost, model: "Shield", status: "device", connection: "network", device_type: "shield", properties: null }];
        case "health_report": return { ram: { free_mb: 1024, total_mb: 4096 }, storage: {}, display: {}, top_memory: [] };
        case "app_list_for_device": return [];
        case "package_states": return {};
        case "get_tweaks": return { ...window.tweaks };
        case "get_private_dns": return { mode: null, hostname: null };
        case "get_display_scaling": return { size: "Physical size: 1920x1080", density: "Physical density: 320" };
        case "media_report": return window.media;
        case "resource_sample": return { cpu_percent: 22.5, interval_ms: 1100, interfaces: [{ name: "wlan0", rx_bytes_per_s: 2048, tx_bytes_per_s: 1024 }, { name: "tun0", rx_bytes_per_s: null, tx_bytes_per_s: null }] };
        case "write_setting":
          if (!pro) throw "LOCKED:TweaksWrite";
          window.tweaks[args.key] = args.value || null;
          return { ok: true, message: "done" };
        default: return { ok: true, message: "done" };
      }
    } };
  }, { pro });
  await page.goto(origin);
  await page.evaluate(async ({ screen, pro }) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session;
    window.router = router;
    session.entitlement = pro ? "pro" : "free";
    await session.connect("A", 5555);
    router.reset("dashboard");
    router.navigate(screen);
  }, { screen, pro });
  await page.getByRole("heading", { name: screen === "tweaks" ? "Tweaks" : "Diagnostics" }).waitFor();
  return page;
}

test("Free diagnostics reports are explicit, honest and per-interface", async (t) => {
  const page = await open(t);
  assert.equal(await page.evaluate(() => calls.filter((call) => ["media_report", "resource_sample"].includes(call.command)).length), 0);
  await page.getByRole("button", { name: "Read playback report" }).click();
  await page.getByText("Listed; acceleration unknown", { exact: true }).waitFor();
  assert.equal(await page.getByText("Unknown / not reported", { exact: true }).count(), 2);
  assert.equal(await page.getByText("Default / unset", { exact: true }).count(), 2);
  await page.getByRole("button", { name: "Sample resources" }).click();
  const wlan = page.getByRole("listitem", { name: "Interface wlan0", exact: true });
  await wlan.getByText("2.0 KiB/s", { exact: true }).waitFor();
  await wlan.getByText("1.0 KiB/s", { exact: true }).waitFor();
  assert.equal(await page.getByRole("listitem", { name: "Interface tun0", exact: true }).getByText("Unavailable", { exact: true }).count(), 2);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
});

for (const command of ["resource_sample", "media_report"]) {
  for (const outcome of ["resolve", "reject"]) {
    test(`${command} ignores stale ${outcome} after reconnect to the same serial`, async (t) => {
      const page = await open(t);
      await page.evaluate((command) => {
        handlers[command] = () => new Promise((resolve, reject) => { window.pending = { resolve, reject }; });
      }, command);
      await page.getByRole("button", { name: command === "media_report" ? "Read playback report" : "Sample resources" }).click();
      await page.waitForFunction(() => !!window.pending);
      await page.evaluate(async ({ command, outcome }) => {
        await session.connect("A", 5555);
        if (outcome === "reject") pending.reject("Old connection failed");
        else pending.resolve(command === "media_report" ? { ...media, verdicts: [{ level: "info", title: "Old report", detail: "Stale result" }] } : { cpu_percent: 99.9, interval_ms: 1000, interfaces: [] });
      }, { command, outcome });
      await page.getByRole("button", { name: command === "media_report" ? "Read playback report" : "Sample resources" }).waitFor();
      assert.equal(await page.getByText("Old connection failed", { exact: true }).count(), 0);
      assert.equal(await page.getByText("Old report", { exact: true }).count(), 0);
      assert.equal(await page.getByText("99.9%", { exact: true }).count(), 0);
    });
  }
}

test("resource sampling admits one request and no live-refresh polling", async (t) => {
  const page = await open(t);
  await page.clock.install();
  await page.evaluate(() => {
    handlers.resource_sample = () => new Promise((resolve) => { window.finishSample = resolve; });
  });
  await page.getByRole("button", { name: "Sample resources" }).click();
  assert.equal(await page.getByRole("button", { name: "Sampling…" }).isDisabled(), true);
  assert.equal(await page.getByRole("button", { name: "Read playback report" }).isDisabled(), true);
  await page.locator(".live-refresh-btn").click();
  await page.clock.fastForward(10000);
  assert.equal(await page.evaluate(() => calls.filter((call) => call.command === "resource_sample").length), 1);
});

test("manual audio toggle preserves unknown and newer encodings through readback", async (t) => {
  const page = await open(t, "tweaks", true);
  await page.getByRole("checkbox", { name: "Dolby Digital (AC-3)", exact: true }).waitFor();
  await page.evaluate(() => {
    handlers.get_tweaks = () => new Promise((resolve) => {
      window.finishReadback = () => resolve({ ...tweaks });
    });
    handlers.write_setting = (args) => new Promise((resolve) => {
      tweaks[args.key] = args.value;
      window.finishWrite = () => resolve({ ok: true, message: "done" });
    });
  });
  await page.getByRole("checkbox", { name: "Dolby Digital (AC-3)", exact: true }).uncheck();
  assert.equal(await page.getByRole("button", { name: "Surround Auto", exact: true }).isDisabled(), true);
  await page.evaluate(() => finishWrite());
  await page.waitForFunction(() => !!window.finishReadback);
  assert.equal(await page.getByRole("button", { name: "Surround Auto", exact: true }).isDisabled(), true);
  await page.evaluate(() => finishReadback());
  await page.waitForFunction(() => document.querySelector('input[type="checkbox"]')?.disabled === false);
  assert.equal(await page.getByRole("checkbox", { name: "Dolby Digital (AC-3)", exact: true }).isChecked(), false);
  assert.deepEqual(await page.evaluate(() => calls.filter((call) => call.command === "write_setting").map((call) => call.args)), [{ serial: "A:5555", namespace: "global", key: "encoded_surround_output_enabled_formats", value: "26,27,999" }]);
  await page.getByText("Configured: MPEG-H LC L4, DTS UHD P1, Encoding 999", { exact: true }).waitFor();
  assert.equal(await page.getByRole("checkbox", { name: "DTS:X" }).count(), 0);
});

test("playback errors do not replace existing health metrics", async (t) => {
  const page = await open(t);
  await page.getByText("3072 / 4096 MB used", { exact: true }).waitFor();
  await page.evaluate(() => { handlers.media_report = () => Promise.reject("Playback unavailable"); });
  await page.getByRole("button", { name: "Read playback report" }).click();
  await page.getByText("Playback unavailable", { exact: true }).waitFor();
  assert.equal(await page.getByText("3072 / 4096 MB used", { exact: true }).count(), 1);
});

test("Free audio writes use the existing paywall and do not fabricate readback", async (t) => {
  const page = await open(t, "tweaks");
  await page.getByRole("button", { name: "Surround Auto", exact: true }).click();
  await page.getByRole("heading", { name: /Unlock/ }).waitFor();
  assert.equal(await page.evaluate(() => tweaks.encoded_surround_output), "3");
});

test("late audio mutation failure cannot overwrite another TV's state", async (t) => {
  const page = await open(t, "tweaks", true);
  await page.evaluate(() => {
    handlers.write_setting = () => new Promise((resolve, reject) => { window.failWrite = reject; });
  });
  await page.getByRole("button", { name: "Surround Auto", exact: true }).click();
  await page.evaluate(async () => {
    await session.connect("B", 5555);
    failWrite("Old write failed");
  });
  await page.getByRole("button", { name: "Surround Auto", exact: true }).waitFor();
  assert.equal(await page.getByText("Old write failed", { exact: true }).count(), 0);
  assert.equal(await page.evaluate(() => calls.find((call) => call.command === "write_setting").args.serial), "A:5555");
});

for (const [width, scale] of [[320, 1], [384, 1], [320, 2]]) {
  test(`report fidelity and wrapping at ${width}px with ${scale * 100}% text`, async (t) => {
    const evidence = process.env.REPORTS_EVIDENCE === "1";
    const dir = "evidence/report-readability-2026-10-17";
    if (evidence) await mkdir(dir, { recursive: true });
    const record = evidence && width === 384;
    const page = await open(t, "diagnostics", false, {
      viewport: { width, height: 812 },
      ...(record ? { recordVideo: { dir: `${dir}/raw`, size: { width, height: 812 } } } : {}),
    });
    await page.evaluate(({ playbackFixture, resourceFixture, scale }) => {
      window.media = playbackFixture;
      window.resourceData = resourceFixture;
      handlers.resource_sample = () => window.resourceData;
      document.documentElement.style.fontSize = `${16 * scale}px`;
    }, { playbackFixture, resourceFixture, scale });
    await page.getByRole("button", { name: "Read playback report" }).click();
    const playback = page.getByRole("region", { name: "Playback report", exact: true });
    for (const label of ["Listed; acceleration unknown", "Not advertised in available configuration", "Software decoder listed", "3840×2160 @ 59.94006 Hz", "1920×1080 @ 23.976024 Hz", "(active)", "Unknown (vendor-policy-999)", ...playbackFixture.verdicts.map(v => v.detail)]) await playback.getByText(label, { exact: true }).waitFor();
    await playback.getByText(playbackFixture.hdr_types.join(", "), { exact: true }).waitFor();
    await playback.getByText(playbackFixture.audio.enabled_formats.join(", "), { exact: true }).waitFor();
    assert.equal(await playback.locator(".mode-list li").count(), 3);
    assert.equal(await playback.locator(".verdict.warn").count(), 1);
    if (evidence) {
      await playback.scrollIntoViewIfNeeded();
      await page.screenshot({ path: `${dir}/playback-${width}-${scale * 100}-simulated.png`, fullPage: true });
    }
    await page.getByRole("button", { name: "Sample resources" }).click();
    const resources = page.getByRole("region", { name: "Resource sample", exact: true });
    for (const value of ["22.6%", "1.12 s", "123456789.3 KiB/s", "Unavailable", "0.0 KiB/s", "0.5 KiB/s"]) await resources.getByText(value, { exact: true }).waitFor();
    for (const network of resourceFixture.interfaces) await resources.getByRole("listitem", { name: `Interface ${network.name}`, exact: true }).waitFor();
    assert.deepEqual(await page.evaluate(() => ({ playback: media, resources: resourceData })), { playback: playbackFixture, resources: resourceFixture });
    assert.deepEqual(await page.locator(".reports").evaluate(root => [...root.querySelectorAll("*")].filter(el => el.scrollWidth > el.clientWidth + 1).map(el => el.className)), []);
    assert.equal(await page.locator('meta[name="viewport"]').getAttribute("content"), "width=device-width, initial-scale=1.0, viewport-fit=cover");
    assert.equal(await playback.locator("h3").evaluate(el => getComputedStyle(el).fontSize), `${17 * scale}px`);
    if (evidence) {
      await resources.scrollIntoViewIfNeeded();
      await resources.screenshot({ path: `${dir}/resources-${width}-${scale * 100}-simulated.png` });
    }
    if (record) {
      for (const region of [playback.locator("h3"), playback.getByRole("heading", { name: "Display", exact: true }), playback.getByRole("heading", { name: "Interpretation", exact: true }), resources.locator("h3"), resources.getByRole("heading", { name: "Network interfaces", exact: true })]) {
        await region.scrollIntoViewIfNeeded();
        await page.waitForTimeout(1000);
      }
      const video = page.video(); await page.close();
      await video.saveAs(`${dir}/reports-simulated.webm`);
    }
  });
}

test("empty reports and failed rereads retain honest unavailable states and keyboard access", async (t) => {
  const page = await open(t);
  await page.evaluate(() => {
    media = { video: [], hdr_types: [], modes: [], audio: { mode: "unset", enabled_formats: [], raw_formats: null }, match_content_frame_rate: null, verdicts: [] };
    handlers.resource_sample = () => ({ cpu_percent: null, interval_ms: null, interfaces: [] });
  });
  const read = page.getByRole("button", { name: "Read playback report" });
  await read.focus();
  await page.keyboard.press("Enter");
  await page.getByText("None / unset", { exact: true }).waitFor();
  assert.equal(await page.getByText("Unknown / not reported", { exact: true }).count(), 2);
  await page.getByRole("button", { name: "Sample resources" }).click();
  await page.getByText("No network counters available.", { exact: true }).waitFor();
  assert.equal(await page.getByRole("region", { name: "Resource sample" }).getByText("Unavailable", { exact: true }).count(), 2);
  await page.evaluate(() => { handlers.media_report = () => Promise.reject("Configuration could not be read from this TV."); });
  await read.click();
  await page.getByRole("alert").getByText("Configuration could not be read from this TV.", { exact: true }).waitFor();
  assert.equal(await page.getByText("None / unset", { exact: true }).count(), 0);
});
