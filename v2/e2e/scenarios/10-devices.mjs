// Devices tab: discovery, identity and the connection lifecycle, against the
// real device commands and the simulated adb server.

import { PIXEL, SHIELD } from "../lib/worlds.mjs";

const row = (ctx, serial) => ctx.page.locator(`[data-serial="${serial}"]`);
/// A saved device's row while it isn't connected.
const savedRow = (ctx, hardwareId) => ctx.page.locator(`[data-saved-id="${hardwareId}"]`);
const savedRecords = (ctx) =>
  ctx.page.evaluate(() => JSON.parse(localStorage.getItem("shieldopt.savedDevices.v1") ?? "{}").devices ?? []);

/// A second box on the LAN that has never approved this computer.
const UNAUTHORIZED_BOX = {
  profile: "shield-tv-pro",
  serial: "0323716101827",
  ip: "192.0.2.71",
  authorized: false,
  transports: [],
};

/// A Pixel reachable under both its mDNS service name and ip:port.
const PIXEL_TWO_WAYS = {
  profile: "pixel-10-pro",
  ip: "192.0.2.2",
  wireless: { connect_port: 41235, connect_instance: PIXEL.instance, pairing: null },
  paired: true,
  transports: [{ key: PIXEL.key }, { key: "192.0.2.2:41235" }],
};

export const scenarios = [
  {
    name: "devices-saved-redial",
    async run(ctx) {
      await ctx.reset({ devices: [{ profile: "shield-tv-pro" }] });
      await ctx.open("/");
      await row(ctx, SHIELD.key).getByRole("button", { name: `Unsave ${SHIELD.name}` }).waitFor();
      await ctx.waitFor(async () => (await savedRecords(ctx)).find((d) => d.hardwareId === SHIELD.serial)?.mac,
        { message: "the MAC was read and saved" });
      const [record] = await savedRecords(ctx);
      ctx.assert.equal(record.address, SHIELD.key);
      ctx.assert.match(record.mac, /^00:1a:11:/);
      await ctx.step("a connected Shield is saved automatically");

      // Asleep: off the list, and neither the launch dial nor the scan gets
      // an answer.
      await ctx.detach(SHIELD.key);
      await ctx.fault({ scope: "adb", matches: `connect ${SHIELD.key}`,
        effect: { type: "fail", stdout: `failed to connect to '${SHIELD.key}': Operation timed out`, exit_code: 1 } });
      await ctx.open("/");
      await savedRow(ctx, SHIELD.serial).getByText("Didn't answer").waitFor({ timeout: 20_000 });
      await ctx.step("relaunch: the saved Shield didn't answer");
      // Woken up.
      await ctx.clearFaults();
      await savedRow(ctx, SHIELD.serial).getByRole("button", { name: `Connect to ${SHIELD.name}` }).click();
      await row(ctx, SHIELD.key).getByText("Online").waitFor();
      ctx.assert.deepEqual((await ctx.state()).transports.map((t) => t.key), [SHIELD.key]);
      await ctx.step("Connect dialled the saved address and it's online");

      // Another TV now answers at the saved address.
      await ctx.reset({ devices: [{ profile: "shield-tv-pro", serial: "0323716101827", transports: [] }] });
      await ctx.open("/");
      await savedRow(ctx, SHIELD.serial).locator(".device-status", { hasText: "Different device" }).waitFor();
      const other = row(ctx, SHIELD.key);
      await other.locator('[data-bookmark="0323716101827"]').waitFor();
      ctx.assert.equal(await savedRow(ctx, SHIELD.serial).locator("a").count(), 0, "the saved row doesn't open the other TV");
      ctx.assert.equal(await ctx.page.locator(".device-section").locator(`[data-serial="${SHIELD.key}"]`).count(), 0,
        "the other TV is listed under Other devices");
      ctx.assert.ok(!(await savedRecords(ctx)).some((d) => d.hardwareId === "0323716101827"),
        "it answered only the saved Shield's dial, so it isn't saved");
      await ctx.step("a different TV at the saved address gets its own unsaved row");
    },
  },
  {
    name: "devices-scan-lan",
    async run(ctx) {
      await ctx.reset({
        devices: [
          { profile: "shield-tv-pro", transports: [] },
          UNAUTHORIZED_BOX,
        ],
      });
      await ctx.open("/");
      await ctx.step("empty list");
      await ctx.page.getByRole("button", { name: "Scan LAN" }).click();
      await ctx.text("Scanned 192.0.2.x", { exact: false }).waitFor();
      await ctx.text(SHIELD.name).first().waitFor();
      await ctx.step("scan connected the Shield and found an unauthorized box");
      await ctx.text("need", { exact: false }).first().waitFor();
      ctx.assert.match(await ctx.page.locator(".connect-message").allInnerTexts().then((t) => t.join(" ")), /1 needs authorization/);
      await row(ctx, "192.0.2.71:5555").getByText("NOT ALLOWED YET").waitFor();
      await row(ctx, "192.0.2.71:5555").getByText("it's probably asking now", { exact: false }).waitFor();
      await row(ctx, "192.0.2.71:5555").getByRole("button", { name: "Request approval from 192.0.2.71:5555" }).waitFor();
      await ctx.step("unauthorized row is one compact row with Request approval");
      const state = await ctx.state();
      const keys = state.transports.map((t) => `${t.key}=${t.state}`).sort();
      ctx.assert.deepEqual(keys, ["192.0.2.1:5555=device", "192.0.2.71:5555=unauthorized"]);
    },
  },
  {
    name: "devices-two-transports-forget",
    async run(ctx) {
      await ctx.reset({ devices: [PIXEL_TWO_WAYS] });
      await ctx.open("/");
      await ctx.text("NOT AN ANDROID TV").first().waitFor();
      ctx.assert.equal(await ctx.page.locator(".device-row").count(), 1, "one device, two transports, one row");
      await ctx.step("one row for two transports");
      await row(ctx, "192.0.2.2:41235").getByRole("button", { name: "Forget" }).click();
      await ctx.waitFor(async () => (await ctx.page.locator(".device-row:not(.saved-offline)").count()) === 0, { message: "row gone" });
      // It was saved when it connected, so it stays listed as a saved device.
      await savedRow(ctx, PIXEL.serial).getByText("Not connected").waitFor();
      await ctx.step("forget disconnected it; the saved row stays");
      const state = await ctx.state();
      ctx.assert.deepEqual(state.transports, [], "both transports dropped");
      const disconnects = (await ctx.log()).filter((i) => i.args[0] === "disconnect").map((i) => i.args[1]).sort();
      ctx.assert.deepEqual(disconnects, ["192.0.2.2:41235", PIXEL.key].sort());
    },
  },
  {
    name: "devices-alias-route",
    async run(ctx) {
      await ctx.reset({ devices: [PIXEL_TWO_WAYS] });
      await ctx.openDevice(PIXEL.key);
      await ctx.page.waitForURL((url) => url.pathname === `/devices/${encodeURIComponent("192.0.2.2:41235")}`);
      await ctx.text("Pixel 10 Pro").first().waitFor();
      ctx.assert.equal(await ctx.page.locator(".error").filter({ hasText: "not found" }).count(), 0);
      await ctx.step("the mDNS alias opened the collapsed row under its ip:port key");
      const resolved = await ctx.invoke("device_profile", { serial: PIXEL.key });
      ctx.assert.equal(resolved.ok, true);
      ctx.assert.equal(resolved.value.serial, "192.0.2.2:41235");
      ctx.assert.equal(resolved.value.properties.serial_number, PIXEL.serial);
      // A key adb does not hold is still not found: nothing is guessed from
      // an address that merely shares the device's IP.
      const stranger = await ctx.invoke("device_profile", { serial: "192.0.2.2:5555" });
      ctx.assert.equal(stranger.ok, false);
      ctx.assert.match(String(stranger.error), /not found/);
    },
  },
  {
    name: "devices-reattach-message",
    async run(ctx) {
      await ctx.reset({
        devices: [{ ...PIXEL_TWO_WAYS, transports: [{ key: PIXEL.key }], auto_attach: true, reattach_after_polls: 2 }],
      });
      await ctx.open("/");
      await row(ctx, PIXEL.key).waitFor();
      await row(ctx, PIXEL.key).getByRole("button", { name: "Forget" }).click();
      await ctx.text("still advertising Wireless debugging", { exact: false }).first().waitFor();
      await ctx.step("forget warns that adb will re-attach it");
      await ctx.text("reconnected by itself", { exact: false }).waitFor({ timeout: 20_000 });
      await row(ctx, PIXEL.key).waitFor();
      await ctx.step("the row came back and the app says why");
    },
  },
  {
    name: "devices-tv-evidence",
    async run(ctx) {
      await ctx.reset({
        devices: [
          {
            profile: "shield-tv-pro",
            serial: "9AB4C21D7E03",
            ip: "192.0.2.77",
            leanback: null,
            props: { "ro.build.characteristics": "nosdcard", "ro.product.brand": "Xiaomi", "ro.product.model": "MiBOX4", "ro.product.manufacturer": "Xiaomi" },
          },
          PIXEL_TWO_WAYS,
        ],
      });
      await ctx.open("/");
      const unconfirmed = row(ctx, "192.0.2.77:5555");
      await unconfirmed.getByText("UNCONFIRMED TV").waitFor();
      ctx.assert.equal(await unconfirmed.evaluate((el) => el.tagName), "A", "an unconfirmed TV still opens");
      const phone = row(ctx, "192.0.2.2:41235");
      await phone.getByText("NOT AN ANDROID TV").waitFor();
      ctx.assert.equal(await phone.evaluate((el) => el.tagName), "DIV", "a phone does not open by default");
      await ctx.step("unconfirmed TV opens, not-a-TV is labelled");
      await phone.getByRole("button", { name: "Open anyway" }).first().click();
      await ctx.step("open anyway asks first");
      await ctx.page.locator(".open-anyway-confirm").getByRole("button", { name: "Open tools" }).click();
      await ctx.page.waitForURL(/\/devices\//);
      await ctx.page.locator("#tab-overview").waitFor();
      await ctx.text("Pixel 10 Pro").first().waitFor();
      await ctx.step("tools opened on the phone");
    },
  },
  {
    name: "devices-pair-pin-auto-connect",
    async run(ctx) {
      await ctx.reset({
        devices: [
          {
            profile: "shield-tv-pro",
            serial: "PAIRTV00001",
            ip: "192.0.2.50",
            legacy_port: null,
            wireless: {
              connect_port: 37011,
              connect_instance: "adb-PAIRTV00001-CoNNeC",
              pairing: ["adb-PAIRTV00001-PaIrNg", 41999, "246810"],
            },
            transports: [],
          },
        ],
      });
      await ctx.open("/");
      await ctx.page.locator(".connect-form").getByRole("button", { name: "Pair PIN" }).click();
      await ctx.page.getByPlaceholder("IP:pair_port — e.g. 192.168.42.71:43219").fill("192.0.2.50:41999");
      await ctx.page.getByPlaceholder("6-digit PIN").fill("246810");
      await ctx.step("pair form filled");
      await ctx.page.getByRole("button", { name: "Pair", exact: true }).click();
      await row(ctx, "192.0.2.50:37011").waitFor({ timeout: 30_000 });
      await ctx.step("paired, then connected on the advertised connect port");
      const state = await ctx.state();
      ctx.assert.deepEqual(state.paired, ["PAIRTV00001"]);
      ctx.assert.deepEqual(state.transports.map((t) => t.key), ["192.0.2.50:37011"]);
      const pairArgs = (await ctx.log()).find((i) => i.args[0] === "pair").args;
      ctx.assert.deepEqual(pairArgs.slice(0, 2), ["pair", "192.0.2.50:41999"], "paired on the pairing port");
    },
  },
  {
    name: "devices-connect-errors",
    async run(ctx) {
      await ctx.reset({ devices: [] });
      await ctx.open("/");
      const box = ctx.page.getByPlaceholder("IP[:port] — e.g. 192.168.42.71");
      await box.fill("192.0.2.999");
      await ctx.page.locator(".connect-form").getByRole("button", { name: "Add by IP" }).click();
      await ctx.text("not an IP address", { exact: false }).waitFor();
      await ctx.step("a typo'd IP is rejected before adb runs");
      ctx.assert.ok(!(await ctx.log()).some((i) => i.args[0] === "connect"), "adb connect never ran");
      await box.fill("192.0.2.123");
      await ctx.page.locator(".connect-form").getByRole("button", { name: "Add by IP" }).click();
      await ctx.page.locator(".connect-message").first().waitFor();
      const message = await ctx.page.locator(".connect-message").first().innerText();
      ctx.assert.doesNotMatch(message, /adb process failed/, "the raw driver error is explained, not dumped");
      await ctx.step("an unreachable address gets an explanation");
    },
  },
];
