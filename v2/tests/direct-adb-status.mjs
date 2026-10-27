import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
process.env.VITE_DEMO = '1';
const evidence = process.env.DIRECT_ADB_EVIDENCE;
if (evidence) await mkdir(evidence, { recursive: true });
let server, browser;
try {
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, hmr: false } });
  await server.listen();
  browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1360, height: 900 }, ...(evidence ? { recordVideo: { dir: `${evidence}/raw` } } : {}) });
  const page = await context.newPage();
  const scenarios = [
    { name: 'first-run-ready', backend: 'direct', available: true, message: '' },
    { name: 'setup-blocked', backend: 'direct', available: false, message: 'Rust ADB setup could not save its identity. Check app storage access and reopen the app. Existing identities were not replaced.' },
    { name: 'existing-identity-invalid', backend: 'direct', available: false, message: 'Rust ADB could not read its saved identity. Existing identities were not replaced. Check app storage access and reopen the app.' },
    { name: 'platform-tools-missing', backend: 'subprocess', available: false, message: 'No platform-tools found.' },
  ];
  for (const scenario of scenarios) {
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
    await page.evaluate(scenario => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__fixtureCalls = [];
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        window.__fixtureCalls.push(command);
        if (command === 'adb_status') return { backend: scenario.backend, available: scenario.available, path: null, last_probe: scenario.message };
        if (command === 'list_devices') {
          if (!scenario.available) throw new Error('Do not enumerate after a failed status probe');
          return [];
        }
        if (command === 'scan_network') throw new Error('Blocked backend must not attempt discovery');
        return original(command, args);
      };
      const label = document.createElement('div');
      label.textContent = 'SIMULATION — first-run fixture; no key, ADB server or TV';
      label.style.cssText = 'position:fixed;top:0;right:0;z-index:99999;background:#182238;color:white;padding:8px';
      document.body.append(label);
    }, scenario);
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await page.getByRole('button', { name: 'Refresh', exact: true }).waitFor();
    if (scenario.backend === 'direct') {
      assert.equal(await page.getByRole('button', { name: 'Download platform-tools', exact: true }).count(), 0);
      if (!scenario.available) {
        await page.getByRole('alert').filter({ hasText: scenario.message }).waitFor();
        assert.equal(await page.getByRole('button', { name: 'Scan LAN', exact: true }).isDisabled(), true);
        assert.equal(await page.getByRole('button', { name: 'Pair PIN', exact: true }).first().isDisabled(), true);
        await page.getByPlaceholder('IP[:port] — e.g. 192.168.42.71').fill('192.0.2.71');
        assert.equal(await page.getByRole('button', { name: 'Add by IP', exact: true }).first().isDisabled(), true);
        const calls = await page.evaluate(() => window.__fixtureCalls);
        assert.equal(calls.includes('list_devices'), false);
        assert.equal(calls.includes('scan_network'), false);
        assert.equal((await page.getByRole('alert').innerText()).includes('SHIELD_OPTIMIZER'), false);
      } else {
        await page.waitForFunction(() => window.__fixtureCalls.includes('list_devices'));
        assert.equal(await page.getByRole('button', { name: 'Scan LAN', exact: true }).isEnabled(), true);
        assert.equal(await page.getByRole('button', { name: 'Pair PIN', exact: true }).first().isEnabled(), true);
        assert.equal(await page.getByRole('alert').count(), 0);
      }
    } else {
      await page.getByRole('button', { name: 'Download platform-tools', exact: true }).waitFor();
    }
    if (evidence) {
      await page.screenshot({ path: `${evidence}/${scenario.name}-simulated.png` });
      await page.waitForTimeout(1400);
    }
    console.log(`PASS ${scenario.name}`);
  }
  if (evidence) {
    const video = page.video();
    await context.close();
    await video.saveAs(`${evidence}/first-run-simulated.webm`);
  } else await context.close();
} finally {
  await browser?.close();
  await server?.close();
}
