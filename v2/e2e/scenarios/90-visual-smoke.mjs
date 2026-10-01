// Visual smoke pass: every screen, both themes, against the real backend and
// the captured Shield. Not an assertion-heavy test — the screenshots are for a
// reviewer (human or agent) to eyeball. It does fail on page errors and on
// any command the simulator could not answer.

import { SHIELD, shieldScenario } from "../lib/worlds.mjs";

const TABS = [
  "overview",
  "health",
  "optimize",
  "apps",
  "launcher",
  "tweaks",
  "snapshot",
  "sideload",
  "remote",
  "files",
  "media",
  "shell",
];

function smoke(theme) {
  return {
    name: `visual-smoke-${theme}`,
    colorScheme: theme,
    async run(ctx) {
      await ctx.reset(shieldScenario());
      await ctx.open("/");
      await ctx.text("Test SHIELD Android TV").first().waitFor();
      await ctx.step("devices");
      await ctx.openDevice(SHIELD.key);
      for (const tab of TABS) {
        await ctx.tab(tab);
        await ctx.page.waitForTimeout(400);
        await ctx.step(`tab ${tab}`);
      }
      await ctx.open("/snapshots");
      await ctx.step("snapshots page");
      const errors = ctx.consoleErrors.filter((e) => !e.includes("favicon"));
      ctx.assert.deepEqual(errors, [], "no page errors");
      const state = await ctx.state();
      ctx.assert.deepEqual(state.gaps, [], "every command was simulated");
    },
  };
}

export const scenarios = [smoke("dark"), smoke("light")];
