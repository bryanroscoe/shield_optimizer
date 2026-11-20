import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";
import { discoveryPage } from "./helpers/discovery-fixtures.mjs";

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


test("fresh scan shows names before authorization and retains the selected name during failure", async () => {
  const { page, context } = await discoveryPage(browser, origin);
  try {
    assert.equal(await page.getByRole("button", { name: /Studio TV/ }).count(), 1);
    assert.equal(await page.getByRole("button", { name: /Kitchen TV/ }).count(), 1);
    assert.equal(await page.getByText("Android TV", { exact: true }).count(), 0);
    assert.equal(await page.evaluate(() => window.connects.length), 0);
    await page.getByRole("button", { name: /Studio TV/ }).click();
    await page.getByText("Studio TV (192.0.2.10)", { exact: true }).waitFor();
    assert.deepEqual(await page.evaluate(() => window.connects.map(({ host, port }) => [host, port])), [["192.0.2.10", 5555]]);
    await page.evaluate(() => window.pendingConnections.shift()({ ok: false, message: "Test TV did not approve debugging." }));
    await page.getByRole("heading", { name: "Couldn't connect" }).waitFor();
    assert.match(await page.locator(".connect-copy .lede").innerText(), /Studio TV \(192\.0\.2\.10\)/);
    await page.getByRole("button", { name: "Back to scan" }).click();
    await page.evaluate(() => { window.discoveryMode = "missing"; });
    await page.getByRole("button", { name: "Scan again" }).click();
    await page.getByRole("button", { name: /Name unavailable/ }).waitFor();
    assert.equal(await page.getByRole("button", { name: /Studio TV/ }).count(), 0);
  } finally { await context.close(); }
});

test("long advertised names stay inside the row at narrow and enlarged text sizes", async () => {
  const { page, context } = await discoveryPage(browser, origin, { viewport: { width: 320, height: 812 } });
  try {
    await page.addStyleTag({ content: "html { font-size: 125%; }" });
    const name = page.getByText("A very long television name in the upstairs entertainment room", { exact: true });
    assert.equal(await name.isVisible(), true);
    assert.equal(await name.evaluate(el => el.scrollWidth <= el.clientWidth + 1), true);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
    await page.getByRole("button", { name: /Studio TV/ }).focus();
    assert.equal(await page.getByRole("button", { name: /Studio TV/ }).evaluate(el => el === document.activeElement), true);
  } finally { await context.close(); }
});


test("upgrade scan shows current TV names beside unchanged saved labels and keeps selected endpoint", async () => {
  const savedDevices = [
    { host: "192.0.2.99", connectPort: 5555, name: "Android TV", deviceType: "shield", hardwareId: "tv-a", lastUsed: "2026-11-05T00:00:00Z" },
    { host: "192.0.2.20", connectPort: 5555, name: "My custom room label", deviceType: "shield", hardwareId: "tv-b", lastUsed: "2026-11-05T00:00:00Z" },
  ];
  for (const colorScheme of ["dark", "light"]) {
    const { page, context } = await discoveryPage(browser, origin, { savedDevices, colorScheme, viewport: { width: 320, height: 812 } });
    try {
      await page.addStyleTag({ content: "html { font-size: 125%; }" });
      await page.getByText("Advertised · saved as Android TV", { exact: true }).waitFor();
      await page.getByText("Advertised · saved as My custom room label", { exact: true }).waitFor();
      assert.equal(await page.locator(".device-name").getByText("Studio TV", { exact: true }).count(), 1);
      assert.equal(await page.locator(".device-name").getByText("Kitchen TV", { exact: true }).count(), 1);
      assert.deepEqual(await page.evaluate(() => JSON.parse(localStorage.getItem("atv.savedDevices.v1"))), savedDevices);
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
      const row = page.getByRole("button", { name: /Studio TV.*Advertised · saved as Android TV/ });
      await row.focus();
      assert.equal(await row.evaluate(el => el === document.activeElement), true);
      await row.click();
      await page.getByText("Studio TV (advertised; saved as Android TV) (192.0.2.10)", { exact: true }).waitFor();
      assert.deepEqual(await page.evaluate(() => window.connects.map(({ host, port }) => [host, port])), [["192.0.2.10", 5555]]);
      await page.evaluate(() => window.pendingConnections.shift()({ ok: false, message: "Fixture authorization refused." }));
      await page.getByRole("heading", { name: "Couldn't connect" }).waitFor();
      assert.match(await page.locator(".connect-copy .lede").innerText(), /Studio TV \(advertised; saved as Android TV\)/);
      await page.getByRole("button", { name: "Back to scan" }).click();
      await page.evaluate(() => { window.discoveryMode = "missing"; });
      await page.getByRole("button", { name: "Scan again" }).click();
      await page.waitForFunction(() => !document.body.textContent.includes("Advertised · saved as Android TV"));
      assert.deepEqual(await page.evaluate(() => JSON.parse(localStorage.getItem("atv.savedDevices.v1"))), savedDevices);
    } finally { await context.close(); }
  }
});
