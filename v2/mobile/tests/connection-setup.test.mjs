// Find your TV › How to turn on debugging: the brand's menu path to Developer
// options and the right debugging switch, on demand in a help sheet.
// SIMULATED: demo fixtures, no phone or TV. The phone app has one (dark) theme.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

process.env.VITE_DEMO = "1";
const evidence = process.env.MOBILE_CONNECTION_SETUP_EVIDENCE;
let server, browser, origin;
before(async () => {
  ({ server, origin } = await startViteServer());
  browser = await chromium.launch();
  if (evidence) await mkdir(evidence, { recursive: true });
});
after(async () => { await browser?.close(); await server?.close(); });

async function open(t, { width = 412, scale = 1, video = false } = {}) {
  const context = await browser.newContext({ viewport: { width, height: 860 }, isMobile: true, hasTouch: true,
    ...(video && evidence ? { recordVideo: { dir: `${evidence}/raw`, size: { width, height: 860 } } } : {}) });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  t.after(() => context.close());
  await page.addInitScript(() => {
    if (!sessionStorage.getItem("seeded")) {
      localStorage.clear();
      sessionStorage.setItem("seeded", "1");
    }
    localStorage.setItem("atv.autoConnect.v1", "0");
  });
  await page.goto(origin, { waitUntil: "networkidle" });
  await page.evaluate(async (scale) => {
    const { router } = await import("/src/lib/router.svelte.ts");
    router.reset("onboarding");
    if (scale > 1) document.documentElement.style.fontSize = `${16 * scale}px`;
    const label = document.createElement("div");
    label.textContent = "SIMULATION · demo fixtures · no phone or TV";
    label.style.cssText = "position:fixed;inset:0 0 auto;z-index:99999;background:#182238;color:white;padding:4px;font:10px sans-serif;text-align:center";
    document.body.append(label);
  }, scale);
  await page.getByRole("heading", { name: "Find your TV", exact: true }).waitFor();
  return page;
}

const steps = async (scope) => (await scope.locator("[data-setup-steps] li").allInnerTexts()).join("\n");

for (const [width, scale] of [[412, 1], [320, 1], [320, 2]]) {
  test(`brand steps open on demand and fit at ${width}px / ${scale * 100}% text`, async (t) => {
    const page = await open(t, { width, scale, video: width === 412 });
    const trigger = page.getByRole("button", { name: "How to turn on debugging", exact: true });
    assert.equal(await page.locator("[data-brand-setup]").count(), 0, "not on the screen until asked for");
    assert.ok((await trigger.boundingBox()).height >= 44);
    await trigger.tap();
    const sheet = page.getByRole("dialog", { name: "How to turn on debugging" });
    const setup = sheet.locator("[data-brand-setup]");
    await setup.waitFor();
    assert.match(await steps(setup), /If Developer options isn't there yet/);
    await setup.getByLabel("TV brand").selectOption({ label: "Nvidia Shield" });
    assert.match(await steps(setup), /Turn on Network debugging\.\nThen scan, or choose Enter IP address manually and Connect \(no code\) on port 5555\./);
    await setup.getByLabel("TV brand").selectOption({ label: "TCL" });
    assert.match(await steps(setup), /Open Settings › System › Developer options\.\nTurn on Wireless debugging\./);
    await setup.getByRole("radio").nth(2).tap();
    assert.match(await steps(setup), /Settings › Device Preferences › About and press OK on Build seven times/);
    for (const el of await sheet.locator("select, button, fieldset label").all()) assert.ok((await el.boundingBox()).height >= 44, "touch targets");
    const box = await sheet.boundingBox();
    assert.ok(box.x >= 0 && box.x + box.width <= width + 1);
    const clipped = await sheet.evaluate((el) => [...el.querySelectorAll("select, li, p, label, h3")]
      .filter((n) => n.scrollWidth > n.clientWidth + 1).map((n) => n.textContent.trim().slice(0, 40)));
    assert.deepEqual(clipped, [], "nothing clipped");
    if (evidence) await page.screenshot({ path: `${evidence}/tcl-${width}-${scale * 100}.png` });
    await sheet.getByRole("button", { name: "Done", exact: true }).tap();
    await sheet.waitFor({ state: "detached" });
    assert.equal(await trigger.evaluate((el) => el === document.activeElement), true, "focus returns to the help link");
    if (evidence && width === 412) {
      const video = page.video();
      await page.context().close();
      await video.saveAs(`${evidence}/connection-setup-mobile-simulated.webm`);
    }
  });
}

test("the last brand picked is remembered", async (t) => {
  const page = await open(t);
  await page.getByRole("button", { name: "How to turn on debugging", exact: true }).tap();
  const sheet = page.getByRole("dialog", { name: "How to turn on debugging" });
  await sheet.getByLabel("TV brand").selectOption({ label: "Amazon Fire TV" });
  await page.reload({ waitUntil: "networkidle" });
  await page.evaluate(async () => {
    const { router } = await import("/src/lib/router.svelte.ts");
    router.reset("onboarding");
  });
  await page.getByRole("button", { name: "How to turn on debugging", exact: true }).tap();
  const again = page.getByRole("dialog", { name: "How to turn on debugging" });
  assert.equal(await again.getByLabel("TV brand").inputValue(), "firetv");
  assert.match(await steps(again), /Settings › My Fire TV › About and press select seven times on the device name/);
});
