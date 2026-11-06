import assert from 'node:assert/strict';
import { mkdir, rename } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';
const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const evidence = process.env.SHIELD_UNINSTALL_EVIDENCE;
const baseline = process.env.SHIELD_UNINSTALL_BASELINE === '1';
process.env.VITE_DEMO = '1';
delete process.env.TAURI_DEV_HOST;
let server, browser, context;
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, hmr: false } });
  await server.listen();
  browser = await chromium.launch();
  context = await browser.newContext({ viewport: { width: 1400, height: 1000 }, ...(evidence && !baseline ? { recordVideo: { dir: evidence, size: { width: 1400, height: 1000 } } } : {}) });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.addInitScript(() => document.addEventListener('DOMContentLoaded', () => {
    const label = document.createElement('div');
    label.textContent = 'SYNTHETIC UI TEST — fixture apps; no TV connected; device writes blocked';
    label.style.cssText = 'position:fixed;bottom:0;left:0;right:0;padding:8px;background:#111;color:white;z-index:99999;pointer-events:none';
    document.body.append(label);
  }, { once: true }));
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
  await page.evaluate(() => {
    const original = window.__TAURI_INTERNALS__.invoke;
    const fixture = (id, overrides = {}) => ({ package: `test.fixture.${id}`, name: `Fixture ${id}`, method: 'disable', play_store: true, risk: 'safe', default_optimize: true, default_restore: false, optimize_description: 'Synthetic optional app; removing it loses its functionality.', ...overrides });
    const entries = [fixture('disable'), fixture('review', { review: true, default_optimize: false, risk: 'caution' }), fixture('keep', { default_optimize: false }), fixture('disabled'), fixture('primary', { method: 'uninstall' }), fixture('reviewprimary', { method: 'uninstall', default_optimize: false, review: true }), fixture('defunctsource', { package: 'org.fdroid.fdroid', play_store: false, defunct: true }), fixture('defunct', { play_store: false, defunct: true }), fixture('nostore', { play_store: false }), fixture('missing'), fixture('unreadable'), fixture('protected'), fixture('unavailable'), fixture('long', { name: 'Fixture long optional streaming application with a deliberately long descriptive name' }), fixture('sideload', { package: 'org.smarttube.stable', play_store: false })];
    window.__UNINSTALL_TEST__ = { entries, calls: [], mode: 'normal', reads: 0 };
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      const t = window.__UNINSTALL_TEST__; t.calls.push({ command, args });
      if (command === 'app_list_for_device') return entries;
      if (command === 'package_states') {
        const fresh = args.packages.length === 1;
        if (fresh) t.reads++;
        if (fresh && t.mode === 'device-change') await new Promise(resolve => t.release = resolve);
        if (fresh && (t.mode === 'state-before' || t.mode === 'state-after' && t.reads > 1)) return {};
        return Object.fromEntries(args.packages.filter(p => !p.endsWith('.unreadable')).map(p => [p, p.endsWith('.missing') ? 'missing' : p.endsWith('.disabled') ? 'disabled' : 'enabled']));
      }
      if (command === 'safety_info' && (args.package.startsWith('test.fixture.') || ['org.smarttube.stable', 'org.fdroid.fdroid'].includes(args.package))) {
        if (args.package.endsWith('.unavailable')) throw new Error('Synthetic safety unavailable');
        const kind = args.package.endsWith('.protected') || t.mode === 'safety-after' && t.reads > 1 ? 'never_disable' : args.package.endsWith('.review') ? 'caution' : 'safe';
        return { kind, reason: 'Synthetic consequence: app functionality is removed.', source: 'reviewed_catalog' };
      }
      if (command === 'list_other_packages') return [{ package: 'test.other.app', name: 'Fixture other app', enabled: true, system: false }];
      if (['disable_package', 'enable_package', 'uninstall_package', 'reinstall_existing', 'backup_apk'].includes(command)) throw new Error('Synthetic test blocks all device writes');
      return original(command, args);
    };

  });
  await page.getByText('NVIDIA SHIELD', { exact: false }).first().click();
  await page.getByRole('tab', { name: 'App List', exact: true }).click();
  await page.getByRole('checkbox', { name: 'Hide not installed', exact: true }).uncheck();
  const row = id => page.locator('#tabpanel-apps table.app-table').first().locator('tr').filter({ has: page.locator('.pkg-id', { hasText: new RegExp(`^${(id === 'sideload' ? 'org.smarttube.stable' : id === 'defunctsource' ? 'org.fdroid.fdroid' : `test.fixture.${id}`).replaceAll('.', '\\.')}\\s*$`) }) });
  const uninstall = id => row(id).locator('.row-verbs button').filter({ hasText: /^Uninstall$/ });
  await row('disable').getByRole('button', { name: 'Disable', exact: true }).waitFor();
  for (const id of ['disable', 'review', 'keep', 'disabled', 'long']) assert.equal(await uninstall(id).count(), baseline ? 0 : 1, id);
  assert.equal(await uninstall('primary').count(), 1, 'primary action must not duplicate');
  assert.equal(await row('reviewprimary').getByRole('button', { name: 'Review: uninstall if unused', exact: true }).count(), 1);
  assert.equal(await uninstall('reviewprimary').count(), 0, 'review primary must not duplicate');
  await row('primary').getByRole('button', { name: 'Keep', exact: true }).click();
  await row('primary').locator('[data-rec=Kept]').waitFor();
  assert.equal(await uninstall('primary').count(), 0, 'preserve kept primary suppression');
  assert.equal(await uninstall('sideload').count(), 1, 'named sideload route preserved');
  for (const id of ['defunct', 'defunctsource', 'nostore', 'missing', 'unreadable', 'protected', 'unavailable']) assert.equal(await uninstall(id).count(), 0, id);
  await row('review').getByRole('button', { name: 'Keep', exact: true }).click();
  assert.equal(await row('review').locator('[data-rec="Kept"]').count(), 1);
  if (!baseline) await uninstall('review').waitFor();
  assert.equal(await uninstall('review').count(), baseline ? 0 : 1, 'Kept row');
  await page.mouse.move(0, 0);
  if (evidence) await page.screenshot({ path: join(evidence, baseline ? 'before-desktop-synthetic.png' : 'after-desktop-synthetic.png') });
  if (evidence) await page.waitForTimeout(1500);
  if (!baseline) {
    for (const mode of ['cancel', 'state-before', 'state-after', 'safety-after', 'repeat', 'normal']) {
      await page.evaluate(mode => { const t = window.__UNINSTALL_TEST__; t.mode = mode; t.reads = 0; t.calls = []; }, mode);
      const dialogs = [];
      const onDialog = async dialog => { dialogs.push(dialog.message()); if (mode === 'cancel') await dialog.dismiss(); else await dialog.accept(); };
      page.on('dialog', onDialog);
      if (mode === 'repeat') await uninstall('disable').evaluate(button => { button.click(); button.click(); });
      else await uninstall('disable').click();
      await page.waitForFunction(() => !document.querySelector('#tabpanel-apps .row-verbs button:disabled'));
      await page.waitForTimeout(evidence ? 600 : 100);
      page.off('dialog', onDialog);
      const calls = await page.evaluate(() => window.__UNINSTALL_TEST__.calls);
      assert.equal(calls.filter(c => c.command === 'uninstall_package').length, ['normal', 'repeat'].includes(mode) ? 1 : 0, mode);
      if (mode === 'normal') {
        assert.equal(calls.filter(c => c.command === 'package_states').length, 2);
        assert.equal(calls.filter(c => c.command === 'safety_info').length, 2);
        assert.match(dialogs[0], /UNINSTALL Fixture disable/);
        assert.match(dialogs[0], /Safety:/);
      }
    }
    for (const theme of ['light', 'dark']) {
      await page.getByRole('button', { name: theme === 'dark' ? 'Dark' : 'Light', exact: true }).click();
      await page.waitForFunction(theme => document.documentElement.dataset.theme === theme, theme);
      for (const width of [1400, 720]) {
        await page.setViewportSize({ width, height: 1100 });
        await page.evaluate(() => document.documentElement.style.fontSize = '');
        if (evidence) { await page.waitForTimeout(800); await page.screenshot({ path: join(evidence, `after-${theme}-${width}-synthetic.png`) }); }
      }
      await page.evaluate(() => document.documentElement.style.fontSize = '200%');
      await row('disable').scrollIntoViewIfNeeded();
      const tableRegion = page.getByRole('region', { name: 'Curated apps', exact: true });
      assert.ok(await tableRegion.evaluate(el => el.scrollWidth > el.clientWidth), 'large text uses contained horizontal scrolling');
      await tableRegion.focus();
      await page.keyboard.press('ArrowRight');
      await row('unavailable').evaluate(el => el.scrollIntoView({ block: 'center', inline: 'nearest' }));
      await tableRegion.evaluate(el => el.scrollLeft = el.scrollWidth);
      await page.waitForTimeout(evidence ? 1000 : 100);
      if (evidence) await page.screenshot({ path: join(evidence, `after-${theme}-720-large-text-synthetic.png`) });
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1), 'table must not overflow page');
      await tableRegion.evaluate(el => el.scrollLeft = 0);
    }
  }
  if (!baseline) {
    await page.setViewportSize({ width: 1400, height: 1000 });
    await page.evaluate(() => { document.documentElement.style.fontSize = ''; const t = window.__UNINSTALL_TEST__; t.mode = 'device-change'; t.reads = 0; t.calls = []; });
    await uninstall('disable').evaluate(button => button.click());
    await page.waitForFunction(() => !!window.__UNINSTALL_TEST__.release);
    await page.getByRole('button', { name: 'Back to devices', exact: true }).click();
    await page.evaluate(() => window.__UNINSTALL_TEST__.release());
    await page.waitForTimeout(100);
    assert.equal(await page.evaluate(() => window.__UNINSTALL_TEST__.calls.filter(c => c.command === 'uninstall_package').length), 0, 'late read cannot cross device-page identity');
  }
  assert.deepEqual(errors, []);
  const video = page.video();
  await context.close(); context = null;
  if (video) await rename(await video.path(), join(evidence, 'desktop-uninstall-synthetic.webm'));
  console.log(`${baseline ? 'Baseline reproduced' : 'PASS'}: rendered action matrix, defunct/no-store exclusions, Kept, primary duplication; final run also covers consent and fresh-evidence refusals.`);
} finally { await context?.close(); await browser?.close(); await server?.close(); }
