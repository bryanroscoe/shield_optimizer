// The after-update pop-up must open once per version increase and never on a
// relaunch at the same version.
//
// A version is recorded as seen only once its notes have actually been shown,
// never merely because a launch happened, and the notes come from the bundled
// CHANGELOG.md rather than a network call that can fail. `legacy` in the fake
// store is `shieldopt.lastSeenVersion`, the per-launch key builds through
// v2-2.3.0 wrote. These tests drive the pure rule and the bundled changelog.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const v2Root = dirname(dirname(fileURLToPath(import.meta.url)));
const load = (rel) =>
  import(
    "data:text/javascript," +
      encodeURIComponent(
        stripTypeScriptTypes(readFileSync(join(v2Root, rel), "utf8"), { mode: "strip" }),
      )
  );
const { decideArrival } = await load("src/lib/notes-seen.ts");
const { parseChangelog, recentReleases, notesFor } = await load("src/lib/changelog.ts");
const { isNewerVersion } = await load("src/lib/version.ts");
const { parseReleaseNotes } = await load("src/lib/release-notes.ts");

const changelog = parseChangelog(readFileSync(join(v2Root, "CHANGELOG.md"), "utf8"));
const text = (block) => block.spans.map((s) => s.text).join("");

// --- the bundled changelog --------------------------------------------------
assert.equal(changelog.some((e) => /unreleased/i.test(e.version)), false, "Unreleased is not a release");
// The authoring preamble must not contain anything that reads as a release
// heading — the release workflow would pull it into that release's notes.
const raw = readFileSync(join(v2Root, "CHANGELOG.md"), "utf8");
assert.deepEqual(parseChangelog(raw.slice(0, raw.indexOf("## Unreleased"))), [], "the preamble holds no release");
for (const v of ["2.3.0", "2.2.0", "2.1.0"]) {
  assert.ok(notesFor(changelog, v), `CHANGELOG.md carries notes for ${v}`);
}
const v230 = changelog.find((e) => e.version === "2.3.0");
assert.equal(v230.date, "2026-09-30", "the date comes off the heading");
assert.equal(/^## /m.test(v230.body), false, "a section stops at the next release");
const blocks230 = parseReleaseNotes(v230.body);
assert.equal(blocks230[0].kind, "heading", "2.3.0 opens with a heading");
assert.equal(text(blocks230[0]), "Renamed to ATV Optimizer");
assert.ok(
  blocks230.some((b) => b.kind === "item" && /now called ATV Optimizer/.test(text(b))),
  "the item under the rename heading survives",
);
assert.ok(blocks230.length > 20, "the 2.3.0 notes are not cut short");

const recent = recentReleases(changelog, "2.3.0");
assert.deepEqual(
  recent.map((e) => e.version).slice(0, 4),
  ["2.3.0", "2.2.0", "2.1.0", "2.0.0"],
  "newest first, starting at the running version",
);
assert.ok(recent.length <= 5, "a few releases, not the whole history");
assert.equal(recent.some((e) => e.version.includes("-")), false, "no betas for a stable build");
assert.deepEqual(
  recentReleases(changelog, "2.2.0").map((e) => e.version).slice(0, 2),
  ["2.2.0", "2.1.0"],
  "a version never lists releases newer than itself",
);

// --- the rule ---------------------------------------------------------------
// Drives a fake store through launches the way +layout.svelte does: decide,
// write `record` if any, and on a shown pop-up write the running version once
// it is dismissed.
function launcher(store) {
  return (running, { notes = true } = {}) => {
    const d = decideArrival({
      running,
      notesSeen: store.notesSeen ?? null,
      legacyLastSeen: store.legacy ?? null,
      hasNotes: notes && notesFor(changelog, running) !== null,
      isNewer: isNewerVersion,
    });
    if (d.show) store.notesSeen = running;
    else if (d.record) store.notesSeen = d.record;
    return d.show;
  };
}

// Three upgrades in a row from a fresh install.
{
  const store = {};
  const launch = launcher(store);
  assert.equal(launch("2.1.0"), false, "a first run shows nothing");
  assert.equal(launch("2.1.0"), false, "a relaunch shows nothing");
  assert.equal(launch("2.2.0"), true, "2.1.0 → 2.2.0 shows once");
  assert.equal(launch("2.2.0"), false, "and not on the next launch");
  assert.equal(launch("2.3.0"), true, "2.2.0 → 2.3.0 shows once");
  assert.equal(launch("2.3.0"), false, "and not on the next launch");
  assert.equal(launch("2.3.0"), false, "or the one after");
}

// Upgrading from a build that wrote only the per-launch key: the bundled notes
// show once, with no network call involved.
{
  const store = { legacy: "2.2.0" };
  const launch = launcher(store);
  assert.equal(launch("2.3.0"), true, "an upgrade from a pre-fix build shows the notes");
  assert.equal(launch("2.3.0"), false, "once");
}

// The per-launch key already reads "2.3.0" but no notes were recorded as
// shown: the 2.3.0 notes show on the next launch, once.
{
  const store = { legacy: "2.3.0" };
  const launch = launcher(store);
  assert.equal(launch("2.3.0"), true, "a user the bug skipped sees 2.3.0's notes");
  assert.equal(launch("2.3.0"), false, "once");
}

// A launch that could not show anything leaves the pop-up owed only if notes
// may yet appear; a build with no notes for itself moves the mark on.
{
  const store = { notesSeen: "2.2.0" };
  const launch = launcher(store);
  assert.equal(launch("2.3.0", { notes: false }), false);
  assert.equal(store.notesSeen, "2.3.0", "nothing will ever be showable for it");
}

// Downgrade and re-upgrade do not replay notes already read.
{
  const store = { notesSeen: "2.3.0" };
  const launch = launcher(store);
  assert.equal(launch("2.2.0"), false, "a downgrade announces nothing");
  assert.equal(store.notesSeen, "2.3.0", "and keeps the higher mark");
  assert.equal(launch("2.3.0"), false, "re-upgrading to a version already read is quiet");
}

// Not dismissed (the app was quit with the pop-up open): still owed.
{
  const d = decideArrival({
    running: "2.3.0",
    notesSeen: "2.2.0",
    legacyLastSeen: null,
    hasNotes: true,
    isNewer: isNewerVersion,
  });
  assert.deepEqual(d, { show: true, record: null }, "nothing is recorded until it has been seen");
}

console.log(
  "Notes-seen passed: bundled 2.3.0 notes parse under their rename heading, and the pop-up shows once per upgrade (2.1.0 → 2.2.0 → 2.3.0), never on a relaunch, and once for installs the old per-launch key skipped.",
);
