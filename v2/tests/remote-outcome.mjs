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
  await pause(4000);
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
