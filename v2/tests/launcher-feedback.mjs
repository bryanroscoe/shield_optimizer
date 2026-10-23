import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const V2 = join(dirname(fileURLToPath(import.meta.url)), '..');
const evidence = process.env.LAUNCHER_FEEDBACK_EVIDENCE === '1';
const dir = join(V2, 'evidence/launcher-feedback-212');
const target = 'com.spocky.projengmenu';
const stock = 'com.google.android.tvlauncher';
process.env.VITE_DEMO = '1';
delete process.env.TAURI_DEV_HOST;
let server, browser;
try {
  server = await createServer({ root: V2, server: { host: '127.0.0.1', port: 0, hmr: false } });
  await server.listen();
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();
  if (evidence) await mkdir(dir, { recursive: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, ...(evidence ? { recordVideo: { dir: join(dir, 'raw'), size: { width: 1280, height: 900 } } } : {}) });
  const page = await context.newPage();
  for (const scenario of ['success', 'failure', 'noop', 'unverified', 'failure-refresh', 'list-refresh', 'current-refresh', 'channel-refresh', 'interrupted', 'cancel-takeover', 'leave-pending']) {
    await page.goto(base, { waitUntil: 'networkidle' });
    await page.evaluate(({ scenario, target, stock }) => {
      let attempted = false, current = stock;
      const original = window.__TAURI_INTERNALS__.invoke;
      window.launcherCalls = [];
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        window.launcherCalls.push({ command, args });
        if (command === 'set_default_launcher') {
          attempted = true;
          if (scenario === 'leave-pending') return new Promise(resolve => { window.releaseLauncher = () => resolve({ ok: true, strategy: 'role', current_launcher: target, last_error: null, stock_takeover_available: false, diagnostics: [] }); });
          await new Promise(resolve => setTimeout(resolve, 200));
          if (scenario === 'interrupted') throw new Error('Simulated connection interrupted');
          const ok = !['failure', 'noop', 'unverified', 'failure-refresh', 'cancel-takeover'].includes(scenario);
          if (ok) current = target;
          return { ok, strategy: ok ? 'role' : null, current_launcher: current,
            last_error: ['failure', 'failure-refresh'].includes(scenario) ? 'Android refused the launcher change.' : scenario === 'noop' ? 'The device accepted the command but Home still resolves to stock.' : scenario === 'cancel-takeover' ? 'Stock still holds Home.' : null,
            stock_takeover_available: scenario === 'cancel-takeover', diagnostics: ok ? [] : ['SIMULATED: set-home returned without verified change'] };
        }
        if (attempted && ((['list-refresh', 'failure-refresh'].includes(scenario) && command === 'list_launchers') || (scenario === 'current-refresh' && command === 'current_launcher') || (scenario === 'channel-refresh' && command === 'channel_provider_disabled'))) throw new Error('Simulated refresh unavailable');
        if (command === 'current_launcher') return { package: current, activity: '.Home' };
        return original(command, args);
      };
    }, { scenario, target, stock });
    await page.getByText('NVIDIA SHIELD', { exact: false }).first().click();
    await page.getByRole('tab', { name: 'Launcher' }).click();
    const row = page.locator('.launcher-list li').filter({ has: page.locator('.launcher-pkg', { hasText: target }) });
    const button = row.getByRole('button', { name: 'Set as default', exact: true });
    await button.waitFor();
    if (evidence) await page.evaluate(scenario => {
      const label = document.createElement('div'); label.id = 'simulation-label';
      label.textContent = `SIMULATED launcher feedback: ${scenario} — no real device`;
      label.style.cssText = 'position:fixed;top:0;right:0;z-index:99999;padding:8px;background:#1a2540;color:white;font:14px sans-serif';
      document.body.appendChild(label);
    }, scenario);
    if (scenario === 'cancel-takeover') page.once('dialog', dialog => dialog.dismiss());
    await button.click();
    if (scenario === 'leave-pending') {
      await row.getByText('Setting default launcher', { exact: false }).waitFor();
      await page.getByRole('button', { name: 'Back to devices' }).click();
      await page.locator('#tabpanel-launcher').waitFor({ state: 'detached' });
      const before = await page.evaluate(() => launcherCalls.filter(c => ['list_launchers', 'current_launcher'].includes(c.command)).length);
      await page.evaluate(() => releaseLauncher());
      await page.waitForTimeout(200);
      assert.equal(await page.evaluate(() => launcherCalls.filter(c => ['list_launchers', 'current_launcher'].includes(c.command)).length), before, 'a departed page must not refresh or publish a late result');
      console.log('Launcher feedback: leave-pending passed');
      continue;
    }
    const result = page.locator('.launcher-set-result');
    await result.waitFor({ timeout: 10000 });
    if (scenario.includes('refresh')) await result.filter({ hasText: 'displayed state may be stale' }).waitFor();
    const message = await result.innerText();
    if (['success', 'list-refresh', 'current-refresh', 'channel-refresh'].includes(scenario)) assert.match(message, /Projectivy Launcher is now your default launcher/);
    if (['failure', 'failure-refresh'].includes(scenario)) assert.match(message, /Android refused/);
    if (scenario === 'unverified') assert.match(message, /Android did not verify.*No success was confirmed/);
    if (scenario === 'noop') assert.match(message, /still resolves to stock/);
    if (scenario === 'interrupted') assert.match(message, /Could not confirm whether.*Refresh/);
    if (scenario === 'cancel-takeover') assert.match(message, /Stock still holds Home/);
    if (!scenario.includes('refresh')) {
      assert.equal(await row.locator('.launcher-set-result').count(), 1, 'feedback stays beside the clicked launcher');
      assert.ok(await result.isVisible());
    }
    if (scenario === 'success') {
      await row.locator('.current-default').waitFor();
      assert.ok(!message.includes('stock launcher was disabled'));
    }
    const mutations = await page.evaluate(() => launcherCalls.filter(c => ['set_default_launcher', 'disable_launcher', 'disable_stock_launcher', 'disable_package'].includes(c.command)));
    assert.equal(mutations.length, 1, 'feedback must not add mutation commands or retry takeover after cancellation');
    assert.equal(mutations[0].args.allowStockDisable, false);
    assert.equal(mutations[0].args.package, target);
    assert.equal(await result.getAttribute('role'), 'status');
    if (scenario === 'failure-refresh') {
      await page.evaluate(() => Object.defineProperty(navigator, 'clipboard', { value: { writeText: async () => { throw new Error('clipboard refused'); } }, configurable: true }));
      await page.getByRole('button', { name: 'Copy diagnostic details' }).click();
      await page.locator('.action-message').filter({ hasText: 'SIMULATED: set-home returned' }).waitFor();
    }
    if (evidence) {
      await result.scrollIntoViewIfNeeded();
      await page.screenshot({ path: join(dir, `${scenario}-simulated.png`), animations: 'disabled' });
      await page.waitForTimeout(350);
    }
    console.log(`Launcher feedback: ${scenario} passed`);
  }
  const video = page.video();
  await context.close();
  if (evidence) await video.saveAs(join(dir, 'launcher-feedback-simulated.webm'));
} finally {
  await browser?.close();
  await server?.close();
}
