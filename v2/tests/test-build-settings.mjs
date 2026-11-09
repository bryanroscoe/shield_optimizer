// Settings in a test build (an owner test package with ATVOPT_TEST_BUILD) and
// in a release build, against the demo fixtures (VITE_DEMO=1). A test build
// says so next to the version, keeps anonymous reports off with the switch
// disabled, and still shows the Support ID its log uploads carry. A release
// build shows none of that.
import assert from 'node:assert/strict';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'vite';
import { chromium } from 'playwright';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
process.env.VITE_DEMO = '1';
let server, browser;
try {
  server = await createServer({ root, server: { host: '127.0.0.1', port: 0, strictPort: false, hmr: false } });
  await server.listen();
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();
  for (const testBuild of [true, false]) {
    const context = await browser.newContext({ viewport: { width: 1100, height: 900 } });
    if (testBuild) await context.addInitScript(() => localStorage.setItem('shieldopt.demo.testBuild', '1'));
    const page = await context.newPage();
    await page.goto(`${base}/settings`, { waitUntil: 'networkidle' });
    await page.locator('[data-support-id]').waitFor();
    const toggle = page.locator('#reports-toggle');
    if (testBuild) {
      await page.locator('[data-test-build-version]').getByText('4f156e4').waitFor();
      assert.match(await page.locator('[data-test-build]').innerText(), /Test build 4f156e4: anonymous usage reports are\s+always off/);
      assert.equal(await toggle.isChecked(), false, 'reports are off');
      assert.equal(await toggle.isDisabled(), true, 'and cannot be turned on');
      assert.match(await page.locator('[data-support-id]').innerText(), /\S/, 'the Support ID its uploads carry');
    } else {
      assert.equal(await page.locator('[data-test-build], [data-test-build-version]').count(), 0, 'a release build is not marked');
      assert.equal(await toggle.isDisabled(), false);
      assert.equal(await page.getByText(/Test build/).count(), 0);
    }
    await context.close();
  }
  console.log('PASS test-build settings: marked next to the version, reports off and locked, Support ID shown; release builds unmarked');
} finally {
  await browser?.close();
  await server?.close();
}
