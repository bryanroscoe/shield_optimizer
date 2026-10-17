import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server, browser, origin;
before(async () => {
  ({ server, origin } = await startViteServer());
  browser = await chromium.launch({ headless: true });
});
after(async () => { await browser?.close(); await server?.close(); });

async function open(t) {
  const page = await browser.newPage({ viewport: { width: 384, height: 420 } });
  t.after(() => page.close());
  await page.goto(origin);
  await page.evaluate(async () => {
    const { modal } = await import("/src/lib/modal.ts");
    document.body.replaceChildren();
    document.body.style.setProperty("width", "100%", "important");
    document.documentElement.style.overscrollBehavior = "auto";
    const background = document.createElement("div");
    background.style.cssText = "height:2400px;padding-top:900px;box-sizing:border-box";
    background.innerHTML = '<button id="opener">Original row</button><button id="other">Other row</button>';
    document.body.append(background);
    document.getElementById("opener").scrollIntoView({ block: "center", behavior: "instant" });
    document.getElementById("opener").focus({ preventScroll: true });
    window.beforeModal = { y: scrollY, body: document.body.getAttribute("style"), root: document.documentElement.getAttribute("style") };
    window.owners = {};
    window.createModal = (name, empty = false) => {
      const node = document.createElement("div");
      node.style.cssText = "position:fixed;inset:0;background:#16191e";
      node.innerHTML = empty ? '<div role="alertdialog" tabindex="-1">Empty dialog</div>' : `<div role="dialog" tabindex="-1"><button>${name}</button><button>Last ${name}</button></div>`;
      document.body.append(node);
      const action = modal(node, { onClose() { window.destroyModal(name); } });
      owners[name] = { node, action };
      if (!empty) node.querySelector("button").focus();
    };
    window.destroyModal = name => { owners[name].action.destroy(); owners[name].node.remove(); };
  });
  return page;
}
const state = page => page.evaluate(() => ({ locked: document.body.style.position === "fixed", focus: document.activeElement?.id, y: scrollY }));
async function restored(page, opener = "opener") {
  const before = await page.evaluate(() => beforeModal);
  assert.deepEqual(await state(page), { locked: false, focus: opener, y: before.y });
  assert.deepEqual(await page.evaluate(() => ({ body: document.body.getAttribute("style"), root: document.documentElement.getAttribute("style") })), { body: before.body, root: before.root });
}

test("same-update replacement preserves document lock and original opener across an empty stack", async t => {
  const page = await open(t);
  await page.evaluate(() => {
    createModal("Menu");
    destroyModal("Menu");
    createModal("Report");
    window.handoffLocked = document.body.style.position === "fixed";
  });
  assert.equal(await page.evaluate(() => handoffLocked), true);
  assert.equal((await state(page)).locked, true);
  await page.evaluate(() => destroyModal("Report"));
  await restored(page);
});

for (const outcome of ["resolve", "cancel", "reject"]) {
  test(`async ${outcome} settles genuine close before any later replacement`, async t => {
    const page = await open(t);
    await page.evaluate(() => {
      createModal("Menu");
      window.transition = new Promise((resolve, reject) => { window.finishTransition = resolve; window.failTransition = reject; })
        .then(show => { if (show) createModal("Report"); }, () => {});
      destroyModal("Menu");
    });
    await restored(page);
    await page.evaluate(outcome => {
      if (outcome === "reject") failTransition(new Error("Canceled lookup"));
      else finishTransition(outcome === "resolve");
      return transition;
    }, outcome);
    if (outcome === "resolve") {
      assert.equal((await state(page)).locked, true);
      await page.evaluate(() => destroyModal("Report"));
    }
    await restored(page);
  });
}

test("genuine empty close clears provenance before an unrelated modal opens", async t => {
  const page = await open(t);
  await page.evaluate(() => { createModal("Menu"); destroyModal("Menu"); });
  await restored(page);
  await page.evaluate(() => { document.getElementById("other").focus({ preventScroll: true }); createModal("Unrelated"); });
  await page.evaluate(() => destroyModal("Unrelated"));
  await restored(page, "other");
});

test("nested out-of-order teardown and replacement retain the document opener", async t => {
  const page = await open(t);
  await page.evaluate(() => { createModal("Outer"); createModal("Inner"); destroyModal("Outer"); });
  assert.equal((await state(page)).locked, true);
  await page.evaluate(() => { destroyModal("Inner"); createModal("Replacement"); });
  assert.equal((await state(page)).locked, true);
  await page.keyboard.press("Escape");
  await restored(page);
});

test("superseded close callbacks cannot steal focus from a new top modal", async t => {
  const page = await open(t);
  await page.evaluate(() => { createModal("Outer"); createModal("Inner"); destroyModal("Inner"); createModal("New"); });
  await page.waitForFunction(() => document.activeElement?.textContent === "New");
  assert.equal((await state(page)).locked, true);
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => document.activeElement?.textContent === "Outer");
  assert.equal((await state(page)).locked, true);
  await page.keyboard.press("Escape");
  await restored(page);
});

test("removed opener and duplicate teardown do not resurrect focus or leak a lock", async t => {
  const page = await open(t);
  await page.evaluate(() => {
    createModal("Outer");
    document.getElementById("opener").remove();
    destroyModal("Outer");
    destroyModal("Outer");
  });
  assert.equal((await state(page)).locked, false);
  assert.equal(await page.evaluate(() => document.activeElement?.isConnected), true);
});

test("alertdialog without enabled controls receives focus and Escape releases the genuine close", async t => {
  const page = await open(t);
  await page.evaluate(() => createModal("Empty", true));
  await page.waitForFunction(() => document.activeElement?.getAttribute("role") === "alertdialog");
  await page.keyboard.press("Tab");
  assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("role")), "alertdialog");
  await page.keyboard.press("Escape");
  await restored(page);
});
