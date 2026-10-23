import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { cp, mkdtemp, mkdir, readFile, readdir, rm, symlink, access } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { test } from "node:test";

const mobile = fileURLToPath(new URL("../", import.meta.url));
const desktop = dirname(mobile.replace(/\/$/, ""));
const execute = promisify(execFile);
async function bundleText(path) {
  const files = await readdir(join(path, "assets"));
  return (await Promise.all(files.filter(f => f.endsWith(".js")).map(f => readFile(join(path, "assets", f), "utf8")))).join("\n");
}

test("mobile production and demo builds need no desktop generated files or dependencies", { timeout: 120000 }, async (t) => {
  const root = await mkdtemp(join(tmpdir(), "atvopt-mobile-build-"));
  t.after(() => rm(root, { recursive: true, force: true }));
  const isolatedDesktop = join(root, "v2");
  const isolatedMobile = join(isolatedDesktop, "mobile");
  await mkdir(isolatedMobile, { recursive: true });
  for (const name of ["src", "index.html", "package.json", "tsconfig.json", "vite.config.ts", "svelte.config.js"]) {
    await cp(join(mobile, name), join(isolatedMobile, name), { recursive: true });
  }
  await cp(join(desktop, "shared"), join(isolatedDesktop, "shared"), { recursive: true });
  await mkdir(join(isolatedDesktop, "src"), { recursive: true });
  await cp(join(desktop, "src/lib"), join(isolatedDesktop, "src/lib"), { recursive: true });
  for (const name of ["package.json", "tsconfig.json"]) await cp(join(desktop, name), join(isolatedDesktop, name));
  await symlink(join(mobile, "node_modules"), join(isolatedMobile, "node_modules"), process.platform === "win32" ? "junction" : "dir");
  const env = { ...process.env };
  delete env.VITE_DEMO;
  const vite = join(mobile, "node_modules/vite/bin/vite.js");
  await execute(process.execPath, [vite, "build"], { cwd: isolatedMobile, env, maxBuffer: 4 * 1024 * 1024 });
  const production = await bundleText(join(isolatedMobile, "build"));
  assert.ok(!production.includes("demo@example.com"), "production must exclude the demo entitlement fixture");
  assert.ok(!production.includes("Fast control channel ready."), "production must exclude the simulated transport reply");
  await execute(process.execPath, [vite, "build", "--outDir", "demo-build"], { cwd: isolatedMobile, env: { ...env, VITE_DEMO: "1" }, maxBuffer: 4 * 1024 * 1024 });
  const demo = await bundleText(join(isolatedMobile, "demo-build"));
  assert.ok(demo.includes("demo@example.com"), "explicit demo builds must retain their fixture");
  assert.ok(demo.includes("Fast control channel ready."));
  await assert.rejects(access(join(isolatedDesktop, ".svelte-kit")), "no desktop sync dependency");
  await assert.rejects(access(join(isolatedDesktop, "node_modules")), "no desktop dependency installation");
});
