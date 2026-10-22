import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const V2 = join(dirname(fileURLToPath(import.meta.url)), '..');
const pkg = 'com.google.android.tvrecommendations';
const evidence = process.env.TVRECOMMENDATIONS_EVIDENCE === '1';
const dir = join(V2, 'evidence/tvrecommendations-safety');
process.env.VITE_DEMO = '1';
delete process.env.TAURI_DEV_HOST;
let server, browser;
try {
  server = await createServer({ root: V2, server: { host: '127.0.0.1', port: 0, hmr: false } });
  await server.listen();
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();
  if (evidence) await mkdir(dir, { recursive: true });
  const context = await browser.newContext({ viewport: { width: 1400, height: 1000 }, ...(evidence ? { recordVideo: { dir: join(dir, 'raw'), size: { width: 1400, height: 1000 } } } : {}) });
  const page = await context.newPage();
  await page.goto(base, { waitUntil: 'networkidle' });
  await page.evaluate(() => {
    window.safetyCalls = [];
    const original = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      window.safetyCalls.push(command);
      const result = await original(command, args);
      if (command === 'prepare_optimize' && args.mode === 'restore') {
        const row = result.items.find(r => r.entry.package === 'com.google.android.tvrecommendations');
        row.action = { kind: 'enable' };
      }
      return result;
    };
    const label = document.createElement('div');
    label.textContent = 'SIMULATED safety catalog — no real device or removal';
    label.style.cssText = 'position:fixed;top:0;right:0;z-index:99999;padding:8px;background:#1a2540;color:white;font:14px sans-serif';
    document.body.appendChild(label);
  });
  await page.getByText('NVIDIA SHIELD', { exact: false }).first().click();
  await page.getByRole('tab', { name: 'App List', exact: true }).click();
  await page.locator('.app-search').fill(pkg);
  const appRow = page.locator('#tabpanel-apps tr').filter({ has: page.locator('.pkg-id', { hasText: pkg }) });
  await appRow.locator('[data-verdict="caution"]').waitFor();
  await appRow.getByRole('button', { name: 'Show why TV Recommendations has this verdict' }).click();
  const detail = page.locator('#tabpanel-apps .safety-detail-row');
  await detail.waitFor();
  const text = await detail.innerText();
  for (const phrase of ['phone app', 'reboot', 'custom launcher', 'darcy', 'foster_e', 'Android 11', 'unverified']) assert.ok(text.includes(phrase), `missing consequence/evidence limit: ${phrase}`);
  assert.ok(!text.includes('No effect'));
  if (evidence) {
    await appRow.scrollIntoViewIfNeeded();
    await page.screenshot({ path: join(dir, 'caution-detail-simulated.png'), animations: 'disabled' });
    await page.waitForTimeout(500);
  }
  await page.getByRole('tab', { name: 'Optimize', exact: true }).click();
  const panel = page.locator('#tabpanel-optimize');
  await panel.getByRole('button', { name: 'Optimize', exact: true }).click();
  const row = panel.locator('tr').filter({ has: page.locator('.pkg-id', { hasText: pkg }) });
  await row.locator('[data-verdict="caution"]').waitFor();
  const active = row.locator('.radio-pill.active .radio-label');
  assert.equal((await active.innerText()).trim(), 'Keep', 'default optimization must leave the phone-remote dependency enabled');
  await panel.getByRole('button', { name: 'Keep all', exact: true }).click();
  await panel.getByRole('button', { name: 'Select all safe', exact: true }).click();
  assert.equal((await active.innerText()).trim(), 'Keep', 'Select all safe must not select Caution');
  await row.getByRole('button', { name: 'Show why TV Recommendations has this verdict' }).click();
  assert.match(await panel.locator('.safety-detail-row').innerText(), /phone app.*reboot/s);
  if (evidence) {
    await row.scrollIntoViewIfNeeded();
    await page.screenshot({ path: join(dir, 'optimize-keep-simulated.png'), animations: 'disabled' });
    await page.waitForTimeout(500);
  }
  await panel.getByRole('button', { name: 'Restore', exact: true }).click();
  await row.locator('[data-verdict="caution"]').waitFor();
  assert.equal((await active.innerText()).trim(), 'Enable', 'existing restore default is preserved for a disabled package');
  const calls = await page.evaluate(() => safetyCalls);
  assert.ok(!calls.some(c => /^(disable_|uninstall_|run_optimize|execute_optimize)/.test(c)), 'review tests must not issue removal commands');
  const video = page.video();
  await context.close();
  if (evidence) await video.saveAs(join(dir, 'caution-review-simulated.webm'));
  console.log('TVRecommendations: Caution, full consequence/model limits, default Keep, Select all safe exclusion and restore Enable passed. No removals executed.');
} finally {
  await browser?.close();
  await server?.close();
}
