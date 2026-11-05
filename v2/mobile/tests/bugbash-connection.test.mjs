// Connection, onboarding and Dashboard regressions from the 2026-11-05 bug
// bash. The backend here models what the Rust transport really does, which
// the older session mock doesn't: a failed probe or a lost socket evicts the
// connection, so `list_devices` stops listing the TV; a failed connect keeps
// whatever was connected before; and a cancel drops the live connection only
// when it names that connection's request id.
import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

/// `v2/shared` sits outside the Vite root, so the browser reaches it by path.
const SHARED_ERRORS = "/@fs" + fileURLToPath(new URL("../../shared/errors.ts", import.meta.url));

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

function installBackend(options) {
  localStorage.clear();
  if (options.savedDevices) localStorage.setItem("atv.savedDevices.v1", JSON.stringify(options.savedDevices));
  if (options.autoConnect === false) localStorage.setItem("atv.autoConnect.v1", "0");
  window.calls = [];
  window.handlers = {};
  window.backend = {
    active: null,
    unreachable: new Set(options.unreachable ?? []),
    ids: options.ids ?? {},
    names: { A: "Living Room", B: "Bedroom" },
    free: {},
  };
  window.telemetry = options.telemetry ?? { enabled: true, available: true, notice_seen: true };
  window.catalog = options.catalog ?? [];
  window.packageStates = options.packageStates ?? {};
  window.deviceFor = ({ host, port }) => ({
    id: 1,
    serial: `${host}:${port}`,
    name: host,
    model: "SHIELD Android TV",
    status: "device",
    connection: "network",
    device_type: "shield",
    tv_evidence: "tv",
    support: { tier: "fully_supported", note: "Tested." },
    properties: {
      friendly_name: window.backend.names[host] ?? null,
      brand: "NVIDIA",
      model: "SHIELD Android TV",
      device_codename: "mdarcy",
      manufacturer: "NVIDIA",
      android_release: "11",
      sdk_level: "30",
      build_id: "",
      board_platform: "",
      serial_number: window.backend.ids[host] ?? "",
    },
  });
  // The TV stops answering: the next probe or command evicts it.
  window.loseTv = (host) => window.backend.unreachable.add(host);
  window.restoreTv = (host) => window.backend.unreachable.delete(host);
  window.__TAURI_INTERNALS__ = {
    invoke: async (command, args) => {
      window.calls.push({ command, args });
      if (window.handlers[command]) return window.handlers[command](args);
      const backend = window.backend;
      switch (command) {
        case "get_entitlement": return "pro";
        case "telemetry_settings": return window.telemetry;
        case "set_telemetry_enabled":
          window.telemetry = { ...window.telemetry, enabled: args.enabled };
          return window.telemetry;
        case "dismiss_telemetry_notice":
          window.telemetry = { ...window.telemetry, notice_seen: true };
          return window.telemetry;
        case "telemetry_action":
        case "telemetry_error":
          return null;
        case "list_devices":
          return backend.active ? [window.deviceFor(backend.active)] : [];
        case "wireless_status":
          if (backend.active && !backend.unreachable.has(backend.active.host)) return { connected: true };
          backend.active = null;
          return { connected: false };
        case "wireless_connect": {
          const host = args.host.trim();
          if (backend.unreachable.has(host)) {
            return { ok: false, message: "Network is unreachable: this phone has no route to that address." };
          }
          backend.active = { host, port: args.port, requestId: args.requestId };
          return { ok: true, message: `Connected to ${host}:${args.port}.` };
        }
        case "wireless_disconnect":
          backend.active = null;
          return { ok: true, message: "Disconnected." };
        case "wireless_cancel_connect":
          if (backend.active?.requestId === args.canceledRequestId) backend.active = null;
          return null;
        case "wireless_pair": return { ok: true, message: "Paired with the TV." };
        case "wireless_discover": return { devices: [], message: "" };
        case "health_report": {
          const host = backend.active?.host;
          if (!host || backend.unreachable.has(host)) {
            backend.active = null;
            throw "Connection to the TV was lost: broken pipe";
          }
          return {
            ram: { free_mb: backend.free[host] ?? 2048, total_mb: 4096 },
            storage: { used_percent: 40 },
            temperature_c: 41,
            display: {},
            top_memory: [],
          };
        }
        case "app_list_for_device": return window.catalog;
        case "package_states": return window.packageStates;
        case "safety_info": return { kind: "unknown", reason: "No rule matched." };
        default: return { ok: true, message: "done" };
      }
    },
  };
}

async function createPage(t, options = {}) {
  const page = await browser.newPage({ viewport: options.viewport ?? { width: 384, height: 812 } });
  t.after(() => page.close());
  page.pageErrors = [];
  page.on("pageerror", (error) => page.pageErrors.push(error.message));
  await page.addInitScript(installBackend, options);
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session;
    window.router = router;
    session.entitlement = "pro";
  });
  return page;
}

async function openDashboard(t, options = {}) {
  const page = await createPage(t, options);
  await page.evaluate(async () => {
    await window.session.connect("A", 5555);
    window.router.reset("dashboard");
  });
  await page.locator(".device-selector").waitFor();
  return page;
}

function commands(page, name) {
  return page.evaluate((name) => window.calls.filter((c) => c.command === name), name);
}

const settle = (page, ms = 40) => page.evaluate((ms) => new Promise((resolve) => setTimeout(resolve, ms)), ms);

async function enterManualHost(page, host) {
  await page.getByRole("button", { name: "Enter IP address manually" }).click();
  await page.getByRole("textbox", { name: "TV IP address" }).fill(host);
}

test("Back while Add a TV pairs or connects never drops the TV already connected (#1)", async (t) => {
  const page = await openDashboard(t);
  const liveRequest = await page.evaluate(() => window.backend.active.requestId);

  // A live, non-connecting session has nothing to cancel.
  await page.evaluate(() => window.session.cancelConnect(window.session.generation));
  assert.equal((await commands(page, "wireless_cancel_connect")).length, 0);

  await page.evaluate(() => {
    window.handlers.wireless_pair = () => new Promise((resolve) => { window.finishPair = resolve; });
    window.router.navigate("devices");
    window.router.navigate("addtv");
  });
  await enterManualHost(page, "B");
  await page.getByRole("button", { name: "Pair with code" }).click();
  await page.getByRole("textbox", { name: "6-digit pairing code" }).fill("123456");
  await page.getByRole("button", { name: "Pair & connect" }).click();
  await page.waitForFunction(() => !!window.finishPair);

  // Hardware Back during pairing steps back to code entry.
  await page.evaluate(() => history.back());
  await page.getByRole("heading", { name: "Enter pairing code" }).waitFor();
  assert.equal(await page.evaluate(() => window.router.current), "addtv");
  assert.equal((await commands(page, "wireless_cancel_connect")).length, 0);

  // Leaving the screen while B's connect is pending cancels B's request only.
  await page.evaluate(() => {
    window.finishPair = null;
    window.handlers.wireless_connect = () => new Promise((resolve) => {
      window.finishConnect = () => resolve({ ok: false, message: "Connection attempt canceled." });
    });
  });
  await page.getByRole("textbox", { name: "6-digit pairing code" }).fill("654321");
  await page.getByRole("button", { name: "Pair & connect" }).click();
  await page.waitForFunction(() => !!window.finishPair);
  await page.evaluate(() => window.finishPair({ ok: true, message: "Paired with the TV." }));
  await page.waitForFunction(() => !!window.finishConnect);
  await page.evaluate(() => window.router.back());
  await page.waitForFunction(() => window.calls.some((c) => c.command === "wireless_cancel_connect"));
  await page.evaluate(() => window.finishConnect());
  await page.waitForFunction(() => window.session.liveness === "live");

  const [connectB] = (await commands(page, "wireless_connect")).filter((c) => c.args.host === "B");
  const cancels = await commands(page, "wireless_cancel_connect");
  assert.equal(cancels.length, 1);
  assert.equal(cancels[0].args.canceledRequestId, connectB.args.requestId);
  assert.notEqual(cancels[0].args.canceledRequestId, liveRequest);
  assert.deepEqual(await page.evaluate(() => ({
    host: window.session.host,
    live: window.session.isConnected,
    backend: window.backend.active?.requestId,
    screen: window.router.current,
  })), { host: "A", live: true, backend: liveRequest, screen: "devices" });
});

test("a failed Retry while lost keeps the TV named, the banner up and the network-return redial armed (#2)", async (t) => {
  const page = await openDashboard(t);
  await page.evaluate(async () => {
    window.loseTv("A");
    await window.session.checkLiveness();
  });
  const lostBanner = page.locator(".conn-banner.lost");
  await lostBanner.waitFor();
  assert.match(await lostBanner.innerText(), /Lost Living Room/);

  await page.evaluate(() => {
    window.handlers.wireless_connect = () => new Promise((resolve) => {
      window.finishRetry = () => resolve({ ok: false, message: "Network is unreachable: no route." });
    });
  });
  await lostBanner.getByRole("button", { name: "Retry" }).click();
  await page.waitForFunction(() => !!window.finishRetry);
  // While the retry runs the TV reads as pending, never as connected.
  assert.equal(await page.locator(".d-dot.pending").count(), 1);
  assert.match(await page.locator(".conn-banner").innerText(), /Reconnecting to Living Room/);
  await page.evaluate(() => window.finishRetry());

  await lostBanner.waitFor();
  assert.equal(await page.locator(".d-dot.lost").count(), 1);
  assert.equal(await page.locator(".device-selector .device-name").innerText(), "Living Room");
  // A support-tier refresh while lost keeps the last profile too.
  await page.evaluate(() => window.session.refreshDevices());
  assert.deepEqual(await page.evaluate(() => ({
    liveness: window.session.liveness,
    held: window.session.connectedDevice?.serial,
  })), { liveness: "lost", held: "A:5555" });

  // The network comes back: one silent redial, with no tap on Retry.
  await page.evaluate(async () => {
    delete window.handlers.wireless_connect;
    window.restoreTv("A");
    window.calls = [];
    await window.session.onNetworkChange("changed");
  });
  assert.equal(await page.evaluate(() => window.session.liveness), "live");
  assert.equal((await commands(page, "wireless_connect")).length, 1);
  await lostBanner.waitFor({ state: "detached" });
});

test("the device menu lists every saved TV that shares an address and hides only the verified live one (#3)", async (t) => {
  const day = (d) => `2026-10-${String(d).padStart(2, "0")}T00:00:00.000Z`;
  const savedDevices = [
    { host: "A", connectPort: 5555, name: "Living Room", deviceType: "shield", hardwareId: "hw-live", lastUsed: day(9) },
    { host: "A", connectPort: 5555, name: "Old TV", deviceType: "google_tv", hardwareId: "hw-old", lastUsed: day(8) },
    { host: "A", connectPort: 5555, name: "Spare", deviceType: "unknown", localId: "spare", lastUsed: day(7) },
    { host: "C", connectPort: 5555, name: "Kitchen", deviceType: "google_tv", hardwareId: "hw-kitchen", lastUsed: day(6) },
    { host: "C", connectPort: 37002, name: "Kitchen spare", deviceType: "unknown", localId: "kitchen", lastUsed: day(5) },
  ];
  const page = await openDashboard(t, { savedDevices, ids: { A: "hw-live" } });
  await page.locator(".device-selector").click();
  const names = await page.locator(".dropdown-item.saved-tv .dropdown-device-copy > span:first-child").allInnerTexts();
  assert.deepEqual(names.sort(), ["Kitchen", "Kitchen spare", "Old TV", "Spare"]);
  assert.deepEqual(page.pageErrors, []);
});

test("a wrong pairing code returns to code entry with the spent code cleared, and Try again never re-pairs (#4)", async (t) => {
  const page = await createPage(t);
  await page.evaluate(() => {
    window.handlers.wireless_pair = () => ({
      ok: false,
      message: "The code didn't match. Check the 6 digits on the TV and try again.",
    });
  });
  await enterManualHost(page, "B");
  await page.getByRole("button", { name: "Pair with code" }).click();
  await page.getByRole("textbox", { name: "6-digit pairing code" }).fill("123456");
  await page.getByRole("button", { name: "Pair & connect" }).click();

  await page.getByRole("heading", { name: "Enter pairing code" }).waitFor();
  const notice = page.locator(".error-notice");
  await notice.waitFor();
  assert.equal(await notice.getAttribute("data-code"), "wrong_pin");
  assert.match(await notice.innerText(), /Pairing failed/);
  assert.equal(await page.getByText(/Allow debugging/).count(), 0);
  assert.equal(await page.getByRole("textbox", { name: "6-digit pairing code" }).inputValue(), "");
  assert.equal(await page.getByRole("button", { name: "Pair & connect" }).isDisabled(), true);
  assert.equal((await commands(page, "wireless_connect")).length, 0);

  // Pairing works but the connect doesn't: Try again only reconnects.
  await page.evaluate(() => {
    window.handlers.wireless_pair = () => ({ ok: true, message: "Paired with the TV." });
    window.handlers.wireless_connect = () => {
      delete window.handlers.wireless_connect;
      return { ok: false, message: "The TV didn't approve debugging in time. Try again, then choose Allow on the TV within 30 seconds." };
    };
  });
  await page.getByRole("textbox", { name: "6-digit pairing code" }).fill("654321");
  await page.getByRole("button", { name: "Pair & connect" }).click();
  await page.getByRole("heading", { name: "Couldn't connect" }).waitFor();
  await page.getByRole("button", { name: "Try again" }).click();
  await page.getByRole("button", { name: "Open dashboard" }).waitFor();
  assert.equal((await commands(page, "wireless_pair")).length, 2);
  assert.equal((await commands(page, "wireless_connect")).length, 2);
});

test("connect stores the trimmed address, so the profile lookup finds the TV (#5)", async (t) => {
  const page = await createPage(t);
  assert.deepEqual(await page.evaluate(async () => {
    const result = await window.session.connect("  A \n", 5555);
    return {
      ok: result.ok,
      host: window.session.host,
      live: window.session.isConnected,
      sent: window.calls.find((c) => c.command === "wireless_connect").args.host,
    };
  }), { ok: true, host: "A", live: true, sent: "A" });
});

test("a silent recovery onto a TV with another hardware id drops the lost TV's stats (#6)", async (t) => {
  const page = await createPage(t, { ids: { A: "hw-1" } });
  assert.deepEqual(await page.evaluate(async () => {
    window.backend.free.A = 1024;
    await window.session.connect("A", 5555);
    await window.session.loadHealth();
    const before = window.session.health.ram.free_mb;
    // The same TV answers the redial: its stats stay.
    window.backend.active = null;
    await window.session.recoverOrMarkLost();
    const sameTv = window.session.health?.ram?.free_mb;
    // Another TV now answers at this address.
    window.backend.active = null;
    window.backend.ids.A = "hw-2";
    await window.session.recoverOrMarkLost();
    return {
      before,
      sameTv,
      after: window.session.health,
      loaded: window.session.healthLoaded,
      live: window.session.isConnected,
    };
  }), { before: 1024, sameTv: 1024, after: null, loaded: false, live: true });
});

test("the approval wait has Cancel, and first-launch Back steps out of it instead of closing the app (#7)", async (t) => {
  const page = await createPage(t);
  await page.evaluate(() => {
    window.pending = [];
    window.handlers.wireless_connect = (args) => new Promise((resolve) => window.pending.push({ args, resolve }));
  });
  await enterManualHost(page, "B");
  await page.getByRole("button", { name: "Connect (no code)" }).click();
  await page.getByRole("heading", { name: "Connecting…" }).waitFor();
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.getByRole("heading", { name: "Find your TV" }).waitFor();

  await page.getByRole("button", { name: "Connect (no code)" }).click();
  await page.getByRole("heading", { name: "Connecting…" }).waitFor();
  await page.evaluate(() => history.back());
  await page.getByRole("heading", { name: "Find your TV" }).waitFor();
  assert.equal(page.url(), `${origin}/`);
  assert.deepEqual(await page.evaluate(() => [...window.router.stack]), ["onboarding"]);

  const pending = await page.evaluate(() => window.pending.map((p) => p.args.requestId));
  const cancels = await commands(page, "wireless_cancel_connect");
  assert.deepEqual(cancels.map((c) => c.args.canceledRequestId), pending);

  // Late answers for the canceled attempts change nothing.
  await page.evaluate(() => window.pending.forEach((p) => p.resolve({ ok: true })));
  await settle(page);
  assert.equal(await page.evaluate(() => window.session.isConnected), false);
  assert.equal(await page.getByRole("heading", { name: "Find your TV" }).count(), 1);
});

test("Copy details for a failure about another TV carries no model or tier from the connected one (#8)", async (t) => {
  const page = await openDashboard(t);
  await page.evaluate(async () => {
    window.copied = {};
    const { mount } = await import("/node_modules/svelte/src/index-client.js");
    const ErrorNotice = (await import("/src/components/ErrorNotice.svelte")).default;
    for (const [id, support] of [["other", null], ["current", undefined]]) {
      const target = document.createElement("div");
      target.id = id;
      document.body.append(target);
      mount(ErrorNotice, { target, props: { error: "Connection refused", title: "Couldn't connect", support } });
    }
  });
  const copiedText = async (id) => {
    await page.evaluate((id) => {
      Object.defineProperty(navigator, "clipboard", {
        configurable: true,
        value: { writeText: async (text) => { window.copied[id] = text; } },
      });
    }, id);
    // The hosts sit under the fixed bottom tabs; press the button directly.
    await page.locator(`#${id}`).getByRole("button", { name: "Copy details" }).evaluate((el) => el.click());
    await page.waitForFunction((id) => window.copied[id] !== undefined, id);
    return page.evaluate((id) => window.copied[id], id);
  };
  const other = await copiedText("other");
  assert.doesNotMatch(other, /^TV:/m);
  assert.doesNotMatch(other, /^Support:/m);
  const current = await copiedText("current");
  assert.match(current, /^TV: SHIELD Android TV$/m);
  assert.match(current, /^Support: fully_supported$/m);
});

test("the report notice and the connection banner stack instead of covering each other (#9)", async (t) => {
  const page = await openDashboard(t, { telemetry: { enabled: true, available: true, notice_seen: false } });
  const notice = page.locator(".telemetry-notice");
  await notice.waitFor();
  await page.evaluate(async () => {
    window.loseTv("A");
    await window.session.checkLiveness();
  });
  const banner = page.locator(".conn-banner.lost");
  await banner.waitFor();
  const [n, b] = [await notice.boundingBox(), await banner.boundingBox()];
  assert.ok(n.y + n.height <= b.y || b.y + b.height <= n.y, `notice ${JSON.stringify(n)} vs banner ${JSON.stringify(b)}`);
  for (const button of [banner.getByRole("button", { name: "Retry" }), notice.getByRole("button", { name: "OK" })]) {
    assert.equal(await button.evaluate((el) => {
      const r = el.getBoundingClientRect();
      return document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2) === el;
    }), true);
  }
});

test("the Enabled count recovers from a connection lost mid-read, and stats reload after Retry (#10)", async (t) => {
  const page = await createPage(t, {
    catalog: [{ package: "com.example.one", name: "One", default_optimize: true, risk: "safe", method: "disable" }],
    packageStates: { "com.example.one": "enabled" },
  });
  await page.evaluate(async () => {
    let first = true;
    window.handlers.package_states = () => {
      if (!first) return window.packageStates;
      first = false;
      window.backend.active = null;
      throw "Connection to the TV was lost: broken pipe";
    };
    await window.session.connect("A", 5555);
    window.router.reset("dashboard");
  });
  // The read died with the socket, the silent redial reconnected, and the
  // count loads again instead of sitting on "Checking…".
  await page.locator(".count-value", { hasText: "1" }).waitFor();
  assert.equal(await page.getByText("Checking installed recommended apps…").count(), 0);
  assert.equal((await commands(page, "package_states")).length, 2);

  // A loss the redial can't fix, then a banner Retry that works: the tiles
  // reload for the fresh connection instead of staying blank.
  await page.locator(".stat-value", { hasText: "2.0 GB" }).waitFor();
  await page.evaluate(async () => {
    window.backend.free.A = 3072;
    window.loseTv("A");
    await window.session.checkLiveness();
  });
  await page.locator(".conn-banner.lost").waitFor();
  await page.evaluate(() => window.restoreTv("A"));
  await page.locator(".conn-banner").getByRole("button", { name: "Retry" }).click();
  await page.locator(".stat-value", { hasText: "3.0 GB" }).waitFor();
  await page.locator(".count-value", { hasText: "1" }).waitFor();
});

test("a screenshot from the previous TV never opens under the next TV's header (#11)", async (t) => {
  const page = await openDashboard(t, {
    autoConnect: false,
    savedDevices: [{ host: "B", connectPort: 5555, name: "Bedroom", deviceType: "google_tv", hardwareId: "hw-b", lastUsed: "2026-10-01T00:00:00.000Z" }],
  });
  await page.evaluate(() => {
    window.handlers.take_screenshot = () => new Promise((resolve) => {
      window.finishShot = () => resolve({ base64: "", path: "/data/user/0/app/screenshots/shot.png" });
    });
  });
  await page.locator("button.action-btn", { hasText: "Screenshot" }).click();
  await page.waitForFunction(() => !!window.finishShot);
  // The menu won't switch TVs while the capture runs.
  await page.locator(".device-selector").click();
  assert.equal(await page.locator(".dropdown-item.saved-tv", { hasText: "Bedroom" }).isDisabled(), true);
  await page.locator(".dropdown-backdrop").click();

  await page.evaluate(async () => {
    await window.session.connect("B", 5555);
    window.finishShot();
  });
  await settle(page, 60);
  assert.equal(await page.locator(".shot-sheet").count(), 0);
  assert.equal(await page.locator(".device-selector .device-name").innerText(), "Bedroom");
});

test("a scan that finds nothing opens connection help (#12)", async (t) => {
  const page = await createPage(t);
  await page.getByRole("button", { name: /Scan network/ }).click();
  const help = page.locator("details.help-disclosure", { hasText: "Connection help" });
  await help.waitFor();
  assert.equal(await help.evaluate((el) => el.open), true);
  const text = await help.innerText();
  assert.match(text, /same Wi-Fi network/);
  assert.match(text, /Network debugging[\s\S]*port 5555/);
  assert.match(text, /Pair device with\s+pairing code/);
});

test("first launch shows the report notice on onboarding, clear of the buttons, with a working opt-out (#13)", async (t) => {
  const page = await createPage(t, { telemetry: { enabled: true, available: true, notice_seen: false } });
  const notice = page.getByRole("status").filter({ hasText: "ATV Optimizer sends anonymous reports" });
  await notice.waitFor();
  const scan = page.getByRole("button", { name: /Scan network/ });
  await scan.scrollIntoViewIfNeeded();
  const [n, s] = [await notice.boundingBox(), await scan.boundingBox()];
  assert.ok(n.y + n.height <= s.y, "the notice sits in the page flow above the buttons");
  assert.equal(await scan.evaluate((el) => {
    const r = el.getBoundingClientRect();
    return document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2)?.closest("button") === el;
  }), true);

  await notice.getByRole("button", { name: "Turn off reports" }).click();
  await page.getByText("Anonymous reports are off.").waitFor();
  assert.deepEqual((await commands(page, "set_telemetry_enabled")).map((c) => c.args), [{ enabled: false }]);
  assert.equal((await commands(page, "dismiss_telemetry_notice")).length, 1);
  await page.locator(".telemetry-notice").getByRole("button", { name: "OK" }).click();
  await page.locator(".telemetry-notice").waitFor({ state: "detached" });
});

test("at 150% text the onboarding radar and the Optimize card stay inside the screen (#14)", async (t) => {
  const page = await createPage(t, { viewport: { width: 275, height: 610 } });
  await page.getByRole("heading", { name: "Find your TV" }).waitFor();
  // The rings pulse out to 1.9x, so sample a whole cycle.
  const widest = await page.evaluate(async () => {
    let max = 0;
    for (let i = 0; i < 26; i++) {
      max = Math.max(max, document.scrollingElement.scrollWidth);
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    return max;
  });
  assert.ok(widest <= 275, `page scrolls sideways: ${widest}px`);

  // All six code boxes fit, none clipped at the edge.
  await enterManualHost(page, "B");
  await page.getByRole("button", { name: "Pair with code" }).click();
  const lastDigit = await page.locator(".digit").last().evaluate((el) => el.getBoundingClientRect().right);
  assert.ok(lastDigit <= 275 - 16, `last code box ends at ${lastDigit}px`);
  await page.getByRole("button", { name: "Back" }).click();

  await page.evaluate(async () => {
    await window.session.connect("A", 5555);
    window.router.reset("dashboard");
  });
  await page.locator(".health-title").waitFor();
  const title = await page.evaluate(() => {
    const card = document.querySelector(".health-card").getBoundingClientRect();
    const el = document.querySelector(".health-title");
    const box = el.getBoundingClientRect();
    return { fits: el.scrollWidth <= el.clientWidth, inside: box.right <= card.right };
  });
  assert.deepEqual(title, { fits: true, inside: true });
});

test("a failed saved-TV reconnect is tagged as a failure, not in the success style (#15)", async (t) => {
  const page = await createPage(t, {
    savedDevices: [{ host: "A", connectPort: 5555, name: "Living Room", deviceType: "shield", lastUsed: "2026-10-01T00:00:00.000Z" }],
    unreachable: ["A"],
  });
  const tag = page.locator(".device-tag", { hasText: "Couldn't reconnect" });
  await tag.waitFor();
  const colors = await tag.evaluate((el) => {
    const probe = document.createElement("span");
    probe.style.color = "var(--danger)";
    document.body.append(probe);
    const danger = getComputedStyle(probe).color;
    probe.remove();
    return { tag: getComputedStyle(el).color, danger };
  });
  assert.equal(colors.tag, colors.danger);
});

test("raw diagnostics stay behind Details: mDNS port clashes and health read failures read plainly (#16)", async (t) => {
  const page = await createPage(t);
  const presented = await page.evaluate(async (path) => {
    const { classifyError, presentError } = await import(path);
    const pick = (p) => ({ code: p.code, message: p.message });
    return {
      mdns: pick(presentError("mdns: failed to bind 0.0.0.0:5353: Address already in use (os error 98)")),
      diagnostic: pick(presentError("io: Broken pipe (os error 32)")),
      sentence: pick(presentError("not an IP address or mDNS service name: 192.0.2.999. Enter the IP and port")),
      wrongCode: classifyError("The code didn't match. Check the 6 digits on the TV and try again."),
      desktopWrongPin: classifyError("Failed: wrong password or connection was dropped"),
    };
  }, SHARED_ERRORS);
  assert.equal(presented.mdns.code, "mdns_unavailable");
  assert.doesNotMatch(presented.mdns.message, /os error|0\.0\.0\.0/);
  assert.deepEqual(presented.diagnostic, { code: "unknown", message: "Something went wrong." });
  assert.equal(presented.sentence.message, "not an IP address or mDNS service name: 192.0.2.999. Enter the IP and port");
  assert.equal(presented.wrongCode, "wrong_pin");
  assert.equal(presented.desktopWrongPin, "wrong_pin");

  await page.evaluate(async () => {
    window.handlers.health_report = () => {
      throw "adb: shell command failed: dumpsys meminfo: exit status 255 (device offline)";
    };
    await window.session.connect("A", 5555);
    window.router.reset("dashboard");
  });
  const notice = page.locator(".health-error .error-notice");
  await notice.waitFor();
  assert.equal(await notice.getAttribute("data-code"), "device_offline");
  assert.doesNotMatch(await notice.locator(".en-msg").innerText(), /dumpsys|exit status/);
  assert.equal(await notice.locator("details pre").isVisible(), false);
  assert.equal(await page.locator(".health-error").getByRole("button", { name: "Retry" }).count(), 1);
});

test("at 320px a long saved-TV name keeps the row width and the actions wrap below it (#17)", async (t) => {
  const page = await createPage(t, {
    viewport: { width: 320, height: 640 },
    autoConnect: false,
    savedDevices: [{
      host: "B",
      connectPort: 37215,
      name: "Chromecast with Google TV (Living room upstairs)",
      deviceType: "google_tv",
      hardwareId: "hw-b",
      lastUsed: "2026-10-12T10:00:00.000Z",
    }],
  });
  await page.evaluate(async () => {
    await window.session.connect("A", 5555);
    window.router.reset("dashboard");
    window.router.navigate("devices");
  });
  const name = page.locator(".o-name", { hasText: "Chromecast" });
  await name.waitFor();
  const layout = await page.evaluate(() => {
    const name = document.querySelector(".o-name").getBoundingClientRect();
    const actions = document.querySelector(".o-actions").getBoundingClientRect();
    return { nameWidth: name.width, actionsBelow: actions.top >= name.bottom, page: document.scrollingElement.scrollWidth };
  });
  assert.ok(layout.nameWidth >= 170, `name squeezed to ${layout.nameWidth}px`);
  assert.equal(layout.actionsBelow, true);
  assert.ok(layout.page <= 320);
});
