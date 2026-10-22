# Device-profile research leads — draft only

This follow-on preserves PR211's merge hold. It is stacked on the unmerged
experiment branch and changes no production catalog, loader, UI, safety engine,
device command or default. PR215's conservative TV Recommendations correction is
separate. Upstream profile membership is a research lead, not proof a package is
installed, compatible with that model, useful, safe or removable.

## Measured results

From pinned [cleanyourtv 30626a8](https://github.com/Gathin23/cleanyourtv/tree/30626a8e4714f86bf837384b109b5c73ee736917):

| Measure | Count |
|---|---:|
| Text profile inputs | 232 |
| Active package-line occurrences | 5,504 |
| Unique candidate IDs after dedupe | 1,190 |
| IDs absent from the 170-ID curated/known-name baseline | 1,113 |
| IDs already in that local registry | 77 |
| Exact-ID matches to the independent F-Droid metadata snapshot | 2 |
| New IDs beyond the F-Droid snapshot | 1,188 |
| Combined F-Droid/profile union | 5,954 |
| Candidates with a descriptive category from F-Droid | 2 |
| Candidates with purpose unknown | 1,188 |
| Candidates with TV evidence unknown | 1,190 |
| Hardware compatibility or removal safety verified here | 0 |

The two independent metadata matches are `com.limelight` (games) and
`org.videolan.vlc` (media). Those descriptions/categories come from the earlier
F-Droid metadata mapping, not upstream removal prose. They establish neither
signer identity nor TV compatibility. Seventy-seven local registry references
are retained as identity pointers only; their safety labels/descriptions are not
imported. Baseline counts exclude launcher/sideload catalogs.

The `docs/devices/` directory contains **43 assets: 38 text notes and five images**.
`note-index.json` records pointers and kinds only. It does not claim 43 validated
hardware notes. Some concern non-Android platforms. No image or note body was
copied, and no text note was promoted into an installation, firmware or testing
claim. Each profile retains source URL, model=null, firmware=null,
`testing_level=upstream_profile_only` and `confidence=research_lead`. Profile slugs
are source labels, not verified models. A later reviewed pass can add sourced
model/firmware claims while keeping testing levels explicit.

## Ingestion and provenance

`input-manifest.json` pins every profile path, upstream Git blob and SHA256.
`provenance.json` pins the normalized seed, manifest and note-index hashes and
records the source commit/date. Only first tokens on non-comment active lines
are accepted as package IDs. Blank/commented lines and inline comments are
excluded. Duplicate lines and repeated IDs across profiles collapse; associations
remain available through `profile_refs` and the profile seed.

The extractor rejects invalid package tokens, traversal paths, changed blobs and
files over 128 KiB. It makes no network requests and executes no input. The public
text files were fetched from their pinned raw GitHub URLs, with four concurrent
reads and a 25-second per-file timeout; downloaded bytes matched the manifest's
Git blobs before extraction. No scripts, APKs, private diagnostics, device
inventories or paid datasets were fetched or executed.

From `v2/experiments/package-catalog`, with Python 3.9+ and no dependencies:

```sh
python3 -m unittest -v
python3 catalog.py --check
python3 profiles.py --check
```

To independently repeat extraction, put only the 232 public text files at their
manifest-relative paths beneath a local directory, then run:

```sh
python3 profiles.py --extract /path/to/profile-inputs --check
```

For each manifest path the source URL is
`https://raw.githubusercontent.com/Gathin23/cleanyourtv/30626a8e4714f86bf837384b109b5c73ee736917/<path>`.
The committed minimal seed supports offline replay without retaining raw prose.
A different snapshot requires an intentional manifest/hash update and fresh review.
`python3 profiles.py` regenerates the two derived reports from the pinned seed;
`--check` performs no writes.

## Licensing, validation and limits

The inspected upstream license is MIT. Minimal copied facts/source associations
retain attribution and full license text in [NOTICE.md](NOTICE.md) and
[UPSTREAM-LICENSE.txt](UPSTREAM-LICENSE.txt). Derived output joined with F-Droid
metadata remains separately AGPL-3.0 under `../data/NOTICE.md`. No upstream code,
prose or safe-to-disable label was imported; this is independent implementation,
not a claim of strict clean-room certification. Production use remains a separate
licensing/product decision, and neither experiment is approved for merging.

Thirteen offline tests pass, including the original seven and six profile tests
covering active-line extraction, dedupe, hash rejection, provenance/accounting,
unknown outcomes, schema/safety isolation and uniqueness. Both generated checks
and extraction against all 232 raw inputs pass. The isolated CI repeats the tests
and checks. Independent review is required before this draft is considered ready.

Screenshots and video are inapplicable: this changes only offline files and an
isolated test workflow. The production-isolation test confirms no runtime source
or config imports the experiment. Existing UI galleries are untouched.
