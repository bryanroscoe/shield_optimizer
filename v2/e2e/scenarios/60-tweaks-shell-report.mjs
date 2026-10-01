// Tweaks (the screensaver flow), the expert shell's acknowledgement gate, and
// the Report-a-bug bundle built by the real backend.

import { SHIELD, shieldScenario } from "../lib/worlds.mjs";

const BASIC = "com.android.dreams.basic/com.android.dreams.basic.BasicDream";
const BACKDROP = "com.google.android.backdrop/.Backdrop";

async function secure(ctx) {
  return (await ctx.device(SHIELD.serial)).settings.secure;
}

export const scenarios = [
  {
    name: "tweaks-screensaver",
    async run(ctx) {
      await ctx.reset(shieldScenario());
      await ctx.openDevice(SHIELD.key, "tweaks");
      const page = ctx.page;
      const section = page.locator(".tweak-row").filter({ hasText: "secure.screensaver_components" }).first();
      await section.getByText(`secure.screensaver_components = ${BACKDROP}`, { exact: false }).waitFor();
      await ctx.step("current screensaver read from the device");

      await section.getByRole("button", { name: "Basic Daydream" }).click();
      await ctx.waitFor(async () => (await secure(ctx)).screensaver_components === BASIC, { message: "Basic Daydream written" });
      ctx.assert.equal((await secure(ctx)).screensaver_enabled, "1");
      await section.getByText(`secure.screensaver_components = ${BASIC}`, { exact: false }).waitFor();
      await ctx.step("Basic Daydream");

      await section.getByRole("button", { name: "None" }).click();
      await ctx.waitFor(async () => (await secure(ctx)).screensaver_enabled === "0", { message: "screensaver off" });
      await ctx.step("None");

      await section.getByRole("button", { name: /Restore previous/ }).click();
      await ctx.waitFor(async () => {
        const s = await secure(ctx);
        return s.screensaver_components === BACKDROP && s.screensaver_enabled === "1";
      }, { message: "previous screensaver restored" });
      await ctx.step("Restore previous");

      // The device refuses the enable flag: neither setting may change.
      await ctx.fault({
        matches: "settings put secure screensaver_enabled",
        effect: { type: "fail", stderr: "java.lang.SecurityException: Permission Denial: writing to settings requires android.permission.WRITE_SECURE_SETTINGS\n", exit_code: 255 },
      });
      await section.getByRole("button", { name: "Basic Daydream" }).click();
      await page.locator(".action-message, .error, [role=status]").filter({ hasText: /screensaver/i }).first().waitFor();
      await ctx.step("refused flag write is reported");
      const s = await secure(ctx);
      ctx.assert.equal(s.screensaver_components, BACKDROP, "component unchanged after a refused flag write");
      ctx.assert.equal(s.screensaver_enabled, "1", "flag unchanged");
      ctx.assert.ok((await ctx.state()).faults[0].fired > 0, "the refusal was exercised");
    },
  },
  {
    // #99: on a real Shield the global key changed nothing, so the tab reports
    // Android's own limit and only offers to remove a leftover key.
    name: "tweaks-background-limit",
    async run(ctx) {
      await ctx.reset(shieldScenario());
      await ctx.openDevice(SHIELD.key, "tweaks");
      const page = ctx.page;
      const row = page.locator(".tweak-row").filter({ hasText: "CUR_MAX_CACHED_PROCESSES" }).first();
      await row.getByText("Up to 32 cached apps").waitFor();
      await row.getByText("global.background_process_limit = 2 (ignored by Android)").waitFor();
      ctx.assert.equal(await row.getByRole("button", { name: /≤/ }).count(), 0, "no limit presets are offered");
      await ctx.step("Android's limit and the leftover key");

      await row.getByRole("button", { name: "Remove old setting" }).click();
      await ctx.waitFor(async () => (await ctx.device(SHIELD.serial)).settings.global.background_process_limit === undefined, {
        message: "leftover key deleted",
      });
      await row.getByRole("button", { name: "Remove old setting" }).waitFor({ state: "detached" });
      await row.getByText("Up to 32 cached apps").waitFor();
      await ctx.step("leftover key removed, limit unchanged");
    },
  },
  {
    name: "shell-acknowledgement-gate",
    async run(ctx) {
      await ctx.reset(shieldScenario());
      await ctx.openDevice(SHIELD.key, "shell");
      const page = ctx.page;
      const input = page.getByPlaceholder("pm list packages -d");
      await input.fill("getprop ro.serialno");
      const run = page.getByRole("button", { name: /^Run/ }).first();
      ctx.assert.equal(await run.isDisabled(), true, "Run is disabled until acknowledged");
      await ctx.step("run gated on the acknowledgement");
      const before = (await ctx.log()).filter((i) => i.args.join(" ").includes("getprop ro.serialno") && i.args.length === 4).length;
      await page.getByText("I understand these risks", { exact: false }).click();
      await run.click();
      await page.getByText(SHIELD.serial).last().waitFor();
      await ctx.step("acknowledged, the command ran on the device");
      const after = (await ctx.log()).filter((i) => i.args.join(" ").includes("getprop ro.serialno") && i.args.length === 4).length;
      ctx.assert.ok(after > before, "the shell command reached adb only after acknowledgement");
    },
  },
  {
    name: "report-bug-diagnostics",
    async run(ctx) {
      await ctx.reset(shieldScenario());
      await ctx.openDevice(SHIELD.key, "overview");
      await ctx.page.getByRole("button", { name: "Report a bug" }).click();
      const bundle = ctx.page.getByLabel("Diagnostics");
      await ctx.waitFor(async () => (await bundle.inputValue()).includes("### Device"), { message: "diagnostics collected" });
      const text = await bundle.inputValue();
      ctx.assert.match(text, /## ATV Optimizer diagnostics/);
      ctx.assert.match(text, new RegExp(SHIELD.key.replace(/\./g, "\\.")));
      ctx.assert.match(text, /1324619053514/, "device identity from the real profile");
      ctx.assert.match(text, /com\.spocky\.projengmenu/, "HOME handlers from the real query");
      ctx.assert.match(text, /adb version: Android Debug Bridge/, "adb version from the driver");
      ctx.assert.doesNotMatch(text, /package:com\./, "no package inventory in the bundle");
      await ctx.step("bundle built from the backend");
    },
  },
];
