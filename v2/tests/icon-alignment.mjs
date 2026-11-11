// Regression guard for icon/label vertical alignment.
//
// Two rules centre every Material Symbols glyph on its label:
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
// catches a control parked above its neighbour on the same row (Forget beside
// the chevron on the Devices list), which the icon-vs-label check cannot see
// because it never looks across siblings.
//
// Run via `npm run test:icon-alignment` from v2/.

import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { mkdir, rename } from "node:fs/promises";
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
  // Resizable textarea alignment is checked against its complete content row
  // below rather than treated as a single-line control here.
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
// skips rows that ask for top alignment (`.device-row.unauthorized`), so it
// cannot catch a row's actions drifting above the icon's centre.
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


const COLLECT_CONTENT_ACTIONS = (max) => {
  const results = [];
  const specs = [
    [".snap-list li", ".snap-main", ".snap-actions"],
    [".snap-table tbody tr", null, ".snap-actions"],
    [".preview-head", ":scope > div", ":scope > button"],
    [".files-table tbody tr", null, ".row-actions"],
    [".shell-run", ".shell-prompt", ".run-wrap"],
    [".settings .row", ".row-text", ".row-control"],
    [".card-header", ":scope > .header-title, :scope > h2", ":scope > .header-actions, :scope > button"],
  ];
  for (const [selector, contentSelector, actionSelector] of specs) {
    for (const row of document.querySelectorAll(selector)) {
      const actions = row.querySelector(actionSelector);
      const content = contentSelector ? row.querySelector(contentSelector) : row;
      if (!actions || !content) continue;
      const a = actions.getBoundingClientRect(), c = content.getBoundingClientRect(), r = row.getBoundingClientRect();
      if (!a.height || !c.height) continue;
      const stacked = content !== row && a.top >= c.bottom - 1;
      const style = getComputedStyle(row);
      const end = r.right - parseFloat(style.paddingRight) - parseFloat(style.borderRightWidth || 0);
      const delta = stacked ? a.right - end : a.top + a.height / 2 - c.top - c.height / 2;
      if (Math.abs(delta) > max) results.push({ selector, text: `${row.textContent.trim().slice(0, 65)} (${stacked ? "wrapped right edge" : "content/action centre"})`, delta, source: "content-actions" });
      if (selector.startsWith(".files-table")) {
        const cell = actions.closest("td").getBoundingClientRect();
        if (Math.abs(cell.height - r.height) > max) results.push({ selector, text: "action cell must fill its table row", delta: cell.height - r.height, source: "table-cell" });
      }
      if (a.right > r.right + max) results.push({ selector, text: "actions extend outside their row", delta: a.right - r.right, source: "overflow" });
    }
  }
  return results;
};

async function collectContentActions(page, screen, offenders) {
  const records = await page.evaluate(COLLECT_CONTENT_ACTIONS, MAX_DELTA);
  for (const record of records) offenders.push({ screen, ...record });
}

async function stressLayouts(page, base, offenders) {
  const evidence = process.env.ALIGNMENT_EVIDENCE;
  if (evidence) await mkdir(evidence, { recursive: true });
  const capture = async (name) => {
    if (!evidence) return;
    await page.waitForTimeout(800);
    await page.screenshot({ path: `${evidence}/${name}.png`, fullPage: true });
  };
  for (const theme of ["light", "dark"]) {
    await page.evaluate(theme => localStorage.setItem("shieldopt.theme", theme), theme);
    await page.setViewportSize({ width: 1100, height: 900 });
    await page.goto(`${base}/devices/${encodeURIComponent(SERIAL)}`, { waitUntil: "networkidle" });
    await page.locator("#tab-snapshot").click();
    await page.locator(".snap-list li").first().waitFor();
    await page.evaluate(() => document.fonts.ready);
    if (evidence) await page.evaluate(() => {
      const label = document.createElement("div"); label.textContent = "SIMULATED desktop · fixture snapshots · no TV";
      label.style.cssText = "position:fixed;bottom:0;right:0;z-index:99999;background:#17243a;color:white;padding:8px;font:13px sans-serif;pointer-events:none";
      document.body.append(label);
    });
    const firstRow = page.locator(".snap-list li").first();
    await firstRow.getByRole("button", { name: "Preview restore", exact: true }).focus();
    // macOS WebKit uses Option-Tab to include buttons when full keyboard access is off.
    await page.keyboard.press(process.platform === "darwin" && process.argv.includes("--webkit") ? "Alt+Tab" : "Tab");
    assert.ok(await firstRow.getByRole("button", { name: /^Delete / }).evaluate(el => el === document.activeElement), "snapshot actions keep their keyboard order");
    await collectContentActions(page, `snapshot/${theme}/1100`, offenders);
    await capture(`snapshot-centred-${theme}`);
    const originals = await page.locator(".snap-list .snap-title strong, .snap-list .snap-meta").allTextContents();
    await page.evaluate(() => {
      for (const el of document.querySelectorAll(".snap-list .snap-title strong")) el.textContent = "Living room television — saved configuration with a long descriptive name";
      for (const el of document.querySelectorAll(".snap-list .snap-meta")) el.textContent = "2026-06-02 14:40:00 UTC · 38 disabled · 14 settings · launcher com.example.launcher." + "longpackage".repeat(12);
    });
    for (const [width, fontSize] of [[1280, "16px"], [720, "16px"], [1440, "32px"]]) {
      await page.setViewportSize({ width, height: 1000 });
      await page.evaluate(size => document.documentElement.style.fontSize = size, fontSize);
      await collectContentActions(page, `snapshot/${theme}/${width}/${fontSize}`, offenders);
      const overflow = await page.evaluate(() => ({ width: innerWidth, scroll: document.documentElement.scrollWidth,
        outside: [...document.querySelectorAll("main *")].filter(el => { const r = el.getBoundingClientRect(); return r.height && r.right > innerWidth + 1; }).slice(-15).map(el => ({ tag: el.tagName, class: el.className, text: el.textContent.slice(0, 40), right: el.getBoundingClientRect().right, overflow: getComputedStyle(el).overflowX })) }));
      if (overflow.scroll > overflow.width) {
        console.error("Layout overflow", width, fontSize, JSON.stringify(overflow));
        offenders.push({ screen: `snapshot/${theme}/${width}/${fontSize}`, selector: "document", text: "page overflows", delta: overflow.scroll - overflow.width, source: "overflow" });
      }
      if (theme === "dark" && width === 720) await capture("snapshot-long-narrow");
    }
    await page.evaluate(originals => {
      document.documentElement.style.fontSize = "";
      [...document.querySelectorAll(".snap-list .snap-title strong, .snap-list .snap-meta")].forEach((el, i) => el.textContent = originals[i]);
    }, originals);
    await page.setViewportSize({ width: 1100, height: 900 });
    await page.locator(".snap-list li").first().getByRole("button", { name: "Preview restore", exact: true }).click();
    await page.getByRole("button", { name: "Restore this snapshot", exact: true }).waitFor();
    await collectContentActions(page, `snapshot/${theme}/selected`, offenders);
    await capture(`snapshot-selected-${theme}`);
    await page.locator("#tab-shell").click();
    await page.locator(".shell-input").evaluate(el => el.style.height = "160px");
    await collectContentActions(page, `shell/${theme}/resized-input`, offenders);
    await page.locator("#tab-files").click();
    await page.locator(".files-table tbody tr").first().waitFor();
    await collectContentActions(page, `files/${theme}`, offenders);
    await page.getByRole("link", { name: "Snapshots", exact: true }).click();
    await page.locator(".snap-table tbody tr").first().waitFor();
    await collectContentActions(page, `global-snapshots/${theme}`, offenders);
    await page.locator(".snap-actions select").first().selectOption({ index: 1 });
    await page.locator(".preview-head").waitFor();
    await collectContentActions(page, `global-snapshots/${theme}/preview`, offenders);
    await capture(`global-snapshots-${theme}`);
    await page.locator(".preview-head").getByRole("button", { name: "Close", exact: true }).click();
    await page.setViewportSize({ width: 720, height: 900 });
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "snapshot table scrolls inside its own container");
    await page.getByRole("link", { name: "Settings", exact: true }).click();
    await page.locator("[data-support-id]").waitFor();
    await collectContentActions(page, `settings/${theme}/720`, offenders);
    await page.setViewportSize({ width: 560, height: 900 });
    await collectContentActions(page, `settings/${theme}/560`, offenders);
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "narrow Settings does not overflow");
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
    const { chromium, webkit } = await import("playwright");
    server = await createServer({
      root: V2,
      server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false },
    });
    await server.listen();
    const address = server.httpServer?.address();
    if (!address || typeof address === "string") throw new Error("Vite did not expose its TCP address");
    const base = `http://127.0.0.1:${address.port}`;

    browser = await (process.argv.includes("--webkit") ? webkit : chromium).launch();
    const evidence = process.env.ALIGNMENT_EVIDENCE;
    if (evidence) await mkdir(evidence, { recursive: true });
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
      await collectContentActions(page, `device/${tab}`, offenders);
    }

    await page.goto(`${base}/snapshots`, { waitUntil: "networkidle" });
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(300);
    await collect(page, "snapshots", offenders, seen);
    await collectSiblings(page, "snapshots", offenders);
    await collectContentActions(page, "snapshots", offenders);
    const stressPage = evidence
      ? await browser.newPage({ recordVideo: { dir: evidence, size: { width: 1280, height: 1000 } } })
      : page;
    if (evidence) await stressPage.goto(base, { waitUntil: "networkidle" });
    await stressLayouts(stressPage, base, offenders);
    if (evidence) {
      await stressPage.close();
      await rename(await stressPage.video().path(), `${evidence}/desktop-polish-simulated.webm`);
    }

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
        `controls in every flex row share a centre line; snapshot/file rows, resizable Shell input and Settings actions pass content-centre/wrap checks in both themes, narrow layouts and 200% text.`,
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
