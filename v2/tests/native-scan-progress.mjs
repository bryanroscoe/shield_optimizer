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
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, hmr: false } });
  await server.listen();
  browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1100, height: 760 }, colorScheme: 'dark', ...(evidence ? { recordVideo: { dir: `${evidence}/raw` } } : {}) });
  await context.addInitScript(() => {
    let bridge;
    Object.assign(window, { calls: [], progress: { pending: [], failed: [] }, ready: false, secondReady: false, available: true, holdStatus: false, activeReads: 0, peakReads: 0 });
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
              return window.ready ? devices.slice(0, window.secondReady ? 2 : 1) : [];
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
      await page.screenshot({ path: `${evidence}/${name}-simulated.png` });
      await page.waitForTimeout(1200);
    }
  };
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
  await page.waitForFunction(() => !!window.releaseScan);
  await page.evaluate(() => { window.progress.pending = ['[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555', '192.0.2.4:5555']; });
  const untrusted = page.locator('[data-serial="[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555"]');
  const failed = page.locator('[data-serial="192.0.2.4:5555"]');
  await untrusted.getByText('CONNECTING…', { exact: true }).waitFor();
  assert.equal(await page.getByRole('heading', { name: 'No devices connected.' }).count(), 0);
  assert.equal(await page.getByText('Online', { exact: true }).count(), 0, 'unapproved discoveries must not appear online');
  await page.evaluate(() => { window.progress.awaiting_authorization = ['[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555']; });
  await untrusted.getByText('UNAUTHORIZED', { exact: true }).waitFor();
  await untrusted.getByRole('button', { name: 'Ask again', exact: true }).waitFor();
  await page.evaluate(() => {
    window.progress.pending = ['[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555'];
    window.progress.failed = [{ address: '192.0.2.4:5555', message: 'SIMULATED connection timeout; check the TV and try again.' }];
  });
  await failed.getByText('CONNECTION FAILED', { exact: true }).waitFor();
  await page.setViewportSize({ width: 720, height: 900 });
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true, 'discovered IPv6 endpoints and actions must fit the minimum window width');
  await capture('unapproved-narrow');
  await page.setViewportSize({ width: 1100, height: 760 });
  await capture('unapproved-and-failed-visible');
  await failed.getByRole('button', { name: 'Try again', exact: true }).click();
  await failed.getByText('CONNECTING…', { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.reprompted), '192.0.2.4:5555');
  await failed.getByRole('button', { name: 'Forget', exact: true }).click();
  await failed.waitFor({ state: 'detached' });
  assert.equal(await page.evaluate(() => window.forgotten), '192.0.2.4:5555');
  await page.evaluate(() => { window.ready = true; window.progress = { pending: ['192.168.1.77:5555'], awaiting_authorization: ['192.168.1.77:5555'], failed: [] }; });
  await page.getByText('NVIDIA SHIELD Android TV', { exact: true }).first().waitFor();
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

  await page.evaluate(() => { window.progress.pending = ['[fd00:1234:5678:9012:abcd:ef01:2345:6789%14]:5555']; });
  await page.getByRole('button', { name: 'Refresh', exact: true }).click();
  await page.getByRole('status').filter({ hasText: 'Still connecting' }).waitFor();
  await page.evaluate(() => { window.available = false; });
  await page.getByRole('alert').filter({ hasText: 'SIMULATED unavailable backend' }).waitFor();
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
  console.log('PASS unapproved discovery rows, observed authorization, persistent failures, explicit retry/Forget, promotion without duplicates, ready rows during scan, continued approval, serialized refresh, no repeated metadata, backend failure and unmount cleanup');
} finally { await browser?.close(); await server?.close(); }
