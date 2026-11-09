import assert from 'node:assert/strict';
import { mkdir, readFile } from 'node:fs/promises';
import { stripTypeScriptTypes } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const evidence = process.env.NATIVE_CONNECTION_CHOICES_EVIDENCE;
const source = stripTypeScriptTypes(await readFile(resolve(root, 'src/lib/native-discovery-rows.ts'), 'utf8'));
const { buildNativeDiscoveryRows, discoveryAddressUsable } = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
const ipv4 = '192.0.2.42:5555';
const ipv6 = '[fe80::42%7]:5555';
const advert = (address, extras = {}) => ({
  service_kind: 'legacy_adb', instance: 'Fixture SHIELD._adb._tcp.local.',
  address: address.includes('[') ? 'fe80::42' : '192.0.2.42', port: 5555,
  interface_name: 'en0', interface_index: 7, scope_id: address.includes('[') ? 7 : null,
  connect_address: address, observed_age_ms: 0, cached: false, warning: null, ...extras,
});
const report = { observation_max_age_ms: 30_000, warnings: [], advertisements: [
  advert(ipv4), advert(ipv6),
  advert('[fe80::42%8]:5555', { interface_name: 'en1', interface_index: 8, scope_id: 8 }),
  advert('192.0.2.99:5555', { instance: 'Old TV._adb._tcp.local.', address: '192.0.2.99', cached: true }),
  advert('192.0.2.42:40000', { service_kind: 'tls_pairing', instance: 'Fixture SHIELD._adb-tls-pairing._tcp.local.', port: 40000 }),
] };
const rows = buildNativeDiscoveryRows(report.advertisements);
assert.equal(rows.length, 4, 'group one service/interface; do not fuse different interfaces or pairing');
assert.deepEqual(rows[0].addresses.map(row => row.connect_address), [ipv4, ipv6]);
assert.equal(buildNativeDiscoveryRows([advert(ipv4, { interface_index: null }), advert(ipv6, { interface_index: null })]).length, 2);
assert.equal(discoveryAddressUsable(advert(ipv4), 30_000, 30_000), false);
assert.equal(discoveryAddressUsable(advert(ipv4, { cached: true }), 0, 30_000), false);
assert.equal(discoveryAddressUsable(advert(ipv6, { scope_id: null }), 0, 30_000), false);
assert.equal(discoveryAddressUsable(advert(ipv4, { observed_age_ms: 29_500 }), 500, 30_000), false);

process.env.VITE_DEMO = '1';
let server, browser, context;
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, hmr: false } });
  await server.listen();
  browser = await chromium.launch();
  context = await browser.newContext({ viewport: { width: 1100, height: 850 }, colorScheme: 'dark', ...(evidence ? { recordVideo: { dir: `${evidence}/raw` } } : {}) });
  await context.addInitScript(({ report, ipv4 }) => {
    let bridge;
    window.connectCalls = [];
    window.scanCalls = 0;
    window.listedAt = null;
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true, get: () => bridge,
      set(value) {
        const original = value.invoke.bind(value);
        value.invoke = async (command, args = {}) => {
          if (command === 'adb_status') return { backend: 'direct', available: true, path: null, connections: { pending: [], failed: [] } };
          if (command === 'list_devices') {
            const devices = await original(command, args);
            return window.listedAt ? [{ ...devices[0], serial: window.listedAt, name: 'Fixture Shield' }] : [];
          }
          if (command === 'scan_network') {
            window.scanCalls++;
            return { subnet: '192.0.2', found: [ipv4], connected: [], pending: [], unauthorized: [], failed: [ipv4], needs_pairing: [], details: [`${ipv4}: SIMULATED route unavailable.`], message: 'SIMULATED: one route failed; alternate advertised routes retained.', native_discovery: structuredClone(report) };
          }
          if (command === 'connect_device') {
            window.connectCalls.push(args.address);
            return new Promise(resolve => { window.finishConnection = () => { window.listedAt = args.address; resolve({ ok: true, message: 'SIMULATED selected address connected.' }); }; });
          }
          return original(command, args);
        };
        bridge = value;
      },
    });
  }, { report, ipv4 });
  const page = await context.newPage();
  await page.clock.install();
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
  await page.getByText('SIMULATED: one route failed', { exact: false }).waitFor();
  await page.getByRole('button', { name: 'Scan LAN', exact: true }).click();
  const details = page.locator('details').filter({ has: page.locator('summary', { hasText: 'Scan details' }) });
  assert.equal(await page.getByRole('region', { name: 'Connection addresses' }).count(), 0, 'picker starts collapsed');
  await details.locator('summary').click();
  const picker = page.getByRole('region', { name: 'Connection addresses' });
  await picker.waitFor();
  assert.equal(await picker.locator('li').count(), 3, 'pairing ports are not offered as connection addresses');
  const first = picker.locator('li').first();
  const selector = first.getByRole('combobox');
  assert.equal(await selector.locator('option').count(), 2);
  assert.equal(await selector.inputValue(), ipv4);
  await selector.selectOption(ipv6);
  assert.deepEqual(await page.evaluate(() => window.connectCalls), [], 'selecting an address does not auto-connect');
  assert.equal(await picker.locator('li').last().getByRole('button', { name: /Connect using/ }).isDisabled(), true, 'cached routes are disabled');
  await first.getByRole('button', { name: `Connect using ${ipv6}`, exact: true }).click();
  await page.waitForFunction(() => !!window.finishConnection);
  assert.equal(await selector.isDisabled(), true, 'address cannot change while the selected connection is pending');
  assert.deepEqual(await page.evaluate(() => window.connectCalls), [ipv6]);
  await page.evaluate(() => window.finishConnection());
  await page.getByText('Fixture Shield', { exact: true }).waitFor();
  await page.waitForFunction(() => !document.querySelector('.connection-choices select').disabled);
  assert.equal(await page.locator('.device-list [data-serial]').count(), 1, 'the chosen route is one row');
  assert.equal(await picker.locator('li').count(), 3, 'the service keeps its other addresses');
  assert.equal(await picker.locator(`option[value="${ipv6}"]`).count(), 0, 'exactly the listed address is not offered again');
  assert.deepEqual(await first.getByRole('combobox').locator('option').allInnerTexts(), [`${ipv4} (IPv4)`]);
  if (evidence) await page.screenshot({ path: `${evidence}/grouped-addresses-dark-simulated.png`, animations: 'disabled' });
  await page.emulateMedia({ colorScheme: 'light' });
  await page.setViewportSize({ width: 720, height: 900 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
  if (evidence) await page.screenshot({ path: `${evidence}/grouped-addresses-narrow-light-simulated.png`, animations: 'disabled' });
  const remaining = picker.locator('li').nth(1);
  await page.clock.fastForward(31_000);
  await picker.getByText('Scan LAN again to connect using these addresses.', { exact: true }).waitFor();
  assert.equal(await picker.getByText(/Expired/).count(), 0, 'rows never read as expired one by one');
  assert.equal(await remaining.getByRole('button', { name: /Connect using/ }).isDisabled(), true, 'old observations must not be dialed');
  assert.deepEqual(await page.evaluate(() => window.connectCalls), [ipv6]);
  await page.getByRole('button', { name: 'Scan LAN', exact: true }).click();
  await details.locator('summary').click();
  await picker.waitFor();
  await page.waitForFunction(() => !document.querySelector('.connection-choices button').disabled);
  assert.equal(await page.evaluate(() => window.scanCalls), 3, 'Scan LAN refreshes address choices through the existing scan (after the boot scan)');
  assert.deepEqual(await page.evaluate(() => window.connectCalls), [ipv6], 'refresh must not implicitly reuse a chosen endpoint');
  const recording = page.video();
  await context.close();
  if (evidence) await recording.saveAs(`${evidence}/grouped-address-choices-simulated.webm`);
  console.log('PASS grouped IP/mDNS address choices, interface and pairing isolation, exact explicit selection, exactly the listed endpoint not offered again while its siblings stay, cached/stale/scope guards without per-row expiry, and refresh');
} finally {
  await context?.close();
  await browser?.close();
  await server?.close();
}
