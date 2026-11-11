import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { test } from "node:test";

// Every text field and toggle needs an accessible name: an aria-label, an
// aria-labelledby, or an enclosing <label>. A placeholder is not a name.
const root = new URL("../src/", import.meta.url);

async function svelteFiles(dir) {
  const out = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const url = new URL(entry.name + (entry.isDirectory() ? "/" : ""), dir);
    if (entry.isDirectory()) out.push(...(await svelteFiles(url)));
    else if (entry.name.endsWith(".svelte")) out.push(url);
  }
  return out;
}

function insideLabel(markup, index) {
  const open = markup.lastIndexOf("<label", index);
  return open >= 0 && markup.lastIndexOf("</label>", index) < open;
}

test("every <input> in the mobile UI has an accessible name", async () => {
  const unnamed = [];
  for (const file of await svelteFiles(root)) {
    const source = await readFile(file, "utf8");
    const markup = source.replace(/<script[\s\S]*?<\/script>/g, (m) => " ".repeat(m.length));
    for (const match of markup.matchAll(/<input\b(?:[^>{]|\{[^}]*\})*>/g)) {
      const tag = match[0];
      if (/type="(hidden|file)"/.test(tag)) continue;
      if (/aria-label(ledby)?=/.test(tag) || insideLabel(markup, match.index)) continue;
      const line = markup.slice(0, match.index).split("\n").length;
      unnamed.push(`${file.pathname.split("/src/")[1]}:${line}`);
    }
  }
  assert.deepEqual(unnamed, []);
});
