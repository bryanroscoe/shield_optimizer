// The release-notes parser turns remote Markdown into structured blocks. It
// must never produce HTML, must never lose a line, and must refuse a link
// scheme it cannot vouch for. Exercised against the notes actually shipped in
// v2/updater/latest.json, not a hand-made fixture.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const v2Root = dirname(dirname(fileURLToPath(import.meta.url)));
const source = readFileSync(join(v2Root, "src/lib/release-notes.ts"), "utf8");
const module = await import(
  "data:text/javascript," +
    encodeURIComponent(stripTypeScriptTypes(source, { mode: "strip" }))
);
const { parseReleaseNotes, stripTrailingBoilerplate } = module;

const text = (block) => block.spans.map((s) => s.text).join("");

// --- the real shipped notes -------------------------------------------------
const manifest = JSON.parse(
  readFileSync(join(v2Root, "updater/latest.json"), "utf8"),
);
const real = parseReleaseNotes(manifest.notes);

// Expectations come from the manifest itself, which every release replaces.
const shown = stripTrailingBoilerplate(manifest.notes);
const firstHeading = shown.match(/^###\s+(.+)$/m)?.[1].trim();
const firstItemWords = shown
  .match(/^- (.+)$/m)?.[1]
  .replace(/[*`]/g, "")
  .split(/\s+/)
  .slice(0, 4)
  .join(" ");
assert.ok(firstItemWords, "fixture has a list item");

// A hotfix's notes can be one paragraph and a short list with no headings
// (2.5.1), so only the list is required of every release.
assert.equal(
  real.filter((b) => b.kind === "item").length,
  shown.split("\n").filter((line) => line.startsWith("- ")).length,
  "every list item in the shipped notes becomes one item block",
);
if (firstHeading) {
  assert.ok(
    real.some((b) => b.kind === "heading" && text(b) === firstHeading),
    `section headings survive: ${firstHeading}`,
  );
}
assert.ok(
  real.some((b) => b.kind === "item" && text(b).includes(firstItemWords)),
  `list items survive: ${firstItemWords}`,
);
// The workflow appends Gatekeeper/SmartScreen boilerplate after a --- rule.
// Useful on the releases page, noise inside a running app.
assert.ok(
  manifest.notes.includes("First-run warnings"),
  "fixture really does contain the boilerplate",
);
assert.ok(
  !real.some((b) => text(b).includes("First-run warnings")),
  "boilerplate is trimmed",
);
assert.ok(
  !real.some((b) => text(b).includes("SmartScreen")),
  "and so is everything after it",
);

// --- structure --------------------------------------------------------------
const blocks = parseReleaseNotes(
  [
    "Lead paragraph that",
    "wraps across lines.",
    "",
    "### Devices and pairing",
    "",
    "- **Bold lead.** Then prose.",
    "- A wrapped item that continues",
    "  on the next line with two spaces.",
    "- Run `npm run check` to verify.",
    "- See [the README](https://example.com/readme) for more.",
  ].join("\n"),
);

assert.equal(blocks[0].kind, "paragraph");
assert.equal(text(blocks[0]), "Lead paragraph that wraps across lines.");
assert.deepEqual(
  { kind: blocks[1].kind, level: blocks[1].level, text: text(blocks[1]) },
  { kind: "heading", level: 3, text: "Devices and pairing" },
);

const bold = blocks[2];
assert.equal(bold.kind, "item");
assert.equal(bold.spans[0].bold, true);
assert.equal(bold.spans[0].text, "Bold lead.");
assert.equal(text(bold), "Bold lead. Then prose.");

assert.equal(
  text(blocks[3]),
  "A wrapped item that continues on the next line with two spaces.",
  "a wrapped list item is one item, not an item plus a paragraph",
);

const code = blocks[4].spans.find((s) => s.code);
assert.equal(code.text, "npm run check");

const link = blocks[5].spans.find((s) => s.href);
assert.deepEqual(
  { text: link.text, href: link.href },
  { text: "the README", href: "https://example.com/readme" },
);

// --- safety -----------------------------------------------------------------
// Blocks are data; the template escapes them. Nothing may carry raw HTML
// through as markup, and no non-http scheme may become a link.
const hostile = parseReleaseNotes(
  [
    "<script>alert(1)</script> and <b>markup</b>",
    "",
    "- [click me](javascript:alert(1))",
    "- [data](data:text/html,<script>alert(1)</script>)",
    "- [safe](https://example.com)",
  ].join("\n"),
);

assert.ok(
  text(hostile[0]).includes("<script>alert(1)</script>"),
  "raw HTML stays literal text, to be escaped by the template",
);
const hrefs = hostile.flatMap((b) => b.spans.filter((s) => s.href));
assert.equal(hrefs.length, 1, `only the https link became a link: ${JSON.stringify(hrefs)}`);
assert.equal(hrefs[0].href, "https://example.com");
assert.ok(
  hostile.some((b) => text(b).includes("javascript:alert(1)")),
  "a rejected link is shown as text rather than silently dropped",
);

// --- degradation ------------------------------------------------------------
assert.deepEqual(parseReleaseNotes(""), []);
assert.deepEqual(parseReleaseNotes("   \n\n  "), []);
const unmatched = parseReleaseNotes("**not closed and `not closed");
assert.equal(text(unmatched[0]), "**not closed and `not closed");
assert.equal(stripTrailingBoilerplate("no rule here"), "no rule here");

console.log(
  "Release notes passed: the shipped notes parse, boilerplate is trimmed, wrapped items rejoin, and only https links become links.",
);
