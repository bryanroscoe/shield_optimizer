// #91: the Remote box accepts paste as well as typing. What it is used for is
// long URLs, usernames and passwords that nobody wants to enter a character at
// a time on a TV remote.
//
// The capture is a non-editable div. A focused one does receive paste with
// clipboardData populated (verified in Chromium and in WebKit, which is what
// Tauri uses on macOS), so this drives a real paste rather than a synthetic
// call into the handler.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");

function serverURL(server) {
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("no address");
  const host = address.address.includes(":") ? `[${address.address}]` : address.address;
  return `http://${host}:${address.port}`;
}

function setHarnessEnvironment() {
  const keys = ["VITE_DEMO", "TAURI_DEV_HOST"];
  const previous = new Map(keys.map((k) => [k, { present: Object.hasOwn(process.env, k), value: process.env[k] }]));
  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;
  return () => {
    for (const [k, prior] of previous) {
      if (prior.present) process.env[k] = prior.value;
      else delete process.env[k];
    }
  };
}

async function openRemote(page, base) {
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "Remote" }).click();
  await page.getByRole("textbox", { name: /Live typing capture/ }).waitFor();
  await page.evaluate(() => {
    const bridge = window.__TAURI_INTERNALS__;
    const original = bridge.invoke.bind(bridge);
    window.__SENT__ = [];
    bridge.invoke = async (command, args = {}) => {
      if (command === "send_text" || command === "send_key") {
        window.__SENT__.push({ command, ...args });
        return { ok: true, message: "", transport: "channel" };
      }
      return original(command, args);
    };
  });
}

const sent = (page) => page.evaluate(() => window.__SENT__);

/// Dispatch a genuine ClipboardEvent carrying a real DataTransfer. Built in
/// the page because a clipboardData object cannot cross the Playwright
/// boundary; the event itself is the same one the browser delivers.
const paste = (page, text) =>
  page.evaluate((value) => {
    const data = new DataTransfer();
    data.setData("text/plain", value);
    const capture = document.querySelector('[aria-label^="Live typing capture"]');
    capture.dispatchEvent(
      new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
    );
  }, text);

async function exercise({ browser, base }) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 950 } });
  await openRemote(page, base);

  const capture = page.getByRole("textbox", { name: /Live typing capture/ });
  await capture.click();

  // A real paste event, as the OS would deliver it.
  const URL_TEXT = "https://example.com/very/long/path?token=abc123";
  await paste(page, URL_TEXT);
  await page.waitForFunction(() => window.__SENT__.length > 0);

  let calls = await sent(page);
  assert.deepEqual(
    calls.map((c) => [c.command, c.text]),
    [["send_text", URL_TEXT]],
    // One call, not 46 keystrokes: that is the whole point over a ~700ms
    // shell fallback.
    `the clipboard should arrive as one send_text: ${JSON.stringify(calls)}`,
  );

  // A multi-line paste cannot ride in `input text`; newlines become Enter.
  await page.evaluate(() => (window.__SENT__ = []));
  await paste(page, "user@example.com\r\nhunter2");
  await page.waitForFunction(() => window.__SENT__.length >= 3);
  calls = await sent(page);
  assert.deepEqual(
    calls.map((c) => [c.command, c.text ?? c.key]),
    [
      ["send_text", "user@example.com"],
      ["send_key", "enter"],
      ["send_text", "hunter2"],
    ],
    `CRLF must become one Enter, in order: ${JSON.stringify(calls)}`,
  );

  // Typing still works and is not disturbed by any of this.
  await page.evaluate(() => (window.__SENT__ = []));
  await capture.press("a");
  await page.waitForFunction(() => window.__SENT__.length > 0);
  calls = await sent(page);
  assert.equal(calls[0].text, "a");

  // Cmd/Ctrl chords must never reach the TV as literal characters — that is
  // why the keydown handler ignores them, and why paste has its own path.
  await page.evaluate(() => (window.__SENT__ = []));
  await capture.press("ControlOrMeta+v");
  await page.waitForTimeout(150);
  assert.deepEqual(await sent(page), [], "a modifier chord must send nothing on its own");

  // An empty clipboard is not an error and must not send an empty string.
  await page.evaluate(() => (window.__SENT__ = []));
  await paste(page, "");
  await page.waitForTimeout(150);
  assert.deepEqual(await sent(page), []);

  // The button is the discoverable path; a dashed box that quietly accepts
  // Cmd+V is not something anyone would guess at.
  await page.getByRole("button", { name: "Paste", exact: true }).waitFor();

  // Finally, a real OS-level paste rather than a constructed event: put text
  // on the actual clipboard and press the actual shortcut. This is what a user
  // does, and it only works because a focused non-editable div still receives
  // paste.
  await page.evaluate(() => (window.__SENT__ = []));
  await page.evaluate(async (value) => {
    const scratch = document.createElement("input");
    document.body.append(scratch);
    scratch.value = value;
    scratch.select();
    document.execCommand("copy");
    scratch.remove();
  }, "real-clipboard-token");
  await capture.click();
  await capture.press("ControlOrMeta+v");
  await page.waitForFunction(() => window.__SENT__.length > 0, null, { timeout: 5000 });
  calls = await sent(page);
  assert.equal(calls[0].text, "real-clipboard-token", JSON.stringify(calls));

  console.log(
    "Remote paste passed: a pasted URL arrives as one send_text, newlines become Enter in order, typing is unaffected, and a modifier chord alone sends nothing.",
  );
}

async function main() {
  const restore = setHarnessEnvironment();
  let server, browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
    await server.listen();
    browser = await chromium.launch();
    await exercise({ browser, base: serverURL(server) });
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    restore();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
