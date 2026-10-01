// App List and the Optimize wizard against the captured Shield's real
// package list: build a plan, arm only Safe rows, run it, check the device,
// restore it, and keep a "Keep" decision tied to the hardware id.

import { SHIELD, shieldScenario } from "../lib/worlds.mjs";

async function safety(ctx, pkg) {
  const r = await ctx.invoke("safety_info", { package: pkg });
  return r.value.kind;
}

function diff(after, before) {
  return after.filter((p) => !before.includes(p));
}

export const scenarios = [
  {
    name: "optimize-select-all-safe-run-restore",
    async run(ctx) {
      await ctx.reset(shieldScenario());
      const before = (await ctx.device(SHIELD.serial)).packages;
      await ctx.openDevice(SHIELD.key, "optimize");
      const page = ctx.page;
      await page.getByRole("button", { name: "Optimize", exact: true }).click();
      await page.getByRole("button", { name: "Select all safe" }).waitFor();
      await page.waitForLoadState("networkidle");
      await ctx.step("plan built from the device");

      await page.getByRole("button", { name: "Keep all" }).click();
      await page.getByRole("button", { name: "Select all safe" }).click();
      const armedText = await page.getByText(/rows are armed/).innerText();
      const armed = Number(armedText.match(/(\d+) of \d+ rows are armed/)[1]);
      ctx.assert.ok(armed > 0, "select all safe armed something");
      await ctx.step(`select all safe armed ${armed} rows`);

      await page.getByRole("button", { name: /^Run plan/ }).click();
      await page.getByText(/Optimize complete:/).waitFor({ timeout: 60_000 });
      await ctx.step("plan ran");

      const after = (await ctx.device(SHIELD.serial)).packages;
      // A third-party uninstall removes the package outright, so "changed" is
      // every app that was enabled before and is not now.
      const changed = diff(before.enabled, after.enabled);
      ctx.assert.equal(changed.length, armed, "the device changed exactly the armed rows");
      for (const pkg of changed) {
        ctx.assert.equal(await safety(ctx, pkg), "safe", `${pkg} was rated Safe to remove`);
      }
      for (const pkg of after.disabled) {
        ctx.assert.notEqual(await safety(ctx, pkg), "never_disable", `protected ${pkg} is not disabled`);
      }
      const handlers = (await ctx.device(SHIELD.serial)).home.handlers;
      ctx.assert.ok(handlers.some((h) => !h.includes("FallbackHome")), "the TV still has a Home app");

      // Restore re-enables what the run disabled.
      const disabledByRun = diff(after.disabled, before.disabled);
      await page.getByRole("button", { name: "Restore", exact: true }).click();
      await page.getByRole("button", { name: "Select all safe" }).waitFor();
      await page.waitForLoadState("networkidle");
      await page.getByRole("button", { name: "Select all safe" }).click();
      await ctx.step("restore plan armed");
      await page.getByRole("button", { name: /^Run plan/ }).click();
      await page.getByText(/Restore complete:/).waitFor({ timeout: 60_000 });
      await ctx.step("restore ran");
      const restored = (await ctx.device(SHIELD.serial)).packages;
      for (const pkg of disabledByRun) {
        ctx.assert.ok(restored.enabled.includes(pkg), `${pkg} re-enabled by Restore`);
      }
    },
  },
  {
    name: "app-list-keep-per-hardware-id",
    async run(ctx) {
      await ctx.reset({
        devices: [
          { profile: "shield-tv-pro" },
          { profile: "shield-tv-pro", serial: "0323220054321", ip: "192.0.2.9" },
        ],
      });
      await ctx.openDevice(SHIELD.key, "apps");
      const page = ctx.page;
      const keep = page.getByRole("button", { name: "Keep", exact: true }).first();
      await keep.waitFor();
      const pkg = (await keep.locator("xpath=ancestor::tr[1]").locator(".pkg-id").innerText()).trim();
      await keep.click();
      await page.locator("tr").filter({ hasText: pkg }).first().getByText("Kept").waitFor();
      await ctx.step(`kept ${pkg}`);

      await page.reload({ waitUntil: "networkidle" });
      await ctx.tab("apps");
      await page.locator("tr").filter({ hasText: pkg }).first().getByText("Kept").waitFor();
      await ctx.step("kept after reload");

      // The same Shield after DHCP gave it a new address.
      await ctx.detach(SHIELD.key);
      await ctx.attach("192.0.2.44:5555", SHIELD.serial);
      await ctx.openDevice("192.0.2.44:5555", "apps");
      await page.locator("tr").filter({ hasText: pkg }).first().getByText("Kept").waitFor();
      await ctx.step("kept at another address of the same hardware id");

      await ctx.openDevice("192.0.2.9:5555", "apps");
      const other = page.locator("tr").filter({ hasText: pkg }).first();
      await other.waitFor();
      ctx.assert.equal(await other.getByText("Kept").count(), 0, "a different TV does not inherit the decision");
      await ctx.step("a different hardware id is unaffected");

      // The wizard honours it: the kept app is not armed by default.
      await ctx.openDevice("192.0.2.44:5555", "optimize");
      await page.getByRole("button", { name: "Optimize", exact: true }).click();
      await page.getByRole("button", { name: "Select all safe" }).waitFor();
      await page.waitForLoadState("networkidle");
      const planRow = page.locator("tr").filter({ hasText: pkg }).first();
      if (await planRow.count()) {
        const active = planRow.locator(".radio-pill.active");
        ctx.assert.match(await active.first().innerText(), /Keep/, "kept app defaults to Keep in the plan");
      }
      await ctx.step("optimize plan defaults the kept app to Keep");
    },
  },
];
