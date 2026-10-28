import assert from 'node:assert/strict';
import { mkdir, readFile, realpath, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const evidence = process.env.REMOTE_OUTCOME_EVIDENCE;
const warning = 'Remote input outcome unknown. It was not resent. Check the TV before retrying. SIMULATION: channel write acknowledgment lost.';
assert.ok((await readFile(resolve(root, 'crates/core/src/commands/input.rs'), 'utf8')).includes('Remote input outcome unknown. It was not resent. Check the TV before retrying.'));
const previous = { demo: process.env.VITE_DEMO, host: process.env.TAURI_DEV_HOST };
process.env.VITE_DEMO = '1';
delete process.env.TAURI_DEV_HOST;
let server, browser;
const pause = ms => evidence ? new Promise(resolve => setTimeout(resolve, ms)) : Promise.resolve();
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, hmr: false, fs: { allow: [root, await realpath(resolve(root, 'node_modules'))] } } });
  await server.listen();
  browser = await chromium.launch();
  const context = await browser.newContext({ viewport: { width: 1280, height: 950 }, ...(evidence ? { recordVideo: { dir: `${evidence}/raw`, size: { width: 1280, height: 950 } } } : {}) });
  const page = await context.newPage();
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: 'networkidle' });
  await page.getByText('NVIDIA SHIELD', { exact: false }).first().click();
  await page.getByRole('tab', { name: 'Remote', exact: true }).click();
  await page.getByRole('textbox', { name: /Live typing capture/ }).waitFor();
  await page.evaluate(warning => {
    const bridge = window.__TAURI_INTERNALS__;
    const original = bridge.invoke.bind(bridge);
    window.remoteOutcomeCalls = [];
    bridge.invoke = async (command, args = {}) => {
      if (command === 'send_key' || command === 'send_text') {
        window.remoteOutcomeCalls.push({ command, key: args.key, text: args.text, forceShell: args.forceShell });
        if (window.remoteOutcomeCalls.length === 1) throw warning;
        return { ok: true, message: 'SIMULATION: explicit next gesture accepted.', transport: 'channel' };
      }
      return original(command, args);
    };
    const label = document.createElement('div');
    label.textContent = 'SIMULATION — injected Remote responses; no ADB or TV';
    label.style.cssText = 'position:fixed;bottom:0;right:0;background:#172337;color:white;padding:9px;z-index:99999;font-size:13px';
    document.body.append(label);
  }, warning);
  await pause(1500);
  await page.getByRole('button', { name: 'Home', exact: true }).click();
  await page.getByText(warning, { exact: true }).waitFor();
  await page.waitForTimeout(800);
  assert.equal(await page.evaluate(() => window.remoteOutcomeCalls.length), 1, 'one failed gesture must not automatically invoke another input command');
  if (evidence) await page.screenshot({ path: `${evidence}/remote-unknown-outcome-simulated.png` });
  await pause(4500);
  await page.getByRole('button', { name: 'Back', exact: true }).click();
  await page.waitForFunction(() => window.remoteOutcomeCalls.length === 2);
  await page.waitForFunction(warning => !document.body.innerText.includes(warning), warning);
  assert.deepEqual(await page.evaluate(() => window.remoteOutcomeCalls.map(c => c.key)), ['home', 'back']);
  if (evidence) {
    await page.screenshot({ path: `${evidence}/remote-explicit-next-gesture-simulated.png` });
    await writeFile(`${evidence}/remote-outcome-calls.json`, JSON.stringify(await page.evaluate(() => window.remoteOutcomeCalls), null, 2));
  }
  await page.evaluate(() => { const previous = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__); window.__TAURI_INTERNALS__.invoke = async (command, args) => command === 'send_key' ? { ok: true, message: '', transport: 'shell' } : previous(command, args); });
  await page.getByRole('button', {name:'Home', exact:true}).click();
  await page.getByText('○ Last input: compatible', {exact:true}).waitFor();
  await page.evaluate(() => window.dispatchEvent(new Event('blur')));
  await page.waitForTimeout(2200);
  assert.equal(await page.getByText('○ Last input: compatible', {exact:true}).count(), 1, 'last-input mode persists after focus loss without claiming a live channel');
  assert.equal(await page.getByText('Each press takes about 0.7s', {exact:true}).count(), 0, 'no fabricated latency estimate');
  const spacing = await page.locator('.remote-header').evaluate(el => ({bottom:el.getBoundingClientRect().bottom, next:el.nextElementSibling.getBoundingClientRect().top}));
  assert.ok(spacing.next - spacing.bottom >= 16, 'heading is separated from Remote controls');
  await pause(4000);
  await page.evaluate(() => { const original = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__); window.__TAURI_INTERNALS__.invoke = async (command, args) => { const result = await original(command, args); if (command === 'get_tweaks') return { ...result, screensaver_components: 'com.codingbuffalo.aerialdream/'.repeat(8) }; return result; }; });
  await page.getByRole('tab', {name:'Tweaks', exact:true}).click();
  await page.getByRole('button', {name:'Restore previous', exact:true}).waitFor();
  await page.evaluate(() => { window.tweakCalls = []; const invoke = window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__); window.__TAURI_INTERNALS__.invoke = async (command, args) => { window.tweakCalls.push(command); return invoke(command, args); }; });
  await page.locator('.tweak-row').filter({hasText:'Main switch'}).getByRole('button',{name:'On',exact:true}).click();
  await page.waitForFunction(() => window.tweakCalls.includes('get_tweaks'));
  await page.waitForTimeout(150);
  assert.deepEqual(await page.evaluate(() => window.tweakCalls), ['write_setting','get_tweaks'], 'CEC readback does not reread DNS, app permissions, packages, or display scaling');
  await page.evaluate(() => window.tweakCalls = []);
  page.once('dialog', dialog => dialog.accept());
  await page.getByRole('button', {name:/^4K 3839/}).click();
  await page.waitForFunction(() => window.tweakCalls.includes('get_display_scaling'));
  assert.deepEqual(await page.evaluate(() => window.tweakCalls), ['set_display_scaling','get_display_scaling'], '4K only writes scaling and performs its one batched readback');

  for (const width of [1280, 760]) {
    await page.setViewportSize({width, height:950});
    const layout = await page.evaluate(() => ({viewport:innerWidth,scroll:document.documentElement.scrollWidth}));
    assert.ok(layout.scroll <= layout.viewport + 2, 'long screensaver component never forces horizontal overflow');
    assert.equal(await page.locator('.scale-note').count() > 0, true, 'display recovery warning stays visible');
    await page.getByRole('heading', {name:'Screensaver',exact:true}).scrollIntoViewIfNeeded();
    if (evidence) await page.screenshot({path:`${evidence}/tweaks-long-component-${width}-simulated.png`});
  }
  const video = page.video();
  await context.close();
  if (evidence) await video.saveAs(`${evidence}/remote-outcome-simulated.webm`);
  console.log('Remote outcome passed: visible unknown-outcome warning, exactly one invocation for failed gesture, explicit next gesture remains usable. Injected browser simulation only; backend no-replay is tested separately.');
} finally {
  await browser?.close();
  await server?.close();
  if (previous.demo === undefined) delete process.env.VITE_DEMO; else process.env.VITE_DEMO = previous.demo;
  if (previous.host === undefined) delete process.env.TAURI_DEV_HOST; else process.env.TAURI_DEV_HOST = previous.host;
}
