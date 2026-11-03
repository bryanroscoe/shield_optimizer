import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createServer } from 'vite';
import { chromium } from 'playwright';

process.env.VITE_DEMO = '1';
const evidence = process.env.ADB_STATUS_EVIDENCE;
if (evidence) await mkdir(evidence, { recursive: true });
let server, browser;
try {
  server = await createServer({ root: resolve(import.meta.dirname, '..'), server: { host: '127.0.0.1', port: 0, hmr: false } });
  await server.listen();
  browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1200, height: 900 }, ...(evidence ? { recordVideo: { dir: `${evidence}/status-video-raw` } } : {}) });
  const scenarios = [
    { name: 'missing', failure: 'missing_binary', message: 'No platform-tools found.' },
    { name: 'stale-path', failure: 'missing_binary', path: '/fixture/missing/adb', message: 'ADB binary missing at fixture path.' },
    { name: 'timeout', failure: 'probe_failed', message: 'adb command timed out after 30s' },
    { name: 'launch-error', failure: 'probe_failed', path: '/fixture/adb', message: 'I/O error: fixture launch denied' },
    { name: 'nonzero', failure: 'probe_failed', message: 'fixture server startup failed' },
    { name: 'healthy', failure: null, message: 'List of devices attached' },
  ];
  for (const scenario of scenarios) {
    const page = await context.newPage();
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
    await page.evaluate(scenario => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.fixtureCalls = [];
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        window.fixtureCalls.push(command);
        if (command === 'adb_status') return { available: !scenario.failure, failure: scenario.failure, path: scenario.path ?? null, last_probe: scenario.message };
        if (command === 'list_devices') return [];
        if (command === 'restart_adb') return { ok: true, server_ready: true, message: 'Simulated server restart.' };
        return original(command, args);
      };
    }, scenario);
    if (evidence) await page.evaluate(() => {
      const label = document.createElement('div');
      label.textContent = 'SIMULATION — mocked commands; no ADB server or TV';
      label.style.cssText = 'position:fixed;bottom:0;right:0;z-index:99999;background:#182238;color:white;padding:8px';
      document.body.append(label);
    });
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    const missing = scenario.failure === 'missing_binary';
    if (missing) await page.getByRole('button', { name: 'Download platform-tools', exact: true }).waitFor();
    else if (scenario.failure) await page.locator('.error').filter({ hasText: scenario.message }).waitFor();
    else await page.getByText('No devices connected.', { exact: true }).waitFor();
    assert.equal(await page.getByRole('button', { name: 'Download platform-tools', exact: true }).count(), missing ? 1 : 0);
    assert.equal(await page.getByRole('button', { name: 'Restart ADB', exact: true }).isEnabled(), !missing);
    assert.equal(await page.evaluate(() => window.fixtureCalls.includes('list_devices')), !scenario.failure);
    if (evidence && scenario.name === 'timeout') {
      await page.screenshot({ path: `${evidence}/devices-timeout-simulated.png` });
      await page.waitForTimeout(1200);
    }
    await page.getByRole('link', { name: 'Settings', exact: true }).click();
    await page.waitForURL('**/settings');
    const adb = page.locator('#adb');
    await adb.getByText(scenario.failure ? (missing ? /Not found/ : scenario.message) : /Using/).first().waitFor();
    assert.equal(await adb.getByRole('button', { name: 'Download platform-tools', exact: true }).count(), missing ? 1 : 0);
    const restart = adb.getByRole('button', { name: 'Restart ADB', exact: true });
    assert.equal(await restart.isEnabled(), !missing);
    if (!missing) {
      page.once('dialog', dialog => dialog.accept());
      await restart.click();
      await page.waitForFunction(() => window.fixtureCalls.includes('restart_adb'));
    }
    if (evidence && scenario.name === 'timeout') {
      await adb.scrollIntoViewIfNeeded();
      await page.screenshot({ path: `${evidence}/settings-timeout-simulated.png` });
      await page.waitForTimeout(1200);
    }
    if (scenario.failure) {
      await page.evaluate(() => { window.fixtureCalls = []; });
      await page.getByRole('link', { name: 'Devices', exact: true }).click();
      await page.waitForFunction(() => window.fixtureCalls.includes('adb_status'));
      await page.getByRole('button', { name: 'Refresh', exact: true }).waitFor();
      assert.equal(await page.evaluate(() => window.fixtureCalls.includes('scan_network')), false);
    }
    const video = page.video();
    await page.close();
    if (evidence && scenario.name === 'timeout') await video.saveAs(`${evidence}/timeout-recovery-simulated.webm`);
    console.log(`PASS platform-tools ${scenario.name} on Devices and Settings`);
  }
  await context.close();
} finally {
  await browser?.close();
  await server?.close();
}
