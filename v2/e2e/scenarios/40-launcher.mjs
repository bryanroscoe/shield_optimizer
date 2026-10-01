// Launcher: which apps are listed, setting Home, and handing Home over from
// the stock launcher — with the device's own answers deciding the outcome.

import { SHIELD, shieldScenario, stockHomeShield } from "../lib/worlds.mjs";

const PROJECTIVY = "com.spocky.projengmenu";
const STOCK = "com.google.android.tvlauncher";
const SETTINGS = "com.android.tv.settings";
const WRAITH = "com.google.android.tungsten.setupwraith";
const MONET = "com.klevico.monet";

async function home(ctx) {
  return (await ctx.device(SHIELD.serial)).home;
}

/// The TV must always have somewhere for the Home key to land.
async function assertKeepsHome(ctx) {
  const h = await home(ctx);
  ctx.assert.ok(
    h.handlers.some((c) => !c.includes("FallbackHome")),
    `a real Home app is still enabled (handlers: ${h.handlers.join(", ")})`,
  );
}

async function openPicker(ctx, pkg) {
  const page = ctx.page;
  await page.getByText("Advanced: set another app as Home…").click();
  const select = page.getByLabel("App to set as Home");
  await ctx.waitFor(async () => (await select.locator(`option[value="${pkg}"]`).count()) > 0, {
    message: `${pkg} in the picker`,
  });
  await select.selectOption(pkg);
}

export const scenarios = [
  {
    name: "launcher-lists-home-handlers-only",
    async run(ctx) {
      await ctx.reset(shieldScenario());
      await ctx.openDevice(SHIELD.key, "launcher");
      const table = ctx.page.locator("ul.launcher-list");
      await table.getByText("Projectivy Launcher").waitFor();
      const text = await table.innerText();
      // Installed TV apps that declare LEANBACK_LAUNCHER, not HOME.
      for (const app of ["com.plexapp.android", "com.google.android.youtube.tv", "com.android.vending", SETTINGS]) {
        ctx.assert.ok(!text.includes(app), `${app} is not listed as a Home app`);
      }
      ctx.assert.match(text, /Android TV Launcher \(Stock\)[\s\S]*DISABLED/);
      await ctx.step("only catalog launchers and real HOME handlers");
    },
  },
  {
    name: "launcher-switch-through-setup-wraith",
    async run(ctx) {
      await ctx.reset(shieldScenario({ setup: [`pm enable ${STOCK}`] }));
      await ctx.openDevice(SHIELD.key, "launcher");
      const row = ctx.page.locator("ul.launcher-list li").filter({ hasText: STOCK });
      await row.getByRole("button", { name: "Set as default" }).waitFor();
      // Android hands Home through Setup Wraith for the next few reads (#122).
      await ctx.deviceControl(SHIELD.serial, {
        transient_home: { holder: "com.google.android.tungsten.setupwraith/.ui.MainActivity", polls: 3 },
      });
      await row.getByRole("button", { name: "Set as default" }).click();
      await ctx.text("is now your default launcher", { exact: false }).waitFor({ timeout: 40_000 });
      await ctx.step("switch landed after the transient holder");
      ctx.assert.match((await home(ctx)).resolved, new RegExp(STOCK));
      ctx.assert.ok((await ctx.device(SHIELD.serial)).packages.enabled.includes(PROJECTIVY), "nothing was disabled");
      await assertKeepsHome(ctx);
    },
  },
  {
    // #122 on 2.3.0: stock disabled, Setup Wraith still enabled with a higher
    // HOME priority. Setting Monet works (it holds the HOME role and the Home
    // key opens it) but resolve-activity keeps naming Setup Wraith.
    name: "launcher-current-is-the-role-holder-not-setup-wraith",
    async run(ctx) {
      await ctx.reset(
        shieldScenario({
          props: { "ro.build.version.sdk": "31" },
          home_policy: "priority_resolver",
          home_apps: [
            { package: WRAITH, class: `${WRAITH}.ui.MainActivity`, priority: 3 },
            { package: MONET, class: `${MONET}.MainActivity`, priority: 0 },
          ],
        }),
      );
      await ctx.openDevice(SHIELD.key, "launcher");
      const rows = ctx.page.locator("ul.launcher-list li");
      const monet = rows.filter({ hasText: MONET });
      const wraith = rows.filter({ hasText: WRAITH });
      await monet.getByRole("button", { name: "Set as default" }).waitFor();
      ctx.assert.match(await wraith.innerText(), /Google TV setup helper — not a launcher/);
      ctx.assert.equal(await wraith.getByRole("button", { name: /set as default|set default/i }).count(), 0, "no Set as default on the setup helper");
      await ctx.step("Setup Wraith is labelled a setup helper");

      await monet.getByRole("button", { name: "Set as default" }).click();
      await ctx.text("is now your default launcher", { exact: false }).waitFor({ timeout: 40_000 });
      const d = await ctx.device(SHIELD.serial);
      ctx.assert.equal(d.home.role_holder, MONET, "Monet holds the HOME role");
      ctx.assert.match(d.home.resolved, new RegExp(WRAITH), "the resolver alone still names Setup Wraith");

      await ctx.page.getByRole("button", { name: "Refresh" }).click();
      await ctx.waitFor(async () => /ACTIVE/.test(await monet.innerText()), { message: "Monet tagged ACTIVE" });
      ctx.assert.doesNotMatch(await wraith.innerText(), /ACTIVE/);
      const current = await ctx.page.locator(".launcher-foot .foot-value").innerText();
      ctx.assert.match(current, new RegExp(MONET), "Current home app is Monet");
      ctx.assert.doesNotMatch(current, new RegExp(WRAITH));
      await ctx.step("Monet is the current launcher after refresh");

      await ctx.openDevice(SHIELD.key, "snapshot");
      await ctx.page.getByRole("button", { name: "Save snapshot" }).first().click();
      await ctx.page.getByRole("button", { name: "Preview restore" }).first().waitFor();
      const snaps = (await ctx.invoke("list_snapshots", {})).value;
      ctx.assert.equal(snaps[0].launcher, MONET, "the snapshot records Monet");
      await ctx.step("snapshot records Monet");
      await assertKeepsHome(ctx);
    },
  },
  {
    name: "launcher-set-home-refused-setter",
    async run(ctx) {
      await ctx.reset({
        ...stockHomeShield(),
        faults: [
          { matches: "set-home-activity", effect: { type: "fail", stdout: "Error: java.lang.SecurityException: Permission denial\n", exit_code: 255 } },
          { matches: "add-role-holder", effect: { type: "fail", stderr: "java.lang.SecurityException: Permission denial\n", exit_code: 255 } },
        ],
      });
      await ctx.openDevice(SHIELD.key, "launcher");
      await openPicker(ctx, PROJECTIVY);
      await ctx.step("picked Projectivy");
      await ctx.page.getByRole("button", { name: "Set as Home" }).click();
      const result = ctx.page.locator(".home-picker-result");
      await result.waitFor();
      ctx.assert.match(await result.innerText(), /Nothing was disabled/);
      await ctx.step("refusal reported, nothing disabled");
      const h = await home(ctx);
      ctx.assert.match(h.resolved, new RegExp(STOCK), "Home is still the stock launcher");
      ctx.assert.ok((await ctx.device(SHIELD.serial)).packages.enabled.includes(STOCK), "stock is still enabled");
      await assertKeepsHome(ctx);
    },
  },
  {
    name: "launcher-disable-stock-hands-home-over",
    async run(ctx) {
      await ctx.reset(stockHomeShield());
      await ctx.openDevice(SHIELD.key, "launcher");
      await openPicker(ctx, PROJECTIVY);
      await ctx.page.getByRole("button", { name: "Set as Home" }).click();
      await ctx.page.locator(".home-picker-result").getByText("stock launcher still holds Home", { exact: false }).waitFor({ timeout: 30_000 });
      await ctx.step("Android took the request but stock still holds Home");
      ctx.assert.ok((await ctx.device(SHIELD.serial)).packages.enabled.includes(STOCK), "Set as Home disabled nothing");
      await ctx.page.locator(".home-picker-row").getByRole("button", { name: "Disable stock launcher" }).click();
      await ctx.step("confirm");
      await ctx.page.locator(".home-picker-confirm").getByRole("button", { name: "Disable stock launcher" }).click();
      await ctx.page.locator(".home-picker-result").getByText("is Home", { exact: false }).waitFor({ timeout: 30_000 });
      await ctx.step("stock disabled, Projectivy is Home");
      const d = await ctx.device(SHIELD.serial);
      ctx.assert.ok(d.packages.disabled.includes(STOCK), "stock launcher disabled");
      ctx.assert.match(d.home.resolved, new RegExp(PROJECTIVY));
      await assertKeepsHome(ctx);
    },
  },
  {
    name: "launcher-takeover-verification-failure-restores-stock",
    async run(ctx) {
      await ctx.reset({
        ...stockHomeShield(),
        // The resolver never moves off stock, whatever is disabled.
        faults: [
          {
            matches: "resolve-activity",
            effect: { type: "output", stdout: `priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\n${STOCK}/.MainActivity\n` },
          },
        ],
      });
      await ctx.openDevice(SHIELD.key, "launcher");
      await openPicker(ctx, PROJECTIVY);
      await ctx.page.getByRole("button", { name: "Set as Home" }).click();
      await ctx.page.locator(".home-picker-result").waitFor({ timeout: 30_000 });
      await ctx.page.locator(".home-picker-row").getByRole("button", { name: "Disable stock launcher" }).click();
      await ctx.page.locator(".home-picker-confirm").getByRole("button", { name: "Disable stock launcher" }).click();
      await ctx.waitFor(async () => /Restore command completed|re-enabled/i.test(await ctx.page.locator(".home-picker-result").innerText()), {
        timeout: 40_000,
        message: "restore reported",
      });
      await ctx.step("verification failed and stock was restored");
      const d = await ctx.device(SHIELD.serial);
      ctx.assert.ok(d.packages.enabled.includes(STOCK), "stock launcher re-enabled");
      const enables = (await ctx.log()).filter((i) => i.args.join(" ").includes(`pm enable ${STOCK}`));
      ctx.assert.ok(enables.length > 0, "a restore `pm enable` was issued");
      await assertKeepsHome(ctx);
    },
  },
  {
    name: "launcher-settings-refused-and-home-kept",
    async run(ctx) {
      await ctx.reset(stockHomeShield());
      const refused = await ctx.invoke("disable_stock_launcher", { serial: SHIELD.key, target: SETTINGS });
      ctx.assert.equal(refused.value.ok, false);
      ctx.assert.match(refused.value.last_error, /Settings recovery fallback/);
      ctx.assert.ok((await ctx.device(SHIELD.serial)).packages.enabled.includes(STOCK), "nothing disabled for Settings");

      // In the UI: Settings can be tried as Home, but disabling stock for it is refused.
      await ctx.openDevice(SHIELD.key, "launcher");
      await openPicker(ctx, SETTINGS);
      await ctx.page.getByRole("button", { name: "Set as Home" }).click();
      await ctx.page.locator(".home-picker-result").waitFor({ timeout: 30_000 });
      const disable = ctx.page.locator(".home-picker-row").getByRole("button", { name: "Disable stock launcher" });
      if (await disable.isEnabled()) {
        await disable.click();
        await ctx.page.locator(".home-picker-confirm").getByRole("button", { name: "Disable stock launcher" }).click();
        await ctx.page.locator(".home-picker-result").getByText("Settings recovery fallback", { exact: false }).waitFor();
      }
      await ctx.step("Settings is never the takeover target");
      ctx.assert.ok((await ctx.device(SHIELD.serial)).packages.enabled.includes(STOCK));

      // The last real launcher can't be disabled from any path.
      await ctx.reset(shieldScenario());
      for (const cmd of ["disable_launcher", "disable_package"]) {
        const r = await ctx.invoke(cmd, { serial: SHIELD.key, package: PROJECTIVY });
        ctx.assert.equal(r.value.ok, false, `${cmd} refused`);
        ctx.assert.match(r.value.message, /only enabled launcher left/);
      }
      ctx.assert.ok((await ctx.device(SHIELD.serial)).packages.enabled.includes(PROJECTIVY));
      await assertKeepsHome(ctx);
    },
  },
];
