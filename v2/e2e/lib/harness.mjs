// Shared plumbing for the E2E harness: build + start the E2E server, start a
// Vite dev server in VITE_E2E mode on a free port, and hand each scenario a
// context with the page, the simulated device's control API and a
// screenshot-per-step helper.

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdirSync, rmSync, existsSync } from "node:fs";
import { createServer as createNetServer } from "node:net";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const HERE = dirname(fileURLToPath(import.meta.url));
export const E2E = join(HERE, "..");
export const V2 = join(E2E, "..");
export const ARTIFACTS = join(E2E, "artifacts");

const EXE = process.platform === "win32" ? ".exe" : "";

export function freePort() {
  return new Promise((resolve, reject) => {
    const srv = createNetServer();
    srv.unref();
    srv.on("error", reject);
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
  });
}

function run(cmd, args, opts) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, { stdio: "inherit", ...opts });
    child.on("error", reject);
    child.on("exit", (code) => (code === 0 ? resolve() : reject(new Error(`${cmd} ${args.join(" ")} exited ${code}`))));
  });
}

export function serverBinary() {
  const target = process.env.CARGO_TARGET_DIR || join(V2, "target");
  return join(target, "debug", `e2e_server${EXE}`);
}

export async function buildServer() {
  if (process.env.E2E_SKIP_BUILD === "1" && existsSync(serverBinary())) return;
  console.log("[e2e] building e2e_server (cargo, feature e2e)…");
  await run("cargo", ["build", "-p", "shield-optimizer-v2", "--features", "e2e", "--bin", "e2e_server"], { cwd: V2 });
}

export async function startServer() {
  const child = spawn(serverBinary(), ["--port", "0"], { cwd: V2, stdio: ["ignore", "pipe", "pipe"] });
  const lines = [];
  const port = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`e2e_server did not start:\n${lines.join("")}`)), 30_000);
    child.stdout.on("data", (chunk) => {
      const text = chunk.toString();
      lines.push(text);
      const m = text.match(/E2E_SERVER_LISTENING port=(\d+)/);
      if (m) {
        clearTimeout(timer);
        resolve(Number(m[1]));
      }
    });
    child.stderr.on("data", (chunk) => lines.push(chunk.toString()));
    child.on("exit", (code) => reject(new Error(`e2e_server exited ${code}:\n${lines.join("")}`)));
  });
  // Keep the pipes drained; the server logs every invoke at info.
  child.stdout.on("data", () => {});
  child.stderr.on("data", () => {});
  return { child, url: `http://127.0.0.1:${port}` };
}

export async function startVite(serverUrl) {
  const { createServer } = await import("vite");
  process.env.VITE_E2E = "1";
  process.env.VITE_E2E_URL = serverUrl;
  delete process.env.VITE_DEMO;
  delete process.env.TAURI_DEV_HOST;
  const port = await freePort();
  const vite = await createServer({
    root: V2,
    configFile: join(V2, "vite.config.js"),
    logLevel: "warn",
    server: { port, strictPort: true, host: "127.0.0.1", hmr: false },
  });
  await vite.listen();
  return { vite, url: `http://127.0.0.1:${port}` };
}

export function control(url) {
  const call = async (method, path, body) => {
    const res = await fetch(`${url}${path}`, {
      method,
      headers: { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const json = await res.json();
    if (!res.ok) throw new Error(`${method} ${path}: ${json?.error ?? res.status}`);
    return json;
  };
  return {
    reset: (scenario) => call("POST", "/control/reset", scenario),
    state: () => call("GET", "/control/state"),
    fault: (rule) => call("POST", "/control/fault", rule),
    clearFaults: () => call("POST", "/control/faults/clear", {}),
    shell: (serial, command) => call("POST", "/control/shell", { serial, command }),
    device: (serial, patch) => call("POST", "/control/device", { serial, ...patch }),
    attach: (key, serial, state = "device") => call("POST", "/control/attach", { key, serial, state }),
    detach: (key) => call("POST", "/control/detach", { key }),
    invokes: () => call("GET", "/control/invokes"),
    log: (since = 0) => call("GET", `/control/log?since=${since}`),
    invoke: (cmd, args = {}) => call("POST", "/invoke", { cmd, args }),
    replay: (path, profile = true) => call("POST", "/control/replay", { path, profile }),
    replayReport: () => call("GET", "/control/replay"),
  };
}

/// Poll `fn` until it returns a truthy value.
export async function waitFor(fn, { timeout = 15_000, interval = 150, message = "condition" } = {}) {
  const end = Date.now() + timeout;
  let last;
  while (Date.now() < end) {
    try {
      last = await fn();
      if (last) return last;
    } catch (e) {
      last = e;
    }
    await new Promise((r) => setTimeout(r, interval));
  }
  throw new Error(`timed out waiting for ${message} (last: ${last instanceof Error ? last.message : JSON.stringify(last)})`);
}

function slug(text) {
  return text.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 60);
}

/// Build the per-scenario context.
export async function scenarioContext({ browser, base, serverUrl, name, colorScheme = "dark" }) {
  const dir = join(ARTIFACTS, name);
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const context = await browser.newContext({ viewport: { width: 1360, height: 900 }, colorScheme });
  const page = await context.newPage();
  const consoleErrors = [];
  page.on("pageerror", (e) => consoleErrors.push(`pageerror: ${e.message}`));
  page.on("console", (m) => {
    if (m.type() === "error") consoleErrors.push(m.text());
  });
  // Native confirm()/alert() dialogs are accepted unless a scenario says otherwise.
  const dialogs = [];
  let dialogAnswer = true;
  page.on("dialog", async (d) => {
    dialogs.push({ type: d.type(), message: d.message() });
    if (dialogAnswer) await d.accept();
    else await d.dismiss();
  });
  const ctl = control(serverUrl);
  let n = 0;
  const ctx = {
    name,
    page,
    base,
    assert,
    dir,
    dialogs,
    consoleErrors,
    ...ctl,
    answerDialogs(accept) {
      dialogAnswer = accept;
    },
    async step(label) {
      n += 1;
      await page.waitForTimeout(150);
      const file = join(dir, `${String(n).padStart(2, "0")}-${slug(label)}.png`);
      await page.screenshot({ path: file, fullPage: true });
      console.log(`    · ${String(n).padStart(2, "0")} ${label}`);
      return file;
    },
    async open(path = "/") {
      await page.goto(`${base}${path}`, { waitUntil: "networkidle" });
    },
    async openDevice(key, tab) {
      await page.goto(`${base}/devices/${encodeURIComponent(key)}`, { waitUntil: "networkidle" });
      await page.locator("#tab-overview").waitFor();
      if (tab) await ctx.tab(tab);
    },
    async tab(tab) {
      await page.locator(`#tab-${tab}`).click();
      await page.waitForLoadState("networkidle");
    },
    async device(serial) {
      const s = await ctl.state();
      const d = s.devices.find((x) => x.serial === serial);
      assert.ok(d, `no simulated device ${serial}`);
      return d;
    },
    deviceControl: ctl.device,
    waitFor,
    text: (t, opts) => page.getByText(t, opts),
    async close() {
      await context.close();
    },
  };
  return ctx;
}
