import assert from 'node:assert/strict';
import { readFile, mkdir, writeFile, realpath } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const report = JSON.parse(await readFile(resolve(root, 'tests/fixtures/native-discovery.json'), 'utf8'));
const result = process.env.NATIVE_DISCOVERY_RUST_RESULT
  ? JSON.parse(await readFile(process.env.NATIVE_DISCOVERY_RUST_RESULT, 'utf8'))
  : { subnet: null, found: [], connected: [], unauthorized: [], failed: [], needs_pairing: [], message: 'Discovery complete. No connections changed.', native_discovery: report };
assert.deepEqual(result.native_discovery, report, 'browser fixtures must match the Rust serialization contract');
const evidence = process.env.NATIVE_DISCOVERY_EVIDENCE;
process.env.VITE_DEMO = '1';
delete process.env.TAURI_DEV_HOST;
let server, browser;
const forbidden = ['connect_paired', 'probe_paired_connect', 'install_adb', 'restart_adb', 'local_address_for'];
const pauses = ms => evidence ? new Promise(resolve => setTimeout(resolve, ms)) : Promise.resolve();

try {
  if (evidence) await mkdir(evidence, { recursive: true });
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, hmr: false, fs: { allow: [root, await realpath(resolve(root, 'node_modules'))] } } });
  await server.listen();
  const url = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();

  async function fresh(scenario = 'native', record = false) {
    const context = await browser.newContext({ viewport: { width: 1360, height: 1000 }, ...(record && evidence ? { recordVideo: { dir: `${evidence}/raw`, size: { width: 1360, height: 1000 } } } : {}) });
    await context.addInitScript(({ scenario, result }) => {
      let bridge;
      window.discoveryMode = scenario;
      window.discoveryCalls = [];
      Object.defineProperty(window, '__TAURI_INTERNALS__', {
        configurable: true,
        get: () => bridge,
        set(value) {
          const original = value.invoke.bind(value);
          value.invoke = async (command, args = {}) => {
            window.discoveryCalls.push({ command, args });
            if (command === 'adb_status' && scenario === 'unknown') throw new Error('SIMULATED status unavailable.');
            if (command === 'adb_status' && scenario === 'unavailable-unknown') return { available: false, path: null, last_probe: 'SIMULATED unavailable backend.' };
            if (command === 'adb_status') return { backend: scenario === 'missing-backend' ? undefined : scenario === 'legacy' ? 'subprocess' : 'direct', available: true, path: null, last_probe: 'SIMULATED' };
            if (command === 'list_devices') return [];
            if (command === 'scan_network') {
              if (window.discoveryMode === 'failure') throw new Error('SIMULATED multicast unavailable; enter an address manually.');
              if (scenario === 'legacy') return { subnet: null, found: [], connected: [], unauthorized: [], failed: [], needs_pairing: ['198.51.100.77:39876'], message: 'SIMULATED legacy scan' };
              const response = structuredClone(result);
              response.needs_pairing = ['192.0.2.99:1111'];
              if (scenario === 'empty') response.native_discovery.advertisements = [];
              if (scenario === 'held') return new Promise(resolve => { window.releaseNativeScan = () => resolve(response); });
              if (scenario === 'invalid') {
                response.native_discovery.advertisements[0].observed_age_ms = 30_001;
                response.native_discovery.advertisements[5].connect_address = '198.51.100.8:41234';
              }
              if (scenario === 'ipv6') {
                Object.assign(response.native_discovery.advertisements[1], { address: 'fe80::9999', connect_address: '[fe80::9999%7]:37111', scope_id: 7, interface_name: 'en7', interface_index: 7 });
              }
              return response;
            }
            if (command === 'pair_device') return { ok: true, message: 'SIMULATION: paired response; connect manually.', instance: 'not-a-physical-identity' };
            if (command === 'connect_device') return { ok: false, message: 'SIMULATION: manual connection requested; no TV or authorization was attempted.' };
            if (scenario === 'legacy' && command === 'local_address_for') return '198.51.100.1';
            if (['connect_paired', 'probe_paired_connect', 'install_adb', 'restart_adb', 'local_address_for'].includes(command)) throw new Error(`Unexpected automatic native command: ${command}`);
            return original(command, args);
          };
          bridge = value;
        },
      });
    }, { scenario, result });
    const page = await context.newPage();
    await page.goto(url, { waitUntil: 'networkidle' });
    if (scenario === 'unknown') await page.locator('.error').filter({ hasText: 'SIMULATED status unavailable.' }).waitFor();
    else if (scenario === 'unavailable-unknown') await page.locator('.error').filter({ hasText: 'SIMULATED unavailable backend.' }).waitFor();
    else await page.getByRole('heading', { name: 'No devices connected.', exact: true }).waitFor();
    await page.evaluate(() => {
      const label = document.createElement('div');
      label.textContent = 'SIMULATION — injected discovery; no multicast daemon, ADB or TV';
      label.style.cssText = 'position:fixed;bottom:0;right:0;z-index:99999;background:#17243a;color:white;padding:8px;font:13px sans-serif';
      document.body.append(label);
    });
    return { context, page };
  }

  const count = async (page, command) => page.evaluate(command => window.discoveryCalls.filter(call => call.command === command).length, command);
  const connectBox = page => page.getByPlaceholder('IP[:port] — e.g. 192.168.42.71');
  const add = page => page.locator('.connect-form').getByRole('button', { name: 'Add by IP', exact: true });
  const choose = (page, address, pairing = false) => page.getByRole('button', { name: `${pairing ? 'Use pairing address' : 'Use address'} ${address}`, exact: true });
  async function scan(page) {
    await page.getByRole('button', { name: 'Scan LAN', exact: true }).click();
    await page.locator('.native-discovery').waitFor();
  }
  async function noImplicitCommands(page) {
    for (const command of forbidden) assert.equal(await count(page, command), 0, command);
  }
  async function callsReceipt(page, name) {
    if (!evidence) return;
    const calls = await page.evaluate(() => window.discoveryCalls.map(call => ({ ...call, args: { ...call.args, ...(call.args.pin ? { pin: '[simulated PIN omitted]' } : {}) } })));
    await writeFile(`${evidence}/${name}-calls.json`, JSON.stringify({ simulation: true, calls }, null, 2));
  }

  {
    const { context, page } = await fresh('native', true);
    assert.equal(await count(page, 'scan_network'), 0, 'native boot does not browse');
    assert.equal(await count(page, 'connect_device'), 0);
    await pauses(1000);
    if (evidence) await page.screenshot({ path: `${evidence}/before-native-scan-simulated.png` });
    await scan(page);
    assert.equal(await page.locator('.native-discovery li').count(), report.advertisements.length, 'same IP/instance does not merge advertisements into a physical TV');
    assert.equal(await page.locator('.pair-form').count(), 0, 'native scan ignores first legacy pairing field');
    assert.equal(await connectBox(page).inputValue(), '');
    assert.equal(await count(page, 'pair_device'), 0);
    await noImplicitCommands(page);
    await pauses(2000);
    if (evidence) await page.screenshot({ path: `${evidence}/native-services-simulated.png` });
    await choose(page, '192.0.2.11:5555').click();
    assert.equal(await connectBox(page).inputValue(), '192.0.2.11:5555');
    assert.equal(await connectBox(page).evaluate(element => element === document.activeElement), true);
    assert.equal(await count(page, 'connect_device'), 0, 'Use address only prefills');
    await pauses(1000);
    await choose(page, '192.0.2.11:37111', true).click();
    const pairingBox = page.getByPlaceholder('IP:pair_port — e.g. 192.168.42.71:43219');
    assert.equal(await pairingBox.inputValue(), '192.0.2.11:37111');
    assert.equal(await pairingBox.evaluate(element => element === document.activeElement), true);
    assert.equal(await count(page, 'pair_device'), 0, 'Use pairing address does not pair');
    await pauses(1500);
    if (evidence) await page.screenshot({ path: `${evidence}/pairing-prefill-simulated.png` });
    await page.getByPlaceholder('6-digit PIN').fill('123456');
    await page.locator('.pair-form').getByRole('button', { name: 'Pair', exact: true }).click();
    await page.getByText('SIMULATION: paired response; connect manually.', { exact: true }).waitFor();
    assert.equal(await connectBox(page).inputValue(), '192.0.2.11:');
    assert.equal(await add(page).isDisabled(), true);
    await connectBox(page).fill('192.0.2.11');
    await connectBox(page).press('Enter');
    assert.equal(await count(page, 'connect_device'), 0, 'post-pair host-only entry cannot guess5555');
    await noImplicitCommands(page);
    await pauses(1000);
    if (evidence) await page.screenshot({ path: `${evidence}/post-pair-manual-port-simulated.png` });
    await choose(page, '192.0.2.11:37222').click();
    assert.equal(await connectBox(page).inputValue(), '192.0.2.11:37222');
    await pauses(1500);
    if (evidence) await page.screenshot({ path: `${evidence}/connect-prefill-simulated.png` });
    await add(page).click();
    await page.getByText('SIMULATION: manual connection requested; no TV or authorization was attempted.', { exact: true }).waitFor();
    assert.equal(await count(page, 'connect_device'), 1);
    assert.equal(await page.evaluate(() => window.discoveryCalls.find(call => call.command === 'connect_device').args.address), '192.0.2.11:37222');
    await pauses(1000);
    await callsReceipt(page, 'manual-flow');
    if (evidence) {
      const video = page.video();
      await context.close();
      await video.saveAs(`${evidence}/native-discovery-manual-flow-simulated.webm`);
    } else await context.close();
  }

  {
    const { context, page } = await fresh('invalid');
    await scan(page);
    const expired = choose(page, '192.0.2.11:5555');
    const cached = choose(page, '198.51.100.8:41234');
    assert.equal(await expired.isDisabled(), true);
    assert.equal(await cached.isDisabled(), true);
    await expired.dispatchEvent('click');
    await cached.dispatchEvent('click');
    assert.equal(await connectBox(page).inputValue(), '');
    const scope = page.locator('.native-discovery li').filter({ hasText: 'IPv6 scope is unavailable' });
    assert.equal(await scope.getByRole('button').isDisabled(), true);
    assert.ok(await scope.getByText('IPv6 scope is unavailable.', { exact: false }).isVisible());
    assert.equal(await count(page, 'connect_device'), 0);
    if (evidence) { await scope.scrollIntoViewIfNeeded(); await page.screenshot({ path: `${evidence}/cached-expired-scope-simulated.png` }); }
    await callsReceipt(page, 'invalid');
    await context.close();
  }

  {
    const { context, page } = await fresh('ipv6');
    await scan(page);
    await choose(page, '[fe80::1234%7]:37333').click();
    assert.equal(await connectBox(page).inputValue(), '[fe80::1234%7]:37333');
    await choose(page, '[fe80::9999%7]:37111', true).click();
    await page.getByPlaceholder('6-digit PIN').fill('123456');
    await page.locator('.pair-form').getByRole('button', { name: 'Pair', exact: true }).click();
    await page.getByText('SIMULATION: paired response; connect manually.', { exact: true }).waitFor();
    assert.equal(await connectBox(page).inputValue(), '[fe80::9999%7]:');
    await connectBox(page).fill('[fe80::9999%7]');
    await connectBox(page).press('Enter');
    assert.equal(await add(page).isDisabled(), true);
    assert.equal(await count(page, 'connect_device'), 0);
    await noImplicitCommands(page);
    const summary = page.locator('.native-discovery .discovery-help summary');
    await summary.focus(); await page.keyboard.press('Enter');
    assert.equal(await summary.evaluate(element => element.parentElement.open), true);
    await page.keyboard.press('Escape');
    assert.equal(await summary.evaluate(element => element.parentElement.open), false);
    assert.equal(await summary.evaluate(element => element === document.activeElement), true);
    await summary.click(); await summary.click();
    await page.setViewportSize({ width: 800, height: 1000 });
    await summary.click();
    assert.ok((await summary.boundingBox()).width <= 800);
    if (evidence) await page.screenshot({ path: `${evidence}/scoped-help-narrow-simulated.png` });
    await context.close();
  }

  {
    const { context, page } = await fresh('native');
    await page.clock.install();
    await scan(page);
    assert.equal(await choose(page, '192.0.2.11:5555').isEnabled(), true);
    await page.clock.fastForward(31_000);
    assert.equal(await choose(page, '192.0.2.11:5555').isDisabled(), true);
    await choose(page, '192.0.2.11:5555').dispatchEvent('click');
    assert.equal(await connectBox(page).inputValue(), '');
    await context.close();
  }

  {
    const { context, page } = await fresh('held');
    const button = page.getByRole('button', { name: 'Scan LAN', exact: true });
    await button.click();
    await page.waitForFunction(() => typeof window.releaseNativeScan === 'function');
    await page.getByRole('button', { name: 'Scanning…', exact: true }).dispatchEvent('click');
    assert.equal(await count(page, 'scan_network'), 1, 'repeated scan cannot start a parallel window');
    await page.evaluate(() => window.releaseNativeScan());
    await page.locator('.native-discovery').waitFor();
    await context.close();
  }

  {
    const { context, page } = await fresh('native');
    await scan(page);
    await page.evaluate(() => { window.discoveryMode = 'failure'; });
    await page.getByRole('button', { name: 'Scan LAN', exact: true }).click();
    await page.getByText('Error: SIMULATED multicast unavailable; enter an address manually.', { exact: true }).waitFor();
    assert.equal(await page.locator('.native-discovery').count(), 0, 'failed refresh does not retain selectable old rows');
    assert.equal(await page.locator('.connect-form').getByRole('button', { name: 'Pair PIN', exact: true }).isEnabled(), true);
    await connectBox(page).fill('198.51.100.9:5555');
    assert.equal(await add(page).isEnabled(), true);
    assert.equal(await count(page, 'connect_device'), 0);
    await noImplicitCommands(page);
    if (evidence) await page.screenshot({ path: `${evidence}/provider-failure-manual-simulated.png` });
    await context.close();
  }

  {
    const { context, page } = await fresh('empty');
    await scan(page);
    assert.ok(await page.getByText("No ADB advertisements observed. You can still enter the TV's IP and port above.", { exact: true }).isVisible());
    await context.close();
  }

  for (const scenario of ['unknown', 'missing-backend', 'unavailable-unknown']) {
    const { context, page } = await fresh(scenario);
    assert.equal(await count(page, 'scan_network'), 0, 'unknown backend must not auto-browse');
    assert.equal(await page.getByRole('button', { name: 'Download platform-tools', exact: true }).count(), 0);
    assert.equal(await connectBox(page).isEnabled(), true);
    if (scenario !== 'missing-backend') assert.ok(await page.getByText('connection status unavailable', { exact: true }).isVisible());
    assert.equal(await count(page, 'connect_device'), 0);
    await page.locator('.connect-form').getByRole('button', { name: 'Pair PIN', exact: true }).click();
    await page.getByPlaceholder('IP:pair_port — e.g. 192.168.42.71:43219').fill('192.0.2.11:37111');
    await page.getByPlaceholder('6-digit PIN').fill('123456');
    await page.locator('.pair-form').getByRole('button', { name: 'Pair', exact: true }).click();
    await page.getByText('SIMULATION: paired response; connect manually.', { exact: true }).waitFor();
    await connectBox(page).fill('192.0.2.11');
    await connectBox(page).press('Enter');
    assert.equal(await add(page).isDisabled(), true);
    assert.equal(await count(page, 'connect_device'), 0);
    await noImplicitCommands(page);
    await callsReceipt(page, scenario);
    await context.close();
  }

  {
    const { context, page } = await fresh('legacy');
    await page.getByPlaceholder('IP:pair_port — e.g. 192.168.42.71:43219').waitFor();
    assert.equal(await count(page, 'scan_network'), 1, 'legacy boot scan is preserved');
    assert.equal(await page.getByPlaceholder('IP:pair_port — e.g. 192.168.42.71:43219').inputValue(), '198.51.100.77:39876');
    await context.close();
  }
  console.log('Native discovery UI passed: explicit observation-only Scan, separate ports, no physical merges, manual prefill/Pair/Connect, no guessed5555, scoped IPv6, cached/expired/ambiguous refusal, provider failure fallback, repeated Scan, keyboard/narrow layout and legacy boot behavior.');
} finally {
  await browser?.close();
  await server?.close();
}
