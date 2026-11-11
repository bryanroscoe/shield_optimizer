// Settings and the Report a bug dialog in a test build (an owner test package
// with ATVOPT_TEST_BUILD) and in a release build, against the demo fixtures
// (VITE_DEMO=1). A test build says so next to the version, keeps anonymous
// reports off with the switch disabled in both places, and still shows the
// Support ID its log uploads carry. One without an endpoint claims nothing
// can be sent. A release build shows none of that.
import { freePort } from './lib/free-port.mjs';
import assert from 'node:assert/strict';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
process.env.VITE_DEMO = '1';
let server, browser;
try {
  server = await createServer({ root, server: { host: '127.0.0.1', port: await freePort(), strictPort: true, hmr: false } });
  await server.listen();
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();
  const LABEL = '4f156e4 (merge commit; PR head 9a8b7c6)';
  const bugDialog = async (page) => {
    await page.getByRole('button', { name: 'Report a bug' }).click();
    const dialog = page.getByRole('dialog', { name: 'Report a bug' });
    await dialog.waitFor();
    return dialog;
  };
  for (const testBuild of [true, false]) {
    const context = await browser.newContext({ viewport: { width: 1100, height: 900 } });
    if (testBuild) await context.addInitScript(() => localStorage.setItem('shieldopt.demo.testBuild', '1'));
    const page = await context.newPage();
    await page.goto(`${base}/settings`, { waitUntil: 'networkidle' });
    await page.locator('[data-support-id]').waitFor();
    const toggle = page.locator('#reports-toggle');
    if (testBuild) {
      await page.locator('[data-test-build-version]').getByText(LABEL).waitFor();
      assert.match(await page.locator('[data-test-build]').innerText(), /^Test build 4f156e4 \(merge commit; PR head 9a8b7c6\): anonymous usage reports are\s+always off\. Send logs still works/);
      assert.equal(await toggle.isChecked(), false, 'reports are off');
      assert.equal(await toggle.isDisabled(), true, 'and cannot be turned on');
      assert.match(await page.locator('[data-support-id]').innerText(), /\S/, 'the Support ID its uploads carry');
      // Report a bug says the same.
      const dialog = await bugDialog(page);
      const reports = dialog.locator('.telemetry-toggle input');
      assert.equal(await reports.isChecked(), false);
      assert.equal(await reports.isDisabled(), true, 'the bug dialog cannot turn reports on either');
      assert.equal(await dialog.locator('[data-bug-reports-note]').innerText(), `Test build ${LABEL}: anonymous usage reports are always off.`);
      const support = await dialog.locator('.bug-support').innerText();
      assert.match(support, /Quote it in the issue so the developer can find the logs you send\./);
      assert.doesNotMatch(support, /your reports/);
    } else {
      assert.equal(await page.locator('[data-test-build], [data-test-build-version]').count(), 0, 'a release build is not marked');
      assert.equal(await toggle.isDisabled(), false);
      assert.equal(await page.getByText(/Test build/).count(), 0);
      const dialog = await bugDialog(page);
      assert.equal(await dialog.locator('.telemetry-toggle input').isDisabled(), false);
      assert.match(await dialog.locator('.bug-support').innerText(), /find your reports/);
    }
    await context.close();
  }
  // A test build without an endpoint can't send logs either, and says so.
  {
    const context = await browser.newContext({ viewport: { width: 1100, height: 900 } });
    await context.addInitScript(() => {
      localStorage.setItem('shieldopt.demo.testBuild', '1');
      localStorage.setItem('shieldopt.demo.telemetryUnavailable', '1');
    });
    const page = await context.newPage();
    await page.goto(`${base}/settings`, { waitUntil: 'networkidle' });
    const note = page.locator('[data-test-build]');
    await note.waitFor();
    assert.match(await note.innerText(), /this build doesn't send\s+reports or logs/);
    assert.doesNotMatch(await note.innerText(), /Send logs still works/);
    assert.equal(await page.locator('#reports-toggle').isDisabled(), true);
    await context.close();
  }
  console.log('PASS test-build settings: marked next to the version, reports off and locked in Settings and Report a bug, Support ID shown with what it finds; a test build without an endpoint claims nothing; release builds unmarked');
} finally {
  await browser?.close();
  await server?.close();
}
