import assert from 'node:assert/strict';
import { mkdir } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const evidence = process.env.HELP_EVIDENCE;
const before = process.env.HELP_BEFORE === '1';
process.env.VITE_DEMO = '1';
let server, browser;
try {
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, hmr: false } });
  await server.listen();
  browser = await chromium.launch();
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
  }
  if (evidence && !before) {
    const video = page.video();
    await context.close();
    await video.saveAs(`${evidence}/desktop-help-simulated.webm`);
  } else await context.close();
  console.log(before ? 'Before desktop fixture captures complete.' : 'Progressive help passed: keyboard, Escape focus return, repeated toggles, six tabs, narrow viewport and visible safeguards.');
} finally {
  await browser?.close();
  await server?.close();
}
