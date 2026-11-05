import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

// Shared shell pieces at the sizes the phone actually lays out at. Android
// WebView turns the system font scale into page zoom, so 150% text on a
// 412dp phone is a ~275 CSS px viewport.
process.env.VITE_DEMO = "1";
let server, browser, origin;
before(async () => {
  ({ server, origin } = await startViteServer());
  browser = await chromium.launch();
});
after(async () => { await browser?.close(); await server?.close(); });

async function app(t, { width = 384, height = 812, screen = null, query = "" } = {}) {
  const context = await browser.newContext({ viewport: { width, height }, isMobile: true, hasTouch: true });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  t.after(() => context.close());
  await page.addInitScript(() => { localStorage.clear(); localStorage.setItem("atv.autoConnect.v1", "0"); });
  await page.goto(origin + query, { waitUntil: "networkidle" });
  await page.evaluate(async (screen) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    await session.connect("192.168.1.42", 5555);
    router.reset("dashboard");
    if (screen) router.navigate(screen);
  }, screen);
  await page.locator(".screen").first().waitFor();
  await page.evaluate(() => document.fonts.ready);
  return page;
}

const LONG = Array.from({ length: 14 }, (_, i) => `Safe to remove: Example app ${i + 1} — a service that shut down years ago.`).join(" ");

async function mountConfirm(page, props = {}) {
  await page.evaluate(async (props) => {
    const { mount } = await import("/node_modules/svelte/src/index-client.js");
    const ConfirmDialog = (await import("/src/components/ConfirmDialog.svelte")).default;
    const target = document.createElement("div");
    document.body.append(target);
    window.dialogEvents = [];
    mount(ConfirmDialog, {
      target,
      props: {
        open: true,
        title: "Apply 14 selected changes?",
        message: "Example — Disable; ".repeat(14),
        icon: "auto_fix_high",
        confirmLabel: "Apply",
        onConfirm: () => window.dialogEvents.push("confirm"),
        onCancel: () => window.dialogEvents.push("cancel"),
        ...props,
      },
    });
  }, props);
  await page.locator(".dialog-card").waitFor();
}

for (const [width, height] of [[384, 812], [320, 640], [275, 610], [740, 360]]) {
  test(`a long confirm keeps both actions on screen and scrolls its body at ${width}x${height}`, async (t) => {
    const page = await app(t, { width, height, screen: "optimize" });
    await mountConfirm(page, { warning: LONG });
    const dialog = page.getByRole("dialog", { name: "Apply 14 selected changes?" });
    for (const name of ["Cancel", "Apply"]) {
      const box = await dialog.getByRole("button", { name, exact: true }).boundingBox();
      assert.ok(box.y >= 0 && box.y + box.height <= height, `${name} is inside the ${height}px viewport (${box.y}+${box.height})`);
      assert.ok(box.x >= 0 && box.x + box.width <= width, `${name} is inside the ${width}px viewport`);
    }
    const card = await page.locator(".dialog-card").boundingBox();
    assert.ok(card.y >= 0 && card.y + card.height <= height, "the card fits the viewport");
    const body = page.locator(".dialog-body");
    const sizes = await body.evaluate((el) => ({ sh: el.scrollHeight, ch: el.clientHeight, oy: getComputedStyle(el).overflowY }));
    assert.equal(sizes.oy, "auto");
    assert.ok(sizes.sh > sizes.ch, "the long body overflows into its own scroller");
    assert.equal(await page.locator(".dialog-actions.divided").count(), 1, "a divider marks the cut-off");

    // A touch drag on the body scrolls it while the page stays locked.
    const pageY = await page.evaluate(() => scrollY);
    const cdp = await page.context().newCDPSession(page);
    const b = await body.boundingBox();
    await cdp.send("Input.synthesizeScrollGesture", {
      x: Math.round(b.x + b.width / 2), y: Math.round(b.y + b.height * 0.7),
      yDistance: -Math.round(b.height * 0.5), gestureSourceType: "touch", speed: 1200,
    });
    await page.waitForFunction(() => document.querySelector(".dialog-body").scrollTop > 0);
    assert.equal(await page.evaluate(() => scrollY), pageY);
    assert.equal(await page.evaluate(() => document.body.style.position), "fixed");
    const after = await dialog.getByRole("button", { name: "Apply", exact: true }).boundingBox();
    assert.ok(after.y + after.height <= height, "Apply is still on screen after scrolling");

    // Focus starts on Cancel; the overflowing text joins the Tab cycle as a
    // named region the keyboard can scroll; Escape cancels.
    await page.waitForFunction(() => document.activeElement?.textContent?.trim() === "Cancel");
    await page.keyboard.press("Tab");
    assert.equal(await page.evaluate(() => document.activeElement?.textContent?.trim()), "Apply");
    await page.keyboard.press("Tab");
    const region = dialog.getByRole("region", { name: "Apply 14 selected changes?" });
    assert.equal(await region.evaluate((el) => el === document.activeElement && el.classList.contains("dialog-body")), true);
    assert.equal(await region.evaluate((el) => getComputedStyle(el).outlineStyle), "solid");
    await region.evaluate((el) => (el.scrollTop = 0));
    await page.keyboard.press("PageDown");
    await page.waitForFunction(() => document.querySelector(".dialog-body").scrollTop > 0);
    assert.equal(await page.evaluate(() => scrollY), pageY);
    await page.keyboard.press("Tab");
    assert.equal(await page.evaluate(() => document.activeElement?.textContent?.trim()), "Cancel");
    await page.keyboard.press("Shift+Tab");
    assert.equal(await region.evaluate((el) => el === document.activeElement), true);
    await page.keyboard.press("Escape");
    assert.deepEqual(await page.evaluate(() => window.dialogEvents), ["cancel"]);
  });
}

test("a short confirm has no scroller divider and keeps its compact look", async (t) => {
  const page = await app(t, { width: 384, height: 812 });
  await mountConfirm(page, { title: "Reboot TV?", message: "The TV restarts.", icon: undefined });
  assert.equal(await page.locator(".dialog-actions.divided").count(), 0);
  const sizes = await page.locator(".dialog-body").evaluate((el) => ({ sh: el.scrollHeight, ch: el.clientHeight }));
  assert.ok(sizes.sh <= sizes.ch + 1);
  await page.waitForFunction(() => document.activeElement?.textContent?.trim() === "Cancel");
  await page.keyboard.press("Tab");
  await page.keyboard.press("Tab");
  assert.equal(await page.evaluate(() => document.activeElement?.textContent?.trim()), "Cancel", "no scroll stop when nothing overflows");
  assert.equal(await page.locator(".dialog-body").getAttribute("tabindex"), null);
  await page.getByRole("dialog").getByRole("button", { name: "Apply", exact: true }).click();
  assert.deepEqual(await page.evaluate(() => window.dialogEvents), ["confirm"]);
});

for (const [width, height] of [[384, 812], [320, 640], [275, 610]]) {
  test(`all five tabs fit without overflow at ${width}px`, async (t) => {
    const page = await app(t, { width, height });
    const nav = page.getByRole("navigation", { name: "Main" });
    const tabs = await nav.evaluate((el) => {
      const labels = [...el.querySelectorAll(".tab-label")];
      return {
        overflow: el.scrollWidth - el.clientWidth,
        docOverflow: document.documentElement.scrollWidth - innerWidth,
        buttons: [...el.querySelectorAll("button")].map((b) => { const r = b.getBoundingClientRect(); return { left: r.left, right: r.right, height: r.height }; }),
        labels: labels.map((l) => ({ text: l.textContent, clipped: l.scrollWidth > l.clientWidth, size: parseFloat(getComputedStyle(l).fontSize) })),
      };
    });
    assert.equal(tabs.overflow, 0);
    assert.ok(tabs.docOverflow <= 0);
    assert.equal(tabs.buttons.length, 5);
    for (const b of tabs.buttons) {
      assert.ok(b.left >= 0 && b.right <= width + 0.5, `tab inside viewport: ${b.left}..${b.right}`);
      assert.ok(b.height >= 44, "tab keeps a 44px touch target");
    }
    for (const l of tabs.labels) {
      assert.equal(l.clipped, false, `${l.text} is not clipped`);
      assert.ok(l.size >= 10);
    }
    for (const name of ["Home", "Apps", "Tweaks", "Remote", "More"]) await nav.getByRole("button", { name, exact: true }).waitFor();
    assert.equal(await nav.getByRole("button", { name: "Home", exact: true }).getAttribute("aria-current"), "page");
    await nav.getByRole("button", { name: "More", exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.bottom-tabs [aria-current="page"]')?.textContent.includes("More"));
  });
}

test("the tab bar is no taller than the scroll padding that keeps focus clear of it", async (t) => {
  const page = await app(t, { width: 384, height: 812 });
  const m = await page.evaluate(() => ({
    bar: document.querySelector(".bottom-tabs").getBoundingClientRect().height,
    pad: parseFloat(getComputedStyle(document.documentElement).scrollPaddingBottom),
  }));
  assert.ok(m.bar <= m.pad, `tab bar ${m.bar}px vs scroll-padding-bottom ${m.pad}px`);
});

test("keyboard focus on Dashboard never lands under the bottom tab bar", async (t) => {
  const page = await app(t, { width: 384, height: 812 });
  const hidden = [];
  for (let i = 0; i < 24; i++) {
    await page.keyboard.press("Tab");
    const r = await page.evaluate(() => {
      const el = document.activeElement;
      const bar = document.querySelector(".bottom-tabs");
      if (!el || el === document.body || bar.contains(el)) return null;
      const box = el.getBoundingClientRect();
      return { label: (el.getAttribute("aria-label") || el.textContent || "").trim().slice(0, 40), bottom: box.bottom, top: bar.getBoundingClientRect().top };
    });
    if (r && r.bottom > r.top + 1) hidden.push(r);
  }
  assert.deepEqual(hidden, []);
});

test("the dim and inactive-tab tokens meet 4.5:1 on the backgrounds they sit on", async (t) => {
  const page = await app(t, { width: 384, height: 812 });
  const ratios = await page.evaluate(() => {
    const rgb = (value) => {
      const probe = document.createElement("i");
      probe.style.color = value;
      document.body.append(probe);
      const out = getComputedStyle(probe).color.match(/[\d.]+/g).slice(0, 3).map(Number);
      probe.remove();
      return out;
    };
    const lum = ([r, g, b]) => [r, g, b].map((v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; }).reduce((s, v, i) => s + v * [0.2126, 0.7152, 0.0722][i], 0);
    const ratio = (a, b) => { const [x, y] = [lum(a), lum(b)].sort((m, n) => n - m); return (x + 0.05) / (y + 0.05); };
    const inactive = document.querySelector(".bottom-tabs .tab-btn:not(.active) .tab-label");
    const fg = getComputedStyle(inactive).color.match(/[\d.]+/g).slice(0, 3).map(Number);
    return {
      dimCanvas: ratio(rgb("var(--dim)"), rgb("var(--canvas)")),
      dimSurface: ratio(rgb("var(--dim)"), rgb("var(--surface)")),
      dimBelowMuted: lum(rgb("var(--dim)")) < lum(rgb("var(--muted)")),
      tab: ratio(fg, rgb("var(--canvas)")),
    };
  });
  assert.ok(ratios.dimCanvas >= 4.5, `dim on canvas ${ratios.dimCanvas}`);
  assert.ok(ratios.dimSurface >= 4.5, `dim on surface ${ratios.dimSurface}`);
  assert.ok(ratios.dimBelowMuted, "dim stays dimmer than muted");
  assert.ok(ratios.tab >= 4.5, `inactive tab label ${ratios.tab}`);
});

test("checkboxes and radios skip the text-field sizing and use the accent", async (t) => {
  const page = await app(t, { width: 384, height: 812 });
  const s = await page.evaluate(() => {
    const host = document.createElement("div");
    host.innerHTML = '<input id="t" type="text"><input id="n" type="number"><input id="c" type="checkbox"><input id="r" type="radio"><style>.own-field{min-height:30px}</style><input id="o" class="own-field">';
    document.body.append(host);
    const cs = (id) => getComputedStyle(document.getElementById(id));
    const box = (id) => document.getElementById(id).getBoundingClientRect();
    const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim();
    return {
      text: cs("t").minHeight, number: cs("n").minHeight, own: cs("o").minHeight,
      check: { minHeight: cs("c").minHeight, padding: cs("c").paddingLeft, h: box("c").height, w: box("c").width, accent: cs("c").accentColor },
      radio: { minHeight: cs("r").minHeight, h: box("r").height },
      accent,
    };
  });
  assert.equal(s.text, "48px");
  assert.equal(s.number, "48px");
  assert.equal(s.own, "30px", "a screen's own field class still overrides the base rule");
  assert.notEqual(s.check.minHeight, "48px");
  assert.equal(s.check.padding, "0px");
  assert.equal(s.check.h, 20);
  assert.equal(s.check.w, 20);
  assert.equal(s.radio.h, 20);
  assert.notEqual(s.radio.minHeight, "48px");
  assert.equal(s.check.accent, "rgb(201, 242, 78)");
});

test("a closed help disclosure is one compact row and opens with room below", async (t) => {
  const page = await app(t, { width: 384, height: 812, screen: "tweaks" });
  const details = page.locator("details.help-disclosure").filter({ has: page.locator("summary", { hasText: "About System Hooks" }) });
  await details.waitFor();
  const closed = await details.evaluate((el) => {
    const cs = getComputedStyle(el);
    return { h: el.getBoundingClientRect().height, summary: el.querySelector("summary").getBoundingClientRect().height, mt: cs.marginTop, mb: cs.marginBottom };
  });
  assert.ok(closed.summary >= 44, "touch target stays 44px");
  assert.ok(closed.h <= closed.summary + 1, `closed disclosure is just its summary row (${closed.h} vs ${closed.summary})`);
  assert.equal(closed.mt, "0px");
  assert.equal(closed.mb, "0px");
  await details.locator("summary").click();
  const open = await details.evaluate((el) => ({ open: el.open, mb: parseFloat(getComputedStyle(el).marginBottom), note: getComputedStyle(el.querySelector(".t-note")).marginBottom }));
  assert.equal(open.open, true);
  assert.ok(open.mb > 0);
  assert.equal(open.note, "10px");
});

test("on a tablet, screens, the tab row and bottom sheets stop at the content width", async (t) => {
  const page = await app(t, { width: 1024, height: 768, screen: "apps" });
  const layout = await page.evaluate(() => {
    const screen = document.querySelector(".screen");
    const cs = getComputedStyle(screen);
    const left = parseFloat(cs.paddingLeft), right = parseFloat(cs.paddingRight);
    const tabs = [...document.querySelectorAll(".bottom-tabs .tab-btn")].map((b) => b.getBoundingClientRect());
    return { content: screen.clientWidth - left - right, left, right, tabsLeft: tabs[0].left, tabsRight: tabs.at(-1).right };
  });
  assert.ok(Math.abs(layout.content - 720) <= 1, `screen content width ${layout.content}`);
  assert.ok(Math.abs(layout.left - layout.right) <= 1, "content is centred");
  assert.ok(layout.tabsRight - layout.tabsLeft <= 721, "tab row is capped");
  await page.locator(".app-row").first().click();
  const sheet = await page.locator(".sheet").first().boundingBox();
  assert.ok(sheet.width <= 721, `sheet width ${sheet.width}`);
  assert.ok(Math.abs(sheet.x - (1024 - sheet.width) / 2) <= 1, "sheet is centred");
});

test("on a phone, screens and sheets keep their full-width layout", async (t) => {
  const page = await app(t, { width: 384, height: 812, screen: "apps" });
  const pad = await page.evaluate(() => { const cs = getComputedStyle(document.querySelector(".screen")); return [cs.paddingLeft, cs.paddingRight]; });
  assert.deepEqual(pad, ["24px", "24px"]);
  await page.locator(".app-row").first().click();
  const sheet = await page.locator(".sheet").first().boundingBox();
  assert.equal(Math.round(sheet.width), 384);
});

for (const [width, height] of [[384, 812], [320, 640], [275, 610]]) {
  test(`the paywall keeps Close and its purchase actions reachable at ${width}x${height}`, async (t) => {
    const page = await app(t, { width, height, screen: "optimize", query: "?billing=free" });
    await page.getByRole("button", { name: "Unlock Pro" }).click();
    const paywall = page.getByRole("dialog", { name: "ATV Optimizer Pro" });
    await paywall.getByRole("button", { name: /Buy Pro/ }).waitFor();
    await paywall.evaluate((el) => Promise.all(el.getAnimations().map((a) => a.finished)));
    const card = await paywall.boundingBox();
    assert.ok(card.y >= 0 && card.y + card.height <= height, "the card fits the viewport");
    for (const name of [/Buy Pro/, "Restore purchases", "Maybe later", "Close"]) {
      const box = await paywall.getByRole("button", { name, exact: typeof name === "string" }).boundingBox();
      assert.ok(box.y >= 0 && box.y + box.height <= height && box.x >= 0 && box.x + box.width <= width, `${name} is on screen`);
      assert.ok(box.height >= 44, `${name} keeps a 44px target`);
    }
    const body = page.locator(".paywall-body");
    const overflows = await body.evaluate((el) => el.scrollHeight > el.clientHeight + 1);
    if (height < 700) assert.equal(overflows, true, "the pitch scrolls inside the card on a short screen");
    // Initial focus is the first purchase action, not the scroll region.
    await page.waitForFunction(() => /Buy Pro/.test(document.activeElement?.textContent ?? ""));
    const order = [];
    for (let i = 0; i < 5; i++) {
      await page.keyboard.press("Tab");
      order.push(await page.evaluate(() => { const el = document.activeElement; return el.getAttribute("role") === "region" ? "region" : (el.getAttribute("aria-label") || el.textContent).trim(); }));
    }
    assert.deepEqual(order, overflows
      ? ["Restore purchases", "Maybe later", "Close", "region", order[4]]
      : ["Restore purchases", "Maybe later", "Close", order[3], order[4]]);
    assert.match(overflows ? order[4] : order[3], /Buy Pro/);
    if (overflows) {
      await body.evaluate((el) => (el.scrollTop = 0));
      await body.focus();
      await page.keyboard.press("PageDown");
      await page.waitForFunction(() => document.querySelector(".paywall-body").scrollTop > 0);
      assert.equal(await page.evaluate(() => document.body.style.position), "fixed");
    }
    await paywall.getByRole("button", { name: "Close", exact: true }).focus();
    await page.keyboard.press("Enter");
    await paywall.waitFor({ state: "detached" });
  });
}

test("modal scroll stops: only overflowing areas without controls, named, and undone when they fit", async (t) => {
  const page = await app(t, { width: 384, height: 600 });
  await page.evaluate(async () => {
    const { modal } = await import("/src/lib/modal.ts");
    const node = document.createElement("div");
    node.style.cssText = "position:fixed;inset:0;background:#16191e";
    node.innerHTML = `<div role="dialog" aria-labelledby="t1" tabindex="-1">
      <h3 id="t1">Long notice</h3>
      <div id="text" style="height:120px;overflow-y:auto"><p style="height:600px">Long text</p></div>
      <div id="authored" tabindex="-1" style="height:120px;overflow-y:auto"><p style="height:600px">Authored</p></div>
      <div id="withControls" style="height:120px;overflow-y:auto"><p style="height:600px">x</p><button>Inner</button></div>
      <button id="ok">OK</button></div>`;
    document.body.append(node);
    window.scrollModal = { node, action: modal(node, { onClose() {} }) };
  });
  await page.waitForFunction(() => document.activeElement?.id === "withControls" || document.activeElement?.textContent === "Inner");
  const attrs = () => page.evaluate(() => Object.fromEntries(["text", "authored", "withControls"].map((id) => {
    const el = document.getElementById(id);
    return [id, { tabindex: el.getAttribute("tabindex"), role: el.getAttribute("role"), label: el.getAttribute("aria-label") }];
  })));
  assert.deepEqual(await attrs(), {
    text: { tabindex: "0", role: "region", label: "Long notice" },
    authored: { tabindex: "-1", role: null, label: null },
    withControls: { tabindex: null, role: null, label: null },
  });
  await page.keyboard.press("Shift+Tab");
  assert.equal(await page.evaluate(() => document.activeElement?.id), "text");
  await page.evaluate(() => { document.querySelector("#text p").style.height = "40px"; document.getElementById("ok").focus(); });
  await page.keyboard.press("Tab");
  assert.deepEqual((await attrs()).text, { tabindex: null, role: null, label: null });
  assert.equal(await page.evaluate(() => document.getElementById("text").hasAttribute("data-modal-scroll-stop")), false);
  await page.evaluate(() => { scrollModal.action.destroy(); scrollModal.node.remove(); });
});
