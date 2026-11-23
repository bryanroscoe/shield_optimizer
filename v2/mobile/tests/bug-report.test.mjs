import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

// More › Report a bug: the description is required, the preview is the
// backend's own text, the summary is built once for the connected TV and
// only the connection steps (typing re-redacts just the description), Send
// uploads that same summary by id, the result names the report, a refusal is
// shown, a build without an endpoint offers GitHub only, and the GitHub
// fallback opens the bug form.

let server;
let browser;
let origin;

before(async () => {
  ({ server, origin } = await startViteServer());
  browser = await chromium.launch({ headless: true });
});

after(async () => {
  await browser?.close();
  await server?.close();
});

const PREVIEW_TEXT = "## ATV Optimizer bug report (Android)\n\n- App version: 0.1.4\n";

async function openMore(t, { telemetry } = {}) {
  const page = await browser.newPage({ viewport: { width: 384, height: 1400 } });
  t.after(() => page.close());
  await page.addInitScript((init) => {
    window.calls = [];
    window.handlers = {};
    window.__OPENED__ = [];
    window.telemetry = init.telemetry ?? { enabled: true, available: true, notice_seen: true, support_id: null };
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 0,
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        if (window.handlers[command]) return window.handlers[command](args);
        switch (command) {
          case "get_entitlement": return "free";
          case "billing_status":
          case "billing_refresh":
            return { entitlement: "free", sources: [], distribution: "play", license_key_entry: false, available: false,
              play: { state: "unavailable", reason: "n/a" }, offer: null, ack_pending: false, product_id: "atvopt_pro" };
          case "license_info": return null;
          case "list_devices": return [{
            id: 1, serial: "A:5555", name: "A", model: "Shield", status: "device",
            connection: "network", device_type: "shield", properties: null,
          }];
          case "wireless_status": return { connected: true };
          case "wireless_connect": return { ok: true, message: "connected" };
          case "telemetry_settings": return window.telemetry;
          case "prepare_bug_report":
            window.prepared = (window.prepared ?? 0) + 1;
            return { id: 6 + window.prepared, text: "## ATV Optimizer bug report (Android)\n\n- App version: 0.1.4\n", bytes: 40 };
          case "redact_bug_note":
            return args.note.trim() ? `[scrubbed] ${args.note.trim()}` : null;
          case "send_bug_report":
            return { support_id: "7d0b0b9e-1f43-4e43-9b47-3a5d6f0f9c11", bytes: 60, report_id: 42 };
          case "collect_diagnostics": return "## ATV Optimizer diagnostics\n\n- App version: 0.1.4\n";
          case "plugin:opener|open_url": window.__OPENED__.push(args.url); return null;
          case "telemetry_action":
          case "telemetry_error":
            return null;
          default: return { ok: true, message: "done" };
        }
      },
    };
  }, { telemetry });
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    await session.connect("A", 5555);
    router.reset("dashboard");
    router.navigate("more");
  });
  await page.getByRole("heading", { name: "More" }).waitFor();
  await page.getByRole("button", { name: "Report a bug" }).click();
  const sheet = page.getByRole("dialog", { name: "Report a bug" });
  await sheet.waitFor();
  return { page, sheet };
}

const calls = (page, command) =>
  page.evaluate((c) => window.calls.filter((call) => call.command === c).map((call) => call.args), command);

test("What's sent discloses the Play Integrity check as record-only", async (t) => {
  const { page, sheet } = await openMore(t);
  await sheet.getByRole("button", { name: "What's sent", exact: true }).click();
  const help = page.getByRole("dialog", { name: "What's sent" });
  await help.getByText("Google Play Integrity check", { exact: false }).waitFor();
  assert.match(await help.innerText(), /only recorded and never changes what the app\s+does/);
});

test("Send needs a description, previews the backend's text, and names the report it became", async (t) => {
  const { page, sheet } = await openMore(t);
  const send = sheet.getByRole("button", { name: "Send", exact: true });
  await sheet.getByText("Send shares your description", { exact: false }).waitFor();
  assert.equal(await send.isDisabled(), true, "nothing to send yet");

  const note = sheet.getByLabel("What went wrong?");
  assert.equal(await note.getAttribute("maxlength"), "4000");
  await note.fill("Remote stops after a minute");
  assert.equal(await send.isDisabled(), false);

  await sheet.getByRole("button", { name: /Show exactly what's sent/ }).click();
  const preview = sheet.getByLabel("Report preview");
  await preview.waitFor();
  const shown = await preview.inputValue();
  assert.match(shown, /### Description\n\n\[scrubbed\] Remote stops after a minute/);
  assert.ok(shown.includes(PREVIEW_TEXT), shown);

  // Typing with the preview open redacts the description again and nothing else.
  await note.fill("Remote stops after two minutes");
  await page.waitForFunction(() =>
    document.querySelector('textarea[aria-label="Report preview"]')?.value.includes("[scrubbed] Remote stops after two minutes"),
  );
  await note.fill("Remote stops after three minutes");
  await page.waitForFunction(() =>
    document.querySelector('textarea[aria-label="Report preview"]')?.value.includes("[scrubbed] Remote stops after three minutes"),
  );
  const redactions = await calls(page, "redact_bug_note");
  assert.ok(redactions.length >= 3, JSON.stringify(redactions));
  for (const call of redactions) assert.equal(call.id, 7);

  await send.click();
  const status = sheet.getByRole("status").filter({ hasText: "Sent as report #42" });
  await status.waitFor();
  assert.match(await status.innerText(), /Support ID is 7D0B-0B9E/);
  const prepares = await calls(page, "prepare_bug_report");
  assert.equal(prepares.length, 1, "the TV is read once, not on every keystroke or again for Send");
  const [prepared] = prepares;
  assert.equal(prepared.serial, "A:5555", "the connected TV, so its build is described");
  assert.ok(Array.isArray(prepared.history));
  for (const step of prepared.history) {
    assert.deepEqual(Object.keys(step).sort(), ["command", "ok"], JSON.stringify(step));
    assert.match(step.command, /^wireless_/);
  }
  const [args] = await calls(page, "send_bug_report");
  assert.deepEqual(args, { id: 7, note: "Remote stops after three minutes" }, "the previewed summary, by id");
  assert.equal(await note.inputValue(), "", "a sent description is cleared");

  // The description never reaches the on-device debug log.
  const log = await page.evaluate(async () => (await import("/src/lib/log.ts")).frontendLogText());
  assert.doesNotMatch(log, /Remote stops/, log);
});

test("a refusal is shown in the sheet and the counter appears near the limit", async (t) => {
  const { page, sheet } = await openMore(t);
  await page.evaluate(() => {
    window.handlers.send_bug_report = () => {
      throw "Bug report limit reached: this app has sent today's maximum. Try again tomorrow, or report it on GitHub instead.";
    };
  });
  const note = sheet.getByLabel("What went wrong?");
  // The live region is there before the count is, so its first change is announced.
  const counter = sheet.locator('p[aria-live="polite"]');
  assert.equal(await counter.count(), 1);
  assert.equal((await counter.innerText()).trim(), "");
  await note.fill("x".repeat(3601));
  await sheet.getByText("3,601 / 4,000 characters").waitFor();
  await sheet.getByRole("button", { name: "Send", exact: true }).click();
  const alert = sheet.getByRole("alert");
  await alert.waitFor();
  assert.match(await alert.innerText(), /^Bug report limit reached/);
  assert.equal(await note.inputValue(), "x".repeat(3601), "a refused description is kept");
  // Without a preview, Send built the summary itself and sent it by id.
  assert.equal((await calls(page, "prepare_bug_report")).length, 1);
  assert.equal((await calls(page, "send_bug_report"))[0].id, 7);
});

test("a build without an endpoint offers GitHub only, and GitHub opens the bug form", async (t) => {
  const { page, sheet } = await openMore(t, {
    telemetry: { enabled: false, available: false, notice_seen: true, support_id: null },
  });
  await sheet.getByText("This build can't send reports to the developer.", { exact: false }).waitFor();
  assert.equal(await sheet.getByRole("button", { name: "Send", exact: true }).count(), 0);
  assert.equal(await sheet.getByRole("button", { name: /Show exactly what's sent/ }).count(), 0);

  await sheet.getByLabel("What went wrong?").fill("Apps tab is empty");
  await sheet.getByRole("button", { name: /Report on GitHub instead/ }).click();
  await sheet.getByText("The GitHub form opened.", { exact: false }).waitFor();
  const opened = await page.evaluate(() => window.__OPENED__);
  const url = new URL(opened.at(-1));
  assert.equal(`${url.origin}${url.pathname}`, "https://github.com/bryanroscoe/shield_optimizer/issues/new");
  assert.equal(url.searchParams.get("template"), "bug_report.yml");
  assert.equal(url.searchParams.get("what-happened"), "Apps tab is empty");
  assert.match(url.searchParams.get("diagnostics"), /ATV Optimizer diagnostics/);
  const [diag] = await calls(page, "collect_diagnostics");
  assert.equal(diag.publicReport, true, "only the allowlisted public report goes to GitHub");
  assert.equal((await calls(page, "send_bug_report")).length, 0);
});
