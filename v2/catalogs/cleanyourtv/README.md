# CleanYourTV informational package leads

This offline catalog preserves package observations from a pinned MIT source for
research through ATV Optimizer's existing review process. The app's read-only
source browser and the Python lookup below expose the original observations.
Profile membership records what the upstream author wrote. Each result is
unreviewed and non-actionable.

The app also exposes a read-only source browser at Catalog on desktop and
More → Device & package catalog on mobile. Package-detail links prefill a package
identity and require the person to choose a source profile. This browser displays
the selected upstream material without matching it to the connected TV. The full
MIT notice is bundled in its Source attribution section. Core app lists, safety
and support tiers retain their existing authorities.

The source is Gathin23/cleanyourtv at commit
`8672e68e8a24c1258e4662cd11daa4ed13795dad`. Full attribution and modification
details are in [NOTICE.md](NOTICE.md), and the complete license is in
[upstream/LICENSE](upstream/LICENSE).

## Lookup and validation

Python 3.9 or later and the standard library are sufficient. Run from this folder.

```sh
python3 catalog.py check
python3 -m unittest discover -s . -p 'test_*.py'
python3 catalog.py lookup com.realtek.leanbacklauncher.partnercustomizer --profile acer-ar3-r3
```

An exact upstream profile ID is required. The result includes original source
lines, pinned links, profile metadata, comments and the full MIT notice. An
`active_disable` observation describes an uncommented upstream entry. A
`commented_out` observation retains an optional or kept entry. Its original text
can also say that it was absent or disabled by personal choice. A
`comment_reference` points to prose mentioning an identity found in a package
line somewhere in the source set. Every comment remains available even when its
identities cannot be indexed confidently. Conflicting occurrences stay separate.

The original `tv`, `os`, `brand`, `notes` and optional `readme_os` labels remain
under `upstream_context`. Firmware/build statements and keep warnings are
preserved in the original list comments. Folder names and prose are never turned
into verified hardware identity. `verified_model`, `verified_firmware` and
`verified_platform` remain null. This also preserves profiles which span multiple
models or firmware versions without inventing a single exact match.

Optional `--model`, `--platform` and `--firmware` arguments describe an observed
device context. Observations, source comments and upstream context notes are
withheld unless all three verified context
fields exactly match. The imported profiles have unknown verified context, so
these queries remain unknown and return no package observations. Researching an
explicit source profile without device arguments remains available.

## Review boundary

The catalog is separate from the app's embedded app lists, friendly-name lookup,
safety engine and support tiers. Its schema supplies no accepted safety verdict.
Every lookup emits `actionable: false` and `review_status: unreviewed`, including
an exact-context query. It provides no ADB commands, batch actions or app writes.

Use [registry-triage](../../tools/registry-triage/README.md) for first-party
proposals. The small [research ledger](research-ledger.json) records selected
identities, independent evidence and unanswered questions. Its conservative
research disposition is separate from the original upstream decision. Accepted
catalog changes require their own normal review, `reviewed_at`, sources and loader
validation. Existing do-not-disable, caution, last-HOME, SetupWraith/network and
user-0 authorities continue to govern actions.

## Reproduce the import

`input-manifest.json` pins every source path, commit, Git blob ID, SHA256 and byte
length. `provenance.json` pins the manifest, generated catalog and full license.
`check` validates all source bytes, rejects extra source files and regenerates the
catalog in memory before comparing it with the committed result.
The local Git attributes preserve upstream bytes across platforms, including the
source's original trailing whitespace, and keep generated JSON in LF form.

Given a local Git object store containing the pinned commit and selected blobs,
the locally authored importer can regenerate the snapshot.

```sh
python3 catalog.py import-git /path/to/cleanyourtv.git
python3 catalog.py check
```

The importer reads Git objects through `git ls-tree` and `git cat-file`. It reads
only regular files matching the documented input paths and the root license.
`rebuild` regenerates derived files from the locally verified snapshot. Upstream
scripts, APKs and assets are never executed by this tooling. External issue, PR
and forum bodies are outside the source set. Links appearing in original comments
retain their source attribution without becoming independently verified evidence.
