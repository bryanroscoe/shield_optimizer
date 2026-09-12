# Archive index — material kept outside this repository

Some working material from the retired coordination tree was too large, too build-output-heavy, or
too internal to belong in the repository, but is worth keeping. It was **copied** (never moved) to a
local directory on Bryan's machine:

```
/Users/bryanroscoe/Developer/shield_optimizer-gastown-archive/
```

That directory is **not** in version control and is **not** backed up by anything in this repo. If
the machine is wiped, it is gone. Nothing in this repo depends on it — treat it as a reference of
last resort when you need to know *why* a merged change looks the way it does.

`MANIFEST-sha256.txt` at the archive root lists every file with its SHA-256
(665 entries, manifest sha256 `5155bf2996e453fca39ff5bc7f9985d01c9a85ceaf9582acccc4b63fcc00967b`).
Verify with `cd <archive> && shasum -a 256 -c MANIFEST-sha256.txt`.

## What is in it

- **`mayor-artifacts/`** — per-task briefs, acceptance contracts, source manifests and patches for
  the changes that were merged into `main` in September 2026. Useful if you need the acceptance
  criteria a specific change was written against.
- **`acceptance-evidence/`** — per-candidate evidence directories: review outputs, test artifacts,
  screenshots and applied patches.

Both are heavy with internal process vocabulary from the retired tracker. The durable, readable
versions of this material are already in the repo: [`GASTOWN-HANDOFF.md`](GASTOWN-HANDOFF.md),
[`RELEASE-DECISION-2026-09-09.md`](RELEASE-DECISION-2026-09-09.md),
[`gastown-shield_optimizer-beads-export.json`](gastown-shield_optimizer-beads-export.json) and
[`../v2/mobile/evidence/`](../v2/mobile/evidence/).

| Directory | Size | Files |
|---|---|---|
| `acceptance-evidence/so-2en-97_k0vje` |  16K | 2 files |
| `acceptance-evidence/so-7iw-pending-harness-pzxumlm6` | 4.4M | 2 files |
| `acceptance-evidence/so-fb3-source-integration-kzhhjrj3` |  24K | 2 files |
| `acceptance-evidence/so-fb3.1.1-accepted-v2vec08_` |  72K | 5 files |
| `acceptance-evidence/so-fb3.2.1-technical-b6wrf4ne` | 168K | 20 files |
| `acceptance-evidence/so-vtm.3-d_91juzj` |  24K | 2 files |
| `acceptance-evidence/so-vtm.4-accepted-nd78ubph` |  20K | 2 files |
| `acceptance-evidence/so-vtm.4-appfilter06-20260905-214805` | 268K | 4 files |
| `acceptance-evidence/so-vtm.4-navigator-final-status-20260906T0255.md` | 8.0K | 1 files |
| `acceptance-evidence/so-vtm.4-navigator-k90yz3qm` | 1.9M | 77 files |
| `acceptance-evidence/so-vtm.6-accepted-p9ssvjk5` |  44K | 2 files |
| `acceptance-evidence/so-vtm.6-mechanic-probe-zfudcjny` | 516K | 10 files |
| `acceptance-evidence/so-vtm.6-navigator-review` | 1.9M | 73 files |
| `acceptance-evidence/so-vtm.7-` | 364K | 4 files |
| `acceptance-evidence/so-vtm.7-integrated` | 388K | 5 files |
| `acceptance-evidence/so-vtm.7-navigator-design` | 8.0K | 1 files |
| `acceptance-evidence/so-vtm.7-runtime-held-_k7ffcul` |  20K | 2 files |
| `acceptance-evidence/so-vtm.8-3mgugq2g` |  96K | 8 files |
| `mayor-artifacts/shield-dolt-hq-wisp-ayddd` |  48K | 12 files |
| `mayor-artifacts/shield-github-issue-handoffs-2026-09-08.md` | 8.0K | 1 files |
| `mayor-artifacts/shield-release-decision-20260909.md` |  20K | 1 files |
| `mayor-artifacts/shield-so-2en-local-integration` |  24K | 4 files |
| `mayor-artifacts/shield-so-2en-saved-folder-intent` | 140K | 20 files |
| `mayor-artifacts/shield-so-3ty-release-readiness` |  28K | 7 files |
| `mayor-artifacts/shield-so-7iw-harness-correction` | 148K | 18 files |
| `mayor-artifacts/shield-so-fb3.1-correction` |  84K | 10 files |
| `mayor-artifacts/shield-so-fb3.1-final-checks-y05pkrtn` |  68K | 8 files |
| `mayor-artifacts/shield-so-fb3.1-final-verified-q_bzvm8e` |  68K | 8 files |
| `mayor-artifacts/shield-so-fb3.1-review-sav5u3bx` |  60K | 7 files |
| `mayor-artifacts/shield-so-fb3.1-source` | 168K | 14 files |
| `mayor-artifacts/shield-so-fb3.2-contract` | 804K | 60 files |
| `mayor-artifacts/shield-so-fb3.2-desktop-consumers` | 580K | 35 files |
| `mayor-artifacts/shield-so-fb3.2-desktop-review` |  36K | 3 files |
| `mayor-artifacts/shield-so-fb3.2-integration-record.txt` | 4.0K | 1 files |
| `mayor-artifacts/shield-so-fb3.2-mobile-consumers` | 592K | 48 files |
| `mayor-artifacts/shield-so-fb3.2-mobile-review` | 960K | 99 files |
| `mayor-artifacts/shield-so-fb3.2-review` | 180K | 24 files |
| `mayor-artifacts/shield-so-fb3.2-verified` | 128K | 16 files |
| `mayor-artifacts/shield-so-vtm.9-integration` |  12K | 1 files |
| `mayor-artifacts/shield-so-x02-launcher-correction` | 384K | 26 files |
| `mayor-artifacts/shield-so-x02-launcher-failures` | 296K | 19 files |

## Deliberately excluded from the copy

- Cargo `target/` trees and `node_modules/` (build output and dependency caches; ~26 MB, the bulk of
  it one `rust-fixture` build directory).
- Vite build output and `.timestamp` files inside evidence directories.

All of it is reproducible from source and none of it is evidence of anything.
