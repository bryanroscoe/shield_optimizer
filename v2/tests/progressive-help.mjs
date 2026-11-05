import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium, webkit } from 'playwright';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const evidence = process.env.HELP_EVIDENCE;
const before = process.env.HELP_BEFORE === '1';
process.env.VITE_DEMO = '1';

async function nativeEmptyConnectionHelp(browser, base) {
  const page = await browser.newPage();
  try {
    await page.addInitScript(() => {
      localStorage.clear();
      localStorage.setItem('shieldopt.demo.noDevices', '1');
      window.nativeEmptyCalls = [];
      let bridge;
      Object.defineProperty(window, '__TAURI_INTERNALS__', {
        configurable: true, get: () => bridge,
        set(value) {
          const original = value.invoke.bind(value);
          value.invoke = async (command, args) => {
            window.nativeEmptyCalls.push(command);
            if (command === 'adb_status') return { backend: 'direct', available: true, path: null, connections: { pending: [], failed: [] } };
            if (command === 'scan_network') return { found: [], connected: [], unauthorized: [], failed: [], needs_pairing: [], pending: [], details: [], subnet: null, message: 'SIMULATED empty native scan.' };
            if (['restart_adb', 'install_adb', 'set_stop_adb_on_quit'].includes(command)) throw new Error('Host-server action in native mode');
            return original(command, args);
          };
          bridge = value;
        },
      });
    });
    await page.goto(base, { waitUntil: 'networkidle' });
    await page.getByRole('heading', { name: 'No devices connected.', exact: true }).waitFor();
    const help = page.locator('.help-details').filter({ has: page.locator('summary', { hasText: 'Connection help' }) });
    assert.equal(await help.evaluate(el => el.open), true, 'a finished empty native scan opens relevant help');
    assert.match(await help.innerText(), /Check that Network debugging is enabled on the TV\./);
    assert.equal(await page.getByRole('button', { name: /Restart ADB/ }).count(), 0, 'native help never offers a host-server restart');
    assert.equal(await help.getByRole('button', { name: 'Pair PIN', exact: true }).isVisible(), true);
    assert.equal(await help.getByRole('button', { name: 'Add by IP', exact: true }).isVisible(), true);
    // Built-in is network-only: USB needs the platform-tools engine.
    assert.match(await help.innerText(), /Connected by USB\? Built-in ADB works over the network only; choose Google platform-tools in Settings\./);
    assert.equal(await help.getByRole('link', { name: 'Settings', exact: true }).getAttribute('href'), '/settings#adb');
    assert.equal(await page.evaluate(() => window.nativeEmptyCalls.some(c => ['restart_adb', 'install_adb', 'set_stop_adb_on_quit'].includes(c))), false);
  } finally { await page.close(); }
}

async function emptyConnectionHelp(browser, base) {
  const out = process.env.EMPTY_HELP_EVIDENCE;
  if (out) await mkdir(out, { recursive: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 },
    ...(out ? { recordVideo: { dir: `${out}/raw`, size: { width: 1280, height: 900 } } } : {}) });
  const page = await context.newPage();
  try {
    await page.addInitScript(() => {
      localStorage.clear();
      localStorage.setItem('shieldopt.demo.noDevices', '1');
      const test = window.emptyHelp = { calls: [], mode: 'empty', holdRestart: false, recover: false };
      let internals;
      Object.defineProperty(window, '__TAURI_INTERNALS__', {
        configurable: true, get: () => internals,
        set: (value) => {
          const original = value.invoke;
          value.invoke = async (command, args) => {
            test.calls.push(command);
            if (command === 'adb_status' && test.mode === 'missing') return { backend: 'subprocess', available: false, path: null, failure: 'missing_binary' };
            if (command === 'list_devices' && test.mode === 'error') throw 'Simulated device-list failure';
            if (command === 'scan_network' && test.mode === 'scanning') {
              await new Promise(resolve => test.releaseScan = resolve);
              return { found: [], connected: [], unauthorized: [], failed: [], needs_pairing: [], subnet: null, message: 'No devices found.' };
            }
            if (command === 'restart_adb') {
              if (test.holdRestart) await new Promise(resolve => test.releaseRestart = resolve);
              if (test.recover) localStorage.removeItem('shieldopt.demo.noDevices');
              return { ok: true, server_ready: true, message: 'ADB server restarted.' };
            }
            return original(command, args);
          };
          internals = value;
        },
      });
    });
    await page.goto(base, { waitUntil: 'networkidle' });
    await page.getByRole('heading', { name: 'No devices connected.', exact: true }).waitFor();
    await page.evaluate(() => {
      const label = document.createElement('div');
      label.textContent = 'SIMULATED desktop — no ADB server or TV';
      label.style.cssText = 'position:fixed;bottom:0;right:0;z-index:99999;background:#17243a;color:white;padding:8px;font:13px sans-serif';
      document.body.append(label);
    });
    const help = page.locator('.help-details').filter({ has: page.locator('summary', { hasText: 'Connection help' }) });
    const summary = help.locator('summary');
    const restart = help.getByRole('button', { name: 'Restart ADB from connection help', exact: true });
    assert.equal(await help.evaluate(el => el.open), true, 'completed empty discovery opens help');
    assert.ok(await restart.isVisible());
    assert.equal(await page.evaluate(() => emptyHelp.calls.filter(c => c === 'restart_adb').length), 0, 'no automatic restart');
    if (out) {
      await page.screenshot({ path: `${out}/empty-help-open-simulated.png` });
      await page.waitForTimeout(900);
    }

    let prompt = '';
    page.once('dialog', async dialog => { prompt = dialog.message(); await dialog.dismiss(); });
    await restart.click();
    assert.match(prompt, /disconnects other tools using ADB/);
    assert.equal(await page.evaluate(() => emptyHelp.calls.filter(c => c === 'restart_adb').length), 0, 'Cancel restarts nothing');
    await page.evaluate(() => emptyHelp.holdRestart = true);
    page.once('dialog', dialog => dialog.accept());
    await restart.click();
    await page.waitForFunction(() => typeof emptyHelp.releaseRestart === 'function');
    assert.equal(await restart.isDisabled(), true, 'help restart is disabled while busy');
    if (out) await page.waitForTimeout(700);
    await page.evaluate(() => { emptyHelp.holdRestart = false; emptyHelp.releaseRestart(); });
    await page.getByText('ADB server restarted.', { exact: true }).waitFor();
    await page.waitForFunction(() => !document.querySelector('.connect-form input').disabled);
    assert.equal(await help.evaluate(el => el.open), true, 'restart without recovery keeps the empty guidance visible');
    assert.equal(await page.locator('.device-row.clickable').count(), 0, 'a restart alone never claims a TV connected');
    assert.equal(await page.evaluate(() => emptyHelp.calls.filter(c => c === 'restart_adb').length), 1);
    if (out) await page.screenshot({ path: `${out}/restart-still-empty-simulated.png` });

    await page.evaluate(() => emptyHelp.mode = 'error');
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await page.getByText('Failed to list devices: Simulated device-list failure', { exact: true }).waitFor();
    assert.equal(await help.evaluate(el => el.open), false, 'a failed list does not claim empty discovery');
    await page.evaluate(() => emptyHelp.mode = 'empty');
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await page.getByRole('heading', { name: 'No devices connected.', exact: true }).waitFor();
    assert.equal(await help.evaluate(el => el.open), true);

    await page.evaluate(() => emptyHelp.mode = 'scanning');
    await page.getByRole('button', { name: 'Scan LAN', exact: true }).click();
    await page.getByText('Scanning for devices…', { exact: true }).waitFor();
    assert.equal(await help.evaluate(el => el.open), false, 'in-flight discovery does not open empty guidance');
    assert.equal(await page.getByRole('heading', { name: 'No devices connected.', exact: true }).count(), 0);
    await page.evaluate(() => { emptyHelp.mode = 'empty'; emptyHelp.releaseScan(); });
    await page.getByRole('heading', { name: 'No devices connected.', exact: true }).waitFor();
    assert.equal(await help.evaluate(el => el.open), true, 'an empty scan opens help again');
    await page.setViewportSize({ width: 720, height: 900 });
    const bounds = await help.boundingBox();
    assert.ok(bounds.x >= 0 && bounds.x + bounds.width <= 720, 'help fits the desktop minimum width');
    if (out) await page.screenshot({ path: `${out}/empty-help-narrow-simulated.png` });
    await page.setViewportSize({ width: 1280, height: 900 });

    await page.evaluate(() => emptyHelp.recover = true);
    page.once('dialog', dialog => dialog.accept());
    await restart.click();
    await page.locator('.device-row.clickable').first().waitFor();
    assert.equal(await help.evaluate(el => el.open), false, 'help collapses when devices become available');
    assert.equal(await page.evaluate(() => emptyHelp.calls.filter(c => c === 'restart_adb').length), 2);
    if (out) {
      await page.screenshot({ path: `${out}/restart-recovered-simulated.png` });
      await page.waitForTimeout(900);
    }
    await summary.focus(); await summary.press('Enter');
    assert.equal(await help.evaluate(el => el.open), true, 'help remains available with devices');
    await help.getByRole('button', { name: 'Pair PIN', exact: true }).click();
    await page.locator('.pair-form').waitFor();
    await help.getByRole('button', { name: 'Add by IP', exact: true }).click();
    assert.equal(await page.getByPlaceholder('IP[:port] — e.g. 192.168.42.71').evaluate(el => el === document.activeElement), true);
    await summary.focus(); await summary.press('Escape');
    assert.equal(await help.evaluate(el => el.open), false);
    assert.equal(await summary.evaluate(el => el === document.activeElement), true);
    await page.evaluate(() => emptyHelp.mode = 'missing');
    await page.getByRole('button', { name: 'Refresh', exact: true }).click();
    await page.waitForFunction(() => document.querySelector('.header-sub')?.textContent.includes('adb not found'));
    assert.equal(await help.count(), 0, 'missing ADB shows its install flow instead');
    if (out) {
      const video = page.video(); await context.close();
      await video.saveAs(`${out}/empty-connection-help-simulated.webm`);
    }
    console.log('Empty connection help passed: initial empty discovery, no automatic restart, confirmation/cancel, busy state, unchanged empty result, list error, scan in flight, recovery, narrow layout, keyboard, pairing/IP actions and missing ADB.');
  } finally { await context.close(); }
}

let server, browser;
try {
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, hmr: false } });
  await server.listen();
  browser = await (process.argv.includes('--webkit') ? webkit : chromium).launch();
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  await emptyConnectionHelp(browser, base);
  await nativeEmptyConnectionHelp(browser, base);
  if (evidence) await mkdir(evidence, { recursive: true });
  const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, ...(evidence && !before ? { recordVideo: { dir: `${evidence}/raw`, size: { width: 1280, height: 900 } } } : {}) });
  const page = await context.newPage();
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
  await page.evaluate(() => {
    const label = document.createElement('div');
    label.textContent = 'SIMULATED desktop — demo fixtures, no device';
    label.style.cssText = 'position:fixed;bottom:0;right:0;z-index:99999;background:#17243a;color:white;padding:8px;font:13px sans-serif';
    document.body.append(label);
  });
  async function toggleHelp(label) {
    const summary = page.locator('.help-details > summary').filter({ hasText: label });
    await summary.waitFor();
    assert.equal(await summary.evaluate(e => e.parentElement.open), false);
    await summary.focus();
    await page.keyboard.press('Enter');
    assert.equal(await summary.evaluate(e => e.parentElement.open), true);
    await page.keyboard.press('Escape');
    assert.equal(await summary.evaluate(e => e.parentElement.open), false);
    assert.equal(await summary.evaluate(e => e === document.activeElement), true);
    await page.keyboard.press('Space');
    assert.equal(await summary.evaluate(e => e.parentElement.open), true);
    await page.keyboard.press('Space');
    assert.equal(await summary.evaluate(e => e.parentElement.open), false);
    await summary.click();
    await summary.click();
    assert.equal(await summary.evaluate(e => e.parentElement.open), false);
  }
  if (!before) {
    await toggleHelp('Connection help');
    const connectionHelp = page.locator('.help-details').filter({ has: page.locator('summary', { hasText: 'Connection help' }) });
    await connectionHelp.locator('summary').click();
    await connectionHelp.getByRole('button', { name: 'Pair PIN' }).focus();
    await page.keyboard.press('Escape');
    assert.equal(await connectionHelp.evaluate(e => e.open), false);
    assert.equal(await connectionHelp.locator('summary').evaluate(e => e === document.activeElement), true);
  }
  if (evidence) await page.screenshot({ path: `${evidence}/${before ? 'before' : 'after'}-devices.png` });
  await page.getByText('NVIDIA SHIELD', { exact: false }).first().click();
  for (const [tab, help] of [['Health', 'Memory report help'], ['Tweaks', 'Display scaling help'], ['Files', 'File tools'], ['Install APK', 'Install help'], ['Remote', 'Typing help'], ['Snapshot', 'Snapshot contents']]) {
    await page.getByRole('tab', { name: tab === 'Snapshot' ? /^Snapshot/ : tab, exact: true }).click();
    if (!before) await toggleHelp(help);
    await page.evaluate(() => window.scrollTo(0, 0));
    if (evidence) await page.screenshot({ path: `${evidence}/${before ? 'before' : 'after'}-${tab.toLowerCase().replaceAll(' ', '-')}.png` });
  }
  if (!before) {
    await page.getByRole('tab', { name: 'Tweaks', exact: true }).click();
    assert.ok(await page.getByText('Restarting the TV does not undo it.', { exact: false }).isVisible());
    assert.ok(await page.getByText("Custom DNS sends the TV's lookups to the server you choose.", { exact: true }).isVisible());
    await page.getByRole('tab', { name: 'Remote', exact: true }).click();
    assert.ok(await page.getByText('Sends typing to the focused TV app.', { exact: true }).isVisible());
    await page.getByRole('tab', { name: 'Shell', exact: true }).click();
    assert.ok(await page.locator('.shell-warning').isVisible());
    await page.getByRole('tab', { name: 'Launcher', exact: true }).click();
    assert.ok(await page.locator('.launcher-callout').isVisible());
    await page.getByRole('tab', { name: 'Logs', exact: true }).click();
    assert.ok(await page.getByText('Logs can contain account details, URLs and tokens.', { exact: false }).isVisible());
    await page.setViewportSize({ width: 800, height: 1000 });
    await page.getByRole('tab', { name: 'Tweaks', exact: true }).click();
    const summary = page.locator('.help-details > summary').filter({ hasText: 'Display scaling help' });
    await summary.click();
    const bounds = await summary.boundingBox();
    assert.ok(bounds.x >= 0 && bounds.x + bounds.width <= 800);
    if (evidence) await page.screenshot({ path: `${evidence}/after-narrow-help-open.png` });
    await page.goBack();
    const connectionSummary = page.locator('.help-details > summary').filter({ hasText: 'Connection help' });
    await connectionSummary.waitFor();
    assert.equal(await connectionSummary.evaluate(e => e.parentElement.open), false, 'Back leaves device help and restores Devices');
    await page.goForward();
    const profileSummary = page.locator('.help-details > summary').filter({ hasText: 'Device details help' });
    await profileSummary.waitFor();
    assert.equal(await profileSummary.evaluate(e => e.parentElement.open), false, 'Forward starts with help collapsed');
  }
  if (evidence && !before) {
    const video = page.video();
    await context.close();
    await video.saveAs(`${evidence}/desktop-help-simulated.webm`);
  } else await context.close();
  console.log(before ? 'Before desktop fixture captures complete.' : 'Progressive help passed: keyboard, Escape focus return, repeated toggles, six tabs, narrow viewport, Back/Forward navigation and visible safeguards.');
} finally {
  await browser?.close();
  await server?.close();
}
