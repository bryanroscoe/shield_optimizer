import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const evidence = process.env.NATIVE_DISCOVERY_EVIDENCE;
process.env.VITE_DEMO = '1';
let server, browser;
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, hmr: false } });
  await server.listen();
  browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1100, height: 760 }, ...(evidence ? { recordVideo: { dir: `${evidence}/raw` } } : {}) });
  await context.addInitScript(() => {
    let bridge;
    window.nativeCalls = [];
    window.nativeReady = false;
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true,
      get: () => bridge,
      set(value) {
        const original = value.invoke.bind(value);
        value.invoke = async (command, args = {}) => {
          window.nativeCalls.push({ command, args });
          if (command === 'adb_status') return { backend: 'direct', available: true, path: null, last_probe: '' };
          if (command === 'list_devices') return window.nativeReady ? (await original(command, args)).filter(d => d.connection === 'network') : [];
          if (command === 'scan_network') return { subnet: '192.0.2', found: ['192.0.2.71:5555'], connected: [], unauthorized: [], failed: ['192.0.2.71:5555'], needs_pairing: [], message: 'SIMULATED: Found 1 TV. Approve debugging on the TV, then use Add by IP.', details: ['SIMULATED endpoint 192.0.2.71:5555: authorization timed out.'] };
          if (command === 'connect_device') return new Promise(resolve => { window.finishNativeConnect = ok => { window.nativeReady = ok; resolve({ ok, message: ok ? 'Connected.' : 'SIMULATED approval timeout. Try again after approving the TV.' }); }; });
          if (command === 'pair_device') return { ok: true, message: 'Paired. Enter the separate connect port.', instance: null };
          if (['connect_paired', 'probe_paired_connect', 'install_adb', 'restart_adb', 'local_address_for'].includes(command)) throw new Error(`Unexpected host-server command: ${command}`);
          return original(command, args);
        };
        bridge = value;
      },
    });
  });
  const page = await context.newPage();
  const capture = async name => {
    if (!evidence) return;
    await page.screenshot({ path: `${evidence}/${name}-simulated.png` });
    await page.waitForTimeout(1800);
  };
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
  await page.getByTestId('adb-backend').filter({ hasText: 'Native Rust ADB' }).waitFor();
  await page.getByText('SIMULATED: Found 1 TV.', { exact: false }).waitFor();
  const scanDetails = page.locator('details').filter({ has: page.locator('summary', { hasText: 'Scan details' }) });
  assert.equal(await scanDetails.locator('pre').isVisible(), false);
  await scanDetails.locator('summary').click();
  assert.equal(await scanDetails.locator('pre').isVisible(), true);
  await scanDetails.locator('summary').click();
  assert.equal(await page.getByRole('button', { name: 'Restart ADB', exact: true }).count(), 0);
  assert.equal(await page.getByText('Stop ADB when quitting', { exact: true }).count(), 0);
  const input = page.getByPlaceholder('IP[:port] — e.g. 192.168.42.71');
  await input.fill('192.0.2.71');
  await page.getByRole('button', { name: 'Add by IP', exact: true }).click();
  await page.getByText('Connecting… Look for Allow debugging', { exact: false }).waitFor();
  await page.waitForFunction(() => !!window.finishNativeConnect);
  await input.press("Enter");
  assert.equal(await page.evaluate(() => window.nativeCalls.filter(c => c.command === "connect_device").length), 1);
  await capture("native-approval");
  await page.evaluate(() => window.finishNativeConnect(false));
  await page.getByRole('button', { name: 'Add by IP', exact: true }).waitFor();
  assert.equal(await input.inputValue(), '192.0.2.71');
  await page.getByRole('button', { name: 'Add by IP', exact: true }).click();
  await page.evaluate(() => window.finishNativeConnect(true));
  await page.getByText('NVIDIA SHIELD Android TV', { exact: true }).first().waitFor();
  await capture("native-devices");
  await page.getByRole('button', { name: 'Pair PIN', exact: true }).first().click();
  await page.getByPlaceholder(/IP:pair_port/).fill('192.0.2.72:37123');
  await page.getByPlaceholder('6-digit PIN').fill('123456');
  await page.getByRole('button', { name: 'Pair', exact: true }).click();
  await page.waitForFunction(() => window.nativeCalls.some(c => c.command === 'pair_device'));
  assert.equal(await input.inputValue(), '192.0.2.72:');
  assert.equal(await page.getByRole('button', { name: 'Add by IP', exact: true }).isDisabled(), true);
  await input.fill('192.0.2.72:41234');
  assert.equal(await page.getByRole('button', { name: 'Add by IP', exact: true }).isEnabled(), true);
  const forbidden = await page.evaluate(() => window.nativeCalls.filter(c => ['connect_paired', 'probe_paired_connect', 'install_adb', 'restart_adb', 'local_address_for'].includes(c.command)));
  assert.deepEqual(forbidden, []);
  if (evidence) {
    await capture("native-pairing");
    const video = page.video();
    await context.close();
    await video.saveAs(`${evidence}/native-existing-flow-simulated.webm`);
  } else await context.close();
  console.log('PASS native backend label, existing scan flow, pending approval, retry, and separate pairing/connect ports');
} finally {
  await browser?.close();
  await server?.close();
}
