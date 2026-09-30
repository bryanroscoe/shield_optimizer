// Regression guard for icon/label vertical alignment.
//
// d35781a centred every Material Symbols glyph on its label with two rules:
// `button:has(> .msr)` / `h1,h2,h3:has(> .msr)` become flex containers with
// `align-items: center`, and `.msr` carries `vertical-align: middle` as the
// inline fallback. Both are easy to lose by accident — a new button that nests
// its icon inside a wrapper span drops out of the `> .msr` child selector and
// the glyph parks itself back on the baseline.
//
// This walks the demo app (VITE_DEMO=1, no device) and, for every visible
// button/heading that directly contains an icon, compares the icon's
// bounding-box centre-Y against the centre-Y of the element's own text.
// Anything off by more than MAX_DELTA px is printed and fails the run.
//
// It also runs a sibling check. For every horizontal flex row that holds a
// button, the direct children that are controls (a button, a field, a
// wrapper around one, or an icon on its own) must share a centre line. That
// is the Forget-vs-chevron case on the Devices list: `align-self: flex-start`
// on the actions wrapper parked Forget above the chevron beside it, and the
// icon-vs-label check never looked across siblings.
//
// Run via `npm run test:icon-alignment` from v2/.

import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");
const SERIAL = "192.168.1.42:5555";

// A sub-pixel wobble is inherent: glyph metrics and text baselines rarely land
// on exactly the same half-pixel. 1.5px is tight enough to catch a baseline
// drop (which is several px at these font sizes) and loose enough not to flake.
const MAX_DELTA = 1.5;

// The device tabs, in the order screenshots/capture.mjs visits them. Each entry
// is the tab's id (the `#tab-*` ids that capture.mjs drives) and a short label
// used only in failure output.
const DEVICE_TABS = [
  "overview",
  "health",
  "media",
  "launcher",
  "apps",
  "optimize",
  "tweaks",
  "remote",
  "files",
  "sideload",
  "snapshot",
  "shell",
];

// Runs in the page. Returns one record per visible element that directly
// contains an icon, with the icon's centre-Y, the label's centre-Y, and enough
// context to identify the offender from the console.
const COLLECT = () => {
  // A readable, stable-ish selector for the console: tag plus id/class.
  const describe = (el) => {
    const id = el.id ? `#${el.id}` : "";
    const cls = el.classList.length ? `.${Array.from(el.classList).join(".")}` : "";
    return `${el.tagName.toLowerCase()}${id}${cls}`;
  };

  // The label's vertical centre. Prefer a Range over the element's own
  // non-icon text nodes (that is the actual glyph box of the words); fall
  // back to the first non-icon child element when the label is wrapped.
  const labelRect = (el) => {
    const range = document.createRange();
    let found = false;
    for (const node of el.childNodes) {
      if (node.nodeType !== Node.TEXT_NODE) continue;
      if (!node.textContent.trim()) continue;
      const start = node.textContent.search(/\S/);
      const end = node.textContent.search(/\S\s*$/) + 1;
      if (!found) {
        range.setStart(node, start);
        found = true;
      }
      range.setEnd(node, end);
    }
    if (found) {
      const rects = Array.from(range.getClientRects()).filter((r) => r.height > 0);
      if (rects.length) return { rect: rects[0], source: "text" };
    }
    for (const child of el.children) {
      if (child.classList.contains("msr")) continue;
      const rect = child.getBoundingClientRect();
      if (rect.height > 0 && child.textContent.trim()) return { rect, source: "child" };
    }
    return null;
  };

  const results = [];
  for (const el of document.querySelectorAll("button, h1, h2, h3")) {
    // Only elements whose icon is a *direct* child — that is exactly what the
    // `:has(> .msr)` rules match, and what this test is guarding.
    const icon = Array.from(el.children).find((c) => c.classList.contains("msr"));
    if (!icon) continue;
    const box = el.getBoundingClientRect();
    if (box.width === 0 || box.height === 0) continue;
    const style = getComputedStyle(el);
    if (style.visibility === "hidden" || style.display === "none") continue;

    const iconRect = icon.getBoundingClientRect();
    if (iconRect.height === 0) continue;
    const label = labelRect(el);
    if (!label) continue; // icon-only control: nothing to align against.

    // Wrapped labels span several lines; the first line's centre is not the
    // icon's reference point, so skip them rather than report a false hit.
    if (label.rect.height > iconRect.height * 2.2) continue;

    results.push({
      selector: describe(el),
      text: el.textContent.replace(/\s+/g, " ").trim().slice(0, 60),
      delta:
        iconRect.top + iconRect.height / 2 - (label.rect.top + label.rect.height / 2),
      source: label.source,
    });
  }
  return results;
};

// Runs in the page. Returns one record per pair of sibling controls in the
// same flex row whose centre lines differ by more than `max` px.
const COLLECT_SIBLINGS = (max) => {
  const describe = (el) => {
    const id = el.id ? `#${el.id}` : "";
    const cls = el.classList.length ? `.${Array.from(el.classList).join(".")}` : "";
    return `${el.tagName.toLowerCase()}${id}${cls}`;
  };
  const visible = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return false;
    const st = getComputedStyle(el);
    return st.visibility !== "hidden" && st.display !== "none";
  };
  // Text a reader sees, without the icon font's ligature names.
  const words = (el) => {
    const clone = el.cloneNode(true);
    for (const icon of clone.querySelectorAll(".msr")) icon.remove();
    return clone.textContent.trim();
  };
  // A textarea is a multi-line field whose top edge is the deliberate anchor
  // (ShellTab's Run button), so it is not a single-line control here.
  const CONTROL = "button, input:not([type=hidden]), select, [role=button]";
  const isControl = (el) =>
    el.matches(CONTROL) ||
    el.querySelector(CONTROL) !== null ||
    (el.matches(".msr") || (el.querySelector(".msr") && !words(el)));

  const results = [];
  for (const row of document.querySelectorAll("*")) {
    const st = getComputedStyle(row);
    if (st.display !== "flex" && st.display !== "inline-flex") continue;
    if (st.flexDirection !== "row" && st.flexDirection !== "row-reverse") continue;
    // A row that asks for top or baseline alignment has said its children
    // are not meant to share a centre line.
    if (/start|end|baseline/.test(st.alignItems)) continue;
    if (!row.querySelector("button") || !visible(row)) continue;
    const controls = Array.from(row.children).filter((c) => visible(c) && isControl(c));
    if (controls.length < 2) continue;
    const boxes = controls.map((c) => ({ el: c, rect: c.getBoundingClientRect() }));
    for (let i = 0; i < boxes.length; i++) {
      for (let j = i + 1; j < boxes.length; j++) {
        const a = boxes[i].rect;
        const b = boxes[j].rect;
        // Different lines of a wrapped row do not share a centre line.
        if (a.bottom <= b.top || b.bottom <= a.top) continue;
        // One control more than twice the other's height is a block beside a
        // control, not two controls in a line.
        if (Math.max(a.height, b.height) > 2 * Math.min(a.height, b.height)) continue;
        const delta = a.top + a.height / 2 - (b.top + b.height / 2);
        if (Math.abs(delta) <= max) continue;
        results.push({
          selector: describe(row),
          text: `${describe(boxes[i].el)} vs ${describe(boxes[j].el)}`,
          delta,
          source: "sibling",
        });
      }
    }
  }
  return results;
};

// Runs in the page. A Devices row's actions must sit on the device icon's
// centre line whatever the row's own `align-items` says. The sibling check
// skips rows that ask for top alignment, which is exactly how the not-a-TV
// row's Open anyway / Copy diagnostics / Forget drifted above the icon's
// centre (`.device-row.not-clickable { align-items: flex-start }`).
const COLLECT_ROW_ACTIONS = (max) => {
  const results = [];
  let checked = 0;
  let notClickable = 0;
  for (const row of document.querySelectorAll(".device-row")) {
    const icon = row.querySelector(":scope > .device-icon");
    const actions = row.querySelector(":scope > .row-actions");
    if (!icon || !actions || !actions.querySelector("button")) continue;
    const a = icon.getBoundingClientRect();
    const b = actions.getBoundingClientRect();
    if (a.height === 0 || b.height === 0) continue;
    checked += 1;
    if (row.classList.contains("not-clickable")) notClickable += 1;
    const delta = a.top + a.height / 2 - (b.top + b.height / 2);
    if (Math.abs(delta) > max) {
      results.push({
        selector: `.device-row${row.classList.contains("not-clickable") ? ".not-clickable" : ""}`,
        text: `${row.querySelector(".device-name")?.textContent?.trim().slice(0, 40)}: icon vs row-actions`,
        delta,
        source: "row-actions",
      });
    }
  }
  return { results, checked, notClickable };
};

async function collectRowActions(page, screen, offenders) {
  const { results, checked, notClickable } = await page.evaluate(COLLECT_ROW_ACTIONS, MAX_DELTA);
  assert.ok(checked > 0, `${screen}: no device rows with actions to check`);
  assert.ok(notClickable > 0, `${screen}: the not-a-TV row must be among the rows checked`);
  for (const record of results) offenders.push({ screen, ...record });
}

async function collectSiblings(page, screen, offenders) {
  const records = await page.evaluate(COLLECT_SIBLINGS, MAX_DELTA);
  for (const record of records) offenders.push({ screen, ...record });
}

async function collect(page, screen, offenders, seen) {
  const records = await page.evaluate(COLLECT);
  assert.ok(records.length > 0, `${screen}: found no icon-bearing buttons or headings to check`);
  for (const record of records) {
    seen.count += 1;
    if (Math.abs(record.delta) > MAX_DELTA) offenders.push({ screen, ...record });
  }
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
    server = await createServer({
      root: V2,
      server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false },
    });
    await server.listen();
    const address = server.httpServer?.address();
    if (!address || typeof address === "string") throw new Error("Vite did not expose its TCP address");
    const base = `http://127.0.0.1:${address.port}`;

    browser = await chromium.launch();
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    // The demo's not-a-TV row, so the Devices check covers the non-clickable
    // row shape as well as the link rows.
    await page.addInitScript(() => localStorage.setItem("shieldopt.demo.notTv", "1"));
    const offenders = [];
    const seen = { count: 0 };

    await page.goto(base, { waitUntil: "networkidle" });
    await page.getByText("NVIDIA SHIELD", { exact: false }).first().waitFor();
    await page.getByText("Pixel Tablet", { exact: true }).waitFor();
    // The icon font has to be loaded before any glyph box means anything.
    await page.evaluate(() => document.fonts.ready);
    await collect(page, "devices", offenders, seen);
    await collectSiblings(page, "devices", offenders);
    await collectRowActions(page, "devices", offenders);

    await page.goto(`${base}/devices/${encodeURIComponent(SERIAL)}`, { waitUntil: "networkidle" });
    await page.locator("#tab-overview").waitFor();
    await page.evaluate(() => document.fonts.ready);
    for (const tab of DEVICE_TABS) {
      await page.locator(`#tab-${tab}`).click();
      await page.locator(`#tabpanel-${tab}`).waitFor();
      if (tab === "optimize") {
        // The wizard renders its per-app rows (and their icons) only once a
        // plan has been built, same as screenshots/capture.mjs does.
        await page.getByRole("button", { name: "Optimize", exact: true }).click();
        await page.waitForTimeout(600);
      }
      await page.waitForTimeout(300);
      await collect(page, `device/${tab}`, offenders, seen);
      await collectSiblings(page, `device/${tab}`, offenders);
    }

    await page.goto(`${base}/snapshots`, { waitUntil: "networkidle" });
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(300);
    await collect(page, "snapshots", offenders, seen);
    await collectSiblings(page, "snapshots", offenders);

    if (offenders.length) {
      console.error(`\n${offenders.length} misaligned icon/label or sibling pair(s):\n`);
      for (const o of offenders) {
        console.error(
          `  [${o.screen}] ${o.selector}  Δ=${o.delta.toFixed(2)}px (label via ${o.source})\n` +
            `      text: ${JSON.stringify(o.text)}`,
        );
      }
      console.error("");
    }
    assert.equal(
      offenders.length,
      0,
      `icon glyphs and sibling controls must sit within ${MAX_DELTA}px of a shared centre`,
    );
    console.log(
      `Icon alignment passed: ${seen.count} icon-bearing buttons and headings across ` +
        `${DEVICE_TABS.length + 2} screens are centred within ${MAX_DELTA}px, and sibling ` +
        `controls in every flex row share a centre line.`,
    );
    await page.close();
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
