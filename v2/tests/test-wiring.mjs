// Every desktop test must run in CI. A test file with no package.json script,
// or a script the v2 Tests workflow never calls, passes locally and silently
// stops guarding anything. Helpers live under tests/lib/, not beside tests.
// Tests that start Vite must take their port and working directory from
// tests/lib/free-port.mjs (see there for why).

import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const TESTS = dirname(fileURLToPath(import.meta.url));
const V2 = join(TESTS, "..");
const scripts = JSON.parse(readFileSync(join(V2, "package.json"), "utf8")).scripts;
const workflow = readFileSync(join(V2, "..", ".github", "workflows", "v2-tests.yml"), "utf8");

const problems = [];
const scriptText = Object.values(scripts).join("\n");
const files = readdirSync(TESTS).filter((f) => f.endsWith(".mjs"));

for (const file of files) {
  if (!new RegExp(`node tests/${file.replace(/\./g, "\\.")}(\\s|$)`).test(scriptText)) {
    problems.push(`tests/${file} is not run by any package.json script`);
  }
  const source = readFileSync(join(TESTS, file), "utf8");
  if (/from ["']vite["']|import\(["']vite["']\)/.test(source) && !source.includes("./lib/free-port.mjs")) {
    problems.push(`tests/${file} starts Vite without tests/lib/free-port.mjs`);
  }
}

for (const name of Object.keys(scripts).filter((n) => n.startsWith("test:"))) {
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  if (!new RegExp(`npm run ${escaped}(?![\\w:-])`).test(workflow)) {
    problems.push(`npm script ${name} is not run by .github/workflows/v2-tests.yml`);
  }
}

assert.deepEqual(problems, [], `Test wiring gaps:\n  ${problems.join("\n  ")}`);
console.log(`Test wiring passed: ${files.length} test files have scripts, and every test:* script runs in CI.`);
