// CleanYourTV's lists in Optimize against the captured Shield, through the
// real command layer: the codename's list is used, its rows are only apps the
// app's own safety check has no record of, a run disables exactly what was
// picked, Restore brings it back from under the same list, and switching or
// turning the lists off is remembered for this TV's hardware id.

import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { SHIELD, shieldScenario } from "../lib/worlds.mjs";

const PRO = "nvidia-shield-tv-pro-2019";
const OLD = "nvidia-shield-tv-2017";
const AUTO = "android.autoinstalls.config.nvidia";
const STORE_KEY = "atvopt.cleanyourtvList";

const catalog = JSON.parse(
  await readFile(fileURLToPath(new URL("../../catalogs/cleanyourtv/catalog.json", import.meta.url)), "utf8"),
);

async function waitForPlan(page) {
  await page.getByRole("button", { name: "Keep all" }).waitFor();
  await page.waitForLoadState("networkidle");
}

const group = (page, profile) => page.locator(`#tabpanel-optimize [data-cleanyourtv-rows="${profile}"]`);

function row(page, profile, pkg) {
  return group(page, profile).locator(`tr[data-package="${pkg}"]`);
}

async function listed(page, profile) {
  return group(page, profile).locator("tr[data-package]").evaluateAll((rows) => rows.map((r) => r.dataset.package));
}

async function source(ctx, pkg) {
  const r = await ctx.invoke("safety_info", { package: pkg });
  return r.value.source;
}

async function choices(r) {
  return r.locator(".action-radio .radio-pill .radio-label").allInnerTexts();
}

export const scenarios = [
  {
    name: "cleanyourtv-list-rows-run-restore-switch",
    async run(ctx) {
      await ctx.reset(shieldScenario());
      const page = ctx.page;
      const before = (await ctx.device(SHIELD.serial)).packages;
      ctx.assert.ok(before.enabled.includes(AUTO), "the captured Shield has the list app on");
      await ctx.openDevice(SHIELD.key, "optimize");
      await waitForPlan(page);
      const card = page.locator("#tabpanel-optimize [data-cleanyourtv-card]");

      // The codename's list is used without asking, and every row it adds is
      // an app the real safety check has no record of, listed once.
      await group(page, PRO).waitFor();
      ctx.assert.match(await card.innerText(), /Sources: ATV Optimizer catalog, then CleanYourTV's list for NVIDIA SHIELD TV Pro 2019/);
      const proRows = await listed(page, PRO);
      ctx.assert.ok(proRows.includes(AUTO), `the list row is shown: ${proRows}`);
      for (const pkg of proRows) {
        ctx.assert.equal(await source(ctx, pkg), "no_record", `${pkg} is a list row only without a verdict of the app's own`);
        ctx.assert.equal(await page.locator(`#tabpanel-optimize tr[data-package="${pkg}"]`).count(), 1, `${pkg} is listed once`);
        ctx.assert.deepEqual(await choices(row(page, PRO, pkg)), ["Keep", "Disable"], `${pkg} offers Disable alone`);
      }
      await ctx.step("the Shield Pro list's rows after the catalog");

      // A run disables exactly the picked list row.
      await row(page, PRO, AUTO).getByRole("button", { name: /^Disable/ }).click();
      await page.getByRole("button", { name: /^Run plan/ }).click();
      await ctx.confirmRun();
      await page.getByText(/Optimize complete:/).waitFor({ timeout: 60_000 });
      const after = (await ctx.device(SHIELD.serial)).packages;
      ctx.assert.deepEqual(before.enabled.filter((p) => !after.enabled.includes(p)), [AUTO], "the run changed the picked row alone");
      ctx.assert.ok(after.disabled.includes(AUTO), "the list app is disabled, not uninstalled");
      await ctx.step("the list row ran");

      // Restore offers it back from under the same list. Keep all first:
      // Restore arms the catalog's own re-enables by default.
      await page.getByRole("button", { name: "Restore", exact: true }).click();
      await waitForPlan(page);
      await page.getByRole("button", { name: "Keep all" }).click();
      await row(page, PRO, AUTO).waitFor();
      ctx.assert.deepEqual(await choices(row(page, PRO, AUTO)), ["Keep", "Enable"], "Restore turns a list app back on");
      await row(page, PRO, AUTO).getByRole("button", { name: /^Enable/ }).click();
      await ctx.step("the list row in Restore");
      await page.getByRole("button", { name: /^Run plan/ }).click();
      await ctx.confirmRun();
      await page.getByText(/Restore complete:/).waitFor({ timeout: 60_000 });
      const restored = (await ctx.device(SHIELD.serial)).packages;
      ctx.assert.ok(restored.enabled.includes(AUTO), "Restore turned the list app back on");
      ctx.assert.deepEqual(restored.disabled.sort(), before.disabled.sort(), "the TV is back where it started");

      // Another list: its rows still pass the real safety check, and an app it
      // lists that the app has a verdict for never becomes a list row.
      await page.getByRole("button", { name: "Optimize", exact: true }).click();
      await waitForPlan(page);
      await card.getByRole("button", { name: "Change list" }).click();
      await page.getByRole("searchbox", { name: "Search CleanYourTV lists" }).fill("shield 2017");
      await page.getByRole("radio", { name: /SHIELD TV 2017 \(darcy\)/i }).check();
      await card.getByRole("button", { name: "Use this list" }).click();
      await group(page, OLD).waitFor();
      ctx.assert.equal(await group(page, PRO).count(), 0, "the replaced list's rows are gone");
      const oldRows = await listed(page, OLD);
      for (const pkg of oldRows) {
        ctx.assert.equal(await source(ctx, pkg), "no_record", `${pkg} from the 2017 list has no verdict of the app's own`);
      }
      const profile = catalog.profiles.find((p) => p.profile === OLD);
      const on = new Set(restored.enabled);
      const judged = [];
      for (const entry of profile.entries.filter((e) => e.upstream_form === "active_disable" && on.has(e.package))) {
        if ((await source(ctx, entry.package)) !== "no_record") judged.push(entry.package);
      }
      ctx.assert.ok(judged.length > 0, "the 2017 list names installed apps the app already judges");
      for (const pkg of judged) {
        ctx.assert.ok(!oldRows.includes(pkg), `${pkg} keeps the app's own verdict, not a list row`);
      }
      await ctx.step(`the 2017 list: ${oldRows.length} rows, ${judged.length} left to the catalog`);

      // Turning the lists off is filed under the hardware id and survives a reload.
      await card.getByRole("button", { name: "Turn off" }).click();
      await page.waitForTimeout(300);
      ctx.assert.equal(await page.locator("#tabpanel-optimize [data-cleanyourtv-rows]").count(), 0, "no list rows when off");
      const stored = await page.evaluate((key) => JSON.parse(localStorage.getItem(key) ?? "{}"), STORE_KEY);
      ctx.assert.deepEqual(stored, { [SHIELD.serial]: "off" }, "the choice is filed under the hardware id");
      await page.reload({ waitUntil: "networkidle" });
      await ctx.tab("optimize");
      await card.getByRole("button", { name: "Turn on CleanYourTV" }).waitFor();
      ctx.assert.equal(await page.locator("#tabpanel-optimize [data-cleanyourtv-rows]").count(), 0, "still off after a reload");
      await ctx.step("lists off for this TV after a reload");
    },
  },
];
