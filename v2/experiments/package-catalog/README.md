# Descriptive package catalog experiment

This is an offline research artifact, excluded from desktop/mobile loaders, builds,
removal recommendations and production defaults. It does not change either app.
It answers how far one free metadata source takes us, not how many Android TV apps
exist. No merge or product integration is proposed yet.

## Snapshot results

On 2026-10-22, a bounded download of F-Droid's active index yielded **4,766 unique
package IDs**, of which **4,759 are new** relative to the 170-ID baseline (89 curated
IDs plus 81 disjoint known-name IDs). Six overlap curated entries and one overlaps
known names. Launcher and sideload catalogs are excluded from this denominator.
No private device inventory was read or committed.

| Measure | IDs | Share of discovered IDs |
|---|---:|---:|
| All source categories mapped to broad descriptive groups | 4,274 | 89.68% |
| Some categories mapped, others explicitly unmapped | 285 | 5.98% |
| Purpose unknown in this taxonomy | 207 | 4.34% |
| Version metadata reports a required TV feature | 3 | 0.06% |
| TV status unknown | 4,763 | 99.94% |
| Verified on Shield / removal safety reviewed | 0 | 0% |

“Classified” means a deterministic mapping of upstream category terms into 13
broad groups, not a human review of the app's actual behavior. The descriptions
are original category-level sentences, not app-specific functional audits. Multiple
groups can apply, so category totals in `data/coverage.json` are nonexclusive.
Unmapped terms remain visible; no package-name heuristics fill gaps.

The three TV-feature candidates are `com.conreo.couchytv`, `com.luncher.launcher`
and `com.github.shadowsocks.tv`. Each retains exact version codes and minimum SDK
from published metadata. A required feature is affirmative manifest metadata, not
a test of TV usability, HOME handling, remote navigation, installation or Shield2017
compatibility. Older and newer versions may differ. Missing feature data stays
unknown: optional Leanback declarations and launcher intent categories are not
represented by this index field. A phone companion can also concern TV without
being a TV app. This sample therefore cannot estimate total TV coverage.

## Provenance and licensing decisions

The official [F-Droid API guide](https://f-droid.org/docs/All_our_APIs/) documents
the [active index](https://f-droid.org/repo/index-v1.json). `sources.json` records
its retrieval date, repository timestamp and raw SHA256, plus a normalized seed
hash. The metadata is untrusted descriptive input; the index signature was not
verified and this output must not be used to install apps or establish signer trust.
Only package identities, category terms and required-feature/version facts are
retained. Upstream app names, descriptions, icons, signatures, download URLs,
permissions, private diagnostics and APKs are absent.

F-Droid Data's [license at inspected commit e2e9df2](https://github.com/f-droid/fdroiddata/blob/e2e9df249c54154ef3e07355286a326237473568/LICENSE)
is AGPL-3.0. The extracted and generated data in `data/` are kept under that
license, with attribution and its full text in `data/NOTICE.md` and
`data/LICENSE-AGPL-3.0.txt`. This separate data license does not relicense the rest
of Shield Optimizer. Any later production incorporation needs a separate licensing
decision; this experiment does not treat the data as unrestricted or the app's
own license as permission to redistribute upstream material.

The feature interpretation is supported by F-Droid's
[manifest parser](https://github.com/f-droid/fdroidserver/blob/784d3deee7ca3d6596a19c473a4590b9cdb7b1ec/fdroidserver/update.py#L2065),
which retains features with `required` missing or true.

Two proposed sources were deliberately excluded after license inspection:

- [UAD-ng](https://github.com/Universal-Debloater-Alliance/universal-android-debloater-next-generation): GPL-3.0. No identities, descriptions, categories or removal labels imported. Its safety labels would not be authority for our safety engine.
- [Awesome Android TV FOSS Apps, pinned 9f0c751](https://github.com/Generator/Awesome-Android-TV-FOSS-Apps/tree/9f0c751ff58b609cee1c482d4634c53c250f266c): CC BY-NC-SA 4.0. No content imported, because the noncommercial restriction is unsuitable for assumed production reuse. This experiment makes no claim to cover its list.

## Reproduce without network or dependencies

From this directory, using Python 3.9 or newer:

```sh
python3 -m unittest -v
python3 catalog.py --check
python3 catalog.py
```

The last command regenerates `data/catalog.json` and `data/coverage.json` from the
committed minimal `data/seed.json` and the repository's current baseline. No command
in this script makes network requests, invokes ADB or runs upstream code.

To independently repeat extraction, obtain **only** the JSON metadata index linked
above (the inspected file is 68,263,633 bytes), then run:

```sh
python3 catalog.py --extract /path/to/index-v1.json --check
```

The raw index must match the pinned SHA256 and repository timestamp. Extraction
sorts and deduplicates identities, categories and version-feature facts, then checks
the normalized seed hash. The live endpoint rotates, so a future download may be
rejected. The committed seed supports offline replay; availability of the old raw
snapshot is not guaranteed. Refreshing the source requires an intentional new
snapshot/hash, regenerated seed, category review and updated counts. Do not bypass
a mismatch. The raw 68 MB index is outside Git; no app archives were downloaded.

## Validation and remaining work

Seven focused tests cover duplicate ingestion, hash rejection, unknown/partial
classification, version-specific TV evidence, schema/provenance/category drift,
reconciled counts and absence of production references. `catalog.py --check`
validates all 4,766 rows and exact reproducibility of generated output. The focused
CI workflow repeats these offline checks; normal v2 CI also runs on this PR.
No full runtime validation is claimed from these Python checks.

Screenshots and video are inapplicable: there is no UI or production runtime
change. The isolation regression checks production source/config files for imports;
the PR diff is limited to this experiment and its isolated CI workflow. Existing
gallery files therefore remain unchanged.

Useful next steps are a reviewed source for optional Leanback/TV intent evidence,
app-specific descriptions for high-priority packages, and an explicitly authorized
private full installed-ID inventory to measure actual device coverage. None is
required to reproduce these results, and no device operation was performed.
