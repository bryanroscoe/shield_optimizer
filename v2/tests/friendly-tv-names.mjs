import assert from 'node:assert/strict';
import { mkdir, readFile } from 'node:fs/promises';
import { stripTypeScriptTypes } from 'node:module';
import { createServer } from 'vite';
import { chromium, webkit } from 'playwright';

// Synthetic endpoints/names only. This test never invokes real discovery or ADB.
const evidence = process.env.FRIENDLY_TV_EVIDENCE;
const advertisement = (kind, name, port, extras = {}) => ({
  source: kind, instance: `${name}.${kind === 'google_cast' ? '_googlecast' : '_androidtvremote2'}._tcp.local.`,
  friendly_name: name, model: kind === 'google_cast' ? 'Fixture TV' : null,
  address: '192.0.2.42', interface_name: 'fixture0', interface_index: 7,
  scope_id: null, setup_host: '192.0.2.42', observed_age_ms: 0, cached: false, warning: null, ...extras,
});
const source = stripTypeScriptTypes(await readFile(new URL('../src/lib/tv-discovery-rows.ts', import.meta.url), 'utf8'));
const { buildTvDiscoveryRows, observationFresh, discoverySetupHost, selectedObservation } = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
const cast = advertisement('google_cast', 'Bedroom TV', 8009);
const remote = advertisement('android_tv_remote', 'Bedroom TV', 6466);
assert.equal(buildTvDiscoveryRows([cast, remote]).length, 2, 'protocols never establish a shared TV identity');
assert.equal(buildTvDiscoveryRows([cast, { ...cast, address: 'fe80::42', setup_host: '[fe80::42%7]', scope_id: 7 }]).length, 1, 'only same service and interface may group addresses');
assert.equal(buildTvDiscoveryRows([cast, { ...cast, interface_index: 8 }]).length, 2);
assert.equal(buildTvDiscoveryRows([{ ...cast, interface_index: null }, { ...cast, interface_index: null }]).length, 2);
assert.equal(buildTvDiscoveryRows([{ ...cast, friendly_name: 'TV\u202e spoof' }])[0].name, 'Nearby device');
assert.equal(observationFresh(cast, 30_000, 30_000), false);
assert.equal(observationFresh({ ...cast, cached: true }, 0, 30_000), false);
assert.equal(discoverySetupHost({ ...cast, setup_host: null }), null);
const multi = buildTvDiscoveryRows([cast, { ...cast, address: '192.0.2.43', setup_host: '192.0.2.43' }])[0];
assert.equal(selectedObservation(multi, undefined, 0, 30_000), null, 'multiple usable hosts require explicit choice');
assert.equal(selectedObservation(multi, '192.0.2.43', 0, 30_000)?.address, '192.0.2.43');
assert.equal(buildTvDiscoveryRows([cast, { ...cast, instance: 'other._googlecast._tcp.local.', address: '192.0.2.99', setup_host: '192.0.2.99' }]).length, 2, 'same friendly name is never identity');
const report = { observations: [advertisement('google_cast', 'Bedroom TV', 8009), advertisement('google_cast', 'Bedroom TV', 8009, { address: '192.0.2.43', setup_host: '192.0.2.43' }), advertisement('android_tv_remote', 'Bedroom TV', 6466)], warnings: [], observation_max_age_ms: 30_000 };
process.env.VITE_DEMO = '1';
let server;
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  server = await createServer({ server: { host: '127.0.0.1', port: 0, hmr: false } });
  await server.listen();
  for (const engine of [chromium, webkit]) {
    const browser = await engine.launch();
    try {
      const context = await browser.newContext({ viewport: { width: 1100, height: 900 }, colorScheme: 'dark', ...(evidence ? { recordVideo: { dir: `${evidence}/raw` } } : {}) });
      await context.addInitScript(({ report }) => {
        let bridge;
        window.friendlyCalls = [];
        Object.defineProperty(window, '__TAURI_INTERNALS__', {
          configurable: true, get: () => bridge,
          set(value) {
            const original = value.invoke.bind(value);
            value.invoke = async (command, args = {}) => {
              window.friendlyCalls.push({ command, args });
              if (command === 'get_debug_logging') return true;
              if (command === 'adb_status') return { backend: 'subprocess', available: true, path: '/synthetic/adb' };
              if (command === 'discover_tv_names') return structuredClone(report);
              if (command === 'scan_network') return { subnet: null, found: [], connected: [], unauthorized: [], failed: [], needs_pairing: [], message: 'SIMULATED: no IPv4 gateway; names remain available.' };
              if (['connect_device', 'pair_device', 'restart_adb', 'install_adb'].includes(command)) throw new Error(`Unexpected device action ${command}`);
              return original(command, args);
            };
            bridge = value;
          },
        });
      }, { report });
      const page = await context.newPage();
      await page.clock.install();
      await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
      const nearby = page.getByRole('region', { name: 'Nearby TV names' });
      await nearby.waitFor();
      if (evidence) await page.evaluate(() => {
        const badge = document.createElement('div');
        badge.textContent = 'Synthetic data · no device connection';
        badge.style.cssText = 'position:fixed;bottom:8px;left:8px;z-index:9999;background:#fff3bd;color:#322600;padding:4px 8px;border:1px solid #a88b2d;border-radius:4px;font:12px sans-serif;pointer-events:none';
        document.body.append(badge);
      });
      const observations = nearby.locator('li[data-connection-service]');
      assert.equal(await observations.count(), 2, 'protocol observations stay separate');
      assert.deepEqual(await observations.locator('strong').allTextContents(), ['Bedroom TV', 'Bedroom TV'], 'friendly name is the only primary title');
      assert.equal(await observations.first().getByText('Google Cast', { exact: true }).isVisible(), true);
      const help = nearby.getByRole('button', { name: 'About Nearby TV names', exact: true });
      assert.equal(await help.getAttribute('aria-expanded'), 'false');
      assert.equal(await nearby.getByText('Names advertised on this network.', { exact: false }).isVisible(), false);
      assert.equal(await nearby.getByText('Bedroom TV · Google Cast: fixture0', { exact: true }).isVisible(), false);
      await help.focus();
      await page.keyboard.press('Enter');
      assert.equal(await help.getAttribute('aria-expanded'), 'true');
      assert.equal(await nearby.getByText('Bedroom TV · Google Cast: fixture0', { exact: true }).isVisible(), true);
      await page.keyboard.press('Escape');
      assert.equal(await help.getAttribute('aria-expanded'), 'false');
      assert.equal(await help.evaluate(element => document.activeElement === element), true);
      assert.equal(await nearby.getByRole('button', { name: /Connect using/ }).count(), 0);
      assert.equal(await page.evaluate(() => window.friendlyCalls.filter(c => c.command === 'discover_tv_names').length), 1, 'boot reads names even with connected devices');
      const setup = nearby.getByRole('button', { name: 'Enter ADB details for Bedroom TV', exact: true }).first();
      assert.equal(await setup.isDisabled(), true, 'multi-address setup starts disabled');
      const addressChoice = nearby.getByRole('combobox', { name: 'Address for Bedroom TV via Google Cast on fixture0' });
      assert.equal(await addressChoice.inputValue(), '');
      await addressChoice.selectOption('192.0.2.43');
      assert.equal(await page.evaluate(() => window.friendlyCalls.some(c => ['connect_device', 'pair_device'].includes(c.command))), false, 'choosing a host does not connect');
      await setup.click();
      const manual = page.getByPlaceholder('IP[:port] — e.g. 192.168.42.71');
      assert.equal(await manual.inputValue(), '192.0.2.43:');
      await manual.press('Enter');
      assert.deepEqual(await page.evaluate(() => window.friendlyCalls.filter(c => c.command === 'log_ui_event' && /Bedroom TV|192\.0\.2\.43/.test(JSON.stringify(c.args)))), [], 'advertised names and selected addresses never enter UI breadcrumbs');
      assert.equal(await page.locator('.connect-form').getByRole('button', { name: 'Add by IP', exact: true }).isDisabled(), true);
      assert.equal(await page.evaluate(() => window.friendlyCalls.some(c => ['connect_device', 'pair_device'].includes(c.command))), false);
      if (evidence) { await page.screenshot({ path: `${evidence}/${engine.name()}-desktop-friendly-dark-synthetic.png`, animations: 'disabled' }); await new Promise(resolve => setTimeout(resolve, 1_000)); }
      await page.emulateMedia({ colorScheme: 'light' });
      await page.setViewportSize({ width: 720, height: 1000 });
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
      await page.evaluate(() => { document.documentElement.style.fontSize = '20px'; });
      await help.click();
      assert.equal(await nearby.getByText('Bedroom TV · Google Cast: fixture0', { exact: true }).isVisible(), true);
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true, 'expanded help fits with larger text');
      if (evidence) {
        await page.screenshot({ path: `${evidence}/${engine.name()}-desktop-friendly-help-light-large-synthetic.png`, animations: 'disabled' });
        await new Promise(resolve => setTimeout(resolve, 1_000));
      }
      await help.click();
      assert.equal(await nearby.evaluate(element => element.scrollWidth <= element.clientWidth && element.getBoundingClientRect().right <= window.innerWidth), true, 'observation panel fits with larger text');
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true, 'whole screen fits with larger text');
      if (evidence) { await page.screenshot({ path: `${evidence}/${engine.name()}-desktop-friendly-light-large-synthetic.png`, animations: 'disabled' }); await new Promise(resolve => setTimeout(resolve, 1_000)); }
      await page.clock.fastForward(31_000);
      assert.equal(await nearby.getByRole('button', { name: /Enter ADB details/ }).first().isDisabled(), true);
      await page.getByRole('button', { name: 'Scan LAN', exact: true }).click();
      await page.getByText('SIMULATED: no IPv4 gateway; names remain available.').waitFor();
      assert.equal(await nearby.getByRole('button', { name: /Enter ADB details/ }).first().isDisabled(), true, 'new scan requires choosing among multiple hosts again');
      assert.equal(await page.evaluate(() => window.friendlyCalls.filter(c => c.command === 'discover_tv_names').length), 2, 'Scan LAN refreshes independent names with no gateway');
      const video = page.video();
      await context.close();
      if (evidence) await video.saveAs(`${evidence}/${engine.name()}-desktop-friendly-synthetic.webm`);
      console.log(`PASS ${engine.name()}: default subprocess boot names, separate protocol observations, no metadata connect, explicit ADB port, expiry/refresh, narrow large-text layout`);
    } finally { await browser.close(); }
  }
} finally { await server?.close(); }
