# Sharing the Android app: readiness, invites and helping people

The owner's one-page answer to three questions: is the phone app ready to hand to other people,
how do people get it, and how do you help them once they have it. [`TESTER-GUIDE.md`](TESTER-GUIDE.md)
is what you send testers; this file is for you. Console click paths are in
[`PLAY-CONSOLE-WALKTHROUGH.md`](PLAY-CONSOLE-WALKTHROUGH.md); builds are in [`RELEASE.md`](RELEASE.md).

## 1. Is it ready? Not yet (checked 2026-11-21)

What Google Play holds, read through the Play Developer API on 2026-11-21:

| Track | Release | State |
| --- | --- | --- |
| Internal testing | 0.1.2 (versionCode 1002), built from `26a783a` | **Draft**: never rolled out |
| Internal testing | 0.1.0 (1000) | Rolled out |
| Closed testing (`alpha`) | 0.1.1 (1001) | Rolled out on the track; no Google Group attached. Whether Google approved and published it is shown only in Console's **Publishing overview** |
| Production | none | |

The one thing that blocks sharing: **nobody has connected a Play-installed build to a TV yet.**
On 2026-11-05 Play 0.1.1 installed on the Pixel 10 Pro, started and found four TVs, but the
Shield never showed its "Allow debugging?" dialog. An app that can't connect is useless to a
tester and burns the goodwill you need for the 14-day clock. Purchases have also never run
against a real Play account.

It is ready to share when these four steps pass, in order:

1. **Put the current build on your phone through Play.** Roll out the 0.1.2 internal draft
   (Test and release > Internal testing > the 0.1.2 draft > Review release > Start rollout), then
   update ATV Optimizer from the Play Store on the Pixel. `main` has moved since `26a783a` only by
   app-list and support-tier data, a server-side telemetry change and CI, so 0.1.2 is the right
   build to test. If step 2 needs a code fix, an agent cuts 0.1.3 instead (RELEASE.md, "Automated
   release (CI)").
2. **Connect it to two TVs yourself** (about 45 minutes). For the Shield, use Network debugging and
   *Enter IP address manually*, port 5555. For a Google TV, use Wireless debugging > *Pair device
   with pairing code*. On each TV: approve the prompt, open the Dashboard and the app list, use the
   remote, turn off one unneeded app and turn it back on, open More > Emergency recovery, then
   force-stop the app, reopen it and check that it reconnects to the same TV. A failure here is the
   blocker to fix before anyone else sees the app. Send the agent the time it happened and the
   More > Debug log.
3. **Buy Pro as a license tester** (about 15 minutes). Add your account under Console Settings >
   License testing, buy with "Test instrument, always approves", wait 5 minutes, confirm in Order
   management that the order is still charged (not auto-refunded), force-stop and reopen the app,
   then refund it. The full script is RELEASE.md §6.
4. **Point the closed track at that build and open it to the group in §2.** Closed testing >
   Create new release > add the build that passed steps 2 and 3 > send it for review. With managed
   publishing on, click **Publish changes** in Publishing overview once Google approves.

Not required before the first testers: Fast-remote Phase 4, the app-restart report (#105), the
16 KB emulator runtime check (its runner currently fails to start the emulator with a missing
`libpulse.so.0`) and the P1 product work. Testers can find those problems with you during the
14 days. Production access still needs 12 testers opted in for 14 days in a row; that clock starts
when the first 12 join, so the sooner step 4 happens the better.

## 2. How people get it: one Google Group, two links

Closed testing only admits listed Google accounts. An email list means collecting every
address and pasting it into the Console. A Google Group that anyone can join turns the invite
into links you can post anywhere.

**One-time setup (about 10 minutes, owner):**

1. At <https://groups.google.com>, **Create group**: name `ATV Optimizer testers`, email
   for example `atv-optimizer-testers@googlegroups.com`.
2. Settings: *Who can search for group*: anyone on the web. *Who can join group*: anyone can join.
   *Who can view conversations* and *Who can post*: group managers (so nobody can spam
   testers). *Who can view members*: group managers.
3. Play Console > ATV Optimizer > Test and release > Testing > Closed testing > Manage track >
   **Testers** > **Google Groups** > add the group's email. Set *Feedback URL or email address* to
   `bryanroscoe+atv@gmail.com`. Save. An agent can attach the group through the Play API instead,
   once you give it the group's address.
4. Keep your existing email list ticked too if it already holds people.

**What you share** (the same for everyone):

1. Join the group: `https://groups.google.com/g/<group-name>` > **Join group**.
2. Become a tester: <https://play.google.com/apps/testing/com.atvoptimizer.mobile> >
   **Become a tester** > **Download it on Google Play**.

The opt-in page works only after the closed release is published, and only for an account in the
group. Joining can take a few minutes to reach Play. After production launch, the only link you
need is <https://play.google.com/store/apps/details?id=com.atvoptimizer.mobile>.

**Invite message** (paste into email, Discord, Reddit or Ko-fi; it carries no private data):

> I'm testing the Android phone version of ATV Optimizer. It cleans up and manages an Android TV,
> Google TV or NVIDIA Shield from your phone over Wi-Fi, with no PC. Fire TV isn't supported.
>
> 1. Join the tester group: `https://groups.google.com/g/<group-name>`
> 2. Then open https://play.google.com/apps/testing/com.atvoptimizer.mobile on the same Google
>    account, tap **Become a tester**, and install from the Play Store.
> 3. Please stay opted in for 14 days. Google needs 12 testers for 14 days in a row before the app
>    can go public.
>
> The app is free. Don't buy Pro to help test. Setup steps and what to try:
> https://github.com/bryanroscoe/shield_optimizer/blob/main/v2/mobile/TESTER-GUIDE.md
> Problems or ideas: bryanroscoe+atv@gmail.com

Ko-fi supporters get the same message plus a Pro promo code (PLAY-CONSOLE-WALKTHROUGH.md
Phase 5). Keep their codes and every tester address out of this repository.

## 3. Helping people

Ask every reporter for four things first. Most problems resolve from them alone.

- The app version: Android Settings > Apps > ATV Optimizer (the version is at the bottom), or the
  Play Store page. The app itself doesn't show it yet.
- Phone model and Android version; TV model and Android version.
- What they tapped, what they expected and the exact message on screen (a screenshot is best).
- When it matters: More > **Bug report** (they review it, then it copies to the clipboard; nothing
  is sent), More > **Debug log** > Copy, and More > Privacy > **Copy Support ID**.

Never ask for a pairing code, a password or a Google order number in public. Move anything with
TV names, addresses or serial numbers to email.

### Can't install, or the test link says "not available"

1. Not published yet: the link only works once the closed release is published (§1 step 4).
2. Wrong Google account: they must join the group and open the link on the account their Play
   Store uses. On a phone with several accounts, switch accounts in the Play Store.
3. Just joined: group membership can take a few minutes to reach Play. Wait, then reopen the link.
4. They joined Internal testing earlier: internal enrollment can take precedence over the closed
   track. Remove them from the internal list.
5. Phone too old: the app needs Android 7.0 or later.

### "No TVs found"

1. Phone and TV on the same Wi-Fi, TV awake. A guest network, "client isolation" or a VPN on the
   phone hides the TV.
2. Debugging must be on: TV Settings > About > select *Build* seven times, then Developer options.
3. Shield and older TVs use **Network debugging**, which often isn't discoverable: *Enter IP address
   manually*, port 5555. The TV's address is in its network settings.
4. Android 11+ Google TVs use **Wireless debugging**: open *Pair device with pairing code* on the
   TV, then scan again or type the address and port the TV shows.
5. Fire TV is not supported: Fire OS blocks the changes this app makes. Once connected, the app
   labels a Fire TV "not supported".

### It finds the TV but won't connect

- **"Allow debugging?" never shows on the TV.** If the error is a timeout, "refused" or
  "unreachable", the phone never reached the TV: check the address and port, and that debugging is
  still on. If it did reach the TV, look at the TV within 30 seconds, choose *Always allow*, then
  *Allow*. Some Android 11 Shields keep a stale prompt; restarting the TV clears it.
- **Pairing failed or wrong code.** The code expires when the TV's pairing dialog closes, and the
  pairing port is different from the connect port. Keep the dialog open, use the code and port it
  shows right now, and try once more.

### Connected, then lost

The banner with *Retry* and *Switch TV* appears when the TV sleeps, reboots or the phone changes
network. Wake the TV and tap Retry. If it keeps happening, ask for the debug log and the time.

### "I turned something off and now the TV misbehaves"

1. More > **Emergency recovery** turns every disabled app back on. It is always free.
2. Home screen gone: Launcher, or Emergency recovery, from the phone. Debugging stays on, so the
   phone can still connect.
3. Emergency recovery re-enables apps only. It doesn't undo settings changes or reinstall an app
   that was uninstalled. An uninstalled app comes back from the Play Store, or for one that came
   with the TV, through Reinstall in its app sheet while its APK is still on the TV.

### Pro and payments

- Bought but still Free: More > **Restore purchases**, on the same Google account that paid.
  A pending payment unlocks once Google completes it.
- Refund: Play Console > Order management > the order > Refund, and tick removing the
  entitlement. Users can also ask Google directly through their Play order history.
- Promo code: they redeem it in the Play Store (profile menu > Redeem code) or with
  `https://play.google.com/redeem?code=<CODE>`, then reopen the app. Promo redemptions don't appear in Order management.
- Testers never need to pay. If a tester's payment sheet shows a real card, they aren't on the
  License testing list: tell them to cancel.

### Delete my data

Ask for the Support ID (More > Privacy > Copy Support ID), then follow the deletion steps under
[Retention in `../telemetry-worker/README.md`](../telemetry-worker/README.md#retention). The public
policy is <https://atv-optimizer-telemetry.bryanroscoe.workers.dev/privacy>.

### Where reports go

Reproducible bugs become GitHub issues with private details removed; mention the TV model and
app version in the title. Device-support evidence feeds the weekly "Device support report"
issue (`docs/DEVICE-SUPPORT.md`). Answer every tester within a day or two during the 14 days:
engaged testers are what Google's production-access review asks about.
