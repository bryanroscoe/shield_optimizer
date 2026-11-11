import { freePort } from './lib/free-port.mjs';
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
  server = await createServer({ root, server: { host: '127.0.0.1', port: await freePort(), strictPort: true, hmr: false } });
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
              if (window.castNamed) {
                result.push({ ...devices[0], serial: '192.0.2.26:5555', name: 'SIMULATED Living Room TV', status: 'unauthorized', model: '', device_type: 'unknown', tv_evidence: 'unknown', properties: null });
              }
              if (window.offlineKinds) {
                result.push({ ...devices[0], serial: '192.0.2.87:5555', name: 'SIMULATED Known offline TV', status: 'offline' });
                result.push({ ...devices[0], serial: 'SIMULATED-USB-OFFLINE', name: 'SIMULATED USB offline device', connection: 'usb', status: 'offline', properties: null });
              }
              return result;
            } finally { window.activeReads--; }
          }
          if (command === 'request_device_approval') {
            window.requested = [...(window.requested ?? []), args.serial];
            // What the backend answers when Cancel or Forget beat the TV being asked.
            if (window.cancelBeforeAsked) return { ok: true, status: 'canceled', message: '' };
            window.progress.not_allowed = (window.progress.not_allowed ?? []).filter(address => address !== args.serial);
            window.progress.awaiting_authorization = [...(window.progress.awaiting_authorization ?? []), args.serial];
            return { ok: true, status: 'waiting', message: 'Choose Allow on the TV.' };
          }
          if (command === 'cancel_device_approval') {
            window.canceled = args.serial;
            window.progress.awaiting_authorization = (window.progress.awaiting_authorization ?? []).filter(address => address !== args.serial);
            window.progress.not_allowed = [...(window.progress.not_allowed ?? []), args.serial];
            return null;
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
            window.progress.not_allowed = (window.progress.not_allowed ?? []).filter(address => address !== args.serial);
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
  // The quiet connect found a TV that hasn't allowed this computer: a normal
  // row in the main list, nothing requested, and the request is a button.
  await page.evaluate(() => {
    window.progress.pending = ['192.0.2.4:5555'];
    window.progress.not_allowed = ['[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555'];
  });
  await untrusted.getByText('NOT ALLOWED YET', { exact: true }).waitFor();
  assert.equal(await untrusted.evaluate(el => !!el.closest('.device-list')), true, 'an unapproved TV keeps a normal row');
  await untrusted.getByText("Hasn't allowed this computer yet.", { exact: true }).waitFor();
  assert.equal(await page.getByText(/requested automatically/).count(), 0, 'nothing is requested automatically');
  assert.equal(await untrusted.getByRole('button', { name: 'Retry approval', exact: true }).count(), 0);
  const request = untrusted.getByRole('button', { name: /^Request approval from / });
  assert.equal(await request.getAttribute('aria-label'), 'Request approval from [fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555',
    'each row names its own TV');
  await request.focus();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog');
  await dialog.getByText('tick Always allow from this computer', { exact: false }).waitFor();
  assert.deepEqual(await page.evaluate(() => window.requested), ['[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555']);
  await dialog.locator('summary', { hasText: 'Not seeing the prompt?' }).click();
  assert.match(await dialog.innerText(), /Allow debugging\?/);
  await capture('approval-waiting');
  // Close keeps the request; the row says so and reopens it.
  await page.keyboard.press('Escape');
  await dialog.waitFor({ state: 'detached' });
  await untrusted.getByText('WAITING FOR ALLOW', { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.canceled ?? null), null, 'closing the dialog cancels nothing');
  await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label')?.startsWith('Show the approval request for'));
  assert.equal(await untrusted.locator('[role="status"] button').count(), 0, 'no control inside a live region');
  assert.match(await untrusted.locator('[role="status"]').innerText(), /waiting for Allow/);
  await page.keyboard.press('Enter');
  await dialog.getByRole('button', { name: 'Cancel request' }).click();
  await dialog.waitFor({ state: 'detached' });
  await untrusted.getByRole('button', { name: /^Request approval from / }).waitFor();
  await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label')?.startsWith('Request approval from'),
    null, { timeout: 5000 });
  assert.equal(await page.evaluate(() => window.canceled), '[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555');
  assert.deepEqual(await page.locator('p.connect-message').allInnerTexts(), ['Scanning local network…'],
    'a cancel the person chose adds no message (only the running scan\'s line is there)');
  // Cancel won the race against the TV being asked: still no message, never blame the TV.
  await page.evaluate(() => { window.cancelBeforeAsked = true; });
  await untrusted.getByRole('button', { name: /^Request approval from / }).click();
  await page.waitForFunction(() => window.requested.length === 2);
  await page.getByRole('dialog').waitFor({ state: 'detached' });
  assert.equal(await page.getByText(/ended the request|without allowing|didn't allow/).count(), 0, 'a cancel never blames the TV');
  await page.evaluate(() => { window.cancelBeforeAsked = false; });
  await page.evaluate(() => {
    window.castNamed = true;
    window.progress.not_allowed.push('192.0.2.25:5555', '192.0.2.26:5555');
  });
  const secondApproval = page.locator('[data-serial="192.0.2.25:5555"]');
  const castNamed = page.locator('[data-serial="192.0.2.26:5555"]');
  await secondApproval.getByRole('button', { name: 'Request approval from 192.0.2.25:5555', exact: true }).waitFor();
  await castNamed.getByText('SIMULATED Living Room TV', { exact: true }).waitFor();
  await castNamed.getByText('192.0.2.26:5555', { exact: true }).waitFor();
  assert.equal(await page.locator('.device-list .unapproved').count(), 3,
    'unidentified endpoints remain distinct until hardware identity is verified');
  assert.equal(await page.locator('.approval-guidance').count(), 0);
  assert.equal(await page.locator('.unapproved .unauthorized-help').count(), 0,
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
  await page.evaluate(() => { document.documentElement.style.fontSize = '125%'; });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true, 'larger text still fits the minimum width');
  await capture('compact-approval-rows-narrow-large-text');
  await page.evaluate(() => { document.documentElement.style.fontSize = ''; });
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.setViewportSize({ width: 1100, height: 760 });
  await page.evaluate(() => {
    window.castNamed = false;
    window.progress.pending = [];
    window.progress.not_allowed = ['[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555'];
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
  await page.locator('[data-serial="192.168.1.77:5555"]').getByText('WAITING FOR ALLOW', { exact: true }).waitFor();
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
  await page.evaluate(() => { window.progress.not_allowed = [...window.progress.pending]; window.progress.pending = []; });
  await disconnected.getByText('NOT ALLOWED YET', { exact: true }).waitFor();
  await disconnected.getByRole('button', { name: /^Request approval from / }).waitFor();
  assert.equal(await disconnected.evaluate(el => !!el.closest('.device-list')), true, 'a quiet retry that was not allowed returns to the main list');
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
  console.log('PASS no automatic approval request, explicit Request approval and Cancel with per-row names, focus kept on the swapped control and a live region without controls, a cancel never blaming the TV, endpoint-only retry/Forget, unprofiled offline route recovery without extra device cards, known offline and USB preservation, ready rows during scan, serialized refresh, no repeated metadata, backend failure and unmount cleanup');
} finally { await browser?.close(); await server?.close(); }
