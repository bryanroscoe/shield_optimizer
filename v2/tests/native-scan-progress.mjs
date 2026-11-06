import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const evidence = process.env.NATIVE_SCAN_PROGRESS_EVIDENCE;
process.env.VITE_DEMO = '1';
let server, browser;
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, strictPort: false, hmr: false } });
  await server.listen();
  browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1100, height: 760 }, colorScheme: 'dark', ...(evidence ? { recordVideo: { dir: `${evidence}/raw` } } : {}) });
  await context.addInitScript(() => {
    let bridge;
    Object.assign(window, { calls: [], progress: { pending: [], failed: [] }, ready: false, secondReady: false, offlineRoute: false, offlineKinds: false, available: true, holdStatus: false, activeReads: 0, peakReads: 0 });
    Object.defineProperty(window, '__TAURI_INTERNALS__', {
      configurable: true, get: () => bridge,
      set(value) {
        const original = value.invoke.bind(value);
        value.invoke = async (command, args = {}) => {
          window.calls.push(command);
          if (command === 'adb_status' || command === 'list_devices') {
            window.activeReads++;
            window.peakReads = Math.max(window.peakReads, window.activeReads);
            try {
              if (command === 'adb_status') {
                if (window.holdStatus) await new Promise(resolve => { window.releaseStatus = resolve; });
                return { backend: 'direct', available: window.available, path: null, last_probe: 'SIMULATED unavailable backend', connections: structuredClone(window.progress) };
              }
              await new Promise(resolve => setTimeout(resolve, 40));
              const devices = (await original(command, args)).filter(d => d.connection === 'network');
              const result = window.ready ? devices.slice(0, window.secondReady ? 2 : 1) : [];
              if (window.offlineRoute) {
                const address = '[fe80::abcd:ef01:2345:6789%14]:5555';
                result.push({ ...devices[0], serial: address, name: address, status: 'offline', model: '', device_type: 'unknown', tv_evidence: 'unknown', properties: null });
              }
              if (window.offlineKinds) {
                result.push({ ...devices[0], serial: '192.0.2.87:5555', name: 'SIMULATED Known offline TV', status: 'offline' });
                result.push({ ...devices[0], serial: 'SIMULATED-USB-OFFLINE', name: 'SIMULATED USB offline device', connection: 'usb', status: 'offline', properties: null });
              }
              return result;
            } finally { window.activeReads--; }
          }
          if (command === 'reprompt_device') {
            window.reprompted = args.serial;
            window.progress.failed = window.progress.failed.filter(f => f.address !== args.serial);
            window.progress.pending.push(args.serial);
            return { ok: true, status: 'none', message: 'SIMULATED reconnect started.' };
          }
          if (command === 'forget_device') {
            window.forgotten = args.serial;
            window.progress.pending = window.progress.pending.filter(address => address !== args.serial);
            window.progress.awaiting_authorization = (window.progress.awaiting_authorization ?? []).filter(address => address !== args.serial);
            if (args.serial === '[fe80::abcd:ef01:2345:6789%14]:5555') window.offlineRoute = false;
            return { ok: true, disconnected: [args.serial], still_advertised: false, message: 'SIMULATED forgotten.' };
          }
          if (command === 'scan_network') return new Promise(resolve => {
            window.releaseScan = () => resolve({ found: ['192.168.1.42:5555', '192.168.1.77:5555'], connected: ['192.168.1.42:5555'], pending: ['192.168.1.77:5555'], failed: [], unauthorized: [], needs_pairing: [], details: [], message: 'SIMULATED discovery finished; one handshake pending.' });
          });
          if (['connect_device', 'restart_adb', 'install_adb'].includes(command)) throw new Error(`Unexpected retry: ${command}`);
          return original(command, args);
        };
        bridge = value;
      },
    });
  });
  const page = await context.newPage();
  const capture = async name => {
    if (evidence) {
      await page.screenshot({ path: `${evidence}/${name}-simulated.png`, animations: 'disabled' });
      await page.waitForTimeout(1200);
    }
  };
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
  await page.waitForFunction(() => !!window.releaseScan);
  await page.evaluate(() => { window.progress.pending = ['[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555', '192.0.2.4:5555']; });
  const untrusted = page.locator('[data-serial="[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555"]');
  const failed = page.locator('[data-serial="192.0.2.4:5555"]');
  const scanDetails = page.locator('details').filter({ has: page.locator('summary', { hasText: 'Scan details' }) });
  await scanDetails.locator('summary').waitFor();
  assert.equal(await untrusted.isVisible(), false, 'unconfirmed connection attempts stay behind Scan details');
  await scanDetails.locator('summary').click();
  await untrusted.getByText('Connecting…', { exact: true }).waitFor();
  assert.equal(await page.getByRole('heading', { name: 'No devices connected.' }).count(), 0);
  assert.equal(await page.getByText('Online', { exact: true }).count(), 0, 'unapproved discoveries must not appear online');
  await page.evaluate(() => { window.progress.awaiting_authorization = ['[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555']; });
  await untrusted.getByText('Approval requested', { exact: true }).waitFor();
  assert.equal(await untrusted.evaluate(el => !!el.closest('.device-list')), true, 'observed AUTH stays in the main list');
  await untrusted.getByRole('button', { name: 'Retry approval', exact: true }).waitFor();
  assert.match(await page.locator('.approval-guidance').innerText(), /Approval was requested automatically/,
    'the first request must be explicit before the user retries');
  assert.doesNotMatch(await page.locator('.approval-guidance').innerText(), /on the TV|TV shows/,
    'an unprofiled endpoint might be a phone, tablet or TV');
  await page.evaluate(() => {
    window.progress.pending.push('192.0.2.25:5555');
    window.progress.awaiting_authorization.push('192.0.2.25:5555');
  });
  const secondApproval = page.locator('[data-serial="192.0.2.25:5555"]');
  await secondApproval.getByRole('button', { name: 'Retry approval', exact: true }).waitFor();
  assert.equal(await page.locator('.device-list .native-approval').count(), 2,
    'unidentified endpoints remain distinct until hardware identity is verified');
  assert.equal(await page.locator('.approval-guidance').count(), 1,
    'approval instructions appear once for the whole list');
  assert.equal(await page.locator('.native-approval .unauthorized-help').count(), 0,
    'each address must not repeat a full authorization guide');
  for (const row of [untrusted, secondApproval]) {
    const address = await row.getAttribute('data-serial');
    assert.equal((await row.innerText()).split(address).length - 1, 1,
      'a raw address appears once, rather than as both a name and metadata');
    assert.equal(await row.getByRole('button', { name: 'Forget', exact: true }).count(), 1);
    assert.equal(await row.locator('a').count(), 0, 'an unidentified endpoint must not open TV tools');
  }
  await page.setViewportSize({ width: 720, height: 900 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
  await capture('compact-approval-rows-narrow');
  await page.emulateMedia({ colorScheme: 'light' });
  await capture('compact-approval-rows-narrow-light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.setViewportSize({ width: 1100, height: 760 });
  await page.evaluate(() => {
    window.progress.pending = ['[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555'];
    window.progress.awaiting_authorization = [...window.progress.pending];
    window.progress.failed = [{ address: '192.0.2.4:5555', message: 'SIMULATED connection timeout; check the TV and try again.' }];
  });
  await failed.getByText("Didn't connect", { exact: true }).waitFor();
  await page.setViewportSize({ width: 720, height: 900 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true, 'discovered IPv6 endpoints and actions must fit the minimum window width');
  await capture('unapproved-narrow');
  await page.setViewportSize({ width: 1100, height: 760 });
  await capture('unapproved-and-failed-visible');
  await failed.getByRole('button', { name: 'Try again', exact: true }).click();
  await failed.getByText('Connecting…', { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.reprompted), '192.0.2.4:5555');
  await failed.getByRole('button', { name: 'Forget', exact: true }).click();
  await failed.waitFor({ state: 'detached' });
  assert.equal(await page.evaluate(() => window.forgotten), '192.0.2.4:5555');
  await page.evaluate(() => { window.ready = true; window.progress = { pending: ['192.168.1.77:5555'], awaiting_authorization: ['192.168.1.77:5555'], failed: [] }; });
  await page.locator('[data-serial="192.168.1.77:5555"]').getByText('Approval requested', { exact: true }).waitFor();
  await page.getByText('NVIDIA SHIELD Android TV', { exact: true }).first().waitFor();
  await page.waitForFunction(() => window.activeReads === 0);
  assert.equal(await page.getByRole('button', { name: 'Scanning…', exact: true }).count(), 1);
  await capture('ready-during-scan');
  const lists = await page.evaluate(() => window.calls.filter(c => c === 'list_devices').length);
  await page.waitForTimeout(2200);
  assert.equal(await page.evaluate(() => window.calls.filter(c => c === 'list_devices').length), lists, 'unchanged pending status must not repeatedly query TV properties');

  await page.evaluate(() => window.releaseScan());
  await page.getByRole('button', { name: 'Scan LAN', exact: true }).waitFor();
  await page.getByRole('status').filter({ hasText: 'Still connecting to 1' }).waitFor();
  await capture('scan-finished-approval-continues');
  await page.evaluate(() => { window.holdStatus = true; window.releaseStatus = null; });
  await page.waitForFunction(() => !!window.releaseStatus);
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await page.evaluate(() => { window.holdStatus = false; window.releaseStatus(); });
  await page.waitForFunction(() => window.activeReads === 0);
  assert.equal(await page.evaluate(() => window.peakReads), 1, 'manual and background refresh must remain serialized');

  await page.evaluate(() => { window.secondReady = true; window.progress.pending = []; window.progress.awaiting_authorization = []; window.progress.ready = ['192.168.1.42:5555', '192.168.1.77:5555']; });
  await page.getByText('Living Room Box', { exact: true }).waitFor();
  assert.equal(await page.getByRole('status').filter({ hasText: 'Still connecting' }).count(), 0);
  assert.equal(await page.locator('[data-serial="192.168.1.77:5555"]').count(), 1, 'approval replaces the discovered row without duplication');
  await capture('late-approval-adds-existing-card');
  const completed = await page.evaluate(() => window.calls.filter(c => c === 'adb_status').length);
  await page.waitForTimeout(2200);
  assert.equal(await page.evaluate(() => window.calls.filter(c => c === 'adb_status').length), completed, 'polling stops after terminal progress');

  await page.evaluate(() => {
    window.progress.failed = [
      { address: '[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555', message: 'SIMULATED authorization timed out.' },
      { address: '[fe80::abcd:ef01:2345:6789%14]:40435', message: 'SIMULATED advertisement expired while queued.' },
      { address: '192.0.2.4:5555', message: 'SIMULATED connection refused.' },
    ];
  });
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await scanDetails.locator('summary').filter({ hasText: '3 other connection attempts' }).waitFor();
  assert.equal(await page.locator('.device-list [data-serial]').count(), 2, 'failed addresses are not presented as extra devices');
  assert.match(await page.locator('.header-sub').innerText(), /^2 connected/);
  assert.equal(await failed.isVisible(), false, 'new failures default to collapsed diagnostics');
  assert.equal(await page.locator('.device-list').getByText('NVIDIA SHIELD is the device this app is built and tested on.', { exact: true }).count(), 0);
  await capture('ready-with-failures-collapsed-dark');
  await page.emulateMedia({ colorScheme: 'light' });
  await capture('ready-with-failures-collapsed-light');
  await page.emulateMedia({ colorScheme: 'dark' });
  await scanDetails.locator('summary').click();
  await failed.getByRole('button', { name: 'Try again', exact: true }).waitFor();
  await page.setViewportSize({ width: 720, height: 900 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true, 'expanded scan attempts fit narrow windows');
  await capture('scan-details-narrow');
  await page.setViewportSize({ width: 1100, height: 760 });
  await scanDetails.locator('summary').click();

  const previousRetries = await page.evaluate(() => window.calls.filter(c => c === 'reprompt_device').length);
  await page.evaluate(() => { window.progress.failed = []; window.offlineRoute = true; });
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await scanDetails.locator('summary').filter({ hasText: '1 other connection attempt' }).waitFor();
  const disconnected = page.locator('[data-serial="[fe80::abcd:ef01:2345:6789%14]:5555"]');
  assert.equal(await page.locator('.device-list [data-serial]').count(), 2,
    'an offline endpoint absent from scan progress is not a third physical device');
  assert.equal(await disconnected.isVisible(), false, 'unprofiled offline routes stay collapsed');
  assert.equal(await page.evaluate(() => window.calls.filter(c => c === 'reprompt_device').length), previousRetries,
    'showing an offline route must not reconnect automatically');
  await capture('disconnected-route-collapsed');
  await scanDetails.locator('summary').click();
  await disconnected.getByText('Disconnected', { exact: true }).waitFor();
  await disconnected.getByRole('button', { name: 'Retry connection', exact: true }).click();
  await disconnected.getByText('Connecting…', { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.reprompted), '[fe80::abcd:ef01:2345:6789%14]:5555');
  await page.evaluate(() => { window.progress.awaiting_authorization = [...window.progress.pending]; });
  await disconnected.getByText('Approval requested', { exact: true }).waitFor();
  await disconnected.getByRole('button', { name: 'Retry approval', exact: true }).waitFor();
  assert.equal(await page.locator('.device-list .clickable').count(), 2,
    'retry and approval retain both existing ready TV cards');
  await disconnected.getByRole('button', { name: 'Forget', exact: true }).click();
  await disconnected.waitFor({ state: 'detached' });
  assert.equal(await page.evaluate(() => window.forgotten), '[fe80::abcd:ef01:2345:6789%14]:5555');

  await page.evaluate(() => { window.offlineKinds = true; });
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await page.locator('.device-list').getByText('SIMULATED Known offline TV', { exact: true }).waitFor();
  await page.locator('.device-list').getByText('SIMULATED USB offline device', { exact: true }).waitFor();
  assert.equal(await page.locator('.device-list [data-serial]').count(), 4,
    'profiled offline devices and USB devices retain their normal cards');
  await page.evaluate(() => { window.offlineKinds = false; });

  await page.evaluate(() => { window.progress.pending = ['[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555']; });
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await page.getByRole('status').filter({ hasText: 'Still connecting' }).waitFor();
  await page.evaluate(() => { window.available = false; });
  await page.getByRole('alert').filter({ hasText: 'SIMULATED unavailable backend' }).waitFor();
  assert.equal(await scanDetails.count(), 0, 'unavailable backend must not expose stale scan actions');
  const unavailable = await page.evaluate(() => window.calls.filter(c => c === 'adb_status').length);
  await page.waitForTimeout(2200);
  assert.equal(await page.evaluate(() => window.calls.filter(c => c === 'adb_status').length), unavailable, 'unavailable backend stops polling');

  await page.evaluate(() => { window.available = true; });
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await page.getByRole('status').filter({ hasText: 'Still connecting' }).waitFor();
  await page.getByRole('link', { name: 'Snapshots', exact: true }).click();
  await page.waitForURL('**/snapshots');
  const departed = await page.evaluate(() => window.calls.filter(c => c === 'adb_status').length);
  await page.waitForTimeout(2200);
  assert.equal(await page.evaluate(() => window.calls.filter(c => c === 'adb_status').length), departed, 'leaving Devices stops polling');
  assert.equal(await page.evaluate(() => window.calls.some(c => ['connect_device', 'restart_adb', 'install_adb'].includes(c))), false);
  if (evidence) {
    const video = page.video();
    await context.close();
    await video.saveAs(`${evidence}/native-scan-progress-simulated.webm`);
  } else await context.close();
  console.log('PASS explicit first approval request, endpoint-only retry/Forget, unprofiled offline route recovery without extra device cards, known offline and USB preservation, ready rows during scan, serialized refresh, no repeated metadata, backend failure and unmount cleanup');
} finally { await browser?.close(); await server?.close(); }
