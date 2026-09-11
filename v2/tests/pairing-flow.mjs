import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");
const PORT = 1422;
const BASE = `http://localhost:${PORT}`;
const PAIR_ADDRESS = "192.168.1.88:43219";
const CONNECT_ADDRESS = "192.168.1.88:37123";
const PIN = "123456";
const MDNS_NAME = "Living Room TV (mDNS)";

async function waitForServer(url, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {
      // The Vite process is still starting.
    }
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
  throw new Error(`dev server did not come up at ${url} within ${timeoutMs}ms`);
}

async function main() {
  const server = spawn("npm", ["run", "dev", "--", "--port", String(PORT)], {
    cwd: V2,
    env: { ...process.env, VITE_DEMO: "1" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  server.stdout.on("data", () => {});
  server.stderr.on("data", (data) => process.stderr.write(`[vite] ${data}`));

  const browser = await chromium.launch();
  try {
    await waitForServer(BASE);
    const page = await browser.newPage();
    await page.goto(BASE, { waitUntil: "networkidle" });
    await page.getByText("NVIDIA SHIELD", { exact: false }).first().waitFor();

    await page.evaluate(({ mdnsName }) => {
      const demo = window.__TAURI_INTERNALS__;
      const originalInvoke = demo.invoke.bind(demo);
      window.__PAIRING_FLOW_CALLS__ = [];
      demo.invoke = async (command, args = {}) => {
        window.__PAIRING_FLOW_CALLS__.push({ command, args });
        if (command === "connect_device") {
          return { ok: false, message: "failed to connect to explicit endpoint" };
        }
        const result = await originalInvoke(command, args);
        if (command === "list_devices") {
          return [
            ...result,
            {
              id: 2,
              serial: "192.168.1.88:37123",
              name: mdnsName,
              model: "TCL QM7L Pro",
              device_type: "google_tv",
              status: "device",
              connection: "network",
              properties: null,
            },
          ];
        }
        return result;
      };
    }, { mdnsName: MDNS_NAME });

    await page.getByRole("button", { name: "Refresh", exact: true }).click();
    await page.getByText(MDNS_NAME, { exact: true }).waitFor();
    await page.evaluate(() => { window.__PAIRING_FLOW_CALLS__ = []; });

    await page.getByRole("button", { name: "Pair PIN" }).click();
    const pairAddress = page.getByPlaceholder("IP:pair_port — e.g. 192.168.42.71:43219");
    const pairPin = page.getByPlaceholder("6-digit PIN");
    await pairAddress.fill(PAIR_ADDRESS);
    await pairPin.fill(PIN);
    await page.getByRole("button", { name: "Pair", exact: true }).click();
    const pairedMessage = page.getByText("Paired successfully.", { exact: false });
    await pairedMessage.waitFor();
    await page.getByText(MDNS_NAME, { exact: true }).waitFor();

    let calls = await page.evaluate(() => window.__PAIRING_FLOW_CALLS__);
    assert.deepEqual(
      calls.filter(({ command }) => command === "pair_device"),
      [{ command: "pair_device", args: { pairAddress: PAIR_ADDRESS, pin: PIN } }],
    );
    assert.equal(calls.filter(({ command }) => command === "connect_device").length, 0);
    assert.equal(calls.filter(({ command }) => command === "list_devices").length, 1);
    assert.equal(await pairAddress.inputValue(), "");
    assert.equal(await pairPin.inputValue(), "");

    const connectAddress = page.getByPlaceholder("IP[:port] — e.g. 192.168.42.71");
    await connectAddress.fill(CONNECT_ADDRESS);
    await page.getByRole("button", { name: "Connect IP" }).click();
    await page.getByText("failed to connect to explicit endpoint", { exact: true }).waitFor();

    calls = await page.evaluate(() => window.__PAIRING_FLOW_CALLS__);
    assert.deepEqual(
      calls.filter(({ command }) => command === "connect_device"),
      [{ command: "connect_device", args: { address: CONNECT_ADDRESS } }],
    );
    assert.notEqual(CONNECT_ADDRESS, PAIR_ADDRESS);
    assert.equal(await connectAddress.inputValue(), CONNECT_ADDRESS);
    await assert.doesNotReject(() => pairedMessage.waitFor({ state: "visible" }));
    await assert.doesNotReject(() => page.getByText(MDNS_NAME, { exact: true }).waitFor());

    console.log("Pairing flow passed: explicit endpoints, retained trust status, and mDNS refresh verified.");
  } finally {
    await browser.close();
    server.kill("SIGTERM");
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
