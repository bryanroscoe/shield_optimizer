import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

// Real More / Optimize / PaywallSheet against a mocked billing backend. The
// backend (billing_commands.rs) is what verifies purchases and owns the
// entitlement; these tests prove the UI only mirrors it, never fabricates Pro,
// hides license-key entry in Play builds and keeps recovery reachable.

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

const OFFER = { formatted_price: "€3,49", title: "ATV Optimizer Pro", name: "ATV Optimizer Pro", description: "" };

function status(over = {}) {
  const play = over.play ?? { state: "not_owned" };
  const owned = play.state === "owned";
  const license = over.license ?? false;
  return {
    entitlement: owned || license ? "pro" : "free",
    sources: [...(owned ? ["play"] : []), ...(license ? ["license"] : [])],
    distribution: over.distribution ?? "play",
    license_key_entry: (over.distribution ?? "play") === "sideload",
    available: over.available ?? true,
    play,
    offer: "offer" in over ? over.offer : OFFER,
    ack_pending: false,
    product_id: "atvopt_pro",
  };
}

async function open(t, initial, screen = "more", { clock = false, events = false } = {}) {
  const page = await browser.newPage({ viewport: { width: 390, height: 1400 } });
  t.after(() => page.close());
  if (clock) await page.clock.install();
  await page.addInitScript(({ init, events }) => {
    window.calls = [];
    window.handlers = {};
    // The connection itself reports a "connect" action; purchases report "other".
    window.purchaseReport = (c) => c.command === "telemetry_action" && c.args?.action === "other";
    window.billingStatus = init;
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 0,
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        if (window.handlers[command]) return window.handlers[command](args);
        switch (command) {
          case "get_entitlement": return window.billingStatus.entitlement;
          case "billing_status":
          case "billing_refresh": return window.billingStatus;
          case "license_info": return null;
          case "list_devices": return [{
            id: 1, serial: "A:5555", name: "A", model: "Shield", status: "device",
            connection: "network", device_type: "shield", properties: null,
          }];
          case "wireless_status": return { connected: true };
          case "wireless_connect": return { ok: true, message: "connected" };
          case "prepare_optimize":
            if (window.billingStatus.entitlement !== "pro") throw "LOCKED:optimize_wizard";
            return { mode: "optimize", device_type: "shield", items: [] };
          case "panic_recovery": return { restored: ["com.example.one"], failed: [], message: "Re-enabled 1 app." };
          default: return { ok: true, message: "done" };
        }
      },
    };
    if (events) {
      // Just enough of Tauri's event plugin for App.svelte's `listen` calls,
      // plus `emitTauriEvent` to deliver a backend event to them.
      const callbacks = new Map();
      let nextId = 0;
      window.eventListeners = [];
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
      window.__TAURI_INTERNALS__.transformCallback = (callback) => {
        callbacks.set(++nextId, callback);
        return nextId;
      };
      window.handlers["plugin:event|listen"] = ({ event, handler }) => {
        window.eventListeners.push({ event, handler, id: ++nextId });
        return nextId;
      };
      window.emitTauriEvent = (event, payload) => {
        for (const listener of window.eventListeners) {
          if (listener.event === event) callbacks.get(listener.handler)?.({ event, id: listener.id, payload });
        }
      };
    }
  }, { init: initial, events });
  await page.goto(origin);
  await page.evaluate(async (target) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    await session.connect("A", 5555);
    router.reset("dashboard");
    router.navigate(target);
  }, screen);
  return page;
}

const licensing = (page) => page.locator(".more-card").filter({ hasText: "Licensing" });
const count = (page, command) =>
  page.evaluate((c) => window.calls.filter((call) => call.command === c).length, command);

test("Play build on Free shows Play's localized price, Restore and the free-recovery promise, but no key entry", async (t) => {
  const page = await open(t, status());
  const card = licensing(page);
  await card.getByRole("button", { name: "Buy Pro · €3,49" }).waitFor();
  await card.getByRole("button", { name: "Restore purchases" }).waitFor();
  await card.getByText("Re-enabling apps and emergency recovery are always free.").waitFor();
  assert.equal(await card.getByPlaceholder("ATVOPT-…").count(), 0);
  assert.equal(await card.locator(".license-badge").textContent(), "Free");
  assert.ok((await count(page, "billing_refresh")) >= 1, "purchases are checked on launch");
});

const TIERS = [
  { product_id: "atvopt_pro_support_1", formatted_price: "€9,99", title: "", name: "", description: "" },
  { product_id: "atvopt_pro_support_2", formatted_price: "€19,99", title: "", name: "", description: "" },
];

test("support tiers buy the same Pro through their own Play product, and Buy Pro sends none", async (t) => {
  const page = await open(t, { ...status(), support_offers: TIERS });
  await page.evaluate(() => {
    window.handlers.billing_purchase = () => {
      window.billingStatus = {
        ...window.billingStatus,
        entitlement: "pro",
        sources: ["play"],
        play: { state: "owned", acknowledged: true, verified_at_ms: 1, cached: false },
      };
      return { outcome: "purchased", message: "Pro unlocked. Thank you!", status: window.billingStatus };
    };
  });
  const card = licensing(page);
  await card.getByText("Support me").waitFor();
  await card.getByRole("button", { name: "Support me with €9,99" }).waitFor();
  await card.getByRole("button", { name: "Support me with €19,99" }).click();
  await card.locator(".license-badge", { hasText: "Pro" }).waitFor();
  const buy = await page.evaluate(() => window.calls.filter((c) => c.command === "billing_purchase").map((c) => c.args));
  assert.deepEqual(buy, [{ productId: "atvopt_pro_support_2" }]);
  assert.equal(await card.getByText("Support me").count(), 0, "tiers disappear once Pro is owned");
});

test("Buy Pro sends no product, and no tiers from Play means no Support me row", async (t) => {
  const page = await open(t, status());
  await page.evaluate(() => {
    window.handlers.billing_purchase = () => ({ outcome: "canceled", message: "", status: window.billingStatus });
  });
  const card = licensing(page);
  await card.getByRole("button", { name: "Buy Pro · €3,49" }).click();
  await page.waitForFunction(() => window.calls.some((c) => c.command === "billing_purchase"));
  const args = await page.evaluate(() => window.calls.find((c) => c.command === "billing_purchase").args);
  assert.deepEqual(args, {});
  assert.equal(await card.getByText("Support me").count(), 0);
});

test("a pending purchase never unlocks and blocks a second purchase", async (t) => {
  const page = await open(t, status());
  await page.evaluate(() => {
    window.handlers.billing_purchase = () => {
      window.billingStatus = { ...window.billingStatus, play: { state: "pending" } };
      return {
        outcome: "pending",
        message: "Your purchase is pending. Pro unlocks as soon as Google Play confirms the payment.",
        status: window.billingStatus,
      };
    };
  });
  const card = licensing(page);
  await card.getByRole("button", { name: "Buy Pro · €3,49" }).click();
  await card.getByText("Purchase pending. Pro unlocks as soon as Google Play confirms the payment.").waitFor();
  assert.equal(await card.getByRole("button", { name: "Buy Pro · €3,49" }).isDisabled(), true);
  assert.equal(await card.locator(".license-badge").textContent(), "Free");
  assert.equal(await page.evaluate(async () => (await import("/src/lib/session.svelte.ts")).session.isPro), false);
});

test("Restore purchases unlocks only when the backend reports a verified Play purchase", async (t) => {
  const page = await open(t, status());
  const card = licensing(page);
  await page.evaluate(() => {
    window.billingStatus = {
      ...window.billingStatus,
      entitlement: "pro",
      sources: ["play"],
      play: { state: "owned", acknowledged: true, verified_at_ms: 1761900000000, cached: false },
    };
  });
  await card.getByRole("button", { name: "Restore purchases" }).click();
  await card.getByText("Pro restored from Google Play.").waitFor();
  await card.getByText("Source: Google Play purchase").waitFor();
  assert.equal(await card.locator(".license-badge").textContent(), "Pro");
  const restore = await page.evaluate(() => window.calls.filter((c) => c.command === "billing_refresh").at(-1));
  assert.deepEqual(restore.args, { force: true });
});

const OWNED = { entitlement: "pro", sources: ["play"], play: { state: "owned", acknowledged: true, verified_at_ms: 1, cached: false } };

test("a promo code redeemed in the Play Store while the app is open unlocks Pro and says so", async (t) => {
  const page = await open(t, status(), "more", { events: true });
  const card = licensing(page);
  await card.getByRole("button", { name: "Restore purchases" }).waitFor();
  await page.waitForFunction(() => window.eventListeners.some((l) => l.event === "purchases-updated"));
  await page.evaluate((owned) => {
    window.billingStatus = { ...window.billingStatus, ...owned };
    window.emitTauriEvent("purchases-updated", null);
  }, OWNED);
  await page.getByRole("status").filter({ hasText: "Pro unlocked from your Google Play purchase." }).waitFor();
  await card.locator(".license-badge", { hasText: "Pro" }).waitFor();
  const last = await page.evaluate(() => window.calls.filter((c) => c.command === "billing_refresh").at(-1));
  assert.deepEqual(last.args, { force: true }, "the event asks Play now instead of reusing the last answer");
});

test("a purchase that arrives by the next resume says Pro unlocked; an owner's launch says nothing", async (t) => {
  const owner = await open(t, status({ play: OWNED.play }));
  await licensing(owner).locator(".license-badge", { hasText: "Pro" }).waitFor();
  const before = await count(owner, "billing_refresh");
  await owner.evaluate(() => document.dispatchEvent(new Event("visibilitychange")));
  await owner.waitForFunction((n) => window.calls.filter((c) => c.command === "billing_refresh").length > n, before);
  await owner.evaluate(() => new Promise((resolve) => setTimeout(resolve, 50)));
  assert.equal(await owner.getByText("Pro unlocked from your Google Play purchase.").count(), 0);

  const page = await open(t, status());
  const card = licensing(page);
  await card.getByRole("button", { name: "Restore purchases" }).waitFor();
  await page.evaluate((owned) => {
    window.billingStatus = { ...window.billingStatus, ...owned };
    document.dispatchEvent(new Event("visibilitychange"));
  }, OWNED);
  await page.getByText("Pro unlocked from your Google Play purchase.").waitFor();
  await card.locator(".license-badge", { hasText: "Pro" }).waitFor();
});

test("Restore purchases says it is for a purchase or promo code made outside the app", async (t) => {
  const page = await open(t, status());
  const card = licensing(page);
  await card.getByText("Already bought Pro, or redeemed a promo code on Google Play? Tap Restore purchases.").waitFor();
  await card.getByRole("button", { name: "Restore purchases" }).click();
  await card.getByText(/No Pro purchase found for the Google account on this phone\. A code redeemed on the Play Store website can take a few minutes/).waitFor();
  assert.equal(await card.locator(".license-badge").textContent(), "Free");
});

test("a refund seen on resume drops Pro, and emergency recovery stays available", async (t) => {
  const page = await open(t, status({ play: { state: "owned", acknowledged: true, verified_at_ms: 1, cached: false } }));
  const card = licensing(page);
  await card.getByText("Source: Google Play purchase").waitFor();
  await page.evaluate(() => {
    window.billingStatus = { ...window.billingStatus, entitlement: "free", sources: [], play: { state: "not_owned" } };
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await card.locator(".license-badge", { hasText: "Free" }).waitFor();
  await card.getByRole("button", { name: /Buy Pro/ }).waitFor();
  const recovery = page.getByRole("button", { name: /Emergency recovery/ });
  assert.equal(await recovery.isDisabled(), false);
});

test("a failed purchase shows Play's error and grants nothing", async (t) => {
  const page = await open(t, status());
  await page.evaluate(() => {
    window.handlers.billing_purchase = () => ({
      outcome: "failed",
      message: "Couldn't reach Google Play. Check your connection and try again.",
      status: window.billingStatus,
    });
  });
  const card = licensing(page);
  await card.getByRole("button", { name: "Buy Pro · €3,49" }).click();
  const notice = card.locator(".error-notice");
  await notice.getByText("Couldn't reach Google Play. Check your connection and try again.").waitFor();
  assert.equal(await card.locator(".license-badge").textContent(), "Free");
  const report = await page.evaluate(() => window.calls.find(purchaseReport)?.args);
  assert.deepEqual(report, { action: "other", serial: null, ok: false, errorCode: "billing_failed", message: null });
});

test("a cancelled purchase is quiet", async (t) => {
  const page = await open(t, status());
  await page.evaluate(() => {
    window.handlers.billing_purchase = () => ({ outcome: "canceled", message: "", status: window.billingStatus });
  });
  const card = licensing(page);
  await card.getByRole("button", { name: "Buy Pro · €3,49" }).click();
  await page.waitForFunction(() => window.calls.some((c) => c.command === "billing_purchase"));
  assert.equal(await card.locator(".error-notice").count(), 0);
  assert.equal(await page.evaluate(() => window.calls.filter(purchaseReport).length), 0, "a cancelled sheet is not reported");
  assert.equal(await card.locator(".license-badge").textContent(), "Free");
});

test("no price from Play means no Buy button to press", async (t) => {
  const page = await open(t, status({ offer: null }));
  const buy = licensing(page).getByRole("button", { name: "Price unavailable" });
  await buy.waitFor();
  assert.equal(await buy.isDisabled(), true);
});

test("a Play build without a license key says purchases are unavailable and offers no key entry", async (t) => {
  const page = await open(
    t,
    status({ available: false, offer: null, play: { state: "unavailable", reason: "Purchases aren't available in this build of the app." } }),
  );
  const card = licensing(page);
  await card.getByText("Purchases aren't available in this build of the app.").waitFor();
  assert.equal(await card.getByRole("button", { name: /Buy Pro/ }).count(), 0);
  assert.equal(await card.getByPlaceholder("ATVOPT-…").count(), 0);
});

test("a sideload build keeps offline license-key entry", async (t) => {
  const page = await open(
    t,
    status({
      distribution: "sideload",
      available: false,
      offer: null,
      play: { state: "unavailable", reason: "Purchases aren't available in this build of the app." },
    }),
  );
  await licensing(page).getByPlaceholder("ATVOPT-…").waitFor();
});

test("a locked Optimize opens a Play paywall with the price and no key hint in Play builds", async (t) => {
  const page = await open(t, status(), "optimize");
  await page.getByRole("heading", { name: "Debloat is a Pro feature" }).waitFor();
  assert.equal(await page.getByRole("button", { name: /Already have a key/ }).count(), 0);
  await page.getByRole("button", { name: "Unlock Pro" }).click();
  const paywall = page.getByRole("dialog", { name: "ATV Optimizer Pro" });
  await paywall.getByRole("heading", { name: "Unlock ATV Optimizer Pro" }).waitFor();
  await paywall.getByRole("button", { name: "Buy Pro · €3,49" }).waitFor();
  await paywall.getByRole("button", { name: "Restore purchases" }).waitFor();
  await paywall.getByText("Already bought Pro, or redeemed a promo code on Google Play? Tap Restore purchases.").waitFor();
  await paywall.getByText(/Re-enabling apps and emergency recovery are always free\. Undoing settings, Home app or snapshot changes needs Pro\./).waitFor();
  assert.equal(await paywall.getByRole("button", { name: "Enter license key" }).count(), 0);
  assert.equal(await paywall.getByText(/license key/i).count(), 0);
});

test("buying from the paywall closes it once the backend grants Pro", async (t) => {
  const page = await open(t, status(), "optimize");
  await page.evaluate(() => {
    window.handlers.billing_purchase = () => {
      window.billingStatus = {
        ...window.billingStatus,
        entitlement: "pro",
        sources: ["play"],
        play: { state: "owned", acknowledged: true, verified_at_ms: 1, cached: false },
      };
      return { outcome: "purchased", message: "Pro unlocked. Thank you!", status: window.billingStatus };
    };
  });
  await page.getByRole("button", { name: "Unlock Pro" }).click();
  const paywall = page.getByRole("dialog", { name: "ATV Optimizer Pro" });
  await paywall.getByRole("button", { name: "Buy Pro · €3,49" }).click();
  await paywall.waitFor({ state: "detached" });
  assert.equal(await page.evaluate(async () => (await import("/src/lib/session.svelte.ts")).session.isPro), true);
  // The screen it was bought from reloads instead of staying locked.
  await page.getByRole("heading", { name: "Debloat is a Pro feature" }).waitFor({ state: "detached" });
  assert.ok((await count(page, "prepare_optimize")) >= 2);
  // The anonymous report carries the outcome only: no order, token, price or product.
  const report = await page.evaluate(() => window.calls.find(purchaseReport)?.args);
  assert.deepEqual(report, { action: "other", serial: null, ok: true, errorCode: null, message: null });
});

test("a pending purchase that completes while the app stays open unlocks without leaving the app", async (t) => {
  const page = await open(t, status({ play: { state: "pending" } }), "more", { clock: true });
  const card = licensing(page);
  await card.getByText("Purchase pending. Pro unlocks as soon as Google Play confirms the payment.").waitFor();
  await page.evaluate(() => {
    window.billingStatus = {
      ...window.billingStatus,
      entitlement: "pro",
      sources: ["play"],
      play: { state: "owned", acknowledged: true, verified_at_ms: 1, cached: false },
    };
  });
  const before = await count(page, "billing_refresh");
  await page.clock.runFor(31_000);
  await card.getByText("Source: Google Play purchase").waitFor();
  assert.ok((await count(page, "billing_refresh")) > before);
  const after = await count(page, "billing_refresh");
  await page.clock.runFor(61_000);
  assert.equal(await count(page, "billing_refresh"), after, "polling stops once the purchase is no longer pending");
});

test("a support tier bought from the paywall closes it and reports no product or price", async (t) => {
  const page = await open(t, { ...status(), support_offers: TIERS }, "optimize");
  await page.evaluate(() => {
    window.handlers.billing_purchase = () => {
      window.billingStatus = {
        ...window.billingStatus,
        entitlement: "pro",
        sources: ["play"],
        play: { state: "owned", acknowledged: true, verified_at_ms: 1, cached: false },
      };
      return { outcome: "purchased", message: "Pro unlocked. Thank you!", status: window.billingStatus };
    };
  });
  await page.getByRole("button", { name: "Unlock Pro" }).click();
  const paywall = page.getByRole("dialog", { name: "ATV Optimizer Pro" });
  await paywall.getByRole("button", { name: "Support me with €9,99" }).click();
  await paywall.waitFor({ state: "detached" });
  const report = await page.evaluate(() => window.calls.find(purchaseReport)?.args);
  assert.deepEqual(report, { action: "other", serial: null, ok: true, errorCode: null, message: null });
});
