// An app on the do-not-disable list has its Disable and Uninstall buttons
// switched off. The hint on them must say why ("Protected — <reason>"), not
// that a safety check is still missing.
//
// Run via `node tests/protected-remove-tip.mjs` from v2/.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");
const PROTECTED = "com.teamsmart.videomanager.tv";
const REASON = "Fixture: the TV needs this to boot";

async function main() {
  const previous = new Map(["VITE_DEMO", "TAURI_DEV_HOST"].map((key) => [key, process.env[key]]));
  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;
  let server;
  let browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
    await server.listen();
    browser = await chromium.launch();
    const page = await browser.newPage({ viewport: { width: 1280, height: 1000 } });
    page.setDefaultTimeout(10000);
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: "networkidle" });
    await page.evaluate(({ pkg, reason }) => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (["disable_package", "uninstall_package"].includes(command)) throw new Error("must not mutate");
        if (command === "safety_info" && args?.package === pkg) {
          return { kind: "never_disable", reason, source: "protected_list" };
        }
        return original(command, args);
      };
    }, { pkg: PROTECTED, reason: REASON });

    await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
    await page.locator("#tab-apps").click();
    const row = page.locator("#tabpanel-apps tr", { has: page.locator(".pkg-id", { hasText: PROTECTED }) }).first();
    const disable = row.getByRole("button", { name: "Disable", exact: true });
    await disable.waitFor();
    await page.waitForFunction(
      (pkg) => [...document.querySelectorAll("#tabpanel-apps tr")]
        .find((tr) => tr.querySelector(".pkg-id")?.textContent === pkg)
        ?.querySelector(".verdict-cell")?.dataset.verdict === "never_disable",
      PROTECTED,
    );
    assert.equal(await disable.isDisabled(), true, "a protected app cannot be disabled");
    for (const name of ["Disable", "Uninstall"]) {
      const tip = await row.getByRole("button", { name, exact: true }).getAttribute("data-tip");
      assert.equal(tip, `Protected — ${REASON}`, `${name} says why it is off`);
    }

    console.log("Protected remove tip passed: Disable and Uninstall on a protected app say Protected and the reason.");
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    for (const [key, value] of previous) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
