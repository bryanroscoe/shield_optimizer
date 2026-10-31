import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createServer } from 'vite';
import { chromium } from 'playwright';

process.env.VITE_DEMO = '1';
const evidence = process.env.RESTART_EVIDENCE;
if (evidence) await mkdir(evidence, { recursive: true });
const target = '192.0.2.1:5555';
let server, browser;
try {
  server = await createServer({ root: resolve(import.meta.dirname, '..'), server: { host: '127.0.0.1', port: 0, hmr: false } });
  await server.listen();
  browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1400, height: 1000 }, ...(evidence ? { recordVideo: { dir: `${evidence}/raw` } } : {}) });
  const page = await context.newPage();
  for (const scenario of ['auth', 'success', 'failure', 'partial', 'restart-fails', 'cancel', 'edited', 'unattempted']) {
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
    await page.evaluate(({ scenario }) => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.recoveryCalls = [];
      let attempts = 0;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        window.recoveryCalls.push({ command, args });
        if (command === 'list_devices') return [];
        if (command === 'connect_device') {
          attempts++;
          return attempts === 1 || scenario === 'failure'
            ? { ok: false, message: 'failed to connect: No route to host' }
            : { ok: true, message: scenario === 'success' ? 'connected to 192.0.2.1:5555' : 'failed to authenticate — approve this computer on the TV, then refresh' };
        }
        if (command === 'restart_adb') return { ok: !['restart-fails', 'partial'].includes(scenario), server_ready: scenario !== 'restart-fails', message: scenario === 'restart-fails' ? 'Simulated start-server failure' : 'ADB server restarted.' };
        return original(command, args);
      };
      const label = document.createElement('div');
      label.textContent = 'SIMULATION — mocked commands; no ADB server or TV';
      label.style.cssText = 'position:fixed;bottom:48px;right:12px;z-index:99999;background:#182238;color:white;padding:8px';
      document.body.append(label);
    }, { scenario });
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await page.getByText('No devices connected.', { exact: true }).waitFor();
    const box = page.getByPlaceholder('IP[:port] — e.g. 192.168.42.71');
    await box.fill(target);
    if (scenario !== 'unattempted') {
      await page.locator('.connect-form').getByRole('button', { name: 'Add by IP', exact: true }).click();
      await page.locator('.connect-details').waitFor();
      assert.equal(await box.inputValue(), target);
      if (evidence && scenario === 'auth') await page.screenshot({ path: `${evidence}/failed-add-simulated.png` });
    }
    if (scenario === 'edited') await box.fill('192.0.2.2:5555');
    let prompt;
    page.once('dialog', async dialog => {
      prompt = dialog.message();
      await (scenario === 'cancel' ? dialog.dismiss() : dialog.accept());
    });
    await page.getByRole('button', { name: 'Restart ADB', exact: true }).click();
    if (scenario !== 'cancel') await page.getByText(scenario === 'restart-fails' ? 'Simulated start-server failure' : 'ADB server restarted.', { exact: true }).waitFor();
    const retry = ['auth', 'success', 'failure', 'partial'].includes(scenario);
    if (retry) await page.waitForFunction(() => window.recoveryCalls.filter(c => c.command === 'connect_device').length === 2, null, { timeout: 3000 });
    const calls = await page.evaluate(() => window.recoveryCalls);
    assert.equal(calls.filter(c => c.command === 'connect_device').length, scenario === 'unattempted' ? 0 : retry ? 2 : 1, scenario);
    assert.equal(calls.filter(c => c.command === 'restart_adb').length, scenario === 'cancel' ? 0 : 1, scenario);
    if (evidence) await writeFile(`${evidence}/${scenario}-calls.json`, JSON.stringify({ prompt, calls }, null, 2));
    if (retry) {
      assert.ok(prompt.includes(`retry Add by IP for ${target} once`));
      assert.equal(calls.filter(c => c.command === 'connect_device')[1].args.address, target);
    }
    if (scenario === 'success') await page.getByText('connected to 192.0.2.1:5555', { exact: true }).waitFor();
    if (scenario === 'auth') {
      await page.getByText('failed to authenticate — approve this computer on the TV, then refresh', { exact: true }).waitFor();
      if (evidence) await page.screenshot({ path: `${evidence}/restart-auth-simulated.png` });
    }
    if (scenario === 'failure') assert.equal(await box.inputValue(), target);
    console.log(`PASS ${scenario}`);
  }
  if (evidence) {
    const video = page.video();
    await context.close();
    await video.saveAs(`${evidence}/onboarding-recovery-simulated.webm`);
  } else await context.close();
} finally {
  await browser?.close();
  await server?.close();
}
