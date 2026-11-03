import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");
const evidence = process.env.SHIELD_APP_STATE_EVIDENCE;
const previousDemo = process.env.VITE_DEMO;
const previousHost = process.env.TAURI_DEV_HOST;
process.env.VITE_DEMO = "1";
delete process.env.TAURI_DEV_HOST;

let server, browser, context, recording;
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  const { createServer } = await import("vite");
  const { chromium } = await import("playwright");
  server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
  await server.listen();
  browser = await chromium.launch();
  context = await browser.newContext({
    viewport: { width: 1280, height: 1000 },
    ...(evidence ? { recordVideo: { dir: evidence, size: { width: 1280, height: 1000 } } } : {}),
  });
  const page = await context.newPage();
  recording = page.video();
  page.setDefaultTimeout(10000);
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: "networkidle" });
  const curated = await page.evaluate(() => {
    const original = window.__TAURI_INTERNALS__.invoke;
    window.__APP_STATE_TEST__ = { curated: [], mutations: [] };
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      if (["disable_package", "enable_package", "uninstall_package", "reinstall_existing"].includes(command)) {
        window.__APP_STATE_TEST__.mutations.push(command);
        throw new Error("Filter tests must not mutate packages");
      }
      const result = await original(command, args);
      if (command === "app_list_for_device") {
        const entries = result.slice(0, 4);
        window.__APP_STATE_TEST__.curated = entries.map((entry) => entry.package);
        return entries;
      }
      if (command === "package_states") {
        const packages = window.__APP_STATE_TEST__.curated;
        // The fourth package was not answered by the inventory read.
        return { [packages[0]]: "enabled", [packages[1]]: "disabled", [packages[2]]: "missing" };
      }
      if (command === "list_other_packages") return [
        { package: "com.teamsmart.videomanager.tv", name: "Fixture enabled app", enabled: true, system: false },
        { package: "ca.devmesh.overseerrtv", name: "Fixture disabled app", enabled: false, system: false },
        { package: "com.android.vending", name: "Fixture enabled system", enabled: true, system: true },
        { package: "com.nvidia.ota", name: "Fixture disabled system", enabled: false, system: true },
      ];
      if (command === "health_report") {
        const pkg = window.__APP_STATE_TEST__.curated[0];
        return { ...result, top_memory: [{ process: pkg, package: pkg, pid: 1234, mb: 12 }] };
      }
      return result;
    };
    return original("app_list_for_device", { deviceType: "shield" }).then((apps) => apps.slice(0, 4).map((entry) => entry.package));
  });
  if (evidence) await page.evaluate(() => {
    const label = document.createElement("div");
    label.textContent = "SIMULATION — fixture package states; no TV connected or changed";
    label.style.cssText = "position:fixed;bottom:0;left:0;right:0;background:#111;color:white;padding:8px;z-index:99999;pointer-events:none";
    document.body.append(label);
  });

  const apps = page.locator("#tabpanel-apps");
  const state = page.getByRole("combobox", { name: "Package state", exact: true });
  const search = page.locator(".app-search");
  const showSystem = page.getByRole("checkbox", { name: "Show system packages", exact: true });
  const hideMissing = page.getByRole("checkbox", { name: "Hide not installed", exact: true });
  const hideDecided = page.getByRole("checkbox", { name: "Hide decided", exact: true });
  async function packages(table, expected) {
    await page.waitForFunction(({ table, expected }) => {
      const tableElement = document.querySelectorAll("#tabpanel-apps table.app-table")[table];
      const actual = tableElement ? [...tableElement.querySelectorAll(".pkg-id")].map((el) => el.textContent.trim()) : [];
      return JSON.stringify(actual) === JSON.stringify(expected);
    }, { table, expected });
  }
  async function counts(curatedCount, otherCount) {
    await page.waitForFunction(({ curatedCount, otherCount }) =>
      document.querySelector("#tabpanel-apps .header-actions")?.textContent.includes(`${curatedCount} of 4 curated · ${otherCount} of 4 other`),
    { curatedCount, otherCount });
  }

  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "App List", exact: true }).click();
  await state.waitFor();
  assert.equal(await state.inputValue(), "all", "All states is the default");
  await packages(0, [curated[0], curated[1], curated[3]]);
  await packages(1, ["com.teamsmart.videomanager.tv", "ca.devmesh.overseerrtv"]);
  await counts(3, 2);
  if (evidence) await page.waitForTimeout(1000);

  await state.selectOption("enabled");
  await packages(0, [curated[0]]);
  await packages(1, ["com.teamsmart.videomanager.tv"]);
  await counts(1, 1);
  if (evidence) await page.waitForTimeout(1000);
  await state.selectOption("disabled");
  await packages(0, [curated[1]]);
  await packages(1, ["ca.devmesh.overseerrtv"]);
  await counts(1, 1);
  if (evidence) await page.screenshot({ path: join(evidence, "disabled-only-simulated.png") });
  if (evidence) await page.waitForTimeout(1500);

  await showSystem.check();
  await packages(1, ["ca.devmesh.overseerrtv", "com.nvidia.ota"]);
  await search.fill("system");
  await packages(0, []);
  await packages(1, ["com.nvidia.ota"]);
  await counts(0, 1);
  await search.fill("does-not-match-any-package");
  await packages(0, []);
  await packages(1, []);
  await counts(0, 0);
  assert.equal(await apps.getByText("No curated apps match your filters.", { exact: true }).count(), 1);
  assert.equal(await apps.getByText("Nothing matches your filters.", { exact: true }).count(), 1);

  await search.fill("");
  await hideMissing.uncheck();
  await packages(0, [curated[1]]);
  await state.selectOption("all");
  await packages(0, curated);
  await packages(1, ["com.teamsmart.videomanager.tv", "ca.devmesh.overseerrtv", "com.android.vending", "com.nvidia.ota"]);
  await counts(4, 4);
  if (evidence) await page.screenshot({ path: join(evidence, "all-states-simulated.png") });
  if (evidence) await page.waitForTimeout(1000);

  await hideDecided.check();
  await state.selectOption("enabled");
  await packages(0, [curated[0]]);
  await hideDecided.uncheck();
  await state.selectOption("disabled");
  await apps.getByRole("button", { name: "Refresh", exact: true }).click();
  await packages(0, [curated[1]]);
  await counts(1, 2);
  assert.equal(await state.inputValue(), "disabled", "Refresh preserves the state filter");

  await page.getByRole("tab", { name: "Health", exact: true }).click();
  await page.locator(`table.mem-table tr[data-package="${curated[0]}"]`).click();
  await state.waitFor();
  assert.equal(await state.inputValue(), "all", "opening a Health package clears a conflicting state filter");
  assert.equal(await search.inputValue(), curated[0]);
  await packages(0, [curated[0]]);

  await state.selectOption("disabled");
  await page.getByRole("button", { name: "Back to devices", exact: false }).click();
  await page.getByText("Living Room Box", { exact: true }).first().click();
  await page.getByRole("tab", { name: "App List", exact: true }).click();
  await state.waitFor();
  assert.equal(await state.inputValue(), "all", "opening a different TV starts with All states");
  assert.equal(await search.inputValue(), "", "the previous TV's search is cleared too");
  await packages(0, [curated[0], curated[1], curated[3]]);

  assert.deepEqual(await page.evaluate(() => window.__APP_STATE_TEST__.mutations), [], "filter controls issue no package mutations");
  assert.deepEqual(errors, [], "the filter flow raises no page errors");
  console.log("App state filters passed: enabled/disabled/all, unknown and missing exclusion, both inventories, search/system/decided filters, empty counts, Refresh and Health navigation.");
} finally {
  await context?.close();
  if (evidence && recording) {
    await recording.saveAs(join(evidence, "app-state-filter-simulated.webm"));
    await recording.delete();
  }
  await browser?.close();
  await server?.close();
  if (previousDemo === undefined) delete process.env.VITE_DEMO;
  else process.env.VITE_DEMO = previousDemo;
  if (previousHost === undefined) delete process.env.TAURI_DEV_HOST;
  else process.env.TAURI_DEV_HOST = previousHost;
}
