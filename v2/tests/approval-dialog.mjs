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
const MERGED = '192.168.1.88:40001';
process.env.VITE_DEMO = '1';

let server, browser;
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, strictPort: false, hmr: false } });
  await server.listen();
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();

  async function open(flags, { video = false, clock = false, colorScheme = 'dark', viewport = { width: 1100, height: 800 } } = {}) {
    const context = await browser.newContext({
      viewport, colorScheme,
      ...(video && evidence ? { recordVideo: { dir: `${evidence}/raw`, size: viewport } } : {}),
    });
    await context.addInitScript((flags) => {
      for (const flag of flags) localStorage.setItem(`shieldopt.demo.${flag}`, '1');
      window.__calls = [];
      window.__inflight = { now: 0, max: 0 };
      let bridge;
      Object.defineProperty(window, '__TAURI_INTERNALS__', {
        configurable: true, get: () => bridge,
        set(value) {
          const original = value.invoke.bind(value);
          value.invoke = (command, args = {}) => {
            window.__calls.push(command);
            if (command !== 'transport_state') return original(command, args);
            window.__inflight.now += 1;
            window.__inflight.max = Math.max(window.__inflight.max, window.__inflight.now);
            return original(command, args).finally(() => { window.__inflight.now -= 1; });
          };
          bridge = value;
        },
      });
    }, flags);
    const page = await context.newPage();
    if (clock) await page.clock.install();
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
    assert.match(await dialog(page).locator('[data-wait-note]').innerText(), /^Waiting up to 10 more minutes\./);
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
    // Listed before this window opened: when the server asked is unknown.
    assert.equal(await row(page).locator('[data-approval-line]').innerText(), "Hasn't allowed this computer yet.");
    assert.equal(await row(page).locator('.unauthorized-help').count(), 0, 'the old box is gone');
    assert.equal(await row(page).getByRole('button', { name: 'Retry approval' }).count(), 0);
    await shot(page, '07-platform-tools-row-dark');
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('On Bedroom TV, tick', { exact: false }).waitFor();
    assert.ok(await page.evaluate(() => window.__calls.includes('reprompt_device')), 'the server asks again');
    assert.ok(!(await page.evaluate(() => window.__calls.includes('request_device_approval'))));
    await shot(page, '08-platform-tools-waiting-dark');
    assert.ok(await page.evaluate(() => window.__calls.includes('transport_state')), 'it watches the one transport');
    await dialog(page).getByRole('heading', { name: 'Bedroom TV allowed this computer' }).waitFor({ timeout: 10_000 });
    await dialog(page).waitFor({ state: 'detached', timeout: 5000 });
    await page.locator(`a.device-row[data-serial="${TV}"]`).waitFor();
    await context.close();
  }

  // Platform-tools: "probably asking now" only right after the server offered
  // its key (a TV that connected in this window), and never after Cancel.
  {
    const { context, page } = await open(['approvalTv', 'approvalTvAfterScan']);
    await page.getByRole('button', { name: 'Scan LAN' }).click();
    await row(page).getByText('NOT ALLOWED YET', { exact: true }).waitFor();
    assert.equal(await row(page).locator('[data-approval-line]').innerText(), "Hasn't allowed this computer yet; it's probably asking now.");
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByRole('button', { name: 'Cancel request' }).click();
    await dialog(page).waitFor({ state: 'detached' });
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).waitFor();
    assert.equal(await row(page).locator('[data-approval-line]').innerText(), "Hasn't allowed this computer yet.");
    await context.close();
  }

  // An approval never closes the Pair panel or clears what was typed in it.
  {
    const { context, page } = await open(['adbEngineBuiltin', 'approvalTv', 'approvalAutoAllow']);
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('On Bedroom TV, tick', { exact: false }).waitFor();
    await dialog(page).getByRole('button', { name: 'Close', exact: true }).click();
    await page.getByRole('button', { name: 'Pair PIN' }).click();
    const pairInput = page.locator('.pair-form input').first();
    await pairInput.fill('192.168.1.50:37000');
    await page.locator(`a.device-row[data-serial="${TV}"]`).waitFor({ timeout: 10_000 });
    await page.locator(`a.device-row.flash[data-serial="${TV}"]`).waitFor({ timeout: 4000 });
    assert.ok(await page.locator('.pair-form').isVisible(), 'the Pair panel stays open');
    assert.equal(await pairInput.inputValue(), '192.168.1.50:37000', 'and keeps what was typed');
    await context.close();
  }

  // Built-in: between Allow and the TV being listed the request is still
  // open, and it is never read as a refusal.
  {
    const { context, page } = await open(['adbEngineBuiltin', 'approvalTv', 'approvalAutoAllow', 'approvalGap']);
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('On Bedroom TV, tick', { exact: false }).waitFor();
    const phases = new Set();
    const until = Date.now() + 15_000;
    while (Date.now() < until) {
      const phase = await page.evaluate(() => document.querySelector('[data-approval-dialog]')?.getAttribute('data-phase') ?? null);
      if (phase) phases.add(phase);
      if (phase === 'approved') break;
      await page.waitForTimeout(100);
    }
    assert.ok(phases.has('approved'), `reached approved: ${[...phases]}`);
    assert.ok(!phases.has('not_approved') && !phases.has('error'), `never a refusal: ${[...phases]}`);
    await context.close();
  }

  // Built-in: the approved transport folded into a row under another key is
  // still an approval, found through that transport.
  {
    const { context, page } = await open(['adbEngineBuiltin', 'approvalTv', 'approvalAutoAllow', 'approvalMerges']);
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByRole('heading', { name: 'Bedroom TV allowed this computer' }).waitFor({ timeout: 10_000 });
    await dialog(page).waitFor({ state: 'detached', timeout: 5000 });
    await page.locator(`a.device-row.flash[data-serial="${MERGED}"]`).waitFor({ timeout: 4000 });
    assert.ok(await page.evaluate(() => window.__calls.includes('device_profile')), 'resolved through its own transport');
    await context.close();
  }

  // Built-in: a refusal moves focus to Try again (the focused button is
  // gone), the trap holds, and a later listing that the TV allowed this
  // computer corrects the dialog.
  {
    const { context, page } = await open(['adbEngineBuiltin', 'approvalTv', 'approvalRefuses', 'approvalLateAllow']);
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('On Bedroom TV, tick', { exact: false }).waitFor();
    await dialog(page).getByRole('button', { name: 'Cancel request' }).focus();
    await dialog(page).getByRole('heading', { name: "Bedroom TV didn't allow this computer" }).waitFor({ timeout: 8000 });
    await page.waitForFunction(() => document.activeElement?.textContent?.trim() === 'Try again'
      && document.activeElement.closest('[role="dialog"]') !== null);
    for (let i = 0; i < 4; i++) {
      await page.keyboard.press('Tab');
      assert.ok(await page.evaluate(() => document.activeElement?.closest('[role="dialog"]') !== null), 'Tab stays in the dialog');
    }
    await dialog(page).getByRole('heading', { name: 'Bedroom TV allowed this computer' }).waitFor({ timeout: 8000 });
    await dialog(page).waitFor({ state: 'detached', timeout: 5000 });
    await page.locator(`a.device-row[data-serial="${TV}"]`).waitFor();
    await context.close();
  }

  // Built-in: with focus knocked out of the dialog, Escape still closes it.
  {
    const { context, page } = await open(['adbEngineBuiltin', 'approvalTv', 'approvalRefuses']);
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText("didn't allow this computer", { exact: false }).waitFor({ timeout: 8000 });
    await page.evaluate(() => document.activeElement?.blur());
    await page.keyboard.press('Escape');
    await dialog(page).waitFor({ state: 'detached' });
    await context.close();
  }

  // Built-in: one request at a time per TV, and a superseded request's late
  // answer changes nothing.
  {
    const { context, page } = await open(['adbEngineBuiltin', 'approvalTv', 'approvalFirstRequestFails']);
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('Asking Bedroom TV…').waitFor();
    assert.equal(await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).count(), 0, 'no second request while asking');
    assert.equal(await row(page).getByRole('button', { name: 'Show the approval request for Bedroom TV' }).count(), 1);
    await dialog(page).getByRole('button', { name: 'Cancel request' }).click();
    await dialog(page).waitFor({ state: 'detached' });
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('On Bedroom TV, tick', { exact: false }).waitFor();
    await page.waitForTimeout(2200);
    assert.equal(await dialog(page).getAttribute('data-phase'), 'waiting', 'the first request\'s failure is dropped');
    assert.equal(await page.getByText('SIMULATED stale request failure').count(), 0);
    await context.close();
  }

  // Built-in: a request that fails after its dialog was closed says so on the row.
  {
    const { context, page } = await open(['adbEngineBuiltin', 'approvalTv', 'approvalUnreachable']);
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('Asking Bedroom TV…').waitFor();
    await dialog(page).getByRole('button', { name: 'Close', exact: true }).click();
    await dialog(page).waitFor({ state: 'detached' });
    const line = row(page).locator('[data-approval-line]');
    await line.getByText("didn't answer within 20 seconds", { exact: false }).waitFor({ timeout: 5000 });
    assert.ok(await line.evaluate(el => el.classList.contains('approval-result')));
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).waitFor();
    assert.equal(await dialog(page).count(), 0, 'the pop-up does not reopen by itself');
    await shot(page, '09-builtin-row-request-failed-dark');
    await context.close();
  }

  // Forget ends the request: the built-in wait is canceled first, and
  // platform-tools stops watching.
  {
    const { context, page } = await open(['adbEngineBuiltin', 'approvalTv']);
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('On Bedroom TV, tick', { exact: false }).waitFor();
    await page.keyboard.press('Escape');
    await row(page).getByText('WAITING FOR ALLOW', { exact: true }).waitFor();
    await row(page).getByRole('button', { name: 'Forget' }).click();
    await row(page).getByText('NOT ALLOWED YET', { exact: true }).waitFor();
    const calls = await page.evaluate(() => window.__calls);
    assert.ok(calls.includes('cancel_device_approval') && calls.indexOf('cancel_device_approval') < calls.lastIndexOf('forget_device'));
    await page.waitForTimeout(1500);
    assert.equal(await dialog(page).count(), 0);
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).waitFor();
    await context.close();
  }
  // Page timers run on Playwright's clock, so "nothing happens" is checked
  // over several poll intervals without waiting for them in real time.
  {
    const { context, page } = await open(['approvalTv'], { clock: true });
    const count = () => page.evaluate(() => window.__calls.filter(c => c === 'transport_state').length);
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('On Bedroom TV, tick', { exact: false }).waitFor();
    await page.keyboard.press('Escape');
    const waiting = await count();
    await page.clock.runFor(5000);
    assert.ok(await count() > waiting, 'polls while the request waits');
    await row(page).getByRole('button', { name: 'Forget' }).click();
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).waitFor();
    await page.clock.runFor(500);
    const before = await count();
    await page.clock.runFor(10_000);
    assert.equal(await count(), before, 'no more polling after Forget');
    await context.close();
  }

  // Platform-tools: one transport read at a time, and no full list re-read
  // while nothing changed.
  {
    const { context, page } = await open(['approvalTv', 'transportStateSlow'], { clock: true });
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('On Bedroom TV, tick', { exact: false }).waitFor();
    const lists = () => page.evaluate(() => window.__calls.filter(c => c === 'list_devices').length);
    const reads = () => page.evaluate(() => window.__calls.filter(c => c === 'transport_state').length);
    const before = await lists();
    // Each read takes 3 s against a 2 s poll: ticks land while one is out.
    await page.clock.runFor(9000);
    const { max } = await page.evaluate(() => window.__inflight);
    assert.ok(await reads() >= 2, 'the slow reads did run');
    assert.equal(max, 1, 'never two transport reads at once');
    assert.equal(await lists(), before, 'no full device list while the TV still asks');
    await context.close();
  }

  // Platform-tools: the 10-minute limit holds even when adb can't be read,
  // and a reopened pop-up says how long is left.
  {
    const { context, page } = await open(['approvalTv', 'transportStateFails'], { clock: true });
    await row(page).getByRole('button', { name: 'Request approval from Bedroom TV' }).click();
    await dialog(page).getByText('On Bedroom TV, tick', { exact: false }).waitFor();
    await page.keyboard.press('Escape');
    await page.clock.fastForward('09:00');
    await row(page).getByRole('button', { name: 'Show the approval request for Bedroom TV' }).click();
    await page.clock.fastForward(6000);
    await dialog(page).locator('[data-wait-note]').getByText('Waiting up to 1 more minute.', { exact: false }).waitFor();
    await page.clock.fastForward('01:05');
    await dialog(page).getByRole('heading', { name: "Bedroom TV didn't allow this computer" }).waitFor({ timeout: 5000 });
    await context.close();
  }

  console.log('PASS approval dialog: request opens a modal with a pulsing TV, focus trapped, Escape and Close keep waiting, Show request reopens, Cancel request stops and returns focus, auto-close on Allow, reduced motion, 720 px and larger text, the same flow under platform-tools; Pair panel untouched, allow gap and folded rows are approvals, late listing corrects a refusal, focus follows the phase, one request at a time, failures on the row, Forget ends requests, bounded polling, wait limit and time left');
} finally {
  await browser?.close();
  await server?.close();
}
