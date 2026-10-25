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
  for (const backend of ['direct', 'subprocess']) {
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
    await page.evaluate(backend => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === 'adb_status') return { backend, available: false, path: null, last_probe: backend === 'direct' ? 'SIMULATED: direct ADB requires an existing PKCS#8 identity; no key was created.' : 'SIMULATED: no platform-tools found.' };
        if (command === 'list_devices') throw new Error('Do not enumerate after a failed status probe');
        return original(command, args);
      };
      const label = document.createElement('div');
      label.textContent = 'SIMULATION — mocked status; no ADB server, key or TV';
      label.style.cssText = 'position:fixed;top:0;right:0;z-index:99999;background:#182238;color:white;padding:8px';
      document.body.append(label);
    }, backend);
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    if (backend === 'direct') {
      await page.locator('.error').filter({ hasText: 'SIMULATED: direct ADB requires an existing PKCS#8 identity; no key was created.' }).waitFor();
      assert.equal(await page.getByRole('button', { name: 'Download platform-tools', exact: true }).count(), 0);
    } else {
      await page.getByRole('button', { name: 'Download platform-tools', exact: true }).waitFor();
    }
    if (evidence) await page.screenshot({ path: `${evidence}/${backend}-error-simulated.png` });
    console.log(`PASS ${backend} status failure uses the correct recovery UI`);
  }
  if (evidence) {
    const video = page.video();
    await context.close();
    await video.saveAs(`${evidence}/backend-status-simulated.webm`);
  } else await context.close();
} finally {
  await browser?.close();
  await server?.close();
}
