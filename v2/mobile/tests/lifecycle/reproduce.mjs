import assert from 'node:assert/strict';
import { after, before, test } from 'node:test';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { chromium } from 'playwright';
import { createServer } from 'vite';

const root = path.resolve(process.env.LIFECYCLE_MOBILE_ROOT || fileURLToPath(new URL('../../', import.meta.url)));
const out = path.resolve(process.env.LIFECYCLE_OUTPUT || '/tmp/so-vtm.8-lifecycle');
const observations = [];
let server, browser, origin;
const saved = (host = 'tv-a.invalid') => ({ host, connectPort: 5555, name: host === 'tv-a.invalid' ? 'Fixture TV A' : 'Fixture TV B', deviceType: 'shield', lastUsed: '2026-09-06T00:00:00.000Z' });
const device = (host) => ({ id: 1, serial: `${host}:5555`, name: 'Fixture TV', model: 'Shield', status: 'device', connection: 'network', device_type: 'shield', properties: { friendly_name: 'Fixture TV A' } });

before(async () => {
  await mkdir(out, { recursive: true });
  server = await createServer({ root, logLevel: 'warn', server: { host: '127.0.0.1', port: 0 } });
  await server.listen();
  origin = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch({ headless: true });
});
after(async () => {
  const files = ['src/App.svelte', 'src/lib/router.svelte.ts', 'src/lib/session.svelte.ts', 'src/lib/savedDevices.ts', 'src/screens/Onboarding.svelte', 'src-tauri/src/remote_lifecycle.rs', 'src-tauri/gen/android/app/src/main/java/com/atvoptimizer/mobile/MainActivity.kt'];
  const hashes = Object.fromEntries(await Promise.all(files.map(async (file) => [file, createHash('sha256').update(await readFile(path.join(root, file))).digest('hex')])));
  await writeFile(path.join(out, 'observations.json'), JSON.stringify({
    scope: 'Host Chromium with mocked Tauri; synthetic visibility events and virtual time; no Android lifecycle or real ADB execution',
    sourceHead: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
    sourceStatus: execFileSync('git', ['status', '--short'], { cwd: root, encoding: 'utf8' }).trim(),
    root, node: process.version, platform: `${process.platform}/${process.arch}`, browser: browser?.version(), hashes, observations,
  }, null, 2) + '\n');
  await browser?.close();
  await server?.close();
});

async function open(t, { rows = [], auto = '0', state, activeHost = '' } = {}) {
  const storageState = state || { cookies: [], origins: [{ origin, localStorage: [
    { name: 'atv.savedDevices.v1', value: JSON.stringify(rows) },
    { name: 'atv.autoConnect.v1', value: auto },
  ] }] };
  const context = await browser.newContext({ viewport: { width: 384, height: 812 }, storageState });
  const model = { activeHost, alive: true, connectMode: 'ok', calls: [], pending: [], errors: [] };
  await context.exposeBinding('__hostOnlyInvoke', async (_, command, args, documentId) => {
    model.calls.push({ command, args, documentId });
    switch (command) {
      case 'get_entitlement': return 'pro';
      case 'list_devices': return model.activeHost ? [device(model.activeHost)] : [];
      case 'wireless_status': return { connected: model.alive && !!model.activeHost };
      case 'wireless_connect': {
        if (model.connectMode === 'hold') await new Promise(resolve => model.pending.push(resolve));
        if (model.connectMode === 'fail') return { ok: false, message: 'Fixture transport unavailable' };
        model.activeHost = args.host;
        return { ok: true, message: 'Fixture connected' };
      }
      case 'wireless_disconnect': model.activeHost = ''; return { ok: true };
      case 'wireless_cancel_connect': return;
      case 'remote_warm': return { transport: 'channel', message: 'Fixture channel' };
      case 'send_key': return { ok: true, transport: 'channel' };
      case 'health_report': return { ram: { total_mb: 4096, used_mb: 3072, free_mb: 1024, swap_mb: 0 }, storage: { total: '16G', used: '8G', available: '8G', used_percent: 50 }, display: { resolution: '1920x1080', refresh_hz: 60, hdr_types: [] }, top_memory: [], temperature_c: null, audio_device: null };
      case 'app_list_for_device': return [];
      default: throw new Error(`Unexpected host fixture command: ${command}`);
    }
  });
  await context.addInitScript(() => {
    const documentId = crypto.randomUUID();
    window.__TAURI_INTERNALS__ = { invoke: (command, args) => window.__hostOnlyInvoke(command, args, documentId) };
  });
  // Only the loopback Vite server is reachable from this fixture browser.
  await context.route('**/*', route => new URL(route.request().url()).origin === origin ? route.continue() : route.abort());
  const page = await context.newPage();
  page.setDefaultTimeout(5_000);
  page.on('pageerror', error => model.errors.push(error.message));
  await page.clock.install();
  await page.clock.pauseAt(new Date());
  await page.goto(origin);
  await attach(page);
  t.after(async () => { assert.deepEqual(model.errors, [], 'browser page errors'); await context.close(); });
  return { context, page, model };
}
async function attach(page) {
  await page.evaluate(async () => {
    window.session = (await import('/src/lib/session.svelte.ts')).session;
    window.router = (await import('/src/lib/router.svelte.ts')).router;
  });
}
async function connect(page, screen = 'diagnostics') {
  await page.evaluate(async screen => {
    await window.session.connect('tv-a.invalid', 5555);
    window.router.reset('dashboard');
    window.router.navigate(screen);
  }, screen);
  await page.waitForFunction(() => window.session.healthLoaded);
}
const count = (model, command) => model.calls.filter(c => c.command === command).length;
async function snapshot(page) {
  return page.evaluate(() => ({ screen: window.router.current, stack: [...window.router.stack], host: window.session.host,
    serial: window.session.serial, liveness: window.session.liveness, isConnected: window.session.isConnected,
    healthLoaded: window.session.healthLoaded, applyInProgress: window.session.applyInProgress,
    heading: document.querySelector('h1')?.textContent.trim(), storage: { ...localStorage } }));
}
async function visibility(page, value) {
  await page.evaluate(value => {
    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => value });
    document.dispatchEvent(new Event('visibilitychange'));
  }, value);
}
async function record(id, page, model, extra = {}) {
  observations.push({ id, ...extra, state: await snapshot(page), calls: [...model.calls] });
}

for (const hiddenMs of [5_000, 35_000, 50_000]) {
  test(`synthetic hidden ${hiddenMs}ms / visible retains screen and probes once`, async t => {
    const { page, model } = await open(t);
    await connect(page);
    const before = await snapshot(page);
    model.calls.length = 0;
    await visibility(page, 'hidden');
    await page.clock.runFor(hiddenMs);
    assert.equal(count(model, 'wireless_status'), 0);
    await visibility(page, 'visible');
    await page.waitForFunction(() => window.session.liveness === 'live');
    assert.equal(count(model, 'wireless_status'), 1);
    assert.equal(count(model, 'wireless_connect'), 0);
    assert.equal((await snapshot(page)).screen, 'diagnostics');
    assert.equal((await snapshot(page)).serial, before.serial);
    await record(`visibility-${hiddenMs}`, page, model, { hiddenMs, time: 'virtual; no native grace timer runs' });
  });
}

test('synthetic resume with lost socket tries once then shows Retry on prior screen', async t => {
  const { page, model } = await open(t);
  await connect(page);
  model.calls.length = 0;
  model.alive = false;
  model.connectMode = 'fail';
  await visibility(page, 'hidden');
  await visibility(page, 'visible');
  await page.waitForFunction(() => window.session.liveness === 'lost');
  await page.getByRole('button', { name: 'Retry', exact: true }).waitFor();
  await visibility(page, 'visible');
  await page.clock.runFor(90_000);
  assert.equal(count(model, 'wireless_connect'), 1);
  assert.equal((await snapshot(page)).screen, 'diagnostics');
  assert.equal((await snapshot(page)).isConnected, false);
  await record('resume-lost', page, model);
});

test('visible heartbeat waits 45 seconds before probing', async t => {
  const { page, model } = await open(t);
  await connect(page);
  model.calls.length = 0;
  await page.clock.runFor(44_999);
  assert.equal(count(model, 'wireless_status'), 0);
  await page.clock.runFor(1);
  assert.equal(count(model, 'wireless_status'), 1);
  await record('heartbeat-45000', page, model);
});

for (const retainedBackend of [false, true]) {
  test(`reload clears prior screen/cache; native fixture retained=${retainedBackend}`, async t => {
    const { page, model } = await open(t);
    await connect(page);
    const before = await snapshot(page);
    if (!retainedBackend) model.activeHost = '';
    model.connectMode = 'hold';
    model.calls.length = 0;
    await page.reload();
    await attach(page);
    await page.getByRole('heading', { name: 'Connecting to Fixture TV A…' }).waitFor();
    const during = await snapshot(page);
    assert.equal(during.screen, 'onboarding');
    assert.equal(during.serial, '');
    assert.equal(during.isConnected, false);
    assert.equal(during.healthLoaded, false);
    assert.equal(count(model, 'wireless_connect'), 1);
    await record(`reload-${retainedBackend ? 'retained' : 'empty'}-pending`, page, model, { before });
    model.connectMode = 'ok';
    model.pending.splice(0).forEach(resolve => resolve());
    await page.getByRole('heading', { name: 'Connected', exact: true }).waitFor();
    assert.equal((await snapshot(page)).screen, 'onboarding');
    await page.getByRole('button', { name: 'Open dashboard' }).click();
    assert.equal((await snapshot(page)).screen, 'dashboard');
    await record(`reload-${retainedBackend ? 'retained' : 'empty'}-connected`, page, model);
  });
}

test('fresh context with saved storage reconstructs bookkeeping, not a live session', async t => {
  const first = await open(t);
  await connect(first.page);
  const state = await first.context.storageState();
  await first.page.close();
  const { page, model } = await open(t, { state });
  await page.getByRole('heading', { name: 'Connected', exact: true }).waitFor();
  assert.equal(count(model, 'wireless_connect'), 1);
  assert.equal((await snapshot(page)).screen, 'onboarding');
  assert.equal((await snapshot(page)).healthLoaded, false);
  await record('fresh-context-saved-storage', page, model);
});

test('explicit Disconnect survives reload and prevents automatic dial', async t => {
  const { page, model } = await open(t);
  await connect(page);
  await page.evaluate(async () => { await window.session.disconnect(); window.router.reset('onboarding'); });
  assert.equal((await snapshot(page)).storage['atv.autoConnect.v1'], '0');
  model.calls.length = 0;
  await page.reload();
  await attach(page);
  await page.getByRole('heading', { name: 'Which TV?' }).waitFor();
  await page.clock.runFor(90_000);
  assert.equal(count(model, 'wireless_connect'), 0);
  assert.equal((await snapshot(page)).isConnected, false);
  await record('disconnect-reload', page, model);
});

test('two saved TVs wait for choice even when a simulated native connection remains', async t => {
  const { page, model } = await open(t, { rows: [saved(), saved('tv-b.invalid')], auto: '1', activeHost: 'tv-a.invalid' });
  await page.getByRole('heading', { name: 'Which TV?' }).waitFor();
  await page.clock.runFor(90_000);
  assert.equal(count(model, 'wireless_connect'), 0);
  assert.equal((await snapshot(page)).isConnected, false);
  await record('two-saved-native-retained', page, model);
});

test('empty storage starts at Find your TV with no scan or connection', async t => {
  const { page, model } = await open(t);
  await page.getByRole('heading', { name: 'Find your TV' }).waitFor();
  await page.clock.runFor(90_000);
  assert.equal(count(model, 'wireless_connect'), 0);
  assert.equal(count(model, 'wireless_discover'), 0);
  await record('empty-storage', page, model);
});

test('browser Back pops detail and root adds no synthetic spare; native exit untested', async t => {
  const { page, model } = await open(t);
  const baseHistory = await page.evaluate(() => history.length);
  await connect(page);
  assert.equal(await page.evaluate(() => history.length), baseHistory + 1);
  await page.evaluate(() => history.back());
  await page.waitForFunction(() => window.router.current === 'dashboard');
  await record('browser-back-to-root', page, model, { baseHistory, note: 'No root Back/Android exit claim; browser retains forward history.' });
});

test('a completed mock remote key is not replayed after reload', async t => {
  const { page, model } = await open(t);
  await connect(page);
  await page.evaluate(() => window.router.navigate('remote'));
  await page.getByRole('button', { name: 'Volume up', exact: true }).click();
  assert.equal(count(model, 'send_key'), 1);
  await page.reload();
  await attach(page);
  await page.getByRole('heading', { name: 'Connected', exact: true }).waitFor();
  await page.clock.runFor(90_000);
  assert.equal(count(model, 'send_key'), 1);
  await record('no-key-replay', page, model, { limit: 'Completed fixture key only; no claim about interrupted native mutations.' });
});
