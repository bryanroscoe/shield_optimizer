# Experimental data license and attribution

`seed.json` retains selected factual fields from the F-Droid active repository
index, retrieved 2026-10-22. Credit: F-Droid contributors and contributing app
maintainers. Source: https://f-droid.org/repo/index-v1.json; upstream metadata:
https://gitlab.com/fdroid/fdroiddata. See `../sources.json` for the exact input hash
and inspected license revision.

Changes: retain only package IDs, category terms and required TV-feature/version
facts; sort and deduplicate; map category terms into original broad descriptions;
add explicit unreviewed safety, unknown TV status and aggregate coverage against
the local registry. `catalog.json` and `coverage.json` are generated adaptations.

These three JSON data files are distributed under GNU Affero General Public
License version 3 (AGPL-3.0); see LICENSE-AGPL-3.0.txt. Original generated text and
selection/arrangement contributions by Bryan Roscoe, 2026, are licensed on the same
terms for these data files. This notice overrides the repository's default license
only for these data files. No warranty or endorsement by F-Droid is implied.
