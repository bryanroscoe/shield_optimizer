// Google TV's Setup Wraith (#122): it declares HOME with a high filter
// priority, so once the stock launcher is disabled it takes the Home button
// back unless the takeover turns it off too. The simulator's
// `stock_then_priority` policy models the reporter's onn 4K Pro.

import { SHIELD, shieldScenario, stockHomeShield } from "../lib/worlds.mjs";

const PROJECTIVY = "com.spocky.projengmenu";
const GTV_HOME = "com.google.android.apps.tv.launcherx";
const WRAITH = "com.google.android.tungsten.setupwraith";
const SDK31 = { "ro.build.version.sdk": "31" };

function googleTv(extra = {}) {
  const { setup = [], ...rest } = extra;
  return shieldScenario({
    props: SDK31,
    home_policy: "stock_then_priority",
    home_apps: [
      { package: GTV_HOME, class: `${GTV_HOME}.home.HomeActivity`, priority: 0 },
      { package: WRAITH, class: `${WRAITH}.ui.MainActivity`, priority: 3 },
      { package: PROJECTIVY, class: `${PROJECTIVY}.ui.home.MainActivity`, priority: 0 },
    ],
    setup,
    ...rest,
  });
}

const HOME_ROLE = `cmd role add-role-holder android.app.role.HOME ${PROJECTIVY}`;

async function device(ctx) {
  return ctx.device(SHIELD.serial);
}

async function assertKeepsHome(ctx) {
  const h = (await device(ctx)).home;
  ctx.assert.ok(
    h.handlers.some((c) => !c.includes("FallbackHome")),
    `a real Home app is still enabled (handlers: ${h.handlers.join(", ")})`,
  );
}

function callout(ctx) {
  return ctx.page.locator(".setup-helper-callout");
}

async function takeOver(ctx) {
  const set = await ctx.invoke("set_home_any", { serial: SHIELD.key, package: PROJECTIVY, activity: null });
  ctx.assert.equal(set.value.ok, true, "Projectivy accepted as Home");
  return ctx.invoke("disable_stock_launcher", { serial: SHIELD.key, target: PROJECTIVY });
}

export const scenarios = [
  {
    name: "setup-wraith-takeover-disables-it-and-recovery-restores-both",
    async run(ctx) {
      await ctx.reset(googleTv());
      const before = await device(ctx);
      ctx.assert.ok(before.packages.enabled.includes(WRAITH), "Setup Wraith starts enabled");
      ctx.assert.ok(before.packages.enabled.includes(GTV_HOME), "stock starts enabled");

      const res = await takeOver(ctx);
      ctx.assert.equal(res.value.ok, true, JSON.stringify(res.value.diagnostics));
      const d = await device(ctx);
      ctx.assert.ok(d.packages.disabled.includes(GTV_HOME), "stock disabled");
      ctx.assert.ok(d.packages.disabled.includes(WRAITH), "Setup Wraith disabled with it");
      ctx.assert.match(d.home.resolved, new RegExp(PROJECTIVY), "Projectivy keeps Home");
      ctx.assert.equal(d.home.role_holder, PROJECTIVY);
      await assertKeepsHome(ctx);

      await ctx.openDevice(SHIELD.key, "launcher");
      await ctx.page.locator("ul.launcher-list").getByText("Google TV setup helper").waitFor();
      await callout(ctx).waitFor();
      ctx.assert.equal(await callout(ctx).getAttribute("data-setup-helper"), "off");
      await ctx.step("takeover left Setup Wraith off, with its way back");

      await callout(ctx).getByRole("button", { name: "Re-enable Setup Wraith" }).click();
      await ctx.waitFor(async () => (await device(ctx)).packages.enabled.includes(WRAITH), { message: "Setup Wraith re-enabled" });
      await ctx.waitFor(async () => (await callout(ctx).getAttribute("data-setup-helper")) === "risk", { message: "callout warns it is on with stock off" });
      await ctx.step("Re-enable Setup Wraith works");

      // Emergency Recovery re-enables everything disabled, stock and helper alike.
      await ctx.invoke("disable_setup_helper", { serial: SHIELD.key, package: WRAITH });
      const rec = await ctx.invoke("panic_recovery", { serial: SHIELD.key });
      ctx.assert.ok(rec.value.restored.includes(GTV_HOME), "recovery restored stock");
      const after = await device(ctx);
      ctx.assert.ok(after.packages.enabled.includes(GTV_HOME), "stock enabled again");
      ctx.assert.ok(after.packages.enabled.includes(WRAITH), "Setup Wraith enabled again");
      await assertKeepsHome(ctx);
    },
  },
  {
    name: "setup-wraith-re-enabling-stock-re-enables-the-helper",
    async run(ctx) {
      await ctx.reset(googleTv());
      const res = await takeOver(ctx);
      ctx.assert.equal(res.value.ok, true, JSON.stringify(res.value.diagnostics));
      await ctx.openDevice(SHIELD.key, "launcher");
      await callout(ctx).waitFor();
      const stockRow = ctx.page.locator("ul.launcher-list li", { hasText: "Google TV Home (Stock)" });
      await stockRow.getByRole("button", { name: "Enable", exact: true }).click();
      await ctx.waitFor(
        async () => {
          const p = (await device(ctx)).packages.enabled;
          return p.includes(GTV_HOME) && p.includes(WRAITH);
        },
        { message: "stock and Setup Wraith both re-enabled" },
      );
      await assertKeepsHome(ctx);
    },
  },
  {
    name: "setup-wraith-takeover-failure-restores-stock-and-the-helper",
    async run(ctx) {
      await ctx.reset({
        ...googleTv(),
        faults: [
          { matches: `pm disable-user --user 0 ${WRAITH}`, effect: { type: "fail", stderr: "java.lang.SecurityException: Permission denial\n", exit_code: 255 } },
        ],
      });
      const res = await takeOver(ctx);
      ctx.assert.equal(res.value.ok, false, "the takeover reports the failure");
      const d = await device(ctx);
      ctx.assert.ok(d.packages.enabled.includes(GTV_HOME), "stock re-enabled");
      ctx.assert.ok(d.packages.enabled.includes(WRAITH), "Setup Wraith re-enabled");
      await assertKeepsHome(ctx);
    },
  },
  {
    name: "setup-wraith-callout-states",
    async run(ctx) {
      // Enabled, stock still on: heads-up pointing at the takeover.
      await ctx.reset(googleTv());
      await ctx.openDevice(SHIELD.key, "launcher");
      await callout(ctx).waitFor();
      ctx.assert.equal(await callout(ctx).getAttribute("data-setup-helper"), "on");
      ctx.assert.match(await callout(ctx).innerText(), /Disable stock launcher turns it off too/);
      ctx.assert.equal(await callout(ctx).getByRole("button").count(), 0, "no action while stock is on");
      await ctx.step("enabled while stock is on");

      // Enabled while stock is off and nothing real holds Home: it grabs Home.
      await ctx.reset(googleTv({ setup: [`pm disable-user --user 0 ${GTV_HOME}`] }));
      await ctx.openDevice(SHIELD.key, "launcher");
      await callout(ctx).waitFor();
      ctx.assert.equal(await callout(ctx).getAttribute("data-setup-helper"), "risk");
      ctx.assert.match(await callout(ctx).innerText(), /will likely grab the Home button/);
      ctx.assert.match((await device(ctx)).home.resolved, new RegExp(WRAITH), "the resolver names Setup Wraith");
      await ctx.step("enabled while stock is off");

      await callout(ctx).getByRole("button", { name: "Turn it off" }).click();
      await ctx.waitFor(async () => (await device(ctx)).packages.disabled.includes(WRAITH), { message: "Setup Wraith disabled" });
      await ctx.waitFor(async () => (await callout(ctx).getAttribute("data-setup-helper")) === "off", { message: "callout says it is off" });
      ctx.assert.match((await device(ctx)).home.resolved, new RegExp(PROJECTIVY), "Home lands on Projectivy");
      ctx.assert.match(await callout(ctx).innerText(), /sign in again or you\s+need to pair a remote/);
      await ctx.step("one-click fix turned it off");
      await assertKeepsHome(ctx);

      // Disabled: the way back is on the Launcher tab.
      await callout(ctx).getByRole("button", { name: "Re-enable Setup Wraith" }).waitFor();
    },
  },
  {
    name: "setup-wraith-one-click-fix-refuses-without-a-real-launcher",
    async run(ctx) {
      await ctx.reset(
        googleTv({
          setup: [`pm disable-user --user 0 ${GTV_HOME}`, `pm disable-user --user 0 ${PROJECTIVY}`],
        }),
      );
      const r = await ctx.invoke("disable_setup_helper", { serial: SHIELD.key, package: WRAITH });
      ctx.assert.equal(r.value.ok, false);
      ctx.assert.ok((await device(ctx)).packages.enabled.includes(WRAITH), "left enabled");
      const notHelper = await ctx.invoke("disable_setup_helper", { serial: SHIELD.key, package: PROJECTIVY });
      ctx.assert.equal(notHelper.value.ok, false, "only a catalogued setup helper is accepted");
      await assertKeepsHome(ctx);
    },
  },
  {
    name: "setup-wraith-confirm-names-both-effects",
    async run(ctx) {
      // The role readback is unavailable, so Android's answer leaves stock in
      // charge and the picker offers the takeover.
      await ctx.reset({
        ...googleTv(),
        faults: [{ matches: "get-role-holders", effect: { type: "fail", stderr: "Error\n", exit_code: 255 } }],
      });
      await ctx.openDevice(SHIELD.key, "launcher");
      await ctx.page.getByText("Advanced: set another app as Home…").click();
      const select = ctx.page.getByLabel("App to set as Home");
      await ctx.waitFor(async () => (await select.locator(`option[value="${PROJECTIVY}"]`).count()) > 0, { message: "picker lists Projectivy" });
      await select.selectOption(PROJECTIVY);
      await ctx.page.getByRole("button", { name: "Set as Home" }).click();
      await ctx.page.locator(".home-picker-result").waitFor({ timeout: 30_000 });
      await ctx.page.locator(".home-picker-row").getByRole("button", { name: "Disable stock launcher" }).click();
      const confirm = ctx.page.locator(".home-picker-confirm");
      await confirm.waitFor();
      const text = await confirm.innerText();
      ctx.assert.match(text, /Also turns off Google TV's setup helper \(Setup Wraith\)/);
      ctx.assert.match(text, /turn it back on briefly to sign in to Google again or\s+pair a remote/);
      ctx.assert.match(text, /Save snapshot first/, "snapshot-first is still offered");
      await ctx.step("confirm names both effects");
    },
  },
  {
    name: "setup-wraith-shield-is-unchanged",
    async run(ctx) {
      // The captured Shield ships Setup Wraith installed but disabled; it must
      // get no row and no callout.
      await ctx.reset(stockHomeShield());
      await ctx.openDevice(SHIELD.key, "launcher");
      await ctx.page.locator("ul.launcher-list").getByText("Projectivy Launcher").waitFor();
      ctx.assert.equal(await callout(ctx).count(), 0, "no setup-helper callout on a Shield");
      const res = await ctx.invoke("set_home_any", { serial: SHIELD.key, package: PROJECTIVY, activity: null });
      ctx.assert.equal(res.value.ok, false, "stock still holds Home");
      const out = await ctx.invoke("disable_stock_launcher", { serial: SHIELD.key, target: PROJECTIVY });
      ctx.assert.equal(out.value.ok, true, JSON.stringify(out.value.diagnostics));
      const d = await device(ctx);
      ctx.assert.ok(d.packages.disabled.includes("com.google.android.tvlauncher"), "stock disabled");
      ctx.assert.match(d.home.resolved, new RegExp(PROJECTIVY));
      const log = (await ctx.log()).map((i) => i.args.join(" ")).join("\n");
      ctx.assert.ok(!log.includes("setupwraith"), "no command named the setup helper");
      await ctx.openDevice(SHIELD.key, "launcher");
      ctx.assert.equal(await callout(ctx).count(), 0, "still no callout");
      await assertKeepsHome(ctx);
    },
  },
];
