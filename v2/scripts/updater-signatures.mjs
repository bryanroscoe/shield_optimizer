#!/usr/bin/env node
// Checks Tauri updater signatures against the public key installed copies
// trust (plugins.updater.pubkey in src-tauri/tauri.conf.json). The release
// workflow runs it so a release whose updater artifacts don't verify fails
// instead of publishing an update every installed copy rejects.
//
// Tauri signatures are minisign signatures, base64-encoded once more. The
// "ED" algorithm signs the BLAKE2b-512 hash of the file with Ed25519; the
// global signature covers the signature plus the trusted comment.
//
//   node scripts/updater-signatures.mjs file <artifact> <artifact.sig>
//   node scripts/updater-signatures.mjs manifest <latest.json> <assets-dir> [--repair]
//
// `manifest` checks every platform entry in latest.json against the file its
// URL names in <assets-dir>. With --repair, an entry that fails is replaced by
// that file's `.sig` from <assets-dir> when the `.sig` verifies, and the
// manifest is rewritten. Either way every entry must verify at the end.
// `--config <tauri.conf.json>` overrides where the public key is read from.

import { createHash, createPublicKey, verify } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ED25519_SPKI_PREFIX = Buffer.from("302a300506032b6570032100", "hex");

function decodeBox(b64, what) {
  const text = Buffer.from(String(b64).trim(), "base64").toString("utf8");
  const lines = text.split("\n");
  if (!lines[0]?.startsWith("untrusted comment:")) {
    throw new Error(`${what} is not a base64-encoded minisign ${what}`);
  }
  return lines;
}

function decodeExact(b64, length, what) {
  const raw = Buffer.from(b64 ?? "", "base64");
  if (raw.length !== length) {
    throw new Error(`${what} is ${raw.length} bytes, expected ${length}`);
  }
  return raw;
}

export function parsePublicKey(b64) {
  const lines = decodeBox(b64, "public key");
  const raw = decodeExact(lines[1], 42, "public key");
  if (raw.subarray(0, 2).toString("latin1") !== "Ed") {
    throw new Error("public key is not an Ed25519 minisign key");
  }
  return {
    keyId: raw.subarray(2, 10),
    key: createPublicKey({
      key: Buffer.concat([ED25519_SPKI_PREFIX, raw.subarray(10)]),
      format: "der",
      type: "spki",
    }),
  };
}

export function keyIdHex(keyId) {
  return Buffer.from(keyId).reverse().toString("hex").toUpperCase();
}

export function parseSignature(b64) {
  const lines = decodeBox(b64, "signature");
  const raw = decodeExact(lines[1], 74, "signature");
  const algorithm = raw.subarray(0, 2).toString("latin1");
  if (algorithm !== "ED" && algorithm !== "Ed") {
    throw new Error(`unknown signature algorithm ${JSON.stringify(algorithm)}`);
  }
  const prefix = "trusted comment: ";
  if (!lines[2]?.startsWith(prefix)) {
    throw new Error("signature has no trusted comment");
  }
  return {
    algorithm,
    keyId: raw.subarray(2, 10),
    signature: raw.subarray(10),
    trustedComment: lines[2].slice(prefix.length),
    globalSignature: decodeExact(lines[3], 64, "global signature"),
  };
}

// Throws with the reason when `data` doesn't verify; returns the trusted
// comment when it does.
export function verifySignature(data, signatureB64, publicKeyB64) {
  const pub = parsePublicKey(publicKeyB64);
  const sig = parseSignature(signatureB64);
  if (!sig.keyId.equals(pub.keyId)) {
    throw new Error(
      `signed by key ${keyIdHex(sig.keyId)}, expected ${keyIdHex(pub.keyId)}`,
    );
  }
  const message =
    sig.algorithm === "ED" ? createHash("blake2b512").update(data).digest() : data;
  if (!verify(null, message, pub.key, sig.signature)) {
    throw new Error("signature does not match the file");
  }
  const global = Buffer.concat([sig.signature, Buffer.from(sig.trustedComment, "utf8")]);
  if (!verify(null, global, pub.key, sig.globalSignature)) {
    throw new Error("trusted comment signature does not verify");
  }
  return sig.trustedComment;
}

export function updaterPublicKey(configPath) {
  const config = JSON.parse(readFileSync(configPath, "utf8"));
  const pubkey = config?.plugins?.updater?.pubkey;
  if (!pubkey) throw new Error(`${configPath} has no plugins.updater.pubkey`);
  return pubkey;
}

function tryVerify(data, signatureB64, publicKeyB64) {
  try {
    verifySignature(data, signatureB64, publicKeyB64);
    return null;
  } catch (e) {
    return e.message;
  }
}

export function assetName(url) {
  return decodeURIComponent(basename(new URL(url).pathname));
}

// Returns { manifest, results, changed }. results: one { platform, file, ok,
// reason, repaired } per platform entry.
export function checkManifest(manifest, assetsDir, publicKeyB64, { repair = false } = {}) {
  const platforms = Object.entries(manifest?.platforms ?? {});
  if (platforms.length === 0) throw new Error("manifest lists no platforms");
  const results = [];
  let changed = false;
  for (const [platform, entry] of platforms) {
    const file = assetName(entry.url);
    const path = join(assetsDir, file);
    if (!existsSync(path)) {
      results.push({ platform, file, ok: false, reason: "artifact not found", repaired: false });
      continue;
    }
    const data = readFileSync(path);
    let reason = tryVerify(data, entry.signature, publicKeyB64);
    let repaired = false;
    if (reason && repair && existsSync(`${path}.sig`)) {
      const replacement = readFileSync(`${path}.sig`, "utf8").trim();
      const replacementReason = tryVerify(data, replacement, publicKeyB64);
      if (!replacementReason) {
        entry.signature = replacement;
        reason = null;
        repaired = true;
        changed = true;
      }
    }
    results.push({ platform, file, ok: !reason, reason, repaired });
  }
  return { manifest, results, changed };
}

function main(argv) {
  const args = [...argv];
  const v2Root = dirname(dirname(fileURLToPath(import.meta.url)));
  let config = join(v2Root, "src-tauri/tauri.conf.json");
  const configAt = args.indexOf("--config");
  if (configAt !== -1) {
    config = args[configAt + 1];
    args.splice(configAt, 2);
  }
  const repairAt = args.indexOf("--repair");
  const repair = repairAt !== -1;
  if (repair) args.splice(repairAt, 1);

  const publicKey = updaterPublicKey(config);
  console.log(`updater public key ${keyIdHex(parsePublicKey(publicKey).keyId)} from ${config}`);
  const [command, a, b] = args;

  if (command === "file" && a && b) {
    const reason = tryVerify(readFileSync(a), readFileSync(b, "utf8"), publicKey);
    if (reason) {
      console.error(`INVALID ${a}: ${reason}`);
      return 1;
    }
    console.log(`ok ${a}`);
    return 0;
  }

  if (command === "manifest" && a && b) {
    const { manifest, results, changed } = checkManifest(
      JSON.parse(readFileSync(a, "utf8")),
      b,
      publicKey,
      { repair },
    );
    for (const r of results) {
      if (r.ok) console.log(`ok ${r.platform}: ${r.file}${r.repaired ? " (signature replaced from .sig)" : ""}`);
      else console.error(`INVALID ${r.platform}: ${r.file}: ${r.reason}`);
    }
    if (changed) {
      writeFileSync(a, JSON.stringify(manifest, null, 2) + "\n");
      console.log(`rewrote ${a}`);
    }
    return results.every((r) => r.ok) ? 0 : 1;
  }

  console.error(
    "usage: updater-signatures.mjs [--config tauri.conf.json] file <artifact> <sig>\n" +
      "       updater-signatures.mjs [--config tauri.conf.json] manifest <latest.json> <assets-dir> [--repair]",
  );
  return 2;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = main(process.argv.slice(2));
}
