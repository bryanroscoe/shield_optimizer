// Saved devices (#411) are matched on verified hardware id and nothing else.
// These drive the pure store and status rules: auto-save, unsave that sticks,
// a different device answering at a saved address, a device that reports no
// id, the MAC filed only under the device that answered, and the list
// surviving a restart (a fresh read of the same storage). Then the real
// Devices screen in demo mode: launch states, Connect retry, save/unsave
// across a reload, and a different device at a saved address.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const v2Root = dirname(dirname(fileURLToPath(import.meta.url)));
const toUrl = (source) =>
  "data:text/javascript," + encodeURIComponent(stripTypeScriptTypes(source, { mode: "strip" }));
const hardwareIdUrl = toUrl(readFileSync(join(v2Root, "src/lib/hardware-id.ts"), "utf8"));
const saved = await import(
  toUrl(readFileSync(join(v2Root, "src/lib/saved-devices.ts"), "utf8").replace('"./hardware-id"', JSON.stringify(hardwareIdUrl)))
);

class MemoryStorage {
  constructor() { this.map = new Map(); }
  getItem(key) { return this.map.has(key) ? this.map.get(key) : null; }
  setItem(key, value) { this.map.set(key, String(value)); }
}

const tv = (over = {}) => ({
  serial: "192.168.1.42:5555",
  name: "Living Room Shield",
  model: "SHIELD Android TV (2019 Pro)",
  status: "device",
  connection: "network",
  device_type: "shield",
  properties: { serial_number: "0323220012345", manufacturer: "NVIDIA" },
  ...over,
});
const empty = { devices: [], unsaved: [] };
const now = new Date("2026-11-23T12:00:00Z");

// --- auto-save ---------------------------------------------------------------
{
  const s = saved.observeConnected(empty, tv(), now);
  assert.equal(s.devices.length, 1, "a connected network TV with a verified id is saved");
  assert.deepEqual(s.devices[0], {
    hardwareId: "0323220012345",
    address: "192.168.1.42:5555",
    name: "Living Room Shield",
    model: "SHIELD Android TV (2019 Pro)",
    manufacturer: "NVIDIA",
    deviceType: "shield",
    lastConnected: now.toISOString(),
  });
  assert.equal(saved.observeConnected(s, tv(), now), s, "no change returns the same store");
}
for (const [why, d] of [
  ["no hardware id", tv({ properties: null })],
  ["the 'unknown' placeholder id", tv({ properties: { serial_number: " unknown " } })],
  ["unauthorized", tv({ status: "unauthorized", properties: null })],
  ["USB, no address", tv({ connection: "usb", serial: "0323220012345" })],
  ["a Wireless debugging service name", tv({ serial: "adb-0323220012345-AbCd._adb-tls-connect._tcp" })],
]) {
  assert.equal(saved.observeConnected(empty, d, now), empty, `not saved: ${why}`);
}

// A saved TV reached at a new address follows it; one reached under a service
// name keeps its last dialable address.
{
  const s = saved.observeConnected(empty, tv(), now);
  const moved = saved.observeConnected(s, tv({ serial: "192.168.1.50:40123" }), now);
  assert.equal(moved.devices[0].address, "192.168.1.50:40123");
  const mdns = saved.observeConnected(s, tv({ serial: "adb-x._adb-tls-connect._tcp", name: "Renamed" }), now);
  assert.equal(mdns.devices[0].address, "192.168.1.42:5555");
  assert.equal(mdns.devices[0].name, "Renamed");
}

// --- unsave sticks -----------------------------------------------------------
{
  const s = saved.observeConnected(empty, tv(), now);
  const off = saved.unsaveDevice(s, "0323220012345");
  assert.equal(off.devices.length, 0);
  assert.deepEqual(off.unsaved, ["0323220012345"]);
  assert.equal(saved.observeConnected(off, tv(), now), off, "an unsaved TV is not re-saved on its next connection");
  const again = saved.saveDevice(off, tv(), now);
  assert.equal(again.devices.length, 1, "the bookmark saves it again");
  assert.deepEqual(again.unsaved, [], "and clears the unsave");
  assert.equal(saved.saveDevice(empty, tv({ properties: null }), now), null, "an id-less device can't be saved");
  assert.equal(saved.saveDevice(empty, tv({ connection: "usb", serial: "X" }), now), null, "nor a USB one");
}

// --- status: identity, never the address -------------------------------------
{
  const s = saved.observeConnected(empty, tv(), now);
  const rec = s.devices[0];
  assert.equal(saved.savedStatus(rec, [tv()], undefined), "online");
  assert.equal(saved.savedStatus(rec, [tv({ serial: "192.168.9.9:5555" })], "no_answer"), "online",
    "online wherever it is, by hardware id");
  assert.equal(saved.savedStatus(rec, [], "connecting"), "connecting");
  assert.equal(saved.savedStatus(rec, [], "no_answer"), "no_answer");
  assert.equal(saved.savedStatus(rec, [], undefined), "not_connected");
  const impostor = tv({ name: "Other Box", properties: { serial_number: "ZZZ999" } });
  assert.equal(saved.savedStatus(rec, [impostor], "no_answer"), "different_device",
    "a different id answering at the saved address is a different device");
  assert.equal(saved.savedStatus(rec, [impostor], "connecting"), "different_device");
  assert.equal(saved.liveDeviceFor(rec, [impostor]), undefined, "and never stands in for the saved row");
  const noId = tv({ status: "unauthorized", properties: null });
  assert.equal(saved.savedStatus(rec, [noId], "no_answer"), "unverified", "a device with no id claims nothing");
  assert.equal(saved.liveDeviceFor(rec, [noId]), undefined);
  // The different device is saved under its own id and keeps its own name.
  const both = saved.observeConnected(s, impostor, now);
  assert.equal(both.devices.length, 2);
  assert.equal(both.devices.find((d) => d.hardwareId === "0323220012345").name, "Living Room Shield");
  assert.equal(both.devices.find((d) => d.hardwareId === "ZZZ999").name, "Other Box");
}

// --- MAC filed only under the device that answered ---------------------------
{
  const s = saved.observeConnected(empty, tv(), now);
  const ok = saved.recordMac(s, "0323220012345", { hardware_id: "0323220012345", mac: "00:04:4B:8C:21:5E" });
  assert.equal(ok.devices[0].mac, "00:04:4b:8c:21:5e");
  assert.equal(saved.recordMac(s, "0323220012345", { hardware_id: "OTHER", mac: "00:04:4b:8c:21:5e" }), s,
    "a read from another device is not filed");
  assert.equal(saved.recordMac(s, "0323220012345", { hardware_id: null, mac: "00:04:4b:8c:21:5e" }), s);
  assert.equal(saved.recordMac(s, "0323220012345", { hardware_id: "0323220012345", mac: null }), s);
  const refreshed = saved.observeConnected(ok, tv({ serial: "192.168.1.51:5555" }), now);
  assert.equal(refreshed.devices[0].mac, "00:04:4b:8c:21:5e", "a refresh keeps the MAC");
}

// --- restart persistence ------------------------------------------------------
{
  const storage = new MemoryStorage();
  let s = saved.observeConnected(empty, tv(), now);
  s = saved.recordMac(s, "0323220012345", { hardware_id: "0323220012345", mac: "00:04:4b:8c:21:5e" });
  s = saved.unsaveDevice(saved.observeConnected(s, tv({ serial: "192.168.1.77:5555", properties: { serial_number: "9AB4" } }), now), "9AB4");
  assert.equal(saved.writeSaved(s, storage), true);
  const reloaded = saved.loadSaved(storage);
  assert.deepEqual(reloaded, s, "a fresh read gives back the same list, MAC and unsaves");
  const stored = storage.getItem(saved.SAVED_DEVICES_KEY);
  assert.doesNotMatch(stored, /pin|key|token|password/i, "no secrets in the record");
}
{
  const storage = new MemoryStorage();
  storage.setItem(saved.SAVED_DEVICES_KEY, JSON.stringify({
    devices: [
      { hardwareId: "unknown", address: "192.168.1.2:5555", name: "placeholder" },
      { hardwareId: "A1", address: "not-an-address", name: "bad address" },
      { hardwareId: "B2", address: "192.168.1.3:5555", name: "old", lastConnected: "2026-01-01T00:00:00Z" },
      { hardwareId: "B2", address: "192.168.1.4:5555", name: "new", lastConnected: "2026-02-01T00:00:00Z", mac: "nope" },
      null,
    ],
    unsaved: ["B2", "C3", "", 7],
  }));
  const loaded = saved.loadSaved(storage);
  assert.deepEqual(loaded.devices.map((d) => [d.hardwareId, d.name]), [["B2", "new"]],
    "malformed and id-less records are dropped, and one id keeps its newest record");
  assert.equal(loaded.devices[0].mac, undefined, "a malformed MAC is dropped");
  assert.deepEqual(loaded.unsaved, ["C3"], "a saved id is not also unsaved");
  storage.setItem(saved.SAVED_DEVICES_KEY, "{not json");
  assert.deepEqual(saved.loadSaved(storage), empty);
  assert.deepEqual(saved.loadSaved(null), empty);
  assert.equal(saved.writeSaved(empty, { getItem: () => null, setItem: () => { throw new Error("quota"); } }), false,
    "a blocked store never throws");
}

// --- addresses ---------------------------------------------------------------
assert.equal(saved.normalizeAddress(" 192.168.1.42:5555 "), "192.168.1.42:5555");
assert.equal(saved.normalizeAddress("[FE80::1]:5555"), "[fe80::1]:5555");
for (const bad of ["192.168.1.42", "192.168.1.42:0", "192.168.1.42:70000", "host.local:5555", "adb-x._adb-tls-connect._tcp", ""]) {
  assert.equal(saved.normalizeAddress(bad), null, bad);
}
assert.equal(saved.hostOfAddress("[fe80::1]:5555"), "fe80::1");
assert.equal(saved.lastConnectedLabel("2026-11-23T11:58:00Z", now.getTime()), "2 min ago");
assert.equal(saved.lastConnectedLabel("2026-11-21T12:00:00Z", now.getTime()), "2 days ago");
assert.equal(saved.lastConnectedLabel("1970-01-01T00:00:00.000Z", now.getTime()), "", "an unknown time is left out");
assert.equal(saved.lastConnectedLabel("garbage", now.getTime()), "");

// --- a sleeping TV's leftover transport --------------------------------------
{
  const rec = saved.observeConnected(empty, tv(), now).devices[0];
  // platform-tools keeps an offline transport when a TV sleeps.
  const asleep = tv({ status: "offline", properties: null });
  assert.equal(saved.savedStatus(rec, [asleep], "no_answer"), "no_answer",
    "an offline transport at the saved address is a TV that didn't answer, not an unknown one");
  assert.equal(saved.savedStatus(rec, [asleep], undefined), "not_connected");
  assert.equal(saved.savedStatus(rec, [tv({ status: "unauthorized", properties: null })], "no_answer"), "unverified");
}

// --- a device that only answered a dial for another saved TV ------------------
{
  const dialled = new Map([["192.168.1.64:5555", "XMS3B0042A"]]);
  const other = tv({ serial: "192.168.1.64:5555", name: "Kitchen Chromecast", properties: { serial_number: "CCHD0099" } });
  assert.equal(saved.observeConnected(empty, other, now, dialled), empty,
    "a different device reached only by dialling a saved address is not auto-saved");
  const same = tv({ serial: "192.168.1.64:5555", properties: { serial_number: "XMS3B0042A" } });
  assert.equal(saved.observeConnected(empty, same, now, dialled).devices.length, 1, "the expected device is");
  assert.equal(saved.saveDevice(empty, other, now).devices.length, 1, "and the bookmark still saves the other one");
  assert.equal(saved.observeConnected(empty, tv(), now, dialled).devices.length, 1, "other addresses are unaffected");
}

// --- a MAC written by a closed screen survives this screen's write ------------
{
  const s1 = saved.observeConnected(empty, tv(), now);
  const disk = saved.recordMac(s1, "0323220012345", { hardware_id: "0323220012345", mac: "00:04:4b:8c:21:5e" });
  const merged = saved.mergeStoredMacs(s1, disk);
  assert.equal(merged.devices[0].mac, "00:04:4b:8c:21:5e");
  assert.equal(saved.mergeStoredMacs(disk, s1), disk, "a MAC already in memory is kept, and nothing changes");
  assert.equal(saved.mergeStoredMacs(s1, empty), s1);
  const unsaved = saved.unsaveDevice(s1, "0323220012345");
  assert.deepEqual(saved.mergeStoredMacs(unsaved, disk).devices, [], "merging never brings back an unsaved record");
}

// --- dialling: bounded wait, one call at a time, a late success counts --------
{
  const session = () => ({ dial: {}, pending: {}, landed: 0 });
  const deferred = () => { let resolve; const promise = new Promise((r) => { resolve = r; }); return { promise, resolve }; };
  const rec = { hardwareId: "XMS3B0042A", address: "192.168.1.64:5555" };

  // Answers after the wait: "Didn't answer" meanwhile, the call stays the only
  // one, and the late success clears the row and is counted.
  let s = session();
  let calls = 0;
  let call = deferred();
  const connect = () => { calls += 1; return call.promise; };
  await saved.dialSavedAddress(s, rec, connect, 20);
  assert.equal(s.dial.XMS3B0042A, "no_answer");
  assert.equal(s.pending.XMS3B0042A, true, "the call is still pending after the wait");
  await saved.dialSavedAddress(s, rec, connect, 20);
  assert.equal(calls, 1, "Connect can't start a second call while the first is pending");
  call.resolve({ ok: true });
  await new Promise((r) => setTimeout(r, 0));
  assert.equal(s.dial.XMS3B0042A, undefined, "a success never reads as Didn't answer");
  assert.equal(s.pending.XMS3B0042A, undefined);
  assert.equal(s.landed, 1, "and whichever screen is open re-reads the list");

  // Fails: Didn't answer. Throws: the same.
  s = session();
  await saved.dialSavedAddress(s, rec, async () => ({ ok: false }), 1000);
  assert.equal(s.dial.XMS3B0042A, "no_answer");
  assert.equal(s.landed, 0);
  s = session();
  await saved.dialSavedAddress(s, rec, async () => { throw new Error("adb"); }, 1000);
  assert.equal(s.dial.XMS3B0042A, "no_answer");
  // Succeeds in time.
  s = session();
  await saved.dialSavedAddress(s, rec, async () => ({ ok: true }), 1000);
  assert.deepEqual([s.dial, s.pending, s.landed], [{}, {}, 1]);
}

// --- Wake-on-LAN: only Connect wakes, and only with a usable MAC --------------
assert.equal(saved.wakeMac(" A4:5E:60:1D:42:0A "), "a4:5e:60:1d:42:0a");
assert.equal(saved.wakeMac("02:11:22:33:44:55"), "02:11:22:33:44:55", "a randomized (locally administered) MAC is unicast");
for (const bad of [undefined, null, "", "nope", "00:00:00:00:00:00", "ff:ff:ff:ff:ff:ff", "01:00:5e:00:00:fb", "a4:5e:60:1d:42"]) {
  assert.equal(saved.wakeMac(bad), null, String(bad));
}
{
  const rec = saved.observeConnected(empty, tv(), now).devices[0];
  assert.equal(saved.savedStatus(rec, [], "waking"), "waking");
  const impostor = tv({ properties: { serial_number: "ZZZ999" } });
  assert.equal(saved.savedStatus(rec, [impostor], "waking"), "different_device");
}
{
  const session = () => ({ dial: {}, pending: {}, landed: 0 });
  const deferred = () => { let resolve; const promise = new Promise((r) => { resolve = r; }); return { promise, resolve }; };
  const withMac = { hardwareId: "XMS3B0042A", address: "192.168.1.64:5555", mac: "a4:5e:60:1d:42:0a" };
  const noMac = { hardwareId: "XMS3B0042A", address: "192.168.1.64:5555" };
  const tick = () => new Promise((r) => setTimeout(r, 0));
  /// A recorder for wake and connect calls; `answers` are connect's replies
  /// in order (the last one repeats).
  const recorder = (answers) => {
    const log = [];
    let active = 0;
    let maxActive = 0;
    let n = 0;
    return {
      log,
      maxActive: () => maxActive,
      wake: async (mac, host) => { log.push(["wake", mac, host]); },
      connect: async (address) => {
        active += 1;
        maxActive = Math.max(maxActive, active);
        log.push(["connect", address]);
        await new Promise((r) => setTimeout(r, 5));
        active -= 1;
        return answers[Math.min(n++, answers.length - 1)];
      },
    };
  };
  const wakeFirst = (r, over = {}) => ({ wake: r.wake, wakeMs: 200, retryMs: 10, waitMs: 1000, ...over });

  // Connect with a MAC: "Waking…", the packet, then the dial.
  {
    const r = recorder([{ ok: true }]);
    const gate = deferred();
    const s = session();
    const done = saved.dialSavedAddress(s, withMac, r.connect, 8000,
      wakeFirst(r, { wake: async (mac, host) => { r.log.push(["wake", mac, host]); await gate.promise; } }));
    assert.equal(s.dial.XMS3B0042A, "waking", "the row says Waking… while the packet goes out");
    await tick();
    assert.deepEqual(r.log, [["wake", "a4:5e:60:1d:42:0a", "192.168.1.64"]], "nothing is dialled before the wake is sent");
    gate.resolve();
    await done;
    assert.deepEqual(r.log.map((e) => e[0]), ["wake", "connect"], "wake, then dial");
    assert.deepEqual([s.dial, s.pending, s.landed], [{}, {}, 1]);
  }

  // Connect without a MAC behaves as before: no wake, one dial.
  {
    let woke = 0;
    const r = recorder([{ ok: false }]);
    const s = session();
    await saved.dialSavedAddress(s, noMac, r.connect, 1000, wakeFirst(r, { wake: async () => { woke += 1; } }));
    assert.equal(woke, 0, "no MAC, no wake");
    assert.deepEqual(r.log, [["connect", "192.168.1.64:5555"]], "and a single dial, no retries");
    assert.equal(s.dial.XMS3B0042A, "no_answer");
    // An unusable MAC is the same, and isn't an error.
    const r2 = recorder([{ ok: true }]);
    await saved.dialSavedAddress(session(), { ...withMac, mac: "ff:ff:ff:ff:ff:ff" }, r2.connect, 1000,
      wakeFirst(r2, { wake: async () => { woke += 1; } }));
    assert.equal(woke, 0, "a broadcast MAC is never woken");
  }

  // The launch and restart redials pass no wake option: a MAC on the record
  // never sends a packet from them.
  {
    const r = recorder([{ ok: false }]);
    const s = session();
    await saved.dialSavedAddress(s, withMac, r.connect, 1000);
    assert.deepEqual(r.log, [["connect", "192.168.1.64:5555"]], "redial: one dial, never a wake");
  }

  // A woken TV boots slowly: dials repeat, one at a time, until one answers.
  {
    const r = recorder([{ ok: false }, { ok: false }, { ok: true }]);
    const s = session();
    await saved.dialSavedAddress(s, withMac, r.connect, 20, wakeFirst(r));
    assert.deepEqual(r.log.map((e) => e[0]), ["wake", "connect", "connect", "connect"]);
    assert.equal(r.maxActive(), 1, "never two dials at once");
    assert.deepEqual([s.dial, s.pending, s.landed], [{}, {}, 1], "the longer wait, not the normal one, applies");
  }

  // Connect again during the longer wait starts nothing: no second wake or dial.
  {
    const r = recorder([{ ok: false }, { ok: true }]);
    const s = session();
    const first = saved.dialSavedAddress(s, withMac, r.connect, 8000, wakeFirst(r, { retryMs: 30 }));
    await new Promise((res) => setTimeout(res, 15));
    await saved.dialSavedAddress(s, withMac, r.connect, 8000, wakeFirst(r));
    await saved.dialSavedAddress(s, withMac, r.connect, 8000);
    await first;
    assert.deepEqual(r.log.map((e) => e[0]), ["wake", "connect", "connect"], "only the first Connect's calls");
    assert.equal(r.maxActive(), 1);
  }

  // It gives up at the longer wait and says Didn't answer.
  {
    const r = recorder([{ ok: false }]);
    const s = session();
    const started = Date.now();
    await saved.dialSavedAddress(s, withMac, r.connect, 8000, wakeFirst(r, { retryMs: 20, waitMs: 120 }));
    await new Promise((res) => setTimeout(res, 60));
    const dials = r.log.filter((e) => e[0] === "connect").length;
    assert.ok(dials >= 2 && dials <= 6, `retried a bounded number of times (${dials})`);
    assert.ok(Date.now() - started < 1000);
    assert.equal(s.dial.XMS3B0042A, "no_answer");
    assert.equal(s.pending.XMS3B0042A, undefined, "and stops dialling");
    await new Promise((res) => setTimeout(res, 60));
    assert.equal(r.log.filter((e) => e[0] === "connect").length, dials, "no dial after giving up");
  }

  // A failed or hung wake never blocks the dial.
  {
    const r = recorder([{ ok: true }]);
    const s = session();
    await saved.dialSavedAddress(s, withMac, r.connect, 8000,
      wakeFirst(r, { wake: () => { throw new Error("socket"); } }));
    assert.deepEqual(r.log, [["connect", "192.168.1.64:5555"]]);
    assert.equal(s.landed, 1);
    const r2 = recorder([{ ok: true }]);
    const s2 = session();
    await saved.dialSavedAddress(s2, withMac, r2.connect, 8000,
      wakeFirst(r2, { wakeMs: 20, wake: () => new Promise(() => {}) }));
    assert.deepEqual(r2.log, [["connect", "192.168.1.64:5555"]], "a wake that never returns is waited on only briefly");
    assert.equal(s2.landed, 1);
  }
}

console.log("Saved device store passed.");

// --- the Devices screen ------------------------------------------------------
const KEY = saved.SAVED_DEVICES_KEY;
const savedSection = (page) => page.locator("section.device-section");
const rowIn = (scope, name) => scope.locator("li", { has: scope.page().getByText(name, { exact: true }) }).first();
const foundList = (page) => page.locator("ul.device-list").last();
const stored = (page) => page.evaluate((key) => JSON.parse(localStorage.getItem(key) ?? "null"), KEY);

/// Records every wake_device and connect_device call from page load on.
function recordWakes(context) {
  return context.addInitScript(() => {
    let wrapped = null;
    window.__wakes = [];
    window.__dials = [];
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      get: () => wrapped,
      set(bridge) {
        const original = bridge.invoke;
        bridge.invoke = (cmd, args = {}) => {
          if (cmd === "wake_device") window.__wakes.push(args);
          if (cmd === "connect_device") window.__dials.push(args.address);
          return original(cmd, args);
        };
        wrapped = bridge;
      },
    });
  });
}

async function exercise(browser, base) {
  const context = await browser.newContext({ viewport: { width: 1280, height: 1100 } });
  await recordWakes(context);
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(base, { waitUntil: "domcontentloaded" });

  // Saved rows render from storage at once, and the sleeping box is dialled.
  const box = rowIn(savedSection(page), "Bedroom Xiaomi Box");
  await box.getByText("Connecting…", { exact: true }).first().waitFor();
  await box.getByText("Didn't answer", { exact: true }).waitFor();
  assert.match(await box.innerText(), /192\.168\.1\.64:5555 · Google TV · MiTV-AFKR0 · last connected 2 days ago/);
  assert.deepEqual(await page.evaluate(() => window.__wakes), [], "the launch dial never sends a wake packet");
  assert.equal(await box.locator("a.device-row").count(), 0, "a saved device that didn't answer doesn't open");
  const shield = rowIn(savedSection(page), "NVIDIA SHIELD Android TV");
  await shield.getByText("Online", { exact: true }).waitFor();
  assert.equal(await shield.locator("a.device-row").count(), 1, "a connected saved device opens as usual");
  assert.equal(await shield.getByRole("button", { name: "Unsave NVIDIA SHIELD Android TV" }).count(), 1);
  // The unsaved box stays in Found, and auto-save leaves it alone.
  const living = rowIn(foundList(page), "Living Room Box");
  assert.equal(await living.getByRole("button", { name: "Save Living Room Box" }).count(), 1);
  assert.equal((await stored(page)).devices.some((d) => d.hardwareId === "9AB4C21D7E03"), false);
  // A USB device has nothing to dial, so it can't be saved.
  const usb = rowIn(foundList(page), "Workshop Shield");
  assert.equal(await usb.locator("[data-bookmark]").count(), 0);

  // Help is a keyboard-reachable control on the section, not text per row.
  await page.getByRole("button", { name: "About Saved" }).focus();
  await page.keyboard.press("Enter");
  await page.getByRole("region", { name: "Saved" }).getByText("never under this saved name", { exact: false }).waitFor();
  await page.getByRole("region", { name: "Saved" }).getByText("hasn't allowed this computer, so it can't say", { exact: false }).waitFor();
  await page.getByRole("region", { name: "Saved" }).getByText("Connect also tries to wake a sleeping TV over the network", { exact: false }).waitFor();
  await page.keyboard.press("Escape");

  // Connect wakes the box (its record has a MAC), then dials the saved
  // address until the woken box answers; it goes Online by id. The demo box
  // misses the first dial and answers the retry.
  const dialsBefore = (await page.evaluate(() => window.__dials)).filter((a) => a === "192.168.1.64:5555").length;
  await box.getByRole("button", { name: "Connect to Bedroom Xiaomi Box" }).click();
  await box.getByText("Waking…", { exact: true }).first().waitFor();
  assert.equal(await box.getByRole("button", { name: "Waking Bedroom Xiaomi Box" }).isDisabled(), true,
    "no second Connect while waking");
  assert.deepEqual(await page.evaluate(() => window.__wakes), [{ mac: "a4:5e:60:1d:42:0a", host: "192.168.1.64" }]);
  await box.getByText("Connecting…", { exact: true }).first().waitFor();
  assert.equal(await box.getByRole("button", { name: "Connecting to Bedroom Xiaomi Box" }).isDisabled(), true);
  await rowIn(savedSection(page), "Bedroom Xiaomi Box").locator("a.device-row").waitFor();
  const boxDials = (await page.evaluate(() => window.__dials)).filter((a) => a === "192.168.1.64:5555").length - dialsBefore;
  assert.equal(boxDials, 2, "the first dial missed, the retry landed");
  assert.equal((await page.evaluate(() => window.__wakes)).length, 1, "one wake per Connect");

  // Forget a saved device: it disconnects and its row returns to the offline state.
  await rowIn(savedSection(page), "Bedroom Xiaomi Box").getByRole("button", { name: "Forget" }).click();
  await page.evaluate(() => localStorage.removeItem("shieldopt.demo.savedBoxAwake"));
  await rowIn(savedSection(page), "Bedroom Xiaomi Box").getByText("Not connected", { exact: true }).waitFor();

  // Unsave a connected device: it moves to Found, focus follows its bookmark,
  // and it stays unsaved on the next launch even though it reconnects.
  await shield.getByRole("button", { name: "Unsave NVIDIA SHIELD Android TV" }).click();
  const moved = rowIn(foundList(page), "NVIDIA SHIELD Android TV");
  await moved.getByRole("button", { name: "Save NVIDIA SHIELD Android TV" }).waitFor();
  assert.equal(await page.evaluate(() => document.activeElement?.getAttribute("aria-label")),
    "Save NVIDIA SHIELD Android TV");
  // Save the Living Room Box instead.
  await living.getByRole("button", { name: "Save Living Room Box" }).click();
  await rowIn(savedSection(page), "Living Room Box").getByRole("button", { name: "Unsave Living Room Box" }).waitFor();
  // Unsave the offline box: its row goes away.
  await rowIn(savedSection(page), "Bedroom Xiaomi Box").getByRole("button", { name: "Unsave Bedroom Xiaomi Box" }).click();
  await savedSection(page).getByText("Bedroom Xiaomi Box", { exact: true }).waitFor({ state: "detached" });

  await page.reload({ waitUntil: "networkidle" });
  await rowIn(savedSection(page), "Living Room Box").getByText("Online", { exact: true }).waitFor();
  // Its MAC is read while it's connected and filed under its id.
  await page.waitForFunction((key) => JSON.parse(localStorage.getItem(key)).devices
    .find((d) => d.hardwareId === "9AB4C21D7E03")?.mac === "a4:5e:60:9a:b4:c2", KEY);
  await rowIn(foundList(page), "NVIDIA SHIELD Android TV").getByRole("button", { name: "Save NVIDIA SHIELD Android TV" }).waitFor();
  const after = await stored(page);
  assert.deepEqual(after.devices.map((d) => d.hardwareId), ["9AB4C21D7E03"], "only the saved box survives the restart");
  assert.deepEqual([...after.unsaved].sort(), ["0323220012345", "XMS3B0042A"]);

  // A different device answering at a saved address: the saved row says so,
  // and the live device keeps its own row and name.
  await page.evaluate((key) => {
    localStorage.setItem(key, JSON.stringify({ devices: [{ hardwareId: "XMS3B0042A", address: "192.168.1.64:5555",
      name: "Bedroom Xiaomi Box", deviceType: "google_tv", lastConnected: new Date().toISOString() }],
      unsaved: ["0323220012345", "9AB4C21D7E03"] }));
  }, KEY);
  await page.reload({ waitUntil: "networkidle" });
  await page.evaluate(() => {
    const bridge = window.__TAURI_INTERNALS__;
    const original = bridge.invoke.bind(bridge);
    bridge.invoke = async (cmd, args = {}) => {
      if (cmd === "list_devices") {
        return [{ id: 9, serial: "192.168.1.64:5555", name: "Kitchen Chromecast", model: "Chromecast HD",
          device_type: "google_tv", tv_evidence: "tv", status: "device", connection: "network",
          properties: { friendly_name: "Kitchen Chromecast", brand: "google", model: "Chromecast HD",
            device_codename: "boreal", manufacturer: "Google", android_release: "12", sdk_level: "31",
            build_id: "STTE", board_platform: "amlogic", characteristics: "tv", serial_number: "CCHD0099",
            leanback: true }, support: { tier: "unknown", note: null } },
        { id: 10, serial: "192.168.1.65:5555", name: "192.168.1.65:5555", model: "", device_type: "unknown",
          tv_evidence: "unknown", status: "unauthorized", connection: "network", properties: null }];
      }
      return original(cmd, args);
    };
  });
  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  const savedBox = rowIn(savedSection(page), "Bedroom Xiaomi Box");
  await savedBox.locator(".device-status", { hasText: "Different device" }).waitFor();
  assert.equal(await savedBox.locator("a.device-row").count(), 0, "the saved row never opens the other device");
  const other = page.locator('[data-serial="192.168.1.64:5555"]');
  assert.equal(await other.count(), 1, "the device at the saved address has its own row");
  assert.match(await other.innerText(), /Kitchen Chromecast/);
  assert.doesNotMatch(await other.innerText(), /Bedroom Xiaomi Box/, "it never inherits the saved name");
  assert.equal(await savedSection(page).locator('[data-serial="192.168.1.64:5555"]').count(), 0, "it isn't in Saved");
  assert.equal(await foundList(page).locator('[data-serial="192.168.1.64:5555"]').count(), 1, "it's under Other devices");
  assert.equal(await page.locator("#found-devices-heading").innerText(), "Other devices");
  await other.getByRole("button", { name: "Save Kitchen Chromecast" }).waitFor();
  const records = (await stored(page)).devices;
  assert.equal(records.some((d) => d.hardwareId === "CCHD0099"), false,
    "a device that only answered the dial for another saved TV is not auto-saved");
  assert.equal(records.find((d) => d.hardwareId === "XMS3B0042A").name, "Bedroom Xiaomi Box", "the saved row keeps its own");
  assert.equal(records.some((d) => d.address === "192.168.1.65:5555"), false, "the unauthorized device isn't saved");

  // Unsaving the last saved device moves focus to a remaining row, never the page.
  await savedBox.getByRole("button", { name: "Unsave Bedroom Xiaomi Box" }).click();
  await savedSection(page).waitFor({ state: "detached" });
  const focused = await page.evaluate(() => document.activeElement?.tagName);
  assert.notEqual(focused, "BODY", "focus has somewhere to go");
  assert.deepEqual(errors, []);
  await context.close();
}

// A launch dial that answers after the user has left Devices and come back:
// the TV must show up Online, not as "Didn't answer" (the screen that
// started the dial is gone, so the new one has to pick it up).
async function exerciseRemount(browser, base) {
  const context = await browser.newContext({ viewport: { width: 1280, height: 1100 } });
  await context.addInitScript(() => {
    let wrapped = null;
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      get: () => wrapped,
      set(bridge) {
        const original = bridge.invoke;
        window.__boxDials = 0;
        window.__wakes = 0;
        bridge.invoke = (cmd, args = {}) => {
          if (cmd === "wake_device") window.__wakes += 1;
          if (cmd === "connect_device" && args.address === "192.168.1.64:5555") window.__boxDials += 1;
          if (cmd === "connect_device" && args.address === "192.168.1.64:5555" && window.__boxDials === 1) {
            // Wakes and answers 2.5 s later.
            return new Promise((resolve) => setTimeout(() => {
              localStorage.setItem("shieldopt.demo.savedBoxAwake", "1");
              resolve(original(cmd, args));
            }, 2500));
          }
          return original(cmd, args);
        };
        wrapped = bridge;
      },
    });
  });
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  await page.goto(base, { waitUntil: "domcontentloaded" });
  const box = () => rowIn(savedSection(page), "Bedroom Xiaomi Box");
  await box().getByText("Connecting…", { exact: true }).first().waitFor();
  await page.getByRole("link", { name: "Snapshots", exact: true }).click();
  await page.waitForURL("**/snapshots");
  await page.getByRole("link", { name: "Devices", exact: true }).click();
  await box().locator("a.device-row").waitFor({ timeout: 20000 });
  assert.equal(await box().getByText("Didn't answer", { exact: true }).count(), 0);
  assert.equal(await page.evaluate(() => localStorage.getItem("shieldopt.demo.savedBoxAwake")), "1");
  assert.equal(await page.evaluate(() => window.__boxDials), 1, "coming back to Devices doesn't redial");

  // A restart drops every connection, so saved devices that aren't
  // connected are dialled again. (The demo's restart drops nothing, so the
  // box is disconnected first.)
  await page.evaluate(() => localStorage.removeItem("shieldopt.demo.savedBoxAwake"));
  await box().getByRole("button", { name: "Forget" }).click();
  await box().getByText("Not connected", { exact: true }).waitFor();
  page.once("dialog", (dialog) => dialog.accept());
  await page.locator(".connect-form").getByRole("button", { name: "Restart ADB", exact: true }).click();
  await page.waitForFunction(() => window.__boxDials === 2);
  await box().getByText("Didn't answer", { exact: true }).waitFor({ timeout: 20000 });

  // Switching the ADB engine redials too, the next time Devices opens.
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  await page.waitForURL("**/settings");
  const engines = page.locator("#adb").getByRole("radiogroup", { name: "ADB engine" });
  const other = (await engines.getByRole("radio", { name: "Built-in", exact: true }).isChecked())
    ? "Google platform-tools" : "Built-in";
  await engines.getByRole("radio", { name: other, exact: true }).check();
  await page.waitForFunction(() => document.querySelector("#adb [role=radiogroup]")?.getAttribute("aria-busy") === "false");
  await page.getByRole("link", { name: "Devices", exact: true }).click();
  await page.waitForFunction(() => window.__boxDials === 3);
  assert.equal(await page.evaluate(() => window.__wakes), 0,
    "the box has a MAC, yet neither the launch dial nor a restart or engine-switch redial woke it");
  await context.close();
}

process.env.VITE_DEMO = "1";
const { createServer } = await import("vite");
const { chromium } = await import("playwright");
let server;
let browser;
try {
  server = await createServer({
    root: v2Root,
    server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false },
  });
  await server.listen();
  browser = await chromium.launch();
  await exercise(browser, `http://127.0.0.1:${server.httpServer.address().port}`);
  await exerciseRemount(browser, `http://127.0.0.1:${server.httpServer.address().port}`);
  console.log("Saved devices screen passed.");
} finally {
  await browser?.close();
  await server?.close();
}
