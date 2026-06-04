# Reddit post — Shield Optimizer v2

Draft / scratch file. Not committed to the repo — delete when you're done.

---

## Title options

1. I rebuilt my Shield/Android TV debloater as a free app — no terminal needed (Win/Mac/Linux, open source)  ← recommended
2. Free open-source app to debloat + tune your Android TV: health reports, launcher wizard, one-click optimize (fully reversible)
3. Made a free app to debloat & tune Android TV — looking for beta testers (Shield, Onn 4K, Chromecast w/ Google TV)

---

## Body (paste-ready — this is Reddit markdown)

A while back I shared a PowerShell script for debloating the Shield. It worked, but "open a terminal and run a script" turned a lot of people off — so I rewrote it as an actual app you just double-click. Free and open source, no accounts, no telemetry.

**What it does**

- Finds your Android TV on the network automatically (or connect by IP)
- **Health report** — temperature, RAM, storage, resolution/HDR, and your biggest memory hogs
- **One-click Optimize** — disables/uninstalls bloat from a curated per-device list, and it's all reversible (Restore puts everything back)
- **Launcher wizard** — install Projectivy / FLauncher / etc. and safely switch off the stock launcher
- Sideload APKs, tweak HDMI-CEC / frame-rate matching / animations, and save **snapshots** of a device's setup you can re-apply or clone to another box
- **Recovery** button that re-enables everything if you ever want a clean slate

**On safety:** it disables rather than uninstalls by default, and there's a hard do-not-disable list so it won't let you kill system UI, the package installer, or anything that'd brick the device.

**Works on:** Nvidia Shield (2015/2017/2019), Onn 4K Pro, Chromecast with Google TV, Google TV Streamer, and most Android TV boxes. Windows, macOS, Linux.

**Get it:** https://github.com/bryanroscoe/shield_optimizer — Mac is a one-line Homebrew install; Windows/Linux have installers.

**Honest heads-up:** it's a beta, and the builds aren't code-signed (not paying for certs on a free hobby project), so you'll hit a "unidentified developer / SmartScreen" warning on first launch — the page has the one-time click to get past it (Homebrew skips it on Mac entirely).

The old PowerShell version still works and is still maintained if you prefer the terminal.

Would love feedback and bug reports — especially from people on devices I can't test myself. What would you want it to do next?

---

## Suggested first comment (post this under your own thread)

**Download + install:**

- **macOS:** `brew tap bryanroscoe/shield-optimizer && brew install --cask shield-optimizer` (skips the Gatekeeper warning entirely)
- **Windows / Linux / manual Mac:** grab an installer from https://github.com/bryanroscoe/shield_optimizer/releases

**Getting past the first-launch warning (unsigned build):**

- **Windows (SmartScreen):** More info → Run anyway
- **macOS (.dmg):** click Done on the dialog, then System Settings → Privacy & Security → Open Anyway (or `xattr -dr com.apple.quarantine "/Applications/Shield Optimizer.app"`)
- **Linux:** `chmod +x Shield*.AppImage`

---

## Posting tips

- Attach the walkthrough GIF (`v2/screenshots/gallery.gif`) — visuals crush walls of text in these subs.
- Post weekday evenings or weekend mornings (US time) for the most eyes.
- Use an "App/Tool" or "Discussion" flair if the sub has one.
- Keep the body clean; put the download links + warning workaround in the first comment (above).

---

## Still open (tell me to finalize)

- **Which sub?** r/ShieldAndroidTV vs r/AndroidTV vs r/GoogleTV changes which devices to lead with and how techy to go.
- **Mention it's AI-assisted?** Your README credits it. I left it out of the body; add a footnote if you want it disclosed.
