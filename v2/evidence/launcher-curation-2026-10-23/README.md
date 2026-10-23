# Launcher curation correction — simulated evidence

PR209 added the shared `com.gathin.tvhome` identity as a curated recommendation. The correction restores the seven pre209 custom entries. No explicit CleanYourTV entry existed before209. Generic HOME discovery, deduplication, disabled-handler recovery, Advanced selection and takeover safety are unchanged.

The four screenshots and `launcher-states-simulated.webm` use demo fixtures only. Missing packages have no recommendation; installed, tracked-disabled and current HOME handlers appear under their neutral package identity with no developer promotion. This package identity cannot distinguish forks. No hardware compatibility or rating claim is made.

Reproduce with `LAUNCHER_CURATION_EVIDENCE=1 npm run test:launcher-rows` from `v2/`. Browser-injected rows exercise rendering; the loader regression separately exercises the actual catalog and pure row builder, including HOME deduplication and tracked-disabled recovery.

Source: [PR209 exact change](https://github.com/bryanroscoe/shield_optimizer/pull/209/files). Both desktop galleries are regenerated. No real device was queried or changed.
