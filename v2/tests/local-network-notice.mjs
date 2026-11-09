// macOS Local Network privacy on the Devices page, against the demo fixtures
// with the probe's answer controlled by the test: the notice shows only on
// "blocked", its help opens from the ? control, the button calls the fixed
// deep-link command, regaining focus re-probes and clears the notice, and a
// "No route to host" failure names Local Network as a possible cause only
// when the probe could not tell.
//
// LOCAL_NETWORK_EVIDENCE=<dir> also writes labelled PNGs and a video.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer as createNetServer } from "node:net";
import { createServer } from "vite";
import { chromium, webkit } from "playwright";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const out = process.env.LOCAL_NETWORK_EVIDENCE;
process.env.VITE_DEMO = "1";

const BLOCKED = "macOS is blocking ATV Optimizer from your local network.";

async function run(browser, base) {
  if (out) await mkdir(out, { recursive: true });
  const context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
    ...(out ? { recordVideo: { dir: `${out}/raw`, size: { width: 1280, height: 900 } } } : {}),
  });
  const page = await context.newPage();
  const shot = async (name) => {
    if (!out) return;
    await page.mouse.move(0, 0);
    await page.screenshot({ path: `${out}/${name}-simulated.png` });
    await page.waitForTimeout(600);
  };
  try {
    await page.addInitScript(() => {
      // State that has to survive a reload lives in sessionStorage.
      if (!sessionStorage.getItem("ln.init")) {
        localStorage.clear();
        sessionStorage.setItem("ln.init", "1");
      }
      const t = (window.ln = {
        calls: [],
        targets: [],
        openFails: false,
        get status() {
          return sessionStorage.getItem("ln.status") ?? "not_applicable";
        },
        set status(v) {
          sessionStorage.setItem("ln.status", v);
        },
      });
      let internals;
      Object.defineProperty(window, "__TAURI_INTERNALS__", {
        configurable: true,
        get: () => internals,
        set: (value) => {
          const original = value.invoke;
          value.invoke = async (command, args) => {
            t.calls.push(command);
            if (command === "local_network_access") t.targets.push(args?.target ?? null);
            if (command === "restart_adb") return { ok: true, server_ready: true, message: "ADB server restarted." };
            if (command === "adb_status" && sessionStorage.getItem("ln.native")) {
              const status = await original(command, args);
              return { ...status, backend: "direct", path: null };
            }
            if (command === "local_network_access") return { status: t.status };
            if (command === "open_local_network_settings") {
              if (t.openFails) throw "exit status: 1";
              return null;
            }
            if (command === "connect_device") {
              return { ok: false, message: "failed to connect to '192.168.1.9:5555': No route to host" };
            }
            return original(command, args);
          };
          internals = value;
        },
      });
    });
    const label = () =>
      page.evaluate(() => {
        const el = document.createElement("div");
        el.textContent = "SIMULATED desktop: demo fixtures, Local Network probe answer forced by the test";
        el.style.cssText =
          "position:fixed;bottom:0;right:0;z-index:99999;background:#17243a;color:white;padding:8px;font:13px sans-serif";
        document.body.append(el);
      });
    const calls = (cmd) => page.evaluate((c) => window.ln.calls.filter((x) => x === c).length, cmd);
    const notice = page.locator(".local-network-notice");

    // Off macOS (and the demo default): no notice, no Mac-only help.
    await page.goto(base, { waitUntil: "networkidle" });
    await page.locator(".device-row").first().waitFor();
    assert.ok((await calls("local_network_access")) >= 1, "probed once at launch");
    assert.equal(await notice.count(), 0, "not_applicable shows nothing");
    assert.equal(await page.locator(".local-network-help").count(), 0);

    // Blocked at launch.
    await page.evaluate(() => (window.ln.status = "blocked"));
    await page.reload({ waitUntil: "networkidle" });
    await label();
    await notice.waitFor();
    assert.equal(await notice.getByText(BLOCKED, { exact: true }).count(), 1);
    const trigger = notice.getByRole("button", { name: `About ${BLOCKED}`, exact: true });
    assert.equal(await trigger.getAttribute("aria-expanded"), "false");
    const helpText = notice.locator(".help-content");
    assert.equal(await helpText.isVisible(), false, "explanation is on demand");
    await shot("local-network-blocked");

    await trigger.focus();
    await page.keyboard.press("Enter");
    assert.equal(await trigger.getAttribute("aria-expanded"), "true");
    const help = await helpText.innerText();
    for (const phrase of [
      "then Local Network, and turn ATV Optimizer on",
      "turn it off and on again",
      "If it's listed more than once, turn all of them on",
      "Then Restart ADB",
      "Don't Allow",
      "Little Snitch or LuLu",
    ]) {
      assert.ok(help.includes(phrase), `help mentions: ${phrase}`);
    }
    assert.doesNotMatch(help, /rename|Shield Optimizer/i, "the rename is not blamed");
    await shot("local-network-blocked-help");
    await page.keyboard.press("Escape");
    assert.equal(await trigger.getAttribute("aria-expanded"), "false", "Escape closes help");

    await notice.getByRole("button", { name: "Open Privacy & Security", exact: true }).click();
    assert.equal(await calls("open_local_network_settings"), 1);

    await page.evaluate(() => (window.ln.openFails = true));
    await notice.getByRole("button", { name: "Open Privacy & Security", exact: true }).click();
    await notice.getByText(/Couldn't open System Settings/).waitFor();
    await page.evaluate(() => (window.ln.openFails = false));

    // Narrow window: nothing spills sideways.
    await page.setViewportSize({ width: 640, height: 900 });
    await trigger.click();
    const box = await notice.boundingBox();
    assert.ok(box.x >= 0 && box.x + box.width <= 640, "notice fits a narrow window");
    await shot("local-network-blocked-narrow");
    await trigger.click();
    await page.setViewportSize({ width: 1280, height: 900 });

    // A no-route connect while blocked states it plainly and leaves the button
    // to the notice above.
    await page.getByPlaceholder("IP[:port]", { exact: false }).fill("192.168.1.9");
    await page.getByRole("button", { name: "Add by IP" }).click();
    const err = page.locator(".connect-form .error-notice");
    await err.waitFor();
    assert.match(await err.innerText(), /Couldn't reach 192\.168\.1\.9\. macOS is blocking ATV Optimizer/);
    assert.equal(await page.locator(".connect-form").getByRole("button", { name: "Open Privacy & Security" }).count(), 0);

    // Access granted in System Settings; coming back to the window re-probes.
    const before = await calls("local_network_access");
    await page.evaluate(() => {
      window.ln.status = "allowed";
      window.dispatchEvent(new Event("focus"));
    });
    await notice.waitFor({ state: "detached" });
    assert.ok((await calls("local_network_access")) > before, "focus re-probed");
    await shot("local-network-cleared");

    // Allowed: focus does not keep probing.
    const settled = await calls("local_network_access");
    await page.evaluate(() => window.dispatchEvent(new Event("focus")));
    await page.waitForTimeout(200);
    assert.equal(await calls("local_network_access"), settled, "no re-probe once allowed");

    // Allowed: the app itself reaches the network, so a no-route from adb
    // points at a stale adb server. Restart ADB, never settings.
    await page.getByPlaceholder("IP[:port]", { exact: false }).fill("192.168.1.9");
    await page.getByRole("button", { name: "Add by IP" }).click();
    await page.waitForFunction(() =>
      document.querySelector(".connect-form .error-notice")?.textContent.includes("Try Restart ADB first"),
    );
    assert.equal(
      await page.evaluate(() => window.ln.targets.at(-1)),
      "192.168.1.9",
      "the failure re-probes with the TV it was about",
    );
    const stale = await err.innerText();
    assert.match(stale, /Couldn't reach 192\.168\.1\.9\. Try Restart ADB first: after an update, macOS can cut off an ADB server the earlier version left running\./);
    assert.doesNotMatch(stale, /Local Network|Privacy|blocking/);
    const remedies = page.locator(".connect-form .remedies");
    await remedies.waitFor();
    assert.equal(await remedies.getByRole("button", { name: "Open Privacy & Security" }).count(), 0, "allowed offers no settings");
    await shot("local-network-allowed-stale-adb");
    let confirmText = "";
    page.once("dialog", async (d) => { confirmText = d.message(); await d.accept(); });
    await remedies.getByRole("button", { name: "Restart ADB", exact: true }).click();
    await page.getByText("ADB server restarted.", { exact: true }).waitFor();
    assert.match(confirmText, /Restart the shared ADB server\?/, "the usual confirmation");
    assert.equal(await calls("restart_adb"), 1);

    // The probe could not tell: Restart ADB first, then Local Network.
    await page.evaluate(() => (window.ln.status = "unknown"));
    await page.getByPlaceholder("IP[:port]", { exact: false }).fill("192.168.1.9");
    await page.getByRole("button", { name: "Add by IP" }).click();
    await page.waitForFunction(() =>
      document.querySelector(".connect-form .error-notice")?.textContent.includes("may be blocking"),
    );
    const possible = await err.innerText();
    assert.match(possible, /Try Restart ADB first.*If that doesn't help, macOS may be blocking local network access for ATV Optimizer, or a VPN or firewall app may be in the way\./s);
    assert.doesNotMatch(possible, /macOS is blocking/);
    assert.equal(await notice.count(), 0, "unknown never shows the blocked notice");
    const both = page.locator(".connect-form .remedies");
    await both.getByRole("button", { name: "Restart ADB", exact: true }).waitFor();
    const inline = both.getByRole("button", { name: "Open Privacy & Security", exact: true });
    await inline.waitFor();
    await shot("local-network-unknown-connect-error");
    await inline.click();
    assert.equal(await calls("open_local_network_settings"), 3);

    // Empty discovery on a Mac: connection help carries the Mac-only item.
    await page.evaluate(() => localStorage.setItem("shieldopt.demo.noDevices", "1"));
    await page.reload({ waitUntil: "networkidle" });
    await label();
    await page.getByRole("heading", { name: "No devices connected.", exact: true }).waitFor();
    const macHelp = page.locator(".help-details .local-network-help");
    await macHelp.waitFor();
    const macText = await macHelp.innerText();
    assert.match(macText, /On a Mac, after an update, macOS can cut off an ADB server the earlier version left running, and Restart ADB replaces it\./);
    assert.match(macText, /check that ATV Optimizer is on in Privacy & Security → Local Network/);
    await shot("local-network-connection-help");
    await macHelp.getByRole("button", { name: "Privacy & Security", exact: true }).click();
    assert.equal(await calls("open_local_network_settings"), 1, "calls restart with the reload");

    // Native Rust ADB: the app itself makes every connection and there is no
    // adb server, so nothing may point at Restart ADB or a stale server.
    await page.evaluate(() => {
      localStorage.removeItem("shieldopt.demo.noDevices");
      sessionStorage.setItem("ln.native", "1");
      window.ln.status = "blocked";
    });
    await page.reload({ waitUntil: "networkidle" });
    await page.getByTestId("adb-backend").getByText("Native Rust ADB · direct connection").waitFor();
    await notice.waitFor();
    const nativeTrigger = notice.getByRole("button", { name: `About ${BLOCKED}`, exact: true });
    await nativeTrigger.click();
    const nativeHelp = await notice.locator(".help-content").innerText();
    assert.ok(nativeHelp.includes("Then try again."), "native help says try again");
    assert.doesNotMatch(nativeHelp, /Restart ADB/, "native help never offers Restart ADB");
    await nativeTrigger.click();
    await page.getByPlaceholder("IP[:port]", { exact: false }).fill("192.168.1.9");
    await page.getByRole("button", { name: "Add by IP" }).click();
    await err.waitFor();
    const nativeBlocked = await err.innerText();
    assert.match(nativeBlocked, /Couldn't reach 192\.168\.1\.9\. macOS is blocking ATV Optimizer.*then try again\./s);
    assert.doesNotMatch(nativeBlocked, /Restart ADB|server/);

    await page.evaluate(() => (window.ln.status = "unknown"));
    await page.getByPlaceholder("IP[:port]", { exact: false }).fill("192.168.1.9");
    await page.getByRole("button", { name: "Add by IP" }).click();
    await page.waitForFunction(() =>
      document.querySelector(".connect-form .error-notice")?.textContent.includes("may be blocking"),
    );
    const nativeUnknown = await err.innerText();
    assert.doesNotMatch(nativeUnknown, /Restart ADB|server/);
    const nativeRemedies = page.locator(".connect-form .remedies");
    await nativeRemedies.getByRole("button", { name: "Open Privacy & Security", exact: true }).waitFor();
    assert.equal(await nativeRemedies.getByRole("button", { name: "Restart ADB", exact: true }).count(), 0);
    await shot("local-network-native-unknown");

    await page.evaluate(() => (window.ln.status = "allowed"));
    await page.getByPlaceholder("IP[:port]", { exact: false }).fill("192.168.1.9");
    await page.getByRole("button", { name: "Add by IP" }).click();
    await page.waitForFunction(() => {
      const text = document.querySelector(".connect-form .error-notice")?.textContent ?? "";
      return text.includes("Couldn't reach") && !text.includes("may be blocking");
    });
    const nativeAllowed = await err.innerText();
    assert.doesNotMatch(nativeAllowed, /Restart ADB|server|Local Network|macOS/);
    assert.equal(await page.locator(".connect-form .remedies").count(), 0, "allowed: no Mac remedies");

    await page.evaluate(() => localStorage.setItem("shieldopt.demo.noDevices", "1"));
    await page.reload({ waitUntil: "networkidle" });
    await page.getByRole("heading", { name: "No devices connected.", exact: true }).waitFor();
    const nativeMacHelp = await page.locator(".help-details .local-network-help").innerText();
    assert.match(nativeMacHelp, /check that ATV Optimizer is on in Privacy & Security → Local Network/);
    assert.doesNotMatch(nativeMacHelp, /Restart ADB|server/);
    await page.evaluate(() => {
      localStorage.removeItem("shieldopt.demo.noDevices");
      sessionStorage.removeItem("ln.native");
    });

    if (out) {
      const video = page.video();
      await context.close();
      await video.saveAs(`${out}/local-network-notice-simulated.webm`);
    }
    console.log(
      "Local Network notice passed: hidden off macOS and when allowed or unknown, shown on blocked with on-demand help (keyboard, Escape, narrow), deep-link button and its failure, focus re-probe clears it, no-route errors offer Restart ADB when allowed, Restart ADB then settings when unknown and state a block only on evidence, the failing TV is re-probed, the Mac-only connection-help item, and native ADB never pointing at Restart ADB or a stale server.",
    );
  } finally {
    await context.close();
  }
}

let server, browser;
try {
  // A free port of our own: vite reads port 0 as its default 5173, which a
  // parallel run may hold.
  const port = await new Promise((done, fail) => {
    const probe = createNetServer().once("error", fail);
    probe.listen(0, "127.0.0.1", () => {
      const { port } = probe.address();
      probe.close(() => done(port));
    });
  });
  server = await createServer({ root, server: { host: "127.0.0.1", port, strictPort: true, hmr: false } });
  await server.listen();
  browser = await (process.argv.includes("--webkit") ? webkit : chromium).launch();
  await run(browser, `http://127.0.0.1:${server.httpServer.address().port}`);
} finally {
  await browser?.close();
  await server?.close();
}
