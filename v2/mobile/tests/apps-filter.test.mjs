import assert from "node:assert/strict";
import { createServer as createNetServer } from "node:net";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { createServer } from "vite";

const packages = [
  { package: "com.example.streambox", name: "StreamBox", system: false, enabled: true },
  { package: "com.example.notes", name: "Notes", system: false, enabled: false },
  { package: "com.android.protected", name: "Android Protected", system: true, enabled: true },
  { package: "com.android.helper", name: "System Helper", system: true, enabled: false },
];

let server;
let browser;
let origin;

async function reservePort() {
  const socket = createNetServer();
  await new Promise((resolve, reject) => {
    socket.once("error", reject);
    socket.listen(0, "127.0.0.1", resolve);
  });
  const address = socket.address();
  if (!address || typeof address === "string") throw new Error("Could not reserve a test port");
  await new Promise((resolve, reject) =>
    socket.close((error) => (error ? reject(error) : resolve())),
  );
  return address.port;
}

before(async () => {
  const port = await reservePort();
  server = await createServer({
    root: fileURLToPath(new URL("../", import.meta.url)),
    logLevel: "silent",
    server: { host: "127.0.0.1", port, strictPort: true },
  });
  await server.listen();
  origin = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch({ headless: true });
});

after(async () => {
  await browser?.close();
  await server?.close();
});

async function openApps(t, fixture = packages) {
  const context = await browser.newContext({ viewport: { width: 384, height: 812 } });
  const page = await context.newPage();
  t.after(() => context.close());
  await page.addInitScript((fixture) => {
    window.calls = [];
    window.__TAURI_INTERNALS__ = {
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        switch (command) {
          case "get_entitlement":
            return "pro";
          case "wireless_connect":
            return { ok: true, message: "" };
          case "list_devices":
            return [{
              id: 1,
              serial: "TV:5555",
              name: "TV",
              model: "Shield",
              status: "device",
              connection: "network",
              device_type: "shield",
              properties: null,
            }];
          case "wireless_status":
            return { connected: true, serial: "TV:5555", host: "TV" };
          case "list_other_packages":
            return fixture.map((app) => ({ ...app }));
          case "app_memory_map":
          case "app_usage_map":
            return {};
          case "safety_info":
            return args.package === "com.android.protected"
              ? { kind: "never_disable", reason: "Required by Android" }
              : { kind: "safe" };
          default:
            return { ok: true, message: "" };
        }
      },
    };
  }, fixture);
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session;
    session.entitlement = "pro";
    await session.connect("TV", 5555);
    router.reset("dashboard");
    router.navigate("apps");
  });
  await page.getByRole("switch", { name: /Show system apps/ }).waitFor();
  return page;
}

async function rowNames(page) {
  return page.locator(".app-row .app-name-text").allTextContents();
}

test("system visibility composes with enabled and disabled filters", async (t) => {
  const page = await openApps(t);
  const systemToggle = page.getByRole("switch", { name: /Show system apps/ });

  assert.equal(await systemToggle.getAttribute("aria-checked"), "false");
  assert.deepEqual(await rowNames(page), ["StreamBox", "Notes"]);
  await page.getByText("2 of 4 visible", { exact: true }).waitFor();

  await systemToggle.click();
  assert.deepEqual(await rowNames(page), ["StreamBox", "Notes", "Android Protected", "System Helper"]);
  await page.getByText("4 of 4 visible", { exact: true }).waitFor();

  await page.getByRole("button", { name: "Enabled", exact: true }).click();
  assert.deepEqual(await rowNames(page), ["StreamBox", "Android Protected"]);
  await page.getByText("2 of 4 visible", { exact: true }).waitFor();

  await page.getByRole("button", { name: "Disabled", exact: true }).click();
  assert.deepEqual(await rowNames(page), ["Notes", "System Helper"]);

  await systemToggle.click();
  assert.deepEqual(await rowNames(page), ["Notes"]);
  await page.getByText("1 of 4 visible", { exact: true }).waitFor();
});

test("search reports empty results and can reveal or clear them", async (t) => {
  const page = await openApps(t, packages.filter((app) => !app.enabled));
  const search = page.getByPlaceholder("Search apps or packages");

  await search.fill("helper");
  await page.getByRole("heading", { name: "No matching apps" }).waitFor();
  await page.getByText("0 results", { exact: true }).waitFor();
  await page.getByRole("button", { name: "Show matching system apps" }).click();
  assert.deepEqual(await rowNames(page), ["System Helper"]);
  await page.getByText("1 result", { exact: true }).waitFor();

  await page.getByRole("button", { name: "Enabled", exact: true }).click();
  await page.getByRole("heading", { name: "No matching apps" }).waitFor();
  await page.getByText("0 results", { exact: true }).waitFor();

  await page.getByRole("button", { name: "Clear search" }).click();
  await page.getByText("0 of 2 visible", { exact: true }).waitFor();
  await page.getByText("No enabled apps are available in this non-curated list.", { exact: true }).waitFor();
  assert.equal(await page.getByText(/user-installed/).count(), 0);
  assert.deepEqual(await rowNames(page), []);
});

test("filtering never mutates packages and protected-app safety stays enforced", async (t) => {
  const page = await openApps(t);

  await page.getByRole("switch", { name: /Show system apps/ }).click();
  await page.getByRole("button", { name: "Disabled", exact: true }).click();
  await page.getByPlaceholder("Search apps or packages").fill("notes");
  await page.getByText("Notes", { exact: true }).waitFor();
  await page.getByRole("button", { name: "All", exact: true }).click();
  await page.getByPlaceholder("Search apps or packages").fill("");
  await page.getByText("Android Protected", { exact: true }).click();

  const disable = page.getByRole("button", { name: "Disable", exact: true });
  const uninstall = page.getByRole("button", { name: /Uninstall/ });
  await page.getByText("This package is protected", { exact: false }).waitFor();
  assert.equal(await disable.isDisabled(), true);
  assert.equal(await uninstall.isDisabled(), true);

  const commands = await page.evaluate(() => window.calls.map((call) => call.command));
  assert.equal(commands.filter((command) => command === "list_other_packages").length, 1);
  assert.equal(commands.filter((command) => command === "safety_info").length, 1);
  assert.deepEqual(
    commands.filter((command) =>
      ["disable_package", "enable_package", "uninstall_package", "force_stop", "open_play_store"].includes(command),
    ),
    [],
  );
});
