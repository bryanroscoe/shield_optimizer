import assert from 'node:assert/strict';
import { readFile, mkdir } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const evidence = join(root, 'evidence/firetv-protection-205-20261019');
const result = JSON.parse(await readFile(join(evidence, 'result-simulated.json'), 'utf8'));
const route = await readFile(join(root, 'src/routes/devices/[serial]/+page.svelte'), 'utf8');
const messageCss = route.match(/  \.action-message \{[^}]+\}/)?.[0];
assert.ok(messageCss, 'reuse the production message wrapping rule');
assert.equal(result.ok, false);
assert.equal(result.current_launcher, 'com.amazon.tv.launcher');
assert.match(result.last_error, /This firmware explicitly rejected changing protected package com.amazon.tv.launcher/);
assert.match(result.last_error, /Stock-disable command failed/);
assert.match(result.last_error, /Stock-restore command failed/);
assert.match(result.last_error, /Android reports com.amazon.tv.launcher as the Home app/);
const browser = await chromium.launch({ headless: true, ...(process.env.PLAYWRIGHT_EXECUTABLE_PATH ? { executablePath: process.env.PLAYWRIGHT_EXECUTABLE_PATH } : {}) });
try {
  for (const [width, scale] of [[980, 100], [384, 100], [384, 200]]) {
    const capture = process.env.LAUNCHER_PROTECTION_EVIDENCE === '1';
    const video = capture && width === 384 && scale === 100;
    if (capture) await mkdir(evidence, { recursive: true });
    const context = await browser.newContext({ viewport: { width, height: 812 }, ...(video ? { recordVideo: { dir: join(evidence, 'raw'), size: { width, height: 812 } } } : {}) });
    const page = await context.newPage();
    // A message-rendering fixture, not the Svelte app or a device screenshot.
    await page.setContent(`<html lang="en"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Simulated protected-package rejection</title><style>
      :root { font-size:${scale}%; --bg-inset:#191e25; --border:#404854; --radius-sm:6px; }
      body { font-family:system-ui,sans-serif; margin:0; background:#101419; color:#e5e8ed; }
      main { padding:20px; max-width:940px; margin:auto; } h1 { font-size:1.35rem; } p { line-height:1.5; }
      .fixture-note { color:#b9c3d0; } .small { font-size:.875rem; } ${messageCss}
      pre { white-space:pre-wrap; overflow-wrap:anywhere; font-size:.8rem; } summary { padding:14px 0; cursor:pointer; }
      </style><main><h1>Simulated launcher result</h1><p class="fixture-note">Message fixture using the mocked shared-core result and production wrapping rule. This is not a running desktop or Android app.</p><p class="small action-message home-picker-result" role="status"></p><p id="home"></p><details><summary>Raw mocked diagnostics</summary><pre></pre></details></main></html>`);
    await page.getByRole('status').evaluate((el, text) => el.textContent = text, result.last_error);
    await page.locator('#home').evaluate((el, home) => el.textContent = `Final Home readback: ${home}`, result.current_launcher);
    await page.locator('pre').evaluate((el, lines) => el.textContent = lines.join('\n'), result.diagnostics);
    assert.equal(await page.getByRole('status').textContent(), result.last_error);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true, `${width}px / ${scale}% text`);
    if (capture) await page.screenshot({ path: join(evidence, `message-${width}-${scale}-simulated.png`), fullPage: true });
    if (video) {
      await page.waitForTimeout(800);
      await page.getByText('Raw mocked diagnostics', { exact: true }).click();
      await page.locator('pre').scrollIntoViewIfNeeded();
      await page.waitForTimeout(1000);
      await page.screenshot({ path: join(evidence, 'raw-diagnostics-simulated.png'), fullPage: true });
      const recording = page.video(); await context.close(); await recording.saveAs(join(evidence, 'message-simulated.webm'));
    } else await context.close();
  }
} finally { await browser.close(); }
console.log('Protected-package message fixture: exact error text, separate restore/Home outcomes, raw diagnostics and wrapping at 980/384px and 200% text pass. No app/device calls.');
