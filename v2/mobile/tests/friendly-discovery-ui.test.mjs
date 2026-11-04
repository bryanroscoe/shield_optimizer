import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { before, after, test } from 'node:test';
import { chromium, webkit } from 'playwright';
import { startViteServer } from './helpers/vite-harness.mjs';
let server, origin;
before(async () => { ({ server, origin } = await startViteServer()); });
after(async () => { await server?.close(); });
const evidence = process.env.FRIENDLY_EVIDENCE_DIR;
for (const [engine, launcher] of [['chromium', chromium], ['webkit', webkit]]) {
  test(`${engine}: advertised names display without making Cast an ADB endpoint`, async () => {
    const browser = await launcher.launch({ headless: true });
    const record = evidence && engine === 'chromium';
    if (record) await mkdir(evidence, { recursive: true });
    const context = await browser.newContext({ viewport: { width: 412, height: 915 }, ...(record ? { recordVideo: { dir: `${evidence}/raw`, size: { width: 412, height: 915 } } } : {}) });
    const page = await context.newPage();
    page.setDefaultTimeout(10000);
    try {
      await page.addInitScript(() => {
        localStorage.clear(); window.calls = []; window.scanCount = 0;
        window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
          window.calls.push({ command, args });
          switch (command) {
            case 'get_entitlement': return 'free';
            case 'billing_status': case 'billing_refresh': return { entitlement: 'free', sources: [], distribution: 'direct', license_key_entry: false, available: false, play: { state: 'unavailable' }, offer: null };
            case 'telemetry_settings': return { available: false, notice_seen: true };
            case 'wireless_pair': return { ok: true, message: 'Synthetic pairing succeeded.' };
            case 'wireless_status': return { connected: false, serial: null, host: null };
            case 'list_devices': return [];
            case 'wireless_discover': {
              ++window.scanCount;
              return { devices: window.nextDiscovery ?? (window.scanCount > 1 ? [] : [
                { host: '198.51.100.20', name: 'cast-study', friendly_name: 'Study Shield', network: 'fixture-net', port: 8009, service: '_googlecast._tcp.' },
                { host: '198.51.100.21', name: 'Guest TV', friendly_name: 'Guest TV', network: 'fixture-net', port: 6466, service: '_androidtvremote2._tcp.' },
                { host: '198.51.100.22', name: 'Cinema TV', friendly_name: 'Cinema TV', network: 'fixture-net', port: 6466, service: '_androidtvremote2._tcp.' },
                { host: '198.51.100.23', name: 'cast-lounge', friendly_name: 'Lounge TV', network: 'fixture-net', port: 8009, service: '_googlecast._tcp.' },
                { host: '198.51.100.24', name: 'adb-SYNTHETIC-a1b2c3', network: 'fixture-net', port: 37123, service: '_adb-tls-connect._tcp.' },
              ]), message: 'Synthetic discovery fixture.' };
            }
            default: return null;
          }
        } };
      });
      await page.goto(origin);
      await page.getByText('Study Shield', { exact: true }).waitFor();
      for (const name of ['Guest TV', 'Cinema TV', 'Lounge TV']) assert.equal(await page.getByText(name, { exact: true }).count(), 1);
      assert.equal(await page.getByRole('button', { name: /^Enter ADB details for / }).count(), 4);
      assert.equal(await page.getByRole('button', { name: /Study Shield.*arrow_forward/ }).count(), 0);
      assert.equal(await page.evaluate(() => calls.filter(c => c.command === 'wireless_connect' || c.command === 'wireless_pair').length), 0);
      if (record) {
        await page.evaluate(() => { const label = document.createElement('div'); label.textContent = 'SIMULATED · synthetic names and addresses'; label.style.cssText = 'position:fixed;top:0;left:0;right:0;z-index:99999;background:#532b00;color:white;text-align:center;font:12px sans-serif;padding:3px'; document.body.append(label); });
        await page.waitForTimeout(350);
        await page.screenshot({ path: `${evidence}/friendly-discovery-dark-simulated.png`, fullPage: true });
        await page.setViewportSize({ width: 320, height: 812 });
        await page.evaluate(() => { document.documentElement.style.fontSize = '20px'; });
        await page.screenshot({ path: `${evidence}/friendly-discovery-narrow-large-text-simulated.png`, fullPage: true });
        assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
        await page.setViewportSize({ width: 412, height: 915 });
        await page.evaluate(() => { document.documentElement.style.fontSize = ''; });
      }
      if (record) await page.waitForTimeout(1000);
      await page.getByRole('button', { name: /^Enter ADB details for / }).first().click();
      const fields = page.locator('.manual input');
      assert.equal(await fields.nth(0).inputValue(), '198.51.100.20');
      assert.equal(await fields.nth(1).inputValue(), '');
      assert.equal(await fields.nth(2).inputValue(), '');
      assert.equal(await page.getByRole('button', { name: 'Connect (no code)', exact: true }).isDisabled(), true);
      if (record) {
        await page.screenshot({ path: `${evidence}/adb-details-required-simulated.png`, fullPage: true });
        await page.waitForTimeout(1000);
      }
      await fields.nth(1).fill('37099');
      await page.getByRole('button', { name: 'Pair with code', exact: true }).click();
      await page.getByRole('textbox', { name: '6-digit pairing code' }).fill('123456');
      await page.getByRole('button', { name: 'Pair with code', exact: true }).click();
      await page.getByText('Pairing succeeded. Scan again to choose an advertised connect service.', { exact: true }).waitFor();
      assert.equal(await page.evaluate(() => calls.filter(c => c.command === 'wireless_connect').length), 0, 'pairing without an explicit connect port cannot dial');
      await page.evaluate(() => { window.nextDiscovery = [{ host: '',  name: 'Utility TV', friendly_name: 'Utility TV', network: null, port: 0, service: '_androidtvremote2._tcp.' }]; });
      await page.getByRole('button', { name: /Scan again/ }).click();
      await page.getByText('Utility TV', { exact: true }).waitFor();
      await page.getByRole('button', { name: 'Enter ADB details for Utility TV', exact: true }).click();
      assert.equal(await fields.nth(0).inputValue(), '');
      assert.equal(await fields.nth(1).inputValue(), '');
      assert.equal(await fields.nth(2).inputValue(), '');
      assert.equal(await page.getByRole('button', { name: 'Connect (no code)', exact: true }).isDisabled(), true);
      if (record) await page.screenshot({ path: `${evidence}/older-android-unlocated-name-simulated.png`, fullPage: true });
      await page.evaluate(() => { window.nextDiscovery = []; });
      await page.getByRole('button', { name: /Scan again/ }).click();
      await page.getByText('No devices found', { exact: true }).waitFor();
      assert.equal(await page.getByText('Study Shield', { exact: true }).count(), 0);
      assert.equal(await page.evaluate(() => calls.filter(c => c.command === 'wireless_connect').length), 0);
      assert.equal(await page.evaluate(() => calls.filter(c => c.command === 'wireless_pair').length), 1);
    } finally {
      const video = record ? page.video() : null;
      await context.close();
      if (video) await video.saveAs(`${evidence}/friendly-discovery-simulated.webm`);
      await browser.close();
    }
  });
}
