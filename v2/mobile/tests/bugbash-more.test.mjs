import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

// More screen regressions from the 2026-11-05 bug bash: report opt-out that
// couldn't be saved, license details after activation, debug-log refresh,
// third-party notices rendering, header and reboot layout, and the Android
// backup exclusion that keeps the ADB key on the phone.

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

function status(over = {}) {
  const license = over.license ?? false;
  return {
    entitlement: license ? "pro" : "free",
    sources: license ? ["license"] : [],
    distribution: "sideload",
    license_key_entry: true,
    available: false,
    play: { state: "unavailable", reason: "Google Play purchases aren't available in this build." },
    offer: null,
    ack_pending: false,
    product_id: "atvopt_pro",
  };
}

async function openMore(t, { telemetry, billingStatus = status(), notices } = {}) {
  const page = await browser.newPage({ viewport: { width: 384, height: 1400 } });
  t.after(() => page.close());
  if (notices !== undefined) {
    await page.route("**/src/lib/notices.generated.ts*", (route) =>
      route.fulfill({
        contentType: "text/javascript",
        body: `export const THIRD_PARTY_NOTICES_VERSION = "test";\nexport const THIRD_PARTY_NOTICES = ${JSON.stringify(notices)};\n`,
      }),
    );
  }
  await page.addInitScript((init) => {
    window.calls = [];
    window.handlers = {};
    window.telemetry = init.telemetry ?? { enabled: true, available: true, notice_seen: true, support_id: null };
    window.billingStatus = init.billingStatus;
    window.nativeLog = "native line 1";
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 0,
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        if (window.handlers[command]) return window.handlers[command](args);
        switch (command) {
          case "get_entitlement": return window.billingStatus.entitlement;
          case "billing_status":
          case "billing_refresh": return window.billingStatus;
          case "license_info": return null;
          case "list_devices": return [{
            id: 1, serial: "A:5555", name: "A", model: "Shield", status: "device",
            connection: "network", device_type: "shield", properties: null,
          }];
          case "wireless_status": return { connected: true };
          case "wireless_connect": return { ok: true, message: "connected" };
          case "telemetry_settings": return window.telemetry;
          case "set_telemetry_enabled":
            window.telemetry = { ...window.telemetry, enabled: args.enabled };
            return window.telemetry;
          case "read_debug_log": return window.nativeLog;
          case "telemetry_action":
          case "telemetry_error":
            return null;
          default: return { ok: true, message: "done" };
        }
      },
    };
  }, { telemetry, billingStatus });
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.api = (await import("/src/lib/api.ts")).api;
    await session.connect("A", 5555);
    router.reset("dashboard");
    router.navigate("more");
  });
  await page.getByRole("heading", { name: "More" }).waitFor();
  return page;
}

const calls = (page, command) =>
  page.evaluate((c) => window.calls.filter((call) => call.command === c).map((call) => call.args), command);

test("Android backup and device transfer exclude every app-data domain", async () => {
  const android = new URL("../src-tauri/gen/android/app/src/main/", import.meta.url);
  const manifest = await readFile(new URL("AndroidManifest.xml", android), "utf8");
  const application = manifest.match(/<application\b[^>]*>/s)?.[0] ?? "";
  assert.match(application, /android:allowBackup="false"/);
  assert.match(application, /android:fullBackupContent="false"/);
  assert.match(application, /android:dataExtractionRules="@xml\/data_extraction_rules"/);

  const rules = await readFile(new URL("res/xml/data_extraction_rules.xml", android), "utf8");
  assert.doesNotMatch(rules, /<include\b/, "an include would re-enable backup of that path");
  const domains = ["root", "file", "database", "sharedpref", "external", "device_root", "device_file", "device_database", "device_sharedpref"];
  for (const section of ["cloud-backup", "device-transfer"]) {
    const body = rules.match(new RegExp(`<${section}>([\\s\\S]*?)</${section}>`))?.[1];
    assert.ok(body, `${section} section present`);
    for (const domain of domains) {
      assert.match(body, new RegExp(`<exclude domain="${domain}" path="\\." />`), `${section} excludes ${domain}`);
    }
  }
});

test("an opt-out that couldn't be saved says reports may come back, and retry saves it", async (t) => {
  const page = await openMore(t);
  await page.evaluate(() => {
    // The backend turns reports off for this session, then fails to persist.
    window.handlers.set_telemetry_enabled = (args) => {
      window.telemetry = { ...window.telemetry, enabled: args.enabled, support_id: null };
      throw "Couldn't save telemetry.json: read-only file system";
    };
  });
  const toggle = page.getByRole("switch", { name: "Share anonymous device and error reports" });
  await toggle.click();
  const warning = page.getByRole("alert").filter({ hasText: "couldn't be saved" });
  await warning.waitFor();
  assert.match(await warning.innerText(), /Reports are off for now, but this choice couldn't be saved\. They may turn back on the next time the app starts\./);
  assert.equal(await toggle.isChecked(), false);

  // Still said after leaving More and coming back in the same session.
  await page.evaluate(async () => {
    const { router } = await import("/src/lib/router.svelte.ts");
    router.navigate("dashboard");
  });
  await warning.waitFor({ state: "detached" });
  await page.evaluate(async () => {
    const { router } = await import("/src/lib/router.svelte.ts");
    router.navigate("more");
  });
  await warning.waitFor();

  await page.evaluate(() => { delete window.handlers.set_telemetry_enabled; });
  await page.getByRole("button", { name: "Try saving again" }).click();
  await warning.waitFor({ state: "detached" });
  assert.deepEqual((await calls(page, "set_telemetry_enabled")).map((a) => a.enabled), [false, false]);
  await page.getByText("Off. Nothing is sent.").waitFor();
});

test("a failed turn-on doesn't claim reports were left off unsaved", async (t) => {
  const page = await openMore(t, { telemetry: { enabled: false, available: true, notice_seen: true, support_id: null } });
  await page.evaluate(() => {
    window.handlers.set_telemetry_enabled = () => { throw "backend unavailable"; };
  });
  await page.getByRole("switch", { name: "Share anonymous device and error reports" }).click();
  await page.getByText("Couldn't change the report setting.").waitFor();
  assert.equal(await page.getByText("couldn't be saved").count(), 0);
});

test("activating a license key shows its source and license details without a resume", async (t) => {
  const page = await openMore(t);
  await page.evaluate(() => {
    window.handlers.activate_license = () => {
      window.billingStatus = { ...window.billingStatus, entitlement: "pro", sources: ["license"] };
      return "pro";
    };
    window.handlers.license_info = () =>
      window.billingStatus.entitlement === "pro"
        ? { plan: "pro", licensee: "supporter@example.com", issued: "2026-01-01", expires: null, key_id: 1 }
        : null;
  });
  const card = page.locator(".more-card").filter({ hasText: "Licensing" });
  await card.getByPlaceholder("ATVOPT-…").fill("ATVOPT-TEST-KEY");
  await card.getByRole("button", { name: "Activate" }).click();
  await card.getByText("Source: License key").waitFor();
  await card.getByText("supporter@example.com").waitFor();
  await card.getByText("Perpetual").waitFor();
  assert.equal(await card.locator(".license-badge").textContent(), "Pro");
});

test("debug log Refresh and Copy include frontend calls made after it was opened", async (t) => {
  const page = await openMore(t);
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  const card = page.locator(".more-card").filter({ hasText: "Debug log" });
  await card.getByRole("button", { name: "Show debug log" }).click();
  const view = page.locator("pre.log-view").filter({ hasText: "# Native log" });
  await view.waitFor();
  assert.doesNotMatch(await view.innerText(), /list_launchers/);

  // Same native text, so only the frontend section can change.
  await page.evaluate(() => window.api.listLaunchers("A:5555"));
  await card.getByRole("button", { name: "Refresh" }).click();
  await page.waitForFunction(() => /list_launchers/.test(document.querySelector("pre.log-view")?.textContent ?? ""));

  await page.evaluate(() => window.api.getTweaks("A:5555"));
  await card.getByRole("button", { name: /Copy$/ }).click();
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  assert.match(copied, /get_tweaks/);
  assert.match(copied, /# Native log\nnative line 1/);
  assert.match(await view.innerText(), /get_tweaks/);
});

test("third-party notices render readably, without generator lines, and never as markup", async (t) => {
  const hostile = [
    "<!-- Generated by mobile/scripts/gen-notices.sh for version 9.9.9. Do not edit. -->",
    "# Third-party notices",
    "",
    "Intro paragraph that wraps",
    "onto a second line.",
    "",
    "Regenerate this document with `mobile/scripts/gen-notices.sh` after any dependency change.",
    "",
    "## Components",
    "",
    "- first <img src=x onerror=\"window.pwned=1\">",
    "- second with [a link](https://example.com/license)",
    "",
    "| Package | License |",
    "| --- | --- |",
    "| `svelte` | [MIT](https://opensource.org/license/mit) |",
    "| <script>window.pwned=2</script> | [bad](javascript:window.pwned=3) |",
    "",
    "<details>",
    "<summary>MIT license (full text)</summary>",
    "",
    "```",
    "Permission is hereby granted <b>free</b> of charge",
    "<!-- not a comment inside a code block -->",
    "```",
    "",
    "</details>",
  ].join("\n");
  const page = await openMore(t, { notices: hostile });
  await page.getByRole("button", { name: "Third-party notices" }).click();
  const notices = page.getByTestId("third-party-notices");
  await notices.waitFor();

  const text = await notices.innerText();
  assert.doesNotMatch(text, /Generated by|Do not edit|Regenerate this document/);
  await notices.getByRole("heading", { name: "Third-party notices" }).waitFor();
  await notices.getByRole("heading", { name: "Components" }).waitFor();
  await notices.getByRole("heading", { name: "MIT license (full text)" }).waitFor();
  assert.match(text, /Intro paragraph that wraps onto a second line\./);
  assert.equal(await notices.locator("li").count(), 2);
  assert.equal(await notices.locator("table tbody tr").count(), 2);
  assert.equal(await notices.locator("table th").first().innerText(), "Package");

  // Everything stays text: no element came from the document's own HTML.
  assert.equal(await notices.locator("img, script, b, details, summary").count(), 0);
  assert.match(text, /first <img src=x onerror="window\.pwned=1">/);
  assert.match(text, /<script>window\.pwned=2<\/script>/);
  assert.match(await notices.locator("pre").innerText(), /<!-- not a comment inside a code block -->/);
  assert.equal(await page.evaluate(() => window.pwned), undefined);

  // Only web links become links; they go through the scoped opener.
  assert.equal(await notices.getByRole("link", { name: "bad" }).count(), 0);
  await notices.getByRole("link", { name: "MIT" }).click();
  const [opened] = await calls(page, "plugin:opener|open_url");
  assert.equal(opened.url, "https://opensource.org/license/mit");
  assert.equal(new URL(page.url()).origin, origin, "the app itself never navigates");
});

test("the real notices document parses without its generator lines", async (t) => {
  const page = await openMore(t);
  await page.getByRole("button", { name: "Third-party notices" }).click();
  const notices = page.getByTestId("third-party-notices");
  await notices.waitFor();
  const text = await notices.innerText();
  assert.doesNotMatch(text, /<!--|Do not edit|Regenerate this document|<details>|<summary>/);
  await notices.getByRole("heading", { name: "Third-party notices" }).waitFor();
  assert.ok((await notices.locator("pre").count()) > 10, "license texts keep their preformatted blocks");
  assert.ok((await notices.locator("table").count()) >= 2);
});

test("More's title sits beside the remote button like the other tabs, and reboot options stay grouped", async (t) => {
  const page = await openMore(t);
  const title = await page.getByRole("heading", { name: "More" }).boundingBox();
  const remote = await page.locator(".topline .header-left").boundingBox();
  assert.ok(title.x - (remote.x + remote.width) < 40, `title starts right after the remote button (gap ${title.x - remote.x - remote.width}px)`);

  const card = page.locator(".more-card").filter({ hasText: "Every option restarts the TV" });
  const layout = async () => {
    const boxes = [];
    for (const name of ["Restart", "Recovery mode", "Bootloader"]) {
      boxes.push(await card.getByRole("button", { name }).boundingBox());
    }
    return {
      widths: new Set(boxes.map((b) => Math.round(b.width))).size,
      rows: new Set(boxes.map((b) => Math.round(b.y))).size,
    };
  };
  // A phone: one column of equal buttons. Wider: all three in one row.
  assert.deepEqual(await layout(), { widths: 1, rows: 3 });
  await page.setViewportSize({ width: 760, height: 1400 });
  assert.deepEqual(await layout(), { widths: 1, rows: 1 });
});
