import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { before, after, test } from 'node:test';
import { chromium } from 'playwright';
import { startViteServer } from './helpers/vite-harness.mjs';
let server, origin, browser;
before(async () => { ({ server, origin } = await startViteServer()); browser = await chromium.launch({ headless: true }); });
after(async () => { await browser?.close(); await server?.close(); });
const pkg = 'com.example.video';
async function open(t, screen, evidence = false) {
  const context = await browser.newContext({ viewport: { width: 384, height: 812 }, ...(evidence ? { recordVideo: { dir: typeof evidence === 'string' ? `${evidence}/raw` : 'evidence/sheet-identity-2026-10-17/raw', size: { width: 384, height: 812 } } } : {}) });
  const page = await context.newPage(); page.setDefaultTimeout(10000); t.after(() => context.close());
  await page.addInitScript(() => {
    localStorage.clear(); window.calls = []; window.handlers = {}; window.activeHost = 'A';
    const entry = { package: 'com.example.video', name: 'Example video', method: 'disable', risk: 'safe', optimize_description: 'Synthetic optional video app.', restore_description: 'Restore.', default_optimize: true, default_restore: false, play_store: true };
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      calls.push({ command, args }); if (handlers[command]) return handlers[command](args);
      switch (command) {
        case 'get_entitlement': return 'pro';
        case 'wireless_connect': activeHost = args.host; return { ok: true };
        case 'wireless_status': return { connected: true };
        case 'list_devices': return [{ id: 1, serial: `${activeHost}:5555`, name: `Mock TV ${activeHost}`, model: 'TV', status: 'device', connection: 'network', device_type: 'shield', properties: null }];
        case 'health_report': return { ram: { total_mb: 2048, free_mb: 1024 }, storage: {}, display: {}, top_memory: [{ process: 'com.example.video:remote', package: 'com.example.video', pid: 1, mb: 55 }] };
        case 'app_list_for_device': return [entry];
        case 'package_states': return Object.fromEntries(args.packages.map(p => [p, 'enabled']));
        case 'list_installed_packages': return [{ package: entry.package, system: false, enabled: true }];
        case 'prepare_optimize': return { mode: args.mode, items: [{ entry, action: { kind: 'disable' }, memory_mb: 55 }] };
        case 'safety_info': return { kind: 'safe', reason: 'Synthetic optional app.' };
        case 'process_safety_info': return { kind: 'unknown', reason: 'Unresolved process.' };
        case 'app_memory_map': return { 'com.example.video': 55 };
        case 'app_usage_map': return {};
        case 'app_storage_map': return {};
        default: return { ok: true, message: 'mock result' };
      }
    } };
  });
  await page.goto(origin);
  await page.evaluate(async screen => {
    const { session } = await import('/src/lib/session.svelte.ts');
    const { router } = await import('/src/lib/router.svelte.ts');
    window.session = session; window.router = router;
    await session.connect('A', 5555); router.reset('dashboard'); router.navigate(screen);
  }, screen);
  return page;
}
async function select(page, screen) {
  // A reconnect clears cached diagnostics; refresh the mock report before the
  // user makes a NEW explicit selection. Do not resurrect the old sheet.
  if (screen === 'diagnostics') await page.evaluate(() => session.loadHealth(true));
  const name = screen === 'optimize' ? 'Details for Example video' : 'Details for com.example.video:remote';
  await page.getByRole('button', { name, exact: true }).click();
  await page.locator('.sheet .app-pkg', { hasText: pkg }).waitFor();
  await page.locator('.sheet').getByRole('button', { name: 'Disable', exact: true }).waitFor({ state: 'visible' });
  await page.waitForFunction(() => ![...document.querySelectorAll('.sheet button')].find(b => b.textContent.trim() === 'Disable')?.disabled);
}
const mutations = page => page.evaluate(() => calls.filter(c => ['disable_package', 'uninstall_package', 'enable_package', 'force_stop', 'open_play_store', 'reinstall_existing'].includes(c.command)));
const settle = page => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
async function change(page, transition) {
  await page.evaluate(async transition => {
    if (transition === 'disconnect') await session.disconnect();
    await session.connect(transition === 'switch' ? 'B' : 'A', 5555);
  }, transition);
  await settle(page);
}

for (const screen of ['diagnostics', 'optimize']) {
  for (const transition of ['disconnect', 'reconnect', 'switch']) test(`${screen}: ${transition} requires a new sheet selection before any action`, async t => {
    const page = await open(t, screen); await select(page, screen);
    await change(page, transition);
    assert.equal(await page.locator('.sheet').count(), 0, 'old selection must not resurrect or bind to the new connection');
    assert.notEqual(await page.evaluate(() => document.body.style.position), 'fixed', 'modal ownership must be released when selection is invalidated');
    assert.equal((await mutations(page)).length, 0);
    await select(page, screen);
    await page.locator('.sheet').getByRole('button', { name: 'Disable', exact: true }).click();
    await page.locator('.dialog-card').getByRole('button', { name: 'Disable', exact: true }).click();
    await page.waitForFunction(() => calls.some(c => c.command === 'disable_package'));
    const writes = await mutations(page);
    assert.deepEqual(writes.map(c => [c.command, c.args.serial, c.args.package]), [['disable_package', transition === 'switch' ? 'B:5555' : 'A:5555', pkg]]);
  });

  test(`${screen}: old pending safety cannot open a confirmation after switch or a fresh same-connection selection`, async t => {
    const page = await open(t, screen); await select(page, screen);
    await page.evaluate(() => { handlers.safety_info = () => new Promise(resolve => { window.resolveOldSafety = resolve; }); });
    await page.locator('.sheet').getByRole('button', { name: 'Disable', exact: true }).click();
    await page.waitForFunction(() => !!window.resolveOldSafety);
    await change(page, 'switch');
    await page.evaluate(() => { delete handlers.safety_info; resolveOldSafety({ kind: 'safe', reason: 'Old A response' }); });
    await settle(page);
    assert.equal(await page.locator('.sheet, .dialog-card, .paywall-card').count(), 0);
    assert.equal((await mutations(page)).length, 0);
    await select(page, screen);
    await page.evaluate(() => { handlers.safety_info = () => new Promise(resolve => { window.resolveOldSafety = resolve; }); });
    await page.locator('.sheet').getByRole('button', { name: 'Disable', exact: true }).click();
    await page.locator('.sheet').getByRole('button', { name: 'Close', exact: true }).click();
    await page.evaluate(() => { delete handlers.safety_info; });
    await select(page, screen);
    await page.evaluate(() => resolveOldSafety({ kind: 'safe', reason: 'Closed selection response' }));
    await settle(page);
    assert.equal(await page.locator('.dialog-card').count(), 0, 'same connection does not authorize an old selection result');
    assert.equal((await mutations(page)).length, 0);
  });

  for (const action of ['Disable', 'Uninstall']) test(`${screen}: ${action} nested confirmation is cleared by same-endpoint reconnect`, async t => {
    const page = await open(t, screen); await select(page, screen);
    await page.locator('.sheet').getByRole('button', { name: new RegExp(`^${action}`) }).click();
    await page.locator('.dialog-card').waitFor();
    await change(page, 'reconnect');
    assert.equal(await page.locator('.sheet, .dialog-card, .paywall-card').count(), 0);
    assert.equal((await mutations(page)).length, 0);
  });

  test(`${screen}: paywall and pending measurements cannot follow the opening identity`, async t => {
    const page = await open(t, screen); await select(page, screen);
    await page.evaluate(() => { handlers.force_stop = () => Promise.reject('LOCKED:force_stop'); });
    await page.locator('.sheet').getByRole('button', { name: /Force stop/ }).click();
    await page.locator('.paywall-card').waitFor();
    await change(page, 'switch');
    assert.equal(await page.locator('.sheet, .dialog-card, .paywall-card').count(), 0);
    await page.evaluate(() => { handlers.app_memory_map = () => new Promise(resolve => { window.resolveOldMemory = resolve; }); });
    await select(page, screen);
    await page.waitForFunction(() => !!window.resolveOldMemory);
    await change(page, 'reconnect');
    await page.evaluate(() => { delete handlers.app_memory_map; resolveOldMemory({ 'com.example.video': 999 }); });
    await select(page, screen);
    await page.locator('.sheet').getByText('55.0 MB', { exact: true }).waitFor();
    assert.equal(await page.locator('.sheet').getByText('999 MB', { exact: true }).count(), 0);
    assert.equal((await mutations(page)).filter(c => c.command !== 'force_stop').length, 0);
  });
}


for (const screen of ['diagnostics', 'optimize']) {
  test(`${screen}: a failed reconnect or transient liveness loss still invalidates the opening`, async t => {
    const page = await open(t, screen); await select(page, screen);
    await page.evaluate(async () => {
      handlers.wireless_connect = () => ({ ok: false, message: 'Synthetic refusal' });
      await session.connect('B', 5555); delete handlers.wireless_connect;
    });
    await settle(page);
    assert.equal(await page.locator('.sheet, .dialog-card, .paywall-card').count(), 0);
    await select(page, screen);
    await page.evaluate(() => { session.liveness = 'lost'; });
    await settle(page);
    await page.evaluate(() => { session.liveness = 'live'; });
    await settle(page);
    assert.equal(await page.locator('.sheet, .dialog-card, .paywall-card').count(), 0);
    assert.equal((await mutations(page)).length, 0);
  });
}


if (process.env.SHEET_IDENTITY_EVIDENCE === '1') test('capture synthetic opening identity invalidation', async t => {
  const dir = 'evidence/sheet-identity-2026-10-17'; await mkdir(dir, { recursive: true });
  const page = await open(t, 'diagnostics', true); await select(page, 'diagnostics');
  await page.screenshot({ path: `${dir}/opened-on-a-simulated.png`, animations: 'disabled' });
  await page.waitForTimeout(600);
  await page.locator('.sheet').getByRole('button', { name: 'Disable', exact: true }).click();
  await page.locator('.dialog-card').waitFor();
  await page.screenshot({ path: `${dir}/a-confirmation-simulated.png`, animations: 'disabled' });
  await page.waitForTimeout(600);
  await change(page, 'switch');
  await page.evaluate(() => session.loadHealth(true)); await settle(page);
  assert.equal(await page.locator('.sheet, .dialog-card, .paywall-card').count(), 0);
  assert.equal((await mutations(page)).length, 0);
  await page.screenshot({ path: `${dir}/b-requires-fresh-selection-simulated.png` });
  await page.waitForTimeout(600);
  await select(page, 'diagnostics');
  await page.screenshot({ path: `${dir}/fresh-b-selection-simulated.png`, animations: 'disabled' });
  await page.waitForTimeout(600);
  await page.locator('.sheet').getByRole('button', { name: 'Disable', exact: true }).click();
  await page.locator('.dialog-card').getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.locator('.sheet').getByRole('button', { name: 'Close', exact: true }).click();
  await page.waitForTimeout(600);
  await writeFile(`${dir}/scenario.json`, JSON.stringify({ fixture: 'synthetic TV A -> B', real_device: false, recorded_mutations: await mutations(page), current_serial: await page.evaluate(() => session.serial), requires_fresh_selection: true, confirmation_canceled: true }, null, 2) + '\n');
  const video = page.video(); await page.context().close(); await video.saveAs(`${dir}/sheet-opening-identity-simulated.webm`);
});

for (const screen of ['diagnostics', 'optimize']) {
  for (const action of ['Disable', 'Uninstall']) test(`${screen}: ${action} landing with a false response discards the stale selection`, async t => {
    const evidence = process.env.DETAIL_STATE_EVIDENCE === '1' && screen === 'optimize' && action === 'Disable' ? 'evidence/detail-state-2026-10-17' : false;
    if (evidence) await mkdir(evidence, { recursive: true });
    const page = await open(t, screen, evidence); await select(page, screen);
    if (evidence) await page.screenshot({ path: `${evidence}/before-inconclusive-simulated.png`, animations: "disabled" });
    await page.evaluate(action => {
      const command = action === 'Disable' ? 'disable_package' : 'uninstall_package';
      handlers[command] = () => {
        window.mutationLanded = true;
        handlers.package_states = ({ packages }) => Object.fromEntries(packages.map(p => [p, action === 'Disable' ? 'disabled' : 'missing']));
        handlers.list_installed_packages = () => action === 'Disable' ? [{ package: 'com.example.video', system: false, enabled: false }] : [];
        handlers.prepare_optimize = ({ mode }) => ({ mode, items: [] });
        return { ok: false, message: 'Synthetic verification inconclusive after mutation.' };
      };
    }, action);
    const before = await page.evaluate(() => calls.length);
    await page.locator('.sheet').getByRole('button', { name: new RegExp(`^${action}`) }).click();
    await page.locator('.dialog-card').getByRole('button', { name: action, exact: true }).click();
    await page.waitForFunction(() => window.mutationLanded);
    await page.getByText('Synthetic verification inconclusive after mutation.', { exact: true }).waitFor();
    await settle(page);
    assert.equal(await page.locator('.sheet, .dialog-card').count(), 0, 'an inconclusive write must clear child and parent selection');
    assert.notEqual(await page.evaluate(() => document.body.style.position), 'fixed');
    await page.waitForFunction(({ before, screen }) => calls.slice(before).some(c => c.command === (screen === 'optimize' ? 'prepare_optimize' : 'health_report')), { before, screen });
    if (evidence) {
      await page.screenshot({ path: `${evidence}/inconclusive-cleared-simulated.png`, animations: "disabled" });
      await page.waitForTimeout(700);
    }
    await change(page, 'reconnect');
    assert.equal(await page.locator('.sheet').count(), 0, 'reconnect cannot reopen the stale parent object');
    assert.equal((await mutations(page)).length, 1);
    if (evidence) { const video = page.video(); await page.context().close(); await video.saveAs(`${evidence}/inconclusive-cleared-simulated.webm`); }
  });
}

test('Restore detail offers Enable and dispatches only enable_package', async t => {
  const evidence = process.env.DETAIL_STATE_EVIDENCE === '1' ? 'evidence/detail-state-2026-10-17' : false;
  if (evidence) await mkdir(evidence, { recursive: true });
  const page = await open(t, 'optimize', evidence);
  await page.evaluate(() => {
    handlers.prepare_optimize = ({ mode }) => ({ mode, items: [{ entry: { package: 'com.example.video', name: 'Example video', method: 'disable', risk: 'safe', optimize_description: 'Synthetic app.', restore_description: 'Restore synthetic app.', default_optimize: true, default_restore: true, play_store: true }, action: { kind: 'enable' }, memory_mb: null }] });
  });
  await page.getByRole('button', { name: 'Restore', exact: true }).click();
  await page.getByRole('button', { name: 'Details for Example video', exact: true }).click();
  const enable = page.locator('.sheet').getByRole('button', { name: 'Enable', exact: true });
  await enable.waitFor();
  assert.equal(await page.locator('.sheet').getByRole('button', { name: 'Disable', exact: true }).count(), 0);
  if (evidence) { await page.screenshot({ path: `${evidence}/restore-enable-simulated.png`, animations: "disabled" }); await page.waitForTimeout(700); }
  await enable.click();
  await page.waitForFunction(() => calls.some(c => c.command === 'enable_package'));
  assert.deepEqual((await mutations(page)).map(c => [c.command, c.args.serial, c.args.package]), [['enable_package', 'A:5555', pkg]]);
  if (evidence) { await page.waitForTimeout(700); const video = page.video(); await page.context().close(); await video.saveAs(`${evidence}/restore-enable-simulated.webm`); }
});

for (const screen of ['diagnostics', 'optimize']) test(`${screen}: successful uninstall retains the explicit Reinstall path`, async t => {
  const page = await open(t, screen); await select(page, screen);
  await page.locator('.sheet').getByRole('button', { name: /^Uninstall/ }).click();
  await page.locator('.dialog-card').getByRole('button', { name: 'Uninstall', exact: true }).click();
  await page.locator('.sheet').getByRole('button', { name: /Reinstall/ }).waitFor();
  assert.equal(await page.locator('.sheet').getByRole('button', { name: 'Disable', exact: true }).count(), 0);
  assert.equal((await mutations(page)).filter(c => c.command === 'uninstall_package').length, 1);
});
