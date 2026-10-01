// Health: every card comes from the real parsers reading the captured
// Shield's dumpsys output, and the memory table names whole processes with
// their PIDs and the same suggestion the App List gives.

import { SHIELD, shieldScenario } from "../lib/worlds.mjs";

export const scenarios = [
  {
    name: "health-cards-and-memory",
    async run(ctx) {
      await ctx.reset(shieldScenario());
      await ctx.openDevice(SHIELD.key, "health");
      const page = ctx.page;
      // RAM comes from the captured `dumpsys meminfo` summary.
      await page.getByText("2946", { exact: false }).first().waitFor();
      for (const card of ["RAM", "Storage", "Swap", "CPU", "Temperature"]) {
        await page.getByText(card, { exact: true }).first().waitFor();
      }
      await page.getByText("3840x2160").first().waitFor();
      await ctx.step("health cards");

      const table = page.locator("table").filter({ hasText: "SUGGESTION" }).or(page.locator("table").filter({ hasText: "Suggestion" })).first();
      await table.waitFor();
      const rowFor = (process) => table.locator("tr").filter({ hasText: process }).first();

      // A process with a `:service` suffix keeps its full name, and its PID.
      const katniss = rowFor("com.google.android.katniss:interactor");
      await katniss.waitFor();
      ctx.assert.match(await katniss.innerText(), /pid\s+15826/);
      // Plex is installed and reviewed: the App List's suggestion, not a guess.
      ctx.assert.match(await rowFor("com.plexapp.android").innerText(), /Review: uninstall if unused/);
      ctx.assert.match(await rowFor("com.google.android.gms").innerText(), /PROTECTED/);
      ctx.assert.match(await rowFor("surfaceflinger").innerText(), /Not an app/);
      ctx.assert.match(await rowFor("surfaceflinger").innerText(), /not a package/);
      await ctx.step("memory rows: full names, pids, suggestions");

      // A disabled app has no process: disable Plex on the device and the
      // refreshed report drops it.
      await ctx.shell(SHIELD.serial, "pm disable-user --user 0 com.plexapp.android");
      await page.getByRole("button", { name: "Refresh" }).first().click();
      await ctx.waitFor(async () => (await table.locator("tr").filter({ hasText: "com.plexapp.android" }).count()) === 0, {
        message: "Plex gone from the memory table",
      });
      await ctx.step("a disabled app leaves the memory table");
    },
  },
];
