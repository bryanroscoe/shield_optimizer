// Cmd/Ctrl+Left (and Alt+Left, Cmd+[) go back like a browser, but never while
// typing in a field or with a dialog open.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const v2Root = dirname(dirname(fileURLToPath(import.meta.url)));
const src = stripTypeScriptTypes(readFileSync(join(v2Root, "src/lib/history-keys.ts"), "utf8"), { mode: "strip" });
const { historyStep } = await import("data:text/javascript," + encodeURIComponent(src));

const key = (k, mods = {}) => ({ key: k, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, isComposing: false, defaultPrevented: false, ...mods });

assert.equal(historyStep(key("ArrowLeft", { metaKey: true }), false, false), -1, "Cmd+Left goes back");
assert.equal(historyStep(key("ArrowLeft", { ctrlKey: true }), false, false), -1, "Ctrl+Left goes back");
assert.equal(historyStep(key("ArrowLeft", { altKey: true }), false, false), -1, "Alt+Left goes back");
assert.equal(historyStep(key("[", { metaKey: true }), false, false), -1, "Cmd+[ goes back");
assert.equal(historyStep(key("ArrowRight", { metaKey: true }), false, false), 1, "Cmd+Right goes forward");
assert.equal(historyStep(key("]", { ctrlKey: true }), false, false), 1, "Ctrl+] goes forward");
assert.equal(historyStep(key("ArrowLeft"), false, false), 0, "a bare arrow scrolls, not navigates");
assert.equal(historyStep(key("ArrowLeft", { metaKey: true, shiftKey: true }), false, false), 0, "Shift selects");
assert.equal(historyStep(key("ArrowLeft", { metaKey: true }), true, false), 0, "a text field keeps its caret keys");
assert.equal(historyStep(key("ArrowLeft", { metaKey: true }), false, true), 0, "an open dialog holds the page");
assert.equal(historyStep(key("ArrowLeft", { metaKey: true, defaultPrevented: true }), false, false), 0, "a handled key is left alone");
console.log("history-keys: ok");
