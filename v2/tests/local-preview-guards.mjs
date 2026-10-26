import assert from 'node:assert/strict';
import { preview } from 'vite';
import { chromium } from 'playwright';
let server, browser;
try {
  server = await preview({ preview: { host: '127.0.0.1', port: 0 } });
  browser = await chromium.launch();
  const page = await browser.newPage();
  await page.addInitScript(() => {
    localStorage.setItem('shieldopt.autoUpdate', 'true');
    localStorage.setItem('shieldopt.stopAdbOnQuit', 'true');
    localStorage.setItem('shieldopt.notesSeenVersion', '2.3.0');
    window.previewCalls = [];
    window.__TAURI_INTERNALS__ = {
      invoke: async (command) => {
        window.previewCalls.push(command);
        if (command === 'plugin:app|version') return '2.4.1';
        if (command === 'adb_status') return { available: true, backend: 'subprocess', path: 'mock-only', last_probe: '' };
        if (command === 'list_devices') return [];
        if (command === 'scan_network') return { subnet: null, found: [], connected: [], unauthorized: [], failed: [], needs_pairing: [], message: 'Mock scan complete' };
        throw new Error(`Unexpected preview command: ${command}`);
      },
      transformCallback: cb => cb,
      unregisterCallback: () => {},
    };
  });
  await page.goto(server.resolvedUrls.local[0], { waitUntil: 'networkidle' });
  await page.getByText('Mock scan complete', { exact: true }).waitFor();
  const state = await page.evaluate(() => ({ calls: window.previewCalls, stop: localStorage.getItem('shieldopt.stopAdbOnQuit'), auto: localStorage.getItem('shieldopt.autoUpdate'), notes: localStorage.getItem('shieldopt.notesSeenVersion') }));
  assert.ok(state.calls.includes('scan_network'));
  assert.ok(state.calls.includes('list_devices'));
  assert.ok(!state.calls.some(c => /updat|stop_adb/.test(c)));
  assert.equal(state.stop, 'true'); assert.equal(state.auto, 'true'); assert.equal(state.notes, '2.3.0');
  console.log('Actual-app local preview guards passed: normal discovery remains, updater/quit-setting calls absent and saved preferences unchanged (mocked transport).');
} finally { await browser?.close(); await server?.httpServer.close(); }
