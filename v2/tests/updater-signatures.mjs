// The release workflow refuses to publish an updater manifest whose
// signatures don't verify against the key installed copies trust. These
// checks run the verifier against a real `tauri signer sign` output (made with
// a throwaway key that was deleted afterwards), so a format mistake here would
// fail before a release does.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const v2Root = dirname(dirname(fileURLToPath(import.meta.url)));
const script = join(v2Root, "scripts/updater-signatures.mjs");
const { checkManifest, keyIdHex, parsePublicKey, updaterPublicKey, verifySignature } =
  await import(script);

const fixtures = join(v2Root, "tests/fixtures/updater-signature");
const data = readFileSync(join(fixtures, "artifact.bin"));
const sig = readFileSync(join(fixtures, "artifact.bin.sig"), "utf8");
const throwawayKey = readFileSync(join(fixtures, "throwaway.key.pub"), "utf8");
const realKey = updaterPublicKey(join(v2Root, "src-tauri/tauri.conf.json"));

const b64 = (text) => Buffer.from(text, "utf8").toString("base64");
const unb64 = (value) => Buffer.from(value.trim(), "base64").toString("utf8");

// Installed copies verify every update against this key. Rotating it breaks
// auto-update for all of them, permanently.
assert.equal(keyIdHex(parsePublicKey(realKey).keyId), "7111D25D5298C7B1");

assert.match(verifySignature(data, sig, throwawayKey), /file:artifact\.bin/);

const flipped = Buffer.from(data);
flipped[0] ^= 1;
assert.throws(() => verifySignature(flipped, sig, throwawayKey), /does not match the file/);
assert.throws(() => verifySignature(data, sig, realKey), /signed by key [0-9A-F]{16}, expected 7111D25D5298C7B1/);

const tampered = b64(unb64(sig).replace("file:artifact.bin", "file:other.bin"));
assert.throws(() => verifySignature(data, tampered, throwawayKey), /trusted comment signature/);
assert.throws(() => verifySignature(data, b64("not a signature"), throwawayKey), /not a base64-encoded minisign/);
assert.throws(() => verifySignature(data, "", throwawayKey), /not a base64-encoded minisign/);

const dir = mkdtempSync(join(tmpdir(), "updater-signatures-"));
try {
  copyFileSync(join(fixtures, "artifact.bin"), join(dir, "ATV Optimizer_9.9.9_amd64.AppImage"));
  copyFileSync(join(fixtures, "artifact.bin.sig"), join(dir, "ATV Optimizer_9.9.9_amd64.AppImage.sig"));
  copyFileSync(join(fixtures, "artifact.bin"), join(dir, "setup.exe"));
  const url = (name) => `https://github.com/o/r/releases/download/v2-9.9.9/${encodeURIComponent(name)}`;
  const manifest = () => ({
    version: "9.9.9",
    platforms: {
      // An AppImage repacked after tauri-action signed it: latest.json still
      // carries a signature that no longer matches, while the re-signed .sig does.
      "linux-x86_64": { url: url("ATV Optimizer_9.9.9_amd64.AppImage"), signature: tampered },
      "linux-x86_64-appimage": { url: url("ATV Optimizer_9.9.9_amd64.AppImage"), signature: tampered },
      "windows-x86_64-nsis": { url: url("setup.exe"), signature: sig },
    },
  });

  let result = checkManifest(manifest(), dir, throwawayKey);
  assert.equal(result.changed, false);
  assert.deepEqual(
    result.results.map((r) => [r.platform, r.ok]),
    [["linux-x86_64", false], ["linux-x86_64-appimage", false], ["windows-x86_64-nsis", true]],
  );

  result = checkManifest(manifest(), dir, throwawayKey, { repair: true });
  assert.equal(result.changed, true);
  assert.ok(result.results.every((r) => r.ok));
  assert.deepEqual(result.results.map((r) => r.repaired), [true, true, false]);
  assert.equal(result.manifest.platforms["linux-x86_64"].signature, sig.trim());

  // A .sig that doesn't verify either is never copied in.
  writeFileSync(join(dir, "setup.exe.sig"), tampered);
  const badExe = manifest();
  badExe.platforms["windows-x86_64-nsis"].signature = tampered;
  result = checkManifest(badExe, dir, throwawayKey, { repair: true });
  assert.equal(result.results[2].ok, false);
  assert.equal(badExe.platforms["windows-x86_64-nsis"].signature, tampered);

  // The wrong key fails everything, repair or not.
  result = checkManifest(manifest(), dir, realKey, { repair: true });
  assert.ok(result.results.every((r) => !r.ok));

  const missing = { platforms: { "darwin-aarch64": { url: url("gone.tar.gz"), signature: sig } } };
  assert.equal(checkManifest(missing, dir, throwawayKey).results[0].reason, "artifact not found");
  assert.throws(() => checkManifest({ platforms: {} }, dir, throwawayKey), /no platforms/);

  // The command line the release workflow runs.
  const config = join(dir, "tauri.conf.json");
  writeFileSync(config, JSON.stringify({ plugins: { updater: { pubkey: throwawayKey.trim() } } }));
  const latest = join(dir, "latest.json");
  writeFileSync(latest, JSON.stringify(manifest()));
  const run = (...args) => spawnSync(process.execPath, [script, "--config", config, ...args], { encoding: "utf8" });

  let cli = run("manifest", latest, dir);
  assert.equal(cli.status, 1, cli.stdout + cli.stderr);
  assert.match(cli.stderr, /INVALID linux-x86_64-appimage/);
  cli = run("manifest", latest, dir, "--repair");
  assert.equal(cli.status, 0, cli.stdout + cli.stderr);
  assert.equal(JSON.parse(readFileSync(latest, "utf8")).platforms["linux-x86_64-appimage"].signature, sig.trim());
  cli = run("manifest", latest, dir);
  assert.equal(cli.status, 0, cli.stdout + cli.stderr);

  const appimage = join(dir, "ATV Optimizer_9.9.9_amd64.AppImage");
  assert.equal(run("file", appimage, `${appimage}.sig`).status, 0);
  assert.equal(run("file", join(dir, "setup.exe"), join(dir, "setup.exe.sig")).status, 1);
  const real = spawnSync(process.execPath, [script, "file", appimage, `${appimage}.sig`], { encoding: "utf8" });
  assert.equal(real.status, 1, "the fixture is not signed by the release key");
  assert.equal(run("bogus").status, 2);
} finally {
  rmSync(dir, { recursive: true, force: true });
}

console.log("updater signatures: ok");
