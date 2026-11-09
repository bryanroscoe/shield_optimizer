// Desktop parity with the phone: Force stop in the app detail panel, the
// Shield-only Find remote control, and the Temperature tile's reported context.
import assert from 'node:assert/strict';
import { mkdir, realpath } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const evidence = process.env.DESKTOP_PARITY_EVIDENCE;
const previous = { demo: process.env.VITE_DEMO, host: process.env.TAURI_DEV_HOST };
process.env.VITE_DEMO = '1';
delete process.env.TAURI_DEV_HOST;
let server, browser;

async function record(page) {
  await page.evaluate(() => {
    const bridge = window.__TAURI_INTERNALS__;
    const original = bridge.invoke.bind(bridge);
    window.parityCalls = [];
    window.parityHealth = null;
    bridge.invoke = async (command, args = {}) => {
      window.parityCalls.push({ command, args });
      if (command === 'health_report' && window.parityHealth) {
        const report = await original(command, args);
        return { ...report, ...window.parityHealth };
      }
      return original(command, args);
    };
  });
}

const calls = (page, command) => page.evaluate(command => window.parityCalls.filter(c => c.command === command), command);

try {
  if (evidence) await mkdir(evidence, { recursive: true });
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, hmr: false, fs: { allow: [root, await realpath(resolve(root, 'node_modules'))] } } });
  await server.listen();
  const url = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();

  for (const theme of ['dark', 'light']) {
    for (const width of [1280, 720]) {
      const video = evidence && theme === 'dark' && width === 1280 ? { recordVideo: { dir: `${evidence}/video`, size: { width, height: 900 } } } : {};
      const context = await browser.newContext({ viewport: { width, height: 900 }, ...video });
      await context.addInitScript(theme => localStorage.setItem('shieldopt.theme', theme), theme);
      const page = await context.newPage();
      await page.goto(url, { waitUntil: 'networkidle' });
      await page.getByText('NVIDIA SHIELD', { exact: false }).first().click();
      await record(page);
      const shot = name => evidence ? page.screenshot({ path: `${evidence}/${name}-${theme}-${width}.png` }) : null;
      const pause = ms => video.recordVideo ? page.waitForTimeout(ms) : null;

      // Temperature: the reading, its sensor threshold and Android's status,
      // all as the TV reported them.
      await page.getByRole('tab', { name: 'Health', exact: true }).click();
      const health = page.locator('#tabpanel-health');
      const tile = health.locator('.stat-card').filter({ hasText: 'Temperature' });
      await tile.locator('.stat-foot').getByText('No throttling reported', { exact: true }).waitFor();
      assert.match(await tile.locator('.stat-big').innerText(), /^78\s*°C$/);
      assert.equal(await tile.locator('.stat-big.warn, .stat-big.danger').count(), 0, 'status 0 below the sensor threshold is not coloured');
      await pause(1200);
      await shot('health-tile');
      await tile.getByRole('button', { name: 'About Temperature' }).click();
      const help = page.getByRole('region', { name: 'Temperature' });
      await help.getByText('11°C below reported sensor threshold', { exact: false }).waitFor();
      await help.getByText('CPU reading 1 · CPU/GPU hardware report', { exact: true }).waitFor();
      await pause(2000);
      await shot('health-temperature');
      await page.keyboard.press('Escape');

      if (theme === 'dark' && width === 1280) {
        // Android's own status colours the figure; a bare number with no
        // status or threshold claims nothing.
        await page.evaluate(() => { window.parityHealth = { thermal_status: 3 }; });
        await health.getByRole('button', { name: 'Refresh', exact: true }).click();
        await tile.locator('.stat-foot').getByText('Severe throttling', { exact: true }).waitFor();
        assert.equal(await tile.locator('.stat-big.warn').count(), 1);
        await page.evaluate(() => { window.parityHealth = { thermal_status: 5 }; });
        await health.getByRole('button', { name: 'Refresh', exact: true }).click();
        await tile.locator('.stat-foot').getByText('Emergency thermal status', { exact: true }).waitFor();
        assert.equal(await tile.locator('.stat-big.danger').count(), 1);
        // A sensor reporting throttling is not hidden behind a calm device status.
        await page.evaluate(() => {
          window.parityHealth = { thermal_status: 0, temperature_reading: { celsius: 78, source: 'thermalservice_current', sensor_name: 'CPU', sensor_type: 'CPU', status: 4, throttling_c: null, shutdown_c: null } };
        });
        await health.getByRole('button', { name: 'Refresh', exact: true }).click();
        await tile.locator('.stat-foot').getByText('Sensor: Critical thermal status', { exact: true }).waitFor();
        assert.equal(await tile.locator('.stat-big.danger').count(), 1);
        await page.evaluate(() => { window.parityHealth = { temperature_c: 95, temperature_reading: null, thermal_status: null }; });
        await health.getByRole('button', { name: 'Refresh', exact: true }).click();
        await tile.locator('.stat-foot').getByText('Thermal status not reported', { exact: true }).waitFor();
        assert.equal(await tile.locator('.stat-big.warn, .stat-big.danger').count(), 0, 'an unqualified number is not called hot');
        await page.evaluate(() => { window.parityHealth = { temperature_c: null, temperature_reading: null, thermal_status: null }; });
        await health.getByRole('button', { name: 'Refresh', exact: true }).click();
        await tile.locator('.stat-foot').getByText('not reported', { exact: true }).waitFor();
        await page.evaluate(() => { window.parityHealth = null; });
      }

      // Find remote: Shield only, rings through the locator activity.
      await page.getByRole('tab', { name: 'Remote', exact: true }).click();
      const remote = page.locator('#tabpanel-remote');
      await remote.getByRole('button', { name: 'Find remote' }).click();
      await remote.getByText('Ringing your Shield remote — listen for the beep.', { exact: true }).waitFor();
      const rang = await calls(page, 'find_remote');
      assert.equal(rang.length, 1);
      assert.equal(rang[0].args.serial, decodeURIComponent(new URL(page.url()).pathname.split('/').pop()));
      await pause(1500);
      await shot('remote-find');

      // Force stop: in the app detail panel, and a stop re-measures.
      await page.getByRole('tab', { name: 'App List', exact: true }).click();
      const caret = page.getByRole('button', { name: /^Show why .* has this verdict$/ }).first();
      await caret.click();
      const stop = page.getByRole('button', { name: 'Force stop', exact: true }).first();
      await stop.waitFor();
      await page.waitForFunction(() => !document.body.innerText.includes('Reading…'));
      const measuredBefore = (await calls(page, 'app_memory_map')).length;
      await stop.click();
      await page.getByText(/^Stopped .+\.$/).first().waitFor();
      const stopped = await calls(page, 'force_stop');
      assert.equal(stopped.length, 1);
      assert.ok(stopped[0].args.package, 'force stop names the package');
      await page.waitForFunction(count => window.parityCalls.filter(c => c.command === 'app_memory_map').length > count, measuredBefore);
      await pause(1500);
      await shot('app-force-stop');
      await context.close();
    }
  }

  // A device that has not reported itself as a Shield gets no Find remote.
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  const page = await context.newPage();
  await page.goto(url, { waitUntil: 'networkidle' });
  await page.getByText('Living Room Box', { exact: false }).first().click();
  // The device type arrives with the profile; wait for it before asserting absence.
  await page.getByRole('heading', { name: 'Living Room Box' }).waitFor();
  await page.getByText('Unknown', { exact: true }).first().waitFor();
  await page.getByRole('tab', { name: 'Remote', exact: true }).click();
  await page.locator('#tabpanel-remote').getByRole('button', { name: 'Back', exact: true }).waitFor();
  assert.equal(await page.getByRole('button', { name: 'Find remote' }).count(), 0, 'Find remote is Shield-only');
  await context.close();
  console.log('desktop parity: ok');
} finally {
  await browser?.close();
  await server?.close();
  if (previous.demo === undefined) delete process.env.VITE_DEMO; else process.env.VITE_DEMO = previous.demo;
  if (previous.host !== undefined) process.env.TAURI_DEV_HOST = previous.host;
}
