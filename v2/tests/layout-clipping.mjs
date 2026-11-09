// Long ids and names must fit their cells at the 1360px window and the 720px
// desktop minimum.
//
// - Snapshot restore preview: the Item column keeps a readable width, package
//   ids break after their dots rather than every few letters, long values
//   (a screensaver component, a Home app id) wrap inside Now and Will become,
//   and no cell runs past its box or past the table's rounded frame.
// - App List: a name too long for its row wraps onto a second line, and past
//   two lines ends in an ellipsis inside the row with the full name in
//   `title`, instead of being cut mid-glyph; at 720px
//   the Actions column wraps so the App column keeps a readable width and the
//   table stays inside the card.
// - Health memory table: process names break at their separators.
//
// Runs against the demo fixture layer (VITE_DEMO=1); the restore plan and one
// app name are swapped for the long values a real Shield produced.
// Set LAYOUT_EVIDENCE_DIR=/abs/dir to save a screenshot of each checked screen.
//
// Run via `npm run test:layout-clipping` from v2/.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");
// The 1360px window, the 720px desktop minimum, and 150% text (a 24px root
// font). Both themes, since the light and dark captures are separate.
const CASES = [
  { width: 1360, fontSize: "", colorScheme: "light", label: "1360" },
  { width: 720, fontSize: "", colorScheme: "dark", label: "720" },
  { width: 1360, fontSize: "24px", colorScheme: "dark", label: "1360-150pct-text" },
];
const EVIDENCE = process.env.LAYOUT_EVIDENCE_DIR || null;
const LONG_NAME = "Amazon Prime Video (Shield preinstalled build for the living room TV)";

// What the e2e simulated Shield previews after a few changes.
const LONG_PLAN = {
  packages_to_disable: ["com.android.printspooler", "com.google.android.tvrecommendations"],
  packages_already_disabled: [
    "com.nvidia.shieldtech.hooks",
    "com.google.android.tungsten.setupwraith",
    "com.amazon.amazonvideo.livingroom.nvidia",
  ],
  packages_not_installed: ["com.nvidia.benchmarkblocker"],
  launcher_to_set: "com.spocky.projengmenu",
  current_launcher: "com.google.android.tvlauncher",
  launcher_not_installed: null,
  launcher_disabled: null,
  packages_kept: [],
  settings_to_write: {
    "global.window_animation_scale": "0.5",
    "secure.screensaver_components": "com.google.android.apps.tv.dreamx/.service.Backdrop",
  },
  settings_to_delete: ["global.encoded_surround_output_enabled_formats"],
  settings_already_set: ["global.hdmi_control_auto_device_off_enabled"],
  current_values: {
    "global.window_animation_scale": "1.0",
    "secure.screensaver_components": "com.google.android.backdrop/.Backdrop",
    "global.encoded_surround_output_enabled_formats": "5,6,7,8,14,15,18,19",
    "global.hdmi_control_auto_device_off_enabled": "1",
  },
  cross_device_warning: null,
};

function serverURL(server) {
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("no address");
  const host = address.address.includes(":") ? `[${address.address}]` : address.address;
  return `http://${host}:${address.port}`;
}

function setHarnessEnvironment() {
  const keys = ["VITE_DEMO", "TAURI_DEV_HOST"];
  const previous = new Map(keys.map((k) => [k, { present: Object.hasOwn(process.env, k), value: process.env[k] }]));
  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;
  return () => {
    for (const [k, prior] of previous) {
      if (prior.present) process.env[k] = prior.value;
      else delete process.env[k];
    }
  };
}

// In the page: every soft line break inside `el` that is not clean. A break
// is clean after a separator or at a space, or inside a segment (the text
// after one separator up to and including the next) that is wider than the
// element's content box on its own, where nothing else could fit it.
const LINE_BREAKS = (el) => {
  const SEP = /[.:/_@\-,\s]/;
  const chars = [];
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    for (let i = 0; i < node.data.length; i++) {
      const range = document.createRange();
      range.setStart(node, i);
      range.setEnd(node, i + 1);
      const rect = Array.from(range.getClientRects()).find((r) => r.width > 0);
      if (rect) chars.push({ ch: node.data[i], top: rect.top, height: rect.height, width: rect.width });
      // A space the line wrapped at collapses to no box; keep it as a marker.
      else if (/\s/.test(node.data[i]) && chars.length) chars.push({ ...chars[chars.length - 1], ch: " ", width: 0 });
    }
  }
  const style = getComputedStyle(el);
  const box = el.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
  const text = chars.map((c) => c.ch).join("");
  const bad = [];
  for (let i = 1; i < chars.length; i++) {
    if (chars[i].top - chars[i - 1].top <= chars[i - 1].height / 2) continue;
    if (SEP.test(chars[i - 1].ch) || /\s/.test(chars[i].ch)) continue;
    // The segment runs to and includes its trailing separator, which stays on
    // the segment's line: "android." has to fit, not just "android".
    let start = i;
    while (start > 0 && !SEP.test(chars[start - 1].ch)) start--;
    let end = i;
    while (end < chars.length && !SEP.test(chars[end].ch)) end++;
    if (end < chars.length && !/\s/.test(chars[end].ch)) end++;
    const segment = chars.slice(start, end).reduce((sum, c) => sum + c.width, 0);
    if (segment > box) continue;
    bad.push({ text, at: i, segment: text.slice(start, end), segmentWidth: Math.round(segment), box: Math.round(box) });
  }
  return bad;
};

async function openDevice(page, base, { plan = false, longName = false } = {}) {
  await page.goto(base, { waitUntil: "networkidle" });
  if (page.rootFontSize) await page.evaluate((size) => (document.documentElement.style.fontSize = size), page.rootFontSize);
  await page.evaluate(
    ({ plan, longName }) => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === "preview_apply" && plan) return structuredClone(plan);
        const result = await original(command, args);
        if (command === "app_list_for_device" && longName && Array.isArray(result)) {
          return result.map((app) =>
            app.package === "com.amazon.amazonvideo.livingroom" ? { ...app, name: longName } : app,
          );
        }
        return result;
      };
    },
    { plan: plan ? LONG_PLAN : null, longName: longName ? LONG_NAME : null },
  );
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
}

// Nothing on the screen may widen the window into a horizontal scroll.
async function noPageScroll(page, where) {
  const { scroll, viewport } = await page.evaluate(() => ({
    scroll: document.documentElement.scrollWidth,
    viewport: document.documentElement.clientWidth,
  }));
  assert.ok(scroll <= viewport, `${where}: the page scrolls sideways (${scroll}px wide in a ${viewport}px window)`);
}

async function shot(page, name) {
  if (!EVIDENCE) return;
  await page.screenshot({ path: join(EVIDENCE, `${name}.png`), fullPage: false, animations: "disabled" });
}

async function restorePreview(page, base, width) {
  await openDevice(page, base, { plan: true });
  await page.getByRole("tab", { name: /Snapshot/ }).click();
  await page.getByRole("button", { name: "Preview restore" }).first().click();
  const table = page.locator("#tabpanel-snapshot .plan-table");
  await table.waitFor({ timeout: 10000 });
  await table.locator("tbody tr").first().waitFor();
  await page.locator("#tabpanel-snapshot .plan-box").scrollIntoViewIfNeeded();
  await shot(page, `restore-preview-${width}`);

  const layout = await table.evaluate((t) => {
    const box = t.closest(".plan-box").getBoundingClientRect();
    const cells = [...t.querySelectorAll("th, td")].map((cell) => {
      const r = cell.getBoundingClientRect();
      return {
        text: cell.innerText.trim(),
        column: cell.cellIndex,
        overflow: cell.scrollWidth - cell.clientWidth,
        outside: Math.max(0, r.right - box.right, box.left - r.left),
      };
    });
    return {
      tableWidth: t.getBoundingClientRect().width,
      boxWidth: box.width,
      itemWidth: t.querySelector("thead th").getBoundingClientRect().width,
      cells,
    };
  });
  assert.ok(
    layout.tableWidth <= layout.boxWidth + 1,
    `${width}: the plan table (${layout.tableWidth}px) is wider than its box (${layout.boxWidth}px)`,
  );
  const minItem = Math.min(180, layout.tableWidth * 0.3);
  assert.ok(
    layout.itemWidth >= minItem,
    `${width}: Item column is ${layout.itemWidth.toFixed(0)}px of ${layout.tableWidth.toFixed(0)}px; needs at least ${minItem.toFixed(0)}px`,
  );
  const spilled = layout.cells.filter((c) => c.overflow > 1 || c.outside > 1);
  assert.deepEqual(spilled, [], `${width}: plan cells must not overflow or leave the box`);
  await noPageScroll(page, `${width} restore preview`);
  const contents = await page.locator("#tabpanel-snapshot .snap-contents-list").evaluate((dl) => {
    const box = dl.getBoundingClientRect();
    return [...dl.children]
      .filter((el) => el.scrollWidth > el.clientWidth + 1 || el.getBoundingClientRect().right > box.right + 1)
      .map((el) => el.textContent);
  });
  assert.deepEqual(contents, [], `${width}: the snapshot's contents list must stay inside its box`);
  for (const value of ["com.google.android.backdrop/.Backdrop", "com.google.android.apps.tv.dreamx/.service.Backdrop"]) {
    assert.ok(
      layout.cells.some((c) => c.text.includes(value)),
      `${width}: the long value ${value} is shown in full`,
    );
  }

  const items = await table.locator("tbody td").evaluateAll(
    (tds, fn) => tds.map((td) => new Function(`return (${fn})`)()(td)),
    LINE_BREAKS.toString(),
  );
  const midWord = items.flat();
  assert.deepEqual(midWord, [], `${width}: ids and values in the plan must break after a separator, not mid-word`);
}

async function appList(page, base, width) {
  await openDevice(page, base, { longName: true });
  await page.getByRole("tab", { name: "App List" }).click();
  const names = page.locator(".app-name");
  await names.first().waitFor({ timeout: 15000 });
  await page.locator(".app-name", { hasText: LONG_NAME }).scrollIntoViewIfNeeded();
  await shot(page, `app-list-${width}`);
  const rows = await names.evaluateAll((els) =>
    els
      .filter((el) => el.getClientRects().length > 0)
      .map((el) => {
        const row = el.closest(".app-name-row").getBoundingClientRect();
        const r = el.getBoundingClientRect();
        const style = getComputedStyle(el);
        return {
          name: el.textContent,
          title: el.getAttribute("title"),
          truncated: el.scrollWidth > el.clientWidth + 1 || el.scrollHeight > el.clientHeight + 1,
          ellipsis:
            style.overflow !== "visible" &&
            (style.textOverflow === "ellipsis" || (style.webkitLineClamp && style.webkitLineClamp !== "none")),
          outside: Math.max(0, r.right - row.right, row.left - r.left),
        };
      }),
  );
  assert.ok(rows.length > 10, `${width}: the App List rendered rows`);
  const appColumn = await page.locator(".app-table td.app-cell").first().evaluate((td) => td.getBoundingClientRect().width);
  // ~140px holds a short name whole ("Google Feedback") at 720px; it was ~40px
  // before the Actions column learned to wrap.
  assert.ok(appColumn >= 140, `${width}: the App column is ${appColumn.toFixed(0)}px; names need at least 140px`);
  await noPageScroll(page, `${width} App List`);
  const clipped = rows.filter((r) => r.outside > 1);
  assert.deepEqual(clipped, [], `${width}: an app name must stay inside its row, not be cut by it`);
  const long = rows.find((r) => r.name === LONG_NAME);
  assert.ok(long, `${width}: the long-named app is listed`);
  assert.ok(long.truncated, `${width}: the fixture name is long enough to need truncating`);
  // In the wide window every catalog name is whole. At 720px a single word
  // wider than the column ("Recommendations") can still need the ellipsis.
  if (width === "1360") {
    const cut = rows.filter((r) => r.truncated && r.name !== LONG_NAME).map((r) => r.name);
    assert.deepEqual(cut, [], `${width}: every catalog name is shown whole`);
  }
  for (const r of rows.filter((row) => row.truncated)) {
    assert.ok(r.ellipsis, `${width}: ${r.name} is truncated without an ellipsis`);
    assert.equal(r.title, r.name, `${width}: a truncated name keeps its full text in title`);
  }
}

async function memoryTable(page, base, width) {
  await openDevice(page, base);
  await page.getByRole("tab", { name: "Health" }).click();
  const cells = page.locator(".mem-table td.pkg");
  await cells.first().waitFor({ timeout: 15000 });
  await page.locator(".mem-table").scrollIntoViewIfNeeded();
  await shot(page, `health-memory-${width}`);
  const names = await page
    .locator(".mem-table td.pkg > .mem-open, .mem-table td.pkg > .mem-process")
    .evaluateAll((els, fn) => els.map((el) => new Function(`return (${fn})`)()(el)), LINE_BREAKS.toString());
  assert.ok(names.length > 5, `${width}: the memory table rendered process names`);
  const midWord = names.flat();
  assert.deepEqual(midWord, [], `${width}: process names must break at a separator, not mid-word`);
}

async function main() {
  const restore = setHarnessEnvironment();
  let server, browser;
  try {
    if (EVIDENCE) await mkdir(EVIDENCE, { recursive: true });
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
    await server.listen();
    browser = await chromium.launch();
    const base = serverURL(server);
    for (const { width: viewportWidth, fontSize, colorScheme, label: width } of CASES) {
      const page = await browser.newPage({ viewport: { width: viewportWidth, height: 900 }, colorScheme });
      page.rootFontSize = fontSize;
      const pageErrors = [];
      page.on("pageerror", (e) => pageErrors.push(String(e)));
      page.on("dialog", (d) => d.dismiss());
      try {
        await restorePreview(page, base, width);
        await appList(page, base, width);
        await memoryTable(page, base, width);
        assert.deepEqual(pageErrors, [], `${width}: the page must not throw`);
      } finally {
        await page.close();
      }
    }
    console.log(
      `Layout clipping passed at ${CASES.map((c) => c.label).join(", ")}: restore preview Item column readable and nothing overflows, long app names wrap or ellipsize inside their row, ids break at separators.`,
    );
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    restore();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
