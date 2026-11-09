# Optimize review-tier audit — 2026-11-08

Bryan, testing desktop Optimize on 2026-11-08: Moonlight, Artemis, Steam Link,
Aerial Dream, ADB TV, X-plore, Send Files to TV and similar apps read as plain
"Safe to remove", and "Select all safe" armed them. "All the ones currently on
this list are ones they should only disable if they don't use them. I think we
maybe expanded our list but didn't actually do it."

`review: true` is the catalog's "remove only if you don't use it" marker. This
audit sets it on every entry for an app a person installs or uses by choice, and
leaves it off only for bloat nobody chooses: OEM promos, preinstalled services
and defunct apps. Nothing else in an entry changed: `risk`, `method`,
`default_optimize`, `default_restore`, descriptions and the do-not-disable rules
are as they were. Each changed entry has a `sources` note pointing here. Its
`reviewed_at` keeps its earlier date: this audit changed the review flag only and
did not re-review any safety rating. Desktop and Android read the same lists.

Two loader tests keep it that way. `chosen_apps_outside_the_default_bloat_are_review`
requires `review` on every Safe, Medium or Advanced entry that isn't default bloat,
unless the entry is named in `PREINSTALLED_NOT_REVIEW`: the Safe and Medium rows
of the preinstalled table below. High is for components the TV needs and isn't
covered. `preinstalled_exceptions_exist_and_are_not_review` fails when a name on
that list is no longer in any app list, or has become review, default bloat or
defunct.

## Set to review (36)

| Package | App | Why |
| --- | --- | --- |
| `org.smarttube.stable` | SmartTube | Sideloaded YouTube client |
| `io.gh.reisxd.tizentube.cobalt` | TizenTube Cobalt | Sideloaded YouTube client |
| `org.xbmc.kodi` | Kodi | Media player installed by choice |
| `org.videolan.vlc` | VLC | Media player installed by choice |
| `org.jellyfin.androidtv` | Jellyfin | Media-server client |
| `com.stremio.one` | Stremio | Media client, sideloaded |
| `com.mxtech.videoplayer.ad` | MX Player | Media player |
| `com.plexapp.mediaserver.smb` | Plex Media Server (Shield) | Media server someone runs on purpose |
| `com.limelight` | Moonlight Game Streaming | Game streaming |
| `com.limelight.noir` | Artemis (Moonlight Noir) | Game streaming, sideloaded |
| `com.valvesoftware.steamlink` | Steam Link | Game streaming |
| `com.retroarch` | RetroArch | Emulator frontend |
| `com.codingbuffalo.aerialdream` | Aerial Dream | Screensaver installed by choice |
| `com.neilturner.aerialviews` | Aerial Views screensaver | Screensaver installed by choice |
| `com.lonelycatgames.Xplore` | X-plore File Manager | File manager |
| `pl.solidexplorer2` | Solid Explorer | File manager |
| `com.cxinventor.file.explorer` | Cx File Explorer | File manager |
| `com.yablio.sendfilestotv` | Send Files to TV | File transfer |
| `com.esaba.downloader` | Downloader by AFTVnews | Download and sideload tool |
| `com.cybercat.adbappcontrol.tv` | ADB TV: App Manager | ADB and app manager |
| `by4a.setedit22` | SetEdit SettingsDatabaseEditor | Settings tool |
| `flar2.homebutton` | Button Mapper | Remote-button remapper |
| `com.tailscale.ipn` | Tailscale | VPN |
| `com.wireguard.android` | WireGuard | VPN |
| `com.surfshark.vpnclient.android` | Surfshark | VPN (stays Caution) |
| `com.phlox.tvwebbrowser` | TV Bro | Web browser |
| `com.espn.score_center` | ESPN | Streaming and sports client |
| `com.fox.foxone` | FOX One | Streaming client |
| `com.getchannels.dvr.app` | Channels: Whole Home DVR | DVR client |
| `com.handmark.sportscaster.androidtv` | CBS Sports: Watch Live | Sports client |
| `com.hoopladigital.android` | Hoopla | Library media client |
| `com.kanopy.tvapp` | Kanopy for Android TV | Library film client |
| `com.moviesanywhere.goo` | Movies Anywhere | Movie-library client |
| `com.silicondust.view` | HDHomeRun | Tuner and DVR client |
| `com.turner.tbs.android.networkapp` | Watch TBS | Streaming client |
| `com.turner.tnt.android.networkapp` | Watch TNT | Streaming client |

The ten streaming clients from `docs/REPORTED-PACKAGE-NAMES-2026-10-29.md`, and
SetEdit, Cx File Explorer, Aerial Views and Surfshark from the same review,
carried an explicit `review: false`. They were the "optional content clients" of
that review, so this owner direction supersedes that one field only.

Fifty entries already had `review: true` (Netflix, Prime Video, YouTube, Spotify,
Plex, Tubi, Pluto TV, the 2026-11-05 and 2026-11-06 batches, and others). They are
unchanged.

## Deliberately left off (35)

**Default bloat** (`default_optimize: true`, which the loader forbids together
with `review`): Google Feedback, Nvidia Telemetry, Nvidia Diagnostics, Nvidia
Feedback, Nvidia Games (Tegra Zone), Walmart App, Home Recommendations.

**Defunct services** (also default bloat): Google Play Movies, Google Play Music,
Funimation, Stadia, Stadia (Android TV), Quibi, HBO Now (legacy).

**Preinstalled components and services nobody installs:**

| Package | Risk | Why left off |
| --- | --- | --- |
| `com.android.printspooler` | Safe | Android print plumbing |
| `com.android.gallery3d` | Safe | Stock photo viewer that ships with Android |
| `com.google.android.tv.remote.service` | Medium | Google's remote and keyboard service |
| `com.android.providers.tv` | High | Live Channels system provider |
| `com.android.dreams.basic` | Medium | Built-in screensaver |
| `com.google.android.katniss` | High | Google Assistant |
| `com.google.android.apps.mediashell` | High | Chromecast built-in |
| `com.google.android.tts` | High | Text-to-speech engine |
| `com.google.android.play.games` | Medium | Preinstalled Google Play Games |
| `com.google.android.tungsten.setupwraith` | High | Setup and Home component |
| `com.google.android.backdrop` | Medium | Preinstalled Ambient Mode screensaver |
| `com.google.android.tvrecommendations` | High | Home recommendations service |
| `com.nvidia.osc` | Medium | Nvidia on-screen controls |
| `com.nvidia.shieldtech.hooks` | Medium | Nvidia system hooks |
| `com.nvidia.nvgamecast` | Medium | Preinstalled Nvidia GameStream |
| `com.nvidia.ota` | High | Nvidia system updater |
| `com.google.android.speech.pumpkin` | High | Speech services |
| `com.google.android.tvlauncher` | High | Stock launcher |
| `com.google.android.tungsten.overscan` | Medium | Overscan calibrator |
| `com.droidlogic.launcher.provider` | Medium | Launcher provider |
| `com.google.android.apps.tv.launcherx` | High | Google TV Home |

Two to note. `com.google.android.backdrop` (Ambient Mode) is preinstalled, so it
stays off even though `com.google.android.apps.tv.dreamx` (Google TV Ambient
Mode) has carried `review: true` since the 2026-11-05 batch. And user-installed
launchers (Projectivy, FLauncher, …) live in `launchers.json`, not in the
Optimize lists, so there was nothing to mark.

## Behavior

A review row starts at Keep, shows "Review: disable if unused" (or "uninstall")
with its last-used date, and gets no "Recommended" label. Desktop Optimize no
longer has "Select all safe", so each one is armed by hand. Restore mode has
"Re-enable all disabled", which arms Enable on every disabled catalog app:
turning apps back on is the safe direction. Android's
"Select all" only appears on the Recommended tab, which holds default bloat and
never a review app, and on Restore's re-enable list.
