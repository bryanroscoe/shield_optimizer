// Brand-specific steps for turning on debugging, inside the existing
// connection help: one shared JSON adapted from CleanYourTV (MIT), with a
// source for every variant and the generic steps wherever a source gives none.
//
// SIMULATED: demo fixtures with no devices, no TV.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { mkdir, readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");
const evidence = process.env.CONNECTION_SETUP_EVIDENCE;
process.env.VITE_DEMO = "1";
delete process.env.TAURI_DEV_HOST;

async function dataChecks(server) {
  const data = JSON.parse(await readFile(join(V2, "catalogs/connection-setup/connection-setup.json"), "utf8"));
  const notice = await readFile(join(V2, "catalogs/connection-setup/NOTICE.md"), "utf8");
  const license = await readFile(join(V2, "catalogs/connection-setup/LICENSE"), "utf8");
  assert.equal(data.schema, 1);
  assert.equal(data.source.license, "MIT");
  assert.match(data.source.commit, /^[0-9a-f]{40}$/);
  assert.ok(notice.includes(data.source.commit), "NOTICE names the pinned commit");
  assert.match(license, /^MIT License[\s\S]*Permission is hereby granted, free of charge/, "the full MIT notice sits beside the data");
  const SOURCE = /^(examples|docs)\/[A-Za-z0-9._-]+(\/[A-Za-z0-9._-]+)*$|^(README|PROMPT)\.md$/;
  const ids = data.brands.map((b) => b.id);
  assert.equal(new Set(ids).size, ids.length, "brand ids are unique");
  for (const required of ["shield", "google", "sony", "tcl", "hisense", "xiaomi", "philips", "onn", "firetv"]) {
    assert.ok(ids.includes(required), `${required} is covered`);
  }
  const menu = (path) => Array.isArray(path) && path.length > 0 && path.every((p) => typeof p === "string" && p.trim() && !/[>›→]/.test(p));
  for (const u of data.generic.unlock) assert.ok(menu(u.path) && u.action);
  for (const b of data.brands) {
    assert.ok(b.name && b.variants.length > 0, b.id);
    for (const v of b.variants) {
      const where = `${b.id}: ${v.models}`;
      assert.ok(v.models, where);
      assert.ok(["port", "pair", "either"].includes(v.connect), where);
      assert.ok(v.turn_on.length > 0 && v.turn_on.every((t) => t.name), `${where}: names a switch`);
      assert.ok(v.unlock === null || (menu(v.unlock.path) && v.unlock.action), `${where}: unlock`);
      assert.ok(v.developer_options === null || menu(v.developer_options), `${where}: developer options path`);
      assert.ok(v.sources.length > 0 && v.sources.every((s) => SOURCE.test(s)), `${where}: every step has a source`);
      assert.ok(v.notes.every((n) => typeof n === "string" && n.endsWith(".")), `${where}: notes are sentences`);
    }
  }
  for (const s of data.generic.sources) assert.ok(SOURCE.test(s));

  const m = await server.ssrLoadModule("/shared/connection-setup.ts");
  const shield = m.brandGuide("shield").variants[0];
  assert.match(shield.steps[0], /^If Developer options isn't there yet, open Settings › Device Preferences › About and press OK on Build seven times\./,
    "no Shield source gives the unlock step, so the generic one shows");
  assert.equal(shield.steps.at(-1), "Turn on Network debugging.");
  assert.equal(shield.connect, "port");
  const sony = m.brandGuide("sony").variants[0];
  assert.match(sony.steps[0], /^Show Developer options: open Settings › System › About and press OK on Android TV OS build seven times/);
  assert.match(sony.steps.at(-1), /^Turn on Wireless debugging, then choose Pair device with pairing code\.$/);
  const fireTvs = m.brandGuide("firetv").variants.find((v) => v.models.includes("smart TVs"));
  assert.ok(!fireTvs.steps.some((s) => s.includes("Device Preferences")), "Fire OS never gets Android TV's generic steps");
  assert.equal(fireTvs.steps[0], "Open Settings › Device & Software › Developer options.");
  assert.match(m.brandGuide("nokia").variants[1].steps.at(-1), /^Turn on USB debugging and Network debugging\.$/);
  assert.equal(m.brandGuide("nope"), null);
  assert.equal(m.genericGuide().connect, "either");
  assert.ok(m.sourceFileUrl("examples/x/debloat.txt").endsWith(`/blob/${data.source.commit}/examples/x/debloat.txt`));
  for (const b of m.brandList()) {
    for (const v of m.brandGuide(b.id).variants) {
      for (const text of [...v.steps, ...v.notes]) assert.doesNotMatch(text, /undefined|null|\s\./, `${b.id}: ${text}`);
    }
  }
  const store = new Map();
  const storage = { getItem: (k) => store.get(k) ?? null, setItem: (k, v) => store.set(k, v), removeItem: (k) => store.delete(k) };
  m.saveBrand(storage, "sony");
  assert.equal(m.savedBrand(storage), "sony");
  store.set("atvopt.connectionBrand", "gone");
  assert.equal(m.savedBrand(storage), null, "an unknown saved brand is ignored");
  console.log(`Connection setup data passed: ${data.brands.length} brands, ${data.brands.reduce((n, b) => n + b.variants.length, 0)} model groups, each sourced; generic fallback; Fire OS exception.`);
}

async function stepsText(scope) {
  return (await scope.locator("[data-setup-steps] li").allInnerTexts()).join("\n");
}

let server, browser;
try {
  server = await createServer({ root: V2, logLevel: "error", server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false } });
  await server.listen();
  await dataChecks(server);
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();
  if (evidence) await mkdir(evidence, { recursive: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 },
    ...(evidence ? { recordVideo: { dir: join(evidence, "raw"), size: { width: 1280, height: 900 } } } : {}) });
  const page = await context.newPage();
  await page.addInitScript(() => {
    if (!sessionStorage.getItem("seeded")) {
      localStorage.clear();
      sessionStorage.setItem("seeded", "1");
    }
    localStorage.setItem("shieldopt.demo.noDevices", "1");
  });
  await page.goto(base, { waitUntil: "networkidle" });
  if (evidence) await page.evaluate(() => {
    const label = document.createElement("div");
    label.textContent = "SIMULATED · demo fixtures, no TV";
    label.style.cssText = "position:fixed;bottom:0;right:0;z-index:99999;padding:6px 10px;background:#1a2540;color:white;font:13px sans-serif";
    document.body.append(label);
  });
  await page.getByRole("heading", { name: "No devices connected.", exact: true }).waitFor();
  const help = page.locator(".help-details").filter({ has: page.locator("summary", { hasText: "Connection help" }) });
  assert.equal(await help.evaluate((el) => el.open), true);
  const setup = help.locator("[data-brand-setup]");
  await setup.waitFor();
  assert.match(await stepsText(setup), /If Developer options isn't there yet/, "generic steps until a brand is picked");

  const brand = setup.getByLabel("TV brand");
  await brand.focus();
  await brand.selectOption({ label: "Nvidia Shield" });
  assert.match(await stepsText(setup), /Turn on Network debugging\.\nThen enter the TV's address in Add by IP/);
  assert.equal(await setup.getByRole("radio").count(), 0, "one model group: no model picker");
  if (evidence) await help.screenshot({ path: join(evidence, "shield-1280-dark.png") });

  await brand.selectOption({ label: "Sony" });
  const models = setup.getByRole("radio");
  assert.match(await stepsText(setup), /Android TV OS build seven times[\s\S]*Then use Pair PIN/);
  await models.nth(1).focus();
  await page.keyboard.press("Space");
  assert.match(await stepsText(setup), /Turn on ADB debugging \(USB debugging on some sets\)\.\nThen enter the TV's address in Add by IP/);
  await brand.selectOption({ label: "Amazon Fire TV" });
  assert.equal(await models.nth(0).isChecked(), true, "a new brand starts at its first model group");
  assert.match(await stepsText(setup), /Settings › My Fire TV › About and press select seven times on the device name/);

  for (const scheme of ["dark", "light"]) {
    await page.evaluate((s) => document.documentElement.setAttribute("data-theme", s), scheme);
    for (const [width, size] of [[1280, "100%"], [720, "100%"], [720, "150%"]]) {
      await page.setViewportSize({ width, height: 900 });
      await page.evaluate((s) => (document.documentElement.style.fontSize = s), size);
      const box = await setup.boundingBox();
      assert.ok(box.x >= 0 && box.x + box.width <= width, `${scheme} ${width}px ${size}: fits`);
      const clipped = await setup.evaluate((el) => [...el.querySelectorAll("select, li, p, label, legend")]
        .filter((n) => n.scrollWidth > n.clientWidth + 1).map((n) => n.textContent.trim().slice(0, 40)));
      assert.deepEqual(clipped, [], `${scheme} ${width}px ${size}: nothing clipped`);
      if (evidence) await help.screenshot({ path: join(evidence, `firetv-${width}-${size.replace("%", "")}-${scheme}.png`) });
    }
  }
  await page.evaluate(() => { document.documentElement.style.fontSize = ""; document.documentElement.setAttribute("data-theme", "dark"); });
  await page.setViewportSize({ width: 1280, height: 900 });

  const link = setup.getByRole("link", { name: "CleanYourTV" });
  assert.match(await link.getAttribute("href"), /^https:\/\/github\.com\/Gathin23\/cleanyourtv\/tree\/[0-9a-f]{40}$/);

  // The choice is remembered, and the Pair PIN help shows the same guide.
  await page.reload({ waitUntil: "networkidle" });
  await page.getByRole("button", { name: "Pair PIN", exact: true }).first().click();
  const pairHelp = page.locator(".pair-form .help-details");
  await pairHelp.locator("summary").click();
  const pairSetup = pairHelp.locator("[data-brand-setup]");
  assert.equal(await pairSetup.getByLabel("TV brand").inputValue(), "firetv", "the last brand picked is remembered");
  await pairSetup.getByLabel("TV brand").selectOption({ label: "Google (Chromecast, Google TV Streamer)" });
  assert.match(await stepsText(pairSetup), /Pair device with pairing code\.\nThen use Pair PIN/);
  if (evidence) await page.locator(".pair-form").screenshot({ path: join(evidence, "pair-panel-google.png") });
  await pairSetup.getByLabel("TV brand").selectOption({ label: "Other or not sure" });
  assert.equal(await page.evaluate(() => localStorage.getItem("atvopt.connectionBrand")), null, "Other forgets the pick");
  if (evidence) {
    const video = page.video();
    await context.close();
    await video.saveAs(join(evidence, "connection-setup-simulated.webm"));
  } else await context.close();
  console.log("Connection setup help passed: generic until a brand is picked, Shield/Sony/Fire TV/Google steps, model groups, remembered brand, Pair PIN help, keyboard, 1280/720/150% in both themes.");
} finally {
  await browser?.close();
  await server?.close();
}
