// Remote › Open app: the TV's launchable apps through the real command layer,
// a launch that the simulated TV starts, one it refuses, and a pinned
// shortcut filed under the TV's hardware id.

import { SHIELD, shieldScenario } from "../lib/worlds.mjs";

const PLEX = "com.plexapp.android";
const VLC = "org.videolan.vlc";

export const scenarios = [
  {
    name: "remote-open-app-ipc",
    async run(ctx) {
      await ctx.reset(shieldScenario());
      const listed = await ctx.invoke("list_launchable_apps", { serial: SHIELD.key });
      ctx.assert.equal(listed.ok, true, String(listed.error));
      const packages = listed.value.map((a) => a.package);
      for (const pkg of [PLEX, VLC, "com.google.android.youtube.tv"]) ctx.assert.ok(packages.includes(pkg), `${pkg} is listed`);
      ctx.assert.ok(!packages.includes("com.nvidia.tegrazone3"), "a disabled app isn't listed");
      ctx.assert.ok(!packages.includes("com.spocky.projengmenu"), "the Home app isn't listed");

      const { value: { generation } } = await ctx.invoke("remote_open", { serial: SHIELD.key });
      const opened = await ctx.invoke("launch_app", { serial: SHIELD.key, package: PLEX, generation });
      ctx.assert.equal(opened.ok, true, String(opened.error));
      ctx.assert.equal(opened.value.ok, true, opened.value.message);
      const injected = await ctx.invoke("launch_app", { serial: SHIELD.key, package: "com.x; reboot", generation });
      ctx.assert.equal(injected.ok, false, "a non-package never reaches the TV");
      await ctx.shell(SHIELD.serial, `pm disable-user --user 0 ${VLC}`);
      const refused = await ctx.invoke("launch_app", { serial: SHIELD.key, package: VLC, generation });
      ctx.assert.equal(refused.value.ok, false);
      ctx.assert.match(refused.value.message, /unable to resolve/);
      const started = (await ctx.device(SHIELD.serial)).started;
      ctx.assert.ok(started.some((s) => s.endsWith(PLEX) && s.includes("LEANBACK_LAUNCHER")), "Plex started through its TV launch activity");
      ctx.assert.ok(!started.some((s) => s.includes("reboot")));
      ctx.assert.equal((await ctx.state()).gaps.length, 0, "no simulator gaps");
    },
  },
  {
    name: "remote-open-app-ui",
    async run(ctx) {
      await ctx.reset(shieldScenario());
      await ctx.openDevice(SHIELD.key, "remote");
      const panel = ctx.page.locator("#tabpanel-remote");
      await panel.getByRole("button", { name: "Open app", exact: true }).click();
      const apps = panel.locator("#remote-apps-panel");
      await apps.locator(`li[data-app="${PLEX}"]`).waitFor();
      await ctx.step("apps on the simulated Shield");
      await apps.getByRole("button", { name: /^Open Plex/ }).click();
      await apps.getByText(/^Opened Plex/).waitFor();
      await ctx.waitFor(async () => (await ctx.device(SHIELD.serial)).started.some((s) => s.endsWith(PLEX)), { message: "Plex started" });
      await apps.getByRole("button", { name: /^Pin Plex/ }).click();
      await panel.locator("[data-shortcuts]").getByRole("button", { name: /^Plex/ }).waitFor();
      const pins = await ctx.page.evaluate(() => JSON.parse(localStorage.getItem("atvopt.remotePins") ?? "{}"));
      ctx.assert.deepEqual(pins, { [SHIELD.serial]: [PLEX] }, "pinned under the TV's hardware id, not its address");
      await ctx.step("Plex pinned as a shortcut");
      ctx.assert.equal((await ctx.state()).gaps.length, 0, "no simulator gaps");
    },
  },
];
