import assert from "node:assert/strict";
import { readdir, readFile, stat } from "node:fs/promises";
import { dirname, extname, join, relative, resolve } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { OPTIMIZE_DEPS_INCLUDE } from "./helpers/vite-harness.mjs";

// A dependency Vite discovers only on a page load re-optimizes and reloads the
// page mid-test, so every bare package the app reaches is pre-bundled by the
// test harness up front.
const mobile = fileURLToPath(new URL("../", import.meta.url));
const src = join(mobile, "src");

async function sourceFiles(dir) {
  const out = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...(await sourceFiles(path)));
    else if (/\.(ts|svelte)$/.test(entry.name)) out.push(path);
  }
  return out;
}

const SPECIFIER = /\b(?:import|export)\s+(type\s+)?(?:[^;'"]*?\bfrom\s*)?["']([^"']+)["']|\bimport\(\s*["']([^"']+)["']\s*\)/g;

function specifiers(source) {
  const found = [];
  for (const match of source.matchAll(SPECIFIER)) {
    if (match[1]) continue; // type-only, erased before Vite sees it
    found.push(match[2] ?? match[3]);
  }
  return found;
}

async function resolveRelative(from, specifier) {
  const base = resolve(dirname(from), specifier);
  for (const candidate of [base, `${base}.ts`, `${base}.svelte`, join(base, "index.ts")]) {
    try {
      if ((await stat(candidate)).isFile() && /\.(ts|svelte)$/.test(extname(candidate))) return candidate;
    } catch {
      // try the next candidate
    }
  }
  return null;
}

// src/** plus everything it reaches through relative imports (v2/shared).
async function barePackages() {
  const queue = await sourceFiles(src);
  const seen = new Set(queue);
  const packages = new Map();
  while (queue.length) {
    const file = queue.pop();
    for (const specifier of specifiers(await readFile(file, "utf8"))) {
      if (specifier.startsWith(".")) {
        const next = await resolveRelative(file, specifier);
        if (next && !seen.has(next)) {
          seen.add(next);
          queue.push(next);
        }
      } else if (!specifier.startsWith("/") && !specifier.includes(":")) {
        if (!packages.has(specifier)) packages.set(specifier, relative(mobile, file));
      }
    }
  }
  return packages;
}

test("the harness pre-bundles every bare package the app imports", async () => {
  const packages = await barePackages();
  assert.ok(packages.has("@tauri-apps/api/core"), "the import scan found nothing; fix the scan");
  // vite-plugin-svelte adds svelte's own entry points to optimizeDeps itself.
  const missing = [...packages]
    .filter(([name]) => name !== "svelte" && !name.startsWith("svelte/"))
    .filter(([name]) => !OPTIMIZE_DEPS_INCLUDE.includes(name))
    .map(([name, file]) => `${name} (first seen in ${file})`);
  assert.deepEqual(missing, [], "add these to OPTIMIZE_DEPS_INCLUDE in tests/helpers/vite-harness.mjs");
});

test("no .svelte script spells an HTML comment marker that breaks Vite's dependency scan", async () => {
  const offenders = [];
  for (const file of await sourceFiles(src)) {
    if (!file.endsWith(".svelte")) continue;
    const source = await readFile(file, "utf8");
    for (const [script] of source.matchAll(/<script\b[\s\S]*?<\/script>/g)) {
      if (script.includes("<!--") || script.includes("-->")) offenders.push(relative(mobile, file));
    }
  }
  assert.deepEqual(offenders, []);
});
