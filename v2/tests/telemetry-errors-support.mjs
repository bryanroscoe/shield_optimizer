// The shared error vocabulary, the support-tier copy and the frontend half of
// the anonymous reports (v2/shared/{errors,support,telemetry}.ts). Pure
// modules, so they run here without a browser: each is type-stripped into a
// temp dir with its relative imports pointed at the stripped siblings.
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const v2Root = dirname(dirname(fileURLToPath(import.meta.url)));
const out = mkdtempSync(join(tmpdir(), "atvopt-shared-"));
try {
  for (const name of ["support", "errors", "telemetry"]) {
    const source = readFileSync(join(v2Root, "shared", `${name}.ts`), "utf8");
    const js = stripTypeScriptTypes(source, { mode: "strip" }).replace(
      /from "\.\/(\w+)"/g,
      'from "./$1.mjs"',
    );
    writeFileSync(join(out, `${name}.mjs`), js);
  }
  const load = (name) => import(pathToFileURL(join(out, `${name}.mjs`)).href);
  const support = await load("support");
  const errors = await load("errors");
  const telemetry = await load("telemetry");

  // ── support copy ──────────────────────────────────────────────────────────
  {
    const { parseSupport, supportLine, errorTierHint, destructiveWarning, TIER_LABEL } = support;
    // Missing, malformed or unknown-tier input claims nothing.
    for (const raw of [undefined, null, {}, { tier: "great" }, "beta", { tier: "unknown", note: "x" }]) {
      assert.deepEqual(parseSupport(raw), { tier: "unknown", note: null }, JSON.stringify(raw));
    }
    const unknownLine = supportLine(parseSupport(null));
    assert.equal(unknownLine, "We haven't verified this TV model yet.");
    assert.doesNotMatch(unknownLine, /\bsupported\b|works?\b/i, "unknown never claims support");
    assert.doesNotMatch(TIER_LABEL.unknown, /supported/i);

    const fire = parseSupport({ tier: "not_supported", note: "Fire OS blocks the changes this app makes." });
    assert.equal(supportLine(fire), "This TV isn't supported: Fire OS blocks the changes this app makes.");
    assert.match(destructiveWarning(fire), /^This TV isn't supported: Fire OS blocks/);
    assert.match(errorTierHint(fire), /isn't supported \(Fire OS blocks the changes this app makes\)/);

    const beta = parseSupport({ tier: "beta", note: "TCL Google TV has had limited testing so far." });
    assert.match(supportLine(beta), /^Beta: this TV may not work fully\./);
    assert.match(supportLine(beta), /Reports from it help/);
    assert.equal(destructiveWarning(beta), null, "only not_supported warns before changes");

    const shield = parseSupport({ tier: "fully_supported", note: "NVIDIA SHIELD is the device this app is built and tested on." });
    assert.equal(errorTierHint(shield), null, "a supported TV isn't blamed for a failure");
    assert.equal(destructiveWarning(shield), null);
  }

  // ── error vocabulary ──────────────────────────────────────────────────────
  {
    const { classifyError, presentError, errorDetails } = errors;
    const cases = [
      ["Connection to the TV was lost: broken pipe", "connection_lost"],
      ["device 192.168.1.4:5555 is in state 'unauthorized', not 'device'", "device_unauthorized"],
      ["error: device offline", "device_offline"],
      ["failed to connect to '192.168.1.88:37123': Connection refused", "connection_refused"],
      ["adb command timed out after 30s", "timeout"],
      ["error: no route to host", "unreachable"],
      ["Failed: wrong password or connection was dropped", "wrong_pin"],
      ["Refusing to disable com.android.systemui: core system UI", "protected_package"],
      ["java.lang.SecurityException: Permission Denial: shell cannot change", "permission_denied"],
      ["Failure [not installed for 0]", "package_not_found"],
      ["Failure [DELETE_FAILED_INTERNAL_ERROR]", "pm_failure"],
      ["LOCKED:curated_debloat", "locked"],
      ["could not locate an adb binary. Tried PATH", "adb_missing"],
      ["something nobody has seen", "unknown"],
    ];
    for (const [raw, code] of cases) assert.equal(classifyError(raw), code, raw);
    assert.equal(classifyError(new Error("adb command timed out after 5s")), "timeout");

    const fire = { tier: "not_supported", note: "Fire OS blocks the changes this app makes." };
    const p = presentError("Failure [DELETE_FAILED_INTERNAL_ERROR]", fire);
    assert.equal(p.code, "pm_failure");
    assert.equal(p.raw, "Failure [DELETE_FAILED_INTERNAL_ERROR]");
    assert.match(p.tierHint, /isn't supported/);
    // A failure the TV can't explain gets no tier hint.
    assert.equal(presentError("LOCKED:x", fire).tierHint, null);
    assert.equal(presentError("adb command timed out after 30s", null).tierHint, null);
    // An error none of the known shapes match keeps the backend's own sentence.
    const typo = "not an IP address or mDNS service name: 192.0.2.999. Enter the IP and port";
    assert.equal(presentError(typo).code, "unknown");
    assert.equal(presentError(typo).message, typo);
    assert.equal(presentError("").message, "Something went wrong.");

    const text = errorDetails(p, { action: "Disable Netflix", tier: "Not supported", app: "ATV Optimizer (desktop)" });
    assert.match(text, /^Error: pm_failure$/m);
    assert.match(text, /^Action: Disable Netflix$/m);
    assert.match(text, /^Details: Failure \[DELETE_FAILED_INTERNAL_ERROR\]$/m);
  }

  // ── telemetry observer ────────────────────────────────────────────────────
  {
    const { observeCall, resolvedOutcome, recordAction, installGlobalErrorReporting } = telemetry;
    const sent = [];
    const send = async (command, args) => {
      sent.push({ command, args });
    };
    const flush = () => new Promise((r) => setTimeout(r, 0));

    // A resolved `{ ok: false }` is a failure, classified from its message.
    observeCall(send, "disable_package", { serial: "s1", package: "com.example.app" }, { ok: false, message: "Failure [DELETE_FAILED_INTERNAL_ERROR]" });
    // A rejection is a failure too.
    observeCall(send, "connect_device", { address: "10.0.0.2" }, undefined, "failed to connect to 10.0.0.2:5555: Connection refused");
    // A success carries no code or message.
    observeCall(send, "enable_package", { serial: "s1", package: "com.example.app" }, { ok: true, message: "enabled com.example.app" });
    await flush();
    assert.deepEqual(sent.map((s) => s.command), ["telemetry_action", "telemetry_action", "telemetry_action"]);
    assert.deepEqual(sent[0].args, {
      action: "disable",
      serial: "s1",
      ok: false,
      errorCode: "pm_failure",
      message: "Failure [DELETE_FAILED_INTERNAL_ERROR]",
    });
    assert.equal(sent[1].args.action, "connect");
    assert.equal(sent[1].args.errorCode, "connection_refused");
    assert.equal(sent[1].args.serial, null);
    assert.deepEqual(sent[2].args, { action: "enable", serial: "s1", ok: true, errorCode: null, message: null });
    sent.length = 0;

    // Telemetry commands are never observed (no loops); quiet polls and
    // successful untracked calls send nothing; locked is an expected state.
    observeCall(send, "telemetry_action", {}, undefined, "boom");
    observeCall(send, "set_telemetry_enabled", { enabled: false }, undefined, "boom");
    observeCall(send, "resource_sample", { serial: "s1" }, undefined, "error: device offline");
    observeCall(send, "list_devices", {}, []);
    observeCall(send, "disable_package", { serial: "s1" }, undefined, "LOCKED:curated_debloat");
    await flush();
    assert.equal(sent.length, 0, JSON.stringify(sent));

    // An untracked command that fails is an error report naming the command.
    observeCall(send, "list_launchers", { serial: "s1" }, undefined, "adb command timed out after 30s");
    await flush();
    assert.deepEqual(sent[0], {
      command: "telemetry_error",
      args: { errorCode: "timeout", message: "list_launchers: adb command timed out after 30s", action: "other", serial: "s1" },
    });
    sent.length = 0;

    // Snapshot apply reports counts, never the names of apps or settings.
    const outcome = resolvedOutcome("apply_snapshot", {
      packages_disabled: ["com.a.b"],
      packages_failed: ["com.secret.app", "com.other.app"],
      settings_failed: ["global/private_key"],
      summary: "x",
    });
    assert.deepEqual(outcome, { ok: false, code: "partial_failure", message: "3 item(s) failed to apply" });
    observeCall(send, "apply_snapshot", { serial: "s1" }, { packages_failed: ["com.secret.app"], settings_failed: [] });
    await flush();
    assert.doesNotMatch(JSON.stringify(sent), /com\.secret/);
    assert.equal(resolvedOutcome("apply_snapshot", { packages_failed: [], settings_failed: [] }).ok, true);
    sent.length = 0;

    // A broken backend never surfaces to the caller.
    assert.doesNotThrow(() => observeCall(() => { throw new Error("no backend"); }, "disable_package", {}, { ok: false }));
    assert.doesNotThrow(() => observeCall(async () => { throw new Error("rejected"); }, "disable_package", {}, { ok: false }));
    await flush();

    recordAction(send, "optimize", "s1", { ok: false, code: "partial_failure", message: "2 of 9 changes failed" });
    await flush();
    assert.equal(sent[0].args.action, "optimize");
    assert.equal(sent[0].args.errorCode, "partial_failure");
    sent.length = 0;

    // Global handlers forward uncaught errors and rejections, and detach.
    const target = new EventTarget();
    const dispose = installGlobalErrorReporting(send, target);
    const err = new Event("error");
    Object.assign(err, { error: new Error("render blew up"), message: "render blew up", filename: "http://x/app/immutable/chunk.js", lineno: 12 });
    target.dispatchEvent(err);
    const rej = new Event("unhandledrejection");
    Object.assign(rej, { reason: "nope" });
    target.dispatchEvent(rej);
    await flush();
    assert.deepEqual(sent.map((s) => s.args.errorCode), ["uncaught_error", "unhandled_rejection"]);
    assert.equal(sent[0].args.message, "render blew up (chunk.js:12)");
    dispose();
    target.dispatchEvent(rej);
    await flush();
    assert.equal(sent.length, 2, "disposed handlers send nothing");
  }
} finally {
  rmSync(out, { recursive: true, force: true });
}

console.log(
  "Shared errors/support/telemetry passed: codes classify the real backend strings, unknown tiers claim nothing, not_supported warns, failures (rejected or ok:false) are reported with codes, telemetry/quiet/locked calls are not, and snapshot apply reports counts only.",
);
