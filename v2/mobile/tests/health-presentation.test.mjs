import assert from 'node:assert/strict';
import { before, after, test } from 'node:test';
import { mkdir, readFile } from 'node:fs/promises';
import { chromium } from 'playwright';
import { startViteServer } from './helpers/vite-harness.mjs';
let server, origin, browser, present;
before(async () => { ({ server, origin } = await startViteServer()); present = await server.ssrLoadModule('/src/lib/healthPresentation.ts'); browser = await chromium.launch({ headless: true }); });
after(async () => { await browser?.close(); await server?.close(); });

const shieldMeminfo = await readFile(new URL('../../crates/core/tests/fixtures/devices/shield-tv-pro/dumpsys-meminfo.txt', import.meta.url), 'utf8');
const fixtureMb = label => {
  const match = shieldMeminfo.match(new RegExp(`^\\s*${label} RAM: *([0-9,]+)K`, 'm'));
  assert.ok(match, `${label} fixture field`); return Math.floor(Number(match[1].replaceAll(',', '')) / 1024);
};
const shieldRam = { total_mb: fixtureMb('Total'), used_mb: fixtureMb('Used'), free_mb: fixtureMb('Free') };

const reading = (celsius, extras = {}) => ({ celsius, source: 'hardware_properties', sensor_name: 'CPU temperatures[1]', sensor_type: 'CPU', status: null, throttling_c: 89, shutdown_c: 102.5, ...extras });
const health = (extra = {}) => ({ ram: { total_mb: 4096, used_mb: 3072, free_mb: 1024 }, storage: { total: '10G', used: '2.5G', available: '7.5G', used_percent: 25 }, display: {}, audio_device: null, top_memory: [], temperature_c: 30, temperature_reading: reading(30), thermal_status: null, ...extra });

test('RAM uses explicit used/total or available complement; zero/full are real values', () => {
  assert.equal(present.memoryUsage({ total_mb: 100, used_mb: 0, free_mb: 100 }).percent, 0);
  assert.equal(present.memoryUsage({ total_mb: 100, used_mb: 100, free_mb: 0 }).percent, 100);
  assert.equal(present.memoryUsage({ total_mb: 100, free_mb: 25 }).percent, 75);
  assert.equal(present.memoryUsage({ total_mb: 100, used_mb: 60 }).percent, 60);
});
test('missing and invalid memory is not painted as zero/full', () => {
  for (const ram of [null, {}, { total_mb: 0, used_mb: 0 }, { total_mb: 100, used_mb: -1 }, { total_mb: 100, used_mb: 101 }, { total_mb: 100, used_mb: NaN }, { total_mb: 100, free_mb: Infinity }, { total_mb: 100, used_mb: NaN, free_mb: 50 }]) assert.equal(present.memoryUsage(ram), null);
});
test('storage retains reported percentages and distinguishes missing/invalid from zero', () => {
  for (const p of [0, 25, 50, 100]) assert.equal(present.storageUsage({ used_percent: p }), p);
  for (const p of [null, undefined, NaN, Infinity, -1, 101]) assert.equal(present.storageUsage({ used_percent: p }), null);
});
test('temperature comparison uses only a matching sensor own threshold', () => {
  const low = present.temperatureContext(health());
  assert.match(low.sensor, /CPU reading 2/); assert.equal(low.comparison, '59°C below reported sensor threshold');
  assert.equal(present.temperatureContext(health({ temperature_c: 89, temperature_reading: reading(89) })).comparison, 'At reported sensor threshold');
  assert.equal(present.temperatureContext(health({ temperature_c: 100, temperature_reading: reading(100) })).comparison, '11°C above reported sensor threshold');
  const legacy = present.temperatureContext({ temperature_c: 100 });
  assert.equal(legacy.sensor, 'Sensor not reported'); assert.equal(legacy.status, null); assert.equal(legacy.throttle, null);
  const mismatched = present.temperatureContext({ temperature_c: 30, temperature_reading: reading(100) });
  assert.equal(mismatched.throttle, null); assert.equal(mismatched.sensor, 'Sensor not reported');
  const cached = present.temperatureContext(health({ temperature_c: 80, temperature_reading: reading(80, { source: 'thermalservice_cached', status: 4 }) }));
  assert.equal(cached.statusScope, 'Cached sensor status'); assert.match(cached.statusHelp, /may not reflect current/);
});
test('reported device status is independent of hottest sensor status; no temperature-derived grade', () => {
  for (let status = 0; status <= 6; status++) {
    const ctx = present.temperatureContext(health({ thermal_status: status })); assert.equal(ctx.status, status); assert.equal(ctx.statusScope, 'Device status');
  }
  const ctx = present.temperatureContext(health({ thermal_status: 4, temperature_reading: reading(30, { status: 0 }) }));
  assert.equal(ctx.statusLabel, 'Critical thermal status');
  const reverse = present.temperatureContext(health({ thermal_status: 0, temperature_reading: reading(30, { status: 4 }) }));
  assert.equal(reverse.status, 0); assert.equal(reverse.sensorStatusLabel, 'Critical thermal status');
  assert.equal(present.temperatureContext(health({ temperature_c: 100, thermal_status: null, temperature_reading: reading(100, { status: 1 }) })).status, 1);
  for (const status of [-1, 7, NaN, 0.5]) assert.equal(present.temperatureContext(health({ thermal_status: status })).status, null);
});

async function open(t, record = false, evidenceDir = "evidence/metric-context-2026-10-17") {
  const context = await browser.newContext({ viewport: { width: 384, height: 812 }, ...(record ? { recordVideo: { dir: `${evidenceDir}/raw`, size: { width: 384, height: 812 } } } : {}) });
  const page = await context.newPage(); page.setDefaultTimeout(10000); t.after(() => context.close());
  await page.addInitScript(initial => {
    localStorage.clear(); window.mockHealth = initial;
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      switch(command) {
        case 'get_entitlement': return 'pro';
        case 'wireless_connect': return { ok: true };
        case 'wireless_status': return { connected: true };
        case 'list_devices': return [{ id: 1, serial: 'MOCK:5555', name: 'Synthetic TV', model: 'TV', status: 'device', connection: 'network', device_type: 'shield', properties: null }];
        case 'health_report': return mockHealth;
        case 'app_list_for_device': case 'list_installed_packages': return [];
        case 'package_states': return {};
        default: return { ok: true };
      }
    } };
  }, health());
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import('/src/lib/session.svelte.ts'); const { router } = await import('/src/lib/router.svelte.ts');
    window.session = session; await session.connect('MOCK', 5555); router.reset('dashboard'); router.navigate('diagnostics');
  });
  await page.getByRole('region', { name: 'RAM usage', exact: true }).waitFor(); return page;
}
async function show(page, value) { await page.evaluate(async value => { window.mockHealth = value; await session.loadHealth(true); }, value); }

test('both visible bars mean used, share neutral styling and show 0/50/100 honestly', async t => {
  const page = await open(t);
  for (const percent of [0, 50, 100]) {
    await show(page, health({ ram: { total_mb: 100, used_mb: percent, free_mb: 100 - percent }, storage: { total: '100G', used: `${percent}G`, available: `${100-percent}G`, used_percent: percent } }));
    const ram = page.getByRole('progressbar', { name: 'RAM used', exact: true }), disk = page.getByRole('progressbar', { name: 'Storage used', exact: true });
    assert.equal(await ram.getAttribute('aria-valuenow'), String(percent)); assert.equal(await disk.getAttribute('aria-valuenow'), String(percent));
    assert.equal(await ram.locator('.usage-fill').evaluate(el => el.style.width), `${percent}%`);
    assert.equal(await ram.locator('.usage-fill').evaluate(el => getComputedStyle(el).backgroundColor), await disk.locator('.usage-fill').evaluate(el => getComputedStyle(el).backgroundColor));
  }
  await show(page, health({ ram: {}, storage: {}, temperature_c: null, temperature_reading: null }));
  assert.equal(await page.getByRole('progressbar').count(), 0);
  await page.getByText('Temperature unavailable', { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
});
test('legacy100C stays unknown; a device-reported critical status is shown even at30C', async t => {
  const page = await open(t);
  await show(page, health({ temperature_c: 100, temperature_reading: null, thermal_status: null }));
  await page.getByText('Sensor not reported', { exact: true }).waitFor();
  assert.equal(await page.locator('.thermal-critical').count(), 0);
  await show(page, health({ thermal_status: 4 }));
  await page.getByText('Device status: Critical thermal status', { exact: true }).waitFor();
  assert.equal(await page.locator('.thermal-critical').count(), 1);
  await page.getByText('30 °C', { exact: true }).waitFor();
  await show(page, health({ thermal_status: 0, temperature_reading: reading(30, { status: 4 }) }));
  await page.getByText('Selected sensor: Critical thermal status', { exact: true }).waitFor();
  await page.getByText('Device status: No throttling reported', { exact: true }).waitFor();
  await show(page, health({ temperature_c: null, temperature_reading: null, thermal_status: 4 }));
  await page.getByText('Temperature unavailable', { exact: true }).waitFor();
  await page.getByText('Device status: Critical thermal status', { exact: true }).waitFor();
  await show(page, health({ temperature_c: 80, thermal_status: 0, temperature_reading: reading(80, { source: 'thermalservice_current', sensor_name: 'cpu0', status: 4, throttling_c: null, shutdown_c: null }) }));
  await page.getByText('Selected sensor: Critical thermal status', { exact: true }).waitFor();
  await page.getByText('80 °C', { exact: true }).waitFor();
  await show(page, health({ temperature_c: 80, thermal_status: null, temperature_reading: reading(80, { source: 'thermalservice_cached', sensor_name: 'cpu0', status: 4, throttling_c: null, shutdown_c: null }) }));
  await page.getByText('Cached sensor status: Critical thermal status', { exact: true }).waitFor();
  await page.getByText('No current sensor reading was supplied; showing a cached value.', { exact: true }).waitFor();
});

if (process.env.METRICS_EVIDENCE === '1') test('capture synthetic capacity and sensor-aware thermal boundaries', async t => {
  const dir = 'evidence/metric-context-2026-10-17'; await mkdir(dir, { recursive: true }); const page = await open(t, true);
  await page.screenshot({ path: `${dir}/usage-and-30c-simulated.png`, fullPage: true }); await page.waitForTimeout(700);
  await show(page, health({ temperature_c: 100, temperature_reading: reading(100) }));
  await page.getByRole('region', { name: 'Temperature context' }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: `${dir}/100c-reported-limit-simulated.png` }); await page.waitForTimeout(700);
  await show(page, health({ temperature_c: 100, temperature_reading: null, thermal_status: null }));
  await page.screenshot({ path: `${dir}/100c-unknown-context-simulated.png` }); await page.waitForTimeout(700);
  await show(page, health({ thermal_status: 4 }));
  await page.screenshot({ path: `${dir}/30c-device-critical-simulated.png` }); await page.waitForTimeout(700);
  await show(page, health({ temperature_c: 80, thermal_status: 0, temperature_reading: reading(80, { source: 'thermalservice_current', sensor_name: 'cpu0', status: 4, throttling_c: null, shutdown_c: null }) }));
  await page.screenshot({ path: `${dir}/current-hal-80c-critical-simulated.png` }); await page.waitForTimeout(700);
  await show(page, health({ temperature_c: 80, thermal_status: null, temperature_reading: reading(80, { source: 'thermalservice_cached', sensor_name: 'cpu0', status: 4, throttling_c: null, shutdown_c: null }) }));
  await page.screenshot({ path: `${dir}/cached-only-80c-simulated.png` }); await page.waitForTimeout(700);
  await show(page, health({ ram: {}, storage: {}, temperature_c: null, temperature_reading: null, thermal_status: null }));
  await page.evaluate(() => scrollTo(0,0)); await page.screenshot({ path: `${dir}/missing-readings-simulated.png`, fullPage: true }); await page.waitForTimeout(700);
  const video = page.video(); await contextClose(page); await video.saveAs(`${dir}/metric-boundaries-simulated.webm`);
});
async function contextClose(page) { await page.context().close(); }

test('below-threshold light/moderate readings do not describe throttling onset or infer HAL severity', () => {
  for (const status of [1, 2]) {
    const ctx = present.temperatureContext(health({ temperature_c: 70, thermal_status: status, temperature_reading: reading(70) }));
    assert.equal(ctx.comparison, '19°C below reported sensor threshold');
    assert.equal(ctx.status, status);
  }
});

test('threshold provenance stays unspecified for hardware, HIDL-like and unknown sources', () => {
  for (const source of ['hardware_properties', 'legacy_hidl', 'unknown', 'thermalservice_current']) {
    const ctx = present.temperatureContext(health({ temperature_c: 70, temperature_reading: reading(70, { source }) }));
    assert.equal(ctx.status, null);
    assert.equal(ctx.comparison, '19°C below reported sensor threshold');
    assert.match(ctx.thresholdHelp, /may already be active below/);
    assert.match(ctx.thresholdHelp, /does not identify this threshold's severity/);
    assert.equal(ctx.throttle, 89);
    assert.equal(ctx.shutdown, 102.5);
  }
  assert.equal(present.temperatureContext(health({ temperature_reading: null })).thresholdHelp, null);
});

test('visible thresholds retain reported status and never claim onset or a severity without provenance', async t => {
  const page = await open(t); await page.setViewportSize({ width: 320, height: 812 });
  const region = page.getByRole('region', { name: 'Temperature context' });
  for (const [status, label] of [[1, 'Light throttling'], [2, 'Moderate throttling']]) {
    await show(page, health({ temperature_c: 70, thermal_status: status, temperature_reading: reading(70) }));
    await region.getByText(`Device status: ${label}`, { exact: true }).waitFor();
    await region.getByText('Reported sensor threshold: 89 °C', { exact: true }).waitFor();
    await region.getByText('19°C below reported sensor threshold', { exact: true }).waitFor();
    await region.getByText(/Throttling may already be active below this value/).waitFor();
    assert.equal(await region.getByText(/Reported throttle point|severe.throttling threshold/i).count(), 0);
  }
  await show(page, health({ temperature_c: 70, temperature_reading: reading(70, { source: 'unknown' }) }));
  await region.getByText('Thermal status not reported', { exact: true }).waitFor();
  assert.equal(await region.getByText('CPU/GPU hardware report', { exact: true }).count(), 0);
  assert.equal(await region.getByText(/No throttling reported|Severe throttling/).count(), 0);
  await show(page, health({ temperature_c: 100, thermal_status: 4, temperature_reading: reading(100) }));
  await region.getByText('Device status: Critical thermal status', { exact: true }).waitFor();
  await region.getByText('11°C above reported sensor threshold', { exact: true }).waitFor();
  await show(page, health({ temperature_c: 70, temperature_reading: reading(70, { throttling_c: null }) }));
  await region.getByText('Matching sensor threshold not reported', { exact: true }).waitFor();
  assert.equal(await region.getByText(/Throttling may already be active below/).count(), 0);
  assert.equal(await region.evaluate(el => el.scrollWidth <= el.clientWidth), true);
});

if (process.env.THERMAL_WORDING_EVIDENCE === '1') test('capture conservative threshold wording with independent statuses', async t => {
  const dir = 'evidence/thermal-threshold-wording-2026-10-17'; await mkdir(dir, { recursive: true });
  const page = await open(t, true, dir);
  const region = page.getByRole('region', { name: 'Temperature context' });
  for (const [name, status, source] of [['light-below-threshold', 1, 'hardware_properties'], ['moderate-below-threshold', 2, 'hardware_properties'], ['unknown-provenance', null, 'unknown']]) {
    await show(page, health({ temperature_c: 70, thermal_status: status, temperature_reading: reading(70, { source }) }));
    await region.scrollIntoViewIfNeeded();
    await region.screenshot({ path: `${dir}/${name}-simulated.png`, animations: 'disabled' });
    await page.waitForTimeout(900);
  }
  const video = page.video(); await page.context().close(); await video.saveAs(`${dir}/threshold-statuses-simulated.webm`);
});

test('checked-in Shield meminfo keeps explicit Used despite non-complementary Free accounting', () => {
  assert.deepEqual(shieldRam, { total_mb: 2946, used_mb: 1711, free_mb: 1384 });
  assert.ok(shieldRam.used_mb + shieldRam.free_mb > shieldRam.total_mb);
  assert.deepEqual(present.memoryUsage(shieldRam), { used: 1711, total: 2946, free: 1384, percent: 58 });
});

test('Shield accounting renders explicit used/free without changing the source report', async t => {
  const evidence = process.env.RAM_ACCOUNTING_EVIDENCE === '1';
  const dir = 'evidence/ram-accounting-2026-10-17';
  if (evidence) await mkdir(dir, { recursive: true });
  const page = await open(t, evidence, dir);
  await show(page, health({ ram: shieldRam }));
  assert.equal(await page.getByRole('progressbar', { name: 'RAM used', exact: true }).getAttribute('aria-valuenow'), '58');
  await page.getByText('1711 / 2946 MB used', { exact: true }).waitFor();
  await page.getByText('1384 MB reported free/available', { exact: true }).waitFor();
  assert.deepEqual(await page.evaluate(() => window.mockHealth.ram), shieldRam);
  if (evidence) { await page.screenshot({ path: `${dir}/shield-ram.png`, fullPage: true }); await page.waitForTimeout(900); }
  await page.evaluate(() => document.documentElement.style.fontSize = '24px');
  await page.getByText('1711 / 2946 MB used', { exact: true }).waitFor();
  if (evidence) { await page.screenshot({ path: `${dir}/shield-ram-large-text.png`, fullPage: true }); await page.waitForTimeout(900); }
  await show(page, health({ ram: {} }));
  assert.equal(await page.getByRole('progressbar', { name: 'RAM used', exact: true }).count(), 0);
  if (evidence) await page.waitForTimeout(900);
});
