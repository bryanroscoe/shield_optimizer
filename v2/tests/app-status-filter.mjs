import assert from 'node:assert/strict';
import { mkdir, realpath } from 'node:fs/promises';
import { resolve } from 'node:path';
process.env.VITE_DEMO = '1';
const { createServer } = await import('vite');
const { chromium } = await import('playwright');
const root = resolve(import.meta.dirname, '..');
const evidence = process.env.SHIELD_STATUS_EVIDENCE;
if (evidence) await mkdir(evidence, { recursive: true });
let server, browser;
try {
  server = await createServer({ root, cacheDir: resolve(root, '.vite-status-test'), server: { host: '127.0.0.1', port: 0, hmr: false, fs: { allow: [root, await realpath(resolve(root, 'node_modules'))] } } });
  await server.listen();
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1440, height: 1050 }, ...(evidence ? { recordVideo: { dir: evidence, size: { width: 1440, height: 1050 } } } : {}) });
  const page = await context.newPage();
  page.setDefaultTimeout(12000);
  await page.addInitScript(() => {
    window.addEventListener('DOMContentLoaded', () => {
      const label = () => {
        if (document.getElementById('simulation-label')) return;
        const el = document.createElement('div'); el.id = 'simulation-label';
        el.textContent = 'SIMULATION — fixture devices; no ADB or snapshot deletion';
        el.style.cssText = 'position:fixed;bottom:0;left:0;background:#111;color:white;padding:8px;z-index:99999';
        document.body.append(el);
      };
      label();
      new MutationObserver(label).observe(document.body, { childList: true });
    });
  });
  await page.goto(base);
  await page.getByText('NVIDIA SHIELD', { exact: false }).first().click();
  await page.getByRole('tab', { name: 'App List', exact: true }).click();
  const panel = page.locator('#tabpanel-apps');
  await panel.locator('tbody tr').first().waitFor();
  // Inject command replies into the real demo bridge, leaving production handlers intact.
  await page.evaluate(() => {
    const original = window.__TAURI_INTERNALS__.invoke;
    window.fixture = { failEnable: false, state: 'disabled', calls: [] };
    window.__TAURI_INTERNALS__.invoke = async (cmd, args) => {
      window.fixture.calls.push(cmd);
      const result = await original(cmd, args);
      if (cmd === 'list_other_packages') result.push({ package: 'org.fixture.disabled', name: 'Disabled fixture', system: true, enabled: false });
      if (cmd === 'package_states') {
        delete result['com.philo.philo'];
        if ('com.nvidia.stats' in result) result['com.nvidia.stats'] = window.fixture.state;
      }
      if (cmd === 'enable_package' && !window.fixture.failEnable) window.fixture.state = 'enabled';
      if (cmd === 'disable_package') { window.fixture.state = 'disabled'; return { ok: true, message: 'simulated disabled' }; }
      if (cmd === 'enable_package') return { ok: !window.fixture.failEnable, message: window.fixture.failEnable ? 'simulated failure' : 'simulated enabled' };
      return result;
    };
  });
  await panel.getByRole('button', { name: 'Refresh', exact: true }).click();
  const status = page.getByRole('combobox', { name: 'App status' });
  await page.getByLabel('Hide not installed', { exact: true }).uncheck();
  await status.selectOption('disabled');
  const rows = () => panel.locator('tbody tr').filter({ has: page.locator('.pkg-id') });
  await rows().first().waitFor();
  const disabled = await rows().count();
  const countIs = async (expected) => {
    const deadline = Date.now() + 3000;
    while (await rows().count() !== expected && Date.now() < deadline) await page.waitForTimeout(25);
    assert.equal(await rows().count(), expected, JSON.stringify(await rows().allTextContents()));
  };
  assert.ok(disabled > 0);
  assert.ok(!(await panel.innerText()).includes('com.philo.philo'), 'unknown is not disabled');
  assert.ok(!(await panel.innerText()).includes('com.disney.disneyplus'), 'missing is not disabled');
  await status.selectOption('all');
  await page.getByLabel('Hide decided', { exact: true }).check();
  await status.selectOption('disabled');
  assert.equal(await rows().count(), disabled, 'explicit Disabled overrides Hide decided');
  assert.ok(await page.getByLabel('Hide decided', { exact: true }).isDisabled());
  await page.getByLabel('Show system packages', { exact: true }).check();
  await panel.getByText('org.fixture.disabled', { exact: true }).first().waitFor();
  await page.locator('.app-search').fill('org.fixture.disabled');
  await countIs(1);
  await status.selectOption('enabled');
  await countIs(0);
  await page.locator('.app-search').fill('no.such.package');
  await countIs(0);
  await page.locator('.app-search').fill('');
  await status.selectOption('disabled');
  const row = panel.locator('tr').filter({ has: page.locator('.pkg-id', { hasText: 'com.nvidia.stats' }) });
  await row.waitFor();
  await page.evaluate(() => { window.fixture.failEnable = true; });
  await row.getByRole('button', { name: 'Enable', exact: true }).click();
  await panel.getByText(/simulated failure/).waitFor();
  await row.waitFor();
  await page.evaluate(() => { window.fixture.failEnable = false; });
  await row.getByRole('button', { name: 'Enable', exact: true }).click();
  await row.waitFor({ state: 'detached' });
  await status.selectOption('enabled');
  await row.waitFor();
  page.on('dialog', dialog => dialog.accept());
  await row.getByRole('button', { name: 'Disable', exact: true }).click();
  await row.waitFor({ state: 'detached' });
  await status.selectOption('disabled');
  await row.waitFor();
  // Health links must expose their target even after a restrictive filter.
  await status.selectOption('disabled');
  await page.getByRole('tab', { name: 'Health', exact: true }).click();
  await page.locator('table.mem-table tr[data-package="com.nvidia.tegrazone3"]').click();
  assert.equal(await status.inputValue(), 'all');
  assert.equal(await page.locator('.app-search').inputValue(), 'com.nvidia.tegrazone3');
  await status.selectOption('disabled');
  await page.getByRole('button', { name: 'Back to devices', exact: false }).click();
  await page.getByText('Living Room Box', { exact: true }).first().click();
  await page.getByRole('tab', { name: 'App List', exact: true }).click();
  assert.equal(await status.inputValue(), 'all', 'device navigation resets status');
  await status.selectOption('disabled');
  if (evidence) {
    await page.screenshot({ path: resolve(evidence, 'disabled-apps-simulated.png') });
  }
  await page.getByRole('tab', { name: 'Snapshot', exact: false }).click();
  const manage = page.getByRole('link', { name: 'Manage snapshots', exact: true });
  assert.equal(await manage.getAttribute('href'), '/snapshots');
  await manage.focus();
  await page.keyboard.press('Enter');
  await page.waitForURL('**/snapshots');
  await page.getByRole('heading', { name: 'Snapshots Beta', exact: true }).waitFor();
  if (evidence) await page.screenshot({ path: resolve(evidence, 'manage-snapshots-simulated.png') });
  const calls = await page.evaluate(() => window.fixture?.calls);
  assert.ok(Array.isArray(calls) && calls.length > 0, 'command recorder survives navigation');
  assert.deepEqual(calls.filter(cmd => ['delete_snapshot', 'save_snapshot', 'apply_snapshot'].includes(cmd)), [], 'navigation never mutates snapshots');
  const video = page.video();
  await context.close();
  if (evidence) await video.saveAs(resolve(evidence, 'app-status-snapshots-simulated.webm'));
  console.log('PASS: status, unknown/missing exclusion, both sections, Hide decided, search/empty, failed/successful enable, successful disable, Health link, navigation reset, keyboard Manage snapshots. No snapshots deleted.');
} finally { await browser?.close(); await server?.close(); }
