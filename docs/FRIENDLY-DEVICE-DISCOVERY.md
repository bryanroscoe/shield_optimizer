# Friendly device discovery

Android TV Remote advertises a user-facing service instance on `_androidtvremote2._tcp.local.`. Google Cast advertises its friendly name in TXT `fn` on `_googlecast._tcp.local.`, with model in `md`. Cast IDs and ADB serials are different identities. The implementation does not infer one from the other.

On 2026-11-04 a bounded, read-only browse of four already-advertising TVs found the same four names on both protocols. For the two TVs already authorized in ADB, `settings get global device_name` agreed with the advertised names. No new ADB connection, pairing, server restart or device write was performed. Raw names, identifiers and network addresses remain private local evidence and are not included here. This establishes the available metadata, not which protocol atvTools uses.

Mobile discovery reuses the Android NSD resolver lifecycle developed in PR289, integrated into the current-main plugin without importing its transport rewrite or network watcher. It starts when opening the add-TV screen. Cast speakers and malformed capability/name records are excluded. The raw ADB instance remains separate from the optional friendly-name field so a metadata record cannot impersonate a saved ADB serial. A fresh, single-instance ADB observation can use the advertised label only when both records have the same nonempty Android network context and address. Conflicting names, networks, multiple ADB instances and missing context withhold enrichment. Names are display hints, not authenticated identity.

Metadata-only observations have no ADB connect or pairing ports and say ADB was not found. Enter ADB details opens manual setup with blank ports. Conflicting observations cannot offer setup as one device. Authenticated device names and verified saved nicknames take precedence; metadata does not rename or merge saved TVs. Cast/remote ports are never substituted for ADB ports. Service removal and scan replacement use the existing owner's cleanup paths. The scan remains bounded and may return a partial set of advertisements.

Protocol references:

- Chromium Open Screen: https://chromium.googlesource.com/openscreen/+/HEAD/cast/docs/protocol_flow.md
- AOSP ADB discovery: https://android.googlesource.com/platform/packages/modules/adb/+/HEAD/docs/dev/adb_wifi.md
- Android NSD: https://developer.android.com/reference/android/net/nsd/NsdManager

On Android versions without native NSD resolve cancellation (below API 34 and T extension 7), metadata never enters the native resolver queue: Android TV Remote instance names are listed without an address, and manual setup requires the address and ADB ports. Cast TXT lookup and address enrichment are withheld there so a hung metadata resolve cannot strand the native ADB resolver slot. On cancellable platforms, ADB receives queue priority and metadata cannot consume the final two resolve windows.

No new mobile discovery library or license change is introduced. Existing Kadb attribution is retained. Browser screenshots/video are synthetic fixtures and do not demonstrate a physical phone's NSD behavior. Real-phone discovery and permission behavior remain acceptance tests. The feature is integrated on current main without requiring the experimental desktop or mobile ADB transport branches. Hosted CI and physical-device acceptance remain release gates.
