// The approval pop-up on Devices, under both ADB engines, against the demo
// fixtures (VITE_DEMO=1): a TV that hasn't allowed this computer, a request
// that waits with a pulsing TV, Close that keeps waiting, Cancel request that
// stops, the dialog closing by itself once the TV allows this computer, and
// the same flow under Google platform-tools. Set APPROVAL_DIALOG_EVIDENCE to a
// folder to keep screenshots and a recording.
import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const evidence = process.env.APPROVAL_DIALOG_EVIDENCE;
const TV = '192.168.1.88:5555';
process.env.VITE_DEMO = '1';

let server, browser;
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, strictPort: false, hmr: false } });
  await server.listen();
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();

  async function open(flags, { video = false, colorScheme = 'dark', viewport = { width: 1100, height: 800 } } = {}) {
    const context = await browser.newContext({
      viewport, colorScheme,
      ...(video && evidence ? { recordVideo: { dir: `${evidence}/raw`, size: viewport } } : {}),
    });
    await context.addInitScript((flags) => {
      for (const flag of flags) localStorage.setItem(`shieldopt.demo.${flag}`, '1');
      window.__calls = [];
      let bridge;
      Object.defineProperty(window, '__TAURI_INTERNALS__', {
        configurable: true, get: () => bridge,
        set(value) {
          const original = value.invoke.bind(value);
          value.invoke = (command, args = {}) => { window.__calls.push(command); return original(command, args); };
          bridge = value;
        },
      });
    }, flags);
    const page = await context.newPage();
    await page.goto(base, { waitUntil: 'networkidle' });
    return { context, page };
  }
  const shot = async (page, name) => {
    if (evidence) await page.screenshot({ path: `${evidence}/${name}.png` });
  };
  const row = (page) => page.locator(`[data-serial="${TV}"]`);
  const dialog = (page) => page.getByRole('dialog');

  // Built-in engine: request, close (keeps waiting), reopen, cancel.
  {
    const { context, page } = await open(['adbEngineBuiltin', 'approvalTv']);
    await row(page).getByText('NOT ALLOWED YET', { exact: true }).waitFor();
    assert.equal(await page.getByText(/This device needs to be authorized/).count(), 0);
    const request = row(page).getByRole('button', { name: 'Request approval from Bedroom TV', exact: true });
    await request.click();
    await dialog(page).getByRole('heading', { name: 'Allow this computer on Bedroom TV' }).waitFor();
    await dialog(page).getByText('On Bedroom TV, tick Always allow from this computer, then choose Allow.').waitFor();
    assert.equal(await dialog(page).getAttribute('data-phase'), 'waiting');
    assert.equal(await dialog(page).getAttribute('aria-modal'), 'true');
    assert.match(await dialog(page).innerText(), /Closing this window keeps waiting; only Cancel request\s+stops it/);
    assert.ok(await page.evaluate(() => document.activeElement?.closest('[role="dialog"]') !== null), 'focus moves into the dialog');
    // Focus stays inside.
    for (let i = 0; i < 6; i++) {
      await page.keyboard.press('Tab');
      assert.ok(await page.evaluate(() => document.activeElement?.closest('[role="dialog"]') !== null), 'Tab stays in the dialog');
    }
    const ring = dialog(page).locator('.ring').first();
    assert.notEqual(await ring.evaluate(el => getComputedStyle(el).animationName), 'none', 'the TV pulses');
    await shot(page, '01-builtin-waiting-dark');
    await dialog(page).locator('summary', { hasText: 'Not seeing the prompt?' }).click();
    const help = await dialog(page).locator('details').innerText();
    assert.match(help, /Wake the TV/);
    assert.match(help, /Chose Cancel on the TV\? The TV doesn't tell this computer/);
    assert.match(help, /Revoke USB debugging authorizations/);
    assert.doesNotMatch(help, /tap this computer/, 'a :5555 transport never appears under Wireless debugging');
    await shot(page, '02-builtin-waiting-help-dark');
    await page.keyboard.press('Escape');
    await dialog(page).waitFor({ state: 'detached' });
    await row(page).getByText('WAITING FOR ALLOW', { exact: true }).waitFor();
    const show = row(page).getByRole('button', { name: 'Show the approval request for Bedroom TV', exact: true });
    await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'Show the approval request for Bedroom TV');
    assert.ok(!(await page.evaluate(() => window.__calls.includes('cancel_device_approval'))), 'closing the dialog cancels nothing');
    await shot(page, '03-builtin-row-waiting-dark');
    await show.click();
    await dialog(page).getByRole('button', { name: 'Cancel request' }).click();
    await dialog(page).waitFor({ state: 'detached' });
    await row(page).getByText('NOT ALLOWED YET', { exact: true }).waitFor();
    assert.ok(await page.evaluate(() => window.__calls.includes('cancel_device_approval')));
    await page.waitForFunction(() => document.activeElement?.getAttribute('aria-label') === 'Request approval from Bedroom TV');
    // Light theme, minimum width, larger text.
    await page.emulateMedia({ colorScheme: 'light' });
    await page.setViewportSize({ width: 720, height: 900 });
    await request.click();
    await dialog(page).getByText('On Bedroom TV, tick', { exact: false }).waitFor();
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth));
    await shot(page, '04-builtin-waiting-light-720');
    await page.evaluate(() => { document.documentElement.style.fontSize = '125%'; });
    const box = await dialog(page).boundingBox();
    assert.ok(box && box.x >= 0 && box.x + box.width <= 720, 'the dialog fits at 720 px with larger text');
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth));
    await shot(page, '05-builtin-waiting-light-720-large-text');
    await page.emulateMedia({ reducedMotion: 'reduce' });
    assert.equal(await ring.evaluate(el => getComputedStyle(el).animationName), 'none', 'no pulse with reduced motion');
    await context.close();
  }

  // Built-in engine: the TV allows this computer; the dialog closes itself.
  {
    const { context, page } = await open(['adbEngineBuiltin', 'approvalTv', 'approvalAutoAllow'], { video: true });
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('On Bedroom TV, tick', { exact: false }).waitFor();
    await page.waitForTimeout(1500);
    await dialog(page).getByRole('heading', { name: 'Bedroom TV allowed this computer' }).waitFor({ timeout: 10_000 });
    await shot(page, '06-builtin-allowed-dark');
    await dialog(page).waitFor({ state: 'detached', timeout: 5000 });
    await page.locator(`a.device-row[data-serial="${TV}"]`).waitFor();
    await page.waitForTimeout(800);
    const video = page.video();
    await context.close();
    if (evidence && video) await video.saveAs(`${evidence}/builtin-request-pulse-and-auto-close.webm`);
  }

  // Google platform-tools: same row and dialog; the adb server already asked.
  {
    const { context, page } = await open(['approvalTv', 'approvalAutoAllow']);
    await row(page).getByText('NOT ALLOWED YET', { exact: true }).waitFor();
    assert.match(await row(page).innerText(), /it's probably asking now/);
    assert.equal(await row(page).locator('.unauthorized-help').count(), 0, 'the old box is gone');
    assert.equal(await row(page).getByRole('button', { name: 'Retry approval' }).count(), 0);
    await shot(page, '07-platform-tools-row-dark');
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('On Bedroom TV, tick', { exact: false }).waitFor();
    assert.ok(await page.evaluate(() => window.__calls.includes('reprompt_device')), 'the server asks again');
    assert.ok(!(await page.evaluate(() => window.__calls.includes('request_device_approval'))));
    await shot(page, '08-platform-tools-waiting-dark');
    await dialog(page).getByRole('heading', { name: 'Bedroom TV allowed this computer' }).waitFor({ timeout: 10_000 });
    await dialog(page).waitFor({ state: 'detached', timeout: 5000 });
    await page.locator(`a.device-row[data-serial="${TV}"]`).waitFor();
    await context.close();
  }

  console.log('PASS approval dialog: request opens a modal with a pulsing TV, focus trapped, Escape and Close keep waiting, Show request reopens, Cancel request stops and returns focus, auto-close on Allow, reduced motion, 720 px and larger text, and the same flow under platform-tools');
} finally {
  await browser?.close();
  await server?.close();
}
