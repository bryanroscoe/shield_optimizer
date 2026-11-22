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
| Closed testing (`alpha`) | 0.1.1 (1001) | Live (Console shows the app in Closed testing). The tester Google Group replaced the owner's email list on 2026-11-21; that change is in Google's review |
| Production | none | |

The [`mobile-v0.1.4` run](https://github.com/bryanroscoe/shield_optimizer/actions/runs/44964485346)
subsequently verified and submitted versionCode 1004 to Internal and Closed testing. Check tester
availability in Play separately. The connection and support fixes after that tag need a new candidate.

Two things block sharing. **No Play-installed build has connected to a TV yet**: on 2026-11-05
Play 0.1.1 installed on the Pixel 10 Pro, started and discovered four LAN endpoints, but the Shield
never showed its "Allow debugging?" dialog. An app that can't connect is useless to a tester and
burns the goodwill you need for the 14-day clock. And **purchases have never run against a real
Play account.**

**How a build reaches people** (owner decision, 2026-11-21). Merging to `main` releases nothing.
An agent bumps the version, merges it and pushes a `mobile-vX.Y.Z` tag. The tag's CI run builds and
verifies the app, uploads it once, and releases it as *completed* on two tracks
(RELEASE.md, "Automated release (CI)"):

- **Internal testing**: available within minutes to accounts on its tester list.
- **Closed testing** (`alpha`): goes to Google's review and reaches the tester group as soon as it
  is approved, usually within hours to a few days. Managed publishing was turned off on
  2026-11-21, so nobody has to click Publish. To stop a closed release that is still in review:
  Play Console > Publishing overview > **Remove changes**.

It is ready to share when these four steps pass, in order:

1. **Install the candidate from the Play Store** once the tag containing the fixes has run.
   Check the installed version before testing. Your account gets it at once if it is on the
   Internal testing list. Otherwise it arrives with the closed release after Google's review; as
   the group's creator you are a member, and the group is now the only way into the closed test.
2. **Connect it to two TVs yourself** (about 45 minutes, on Free). Shield: Network debugging,
   *Enter IP address manually*, port 5555, then approve "Allow debugging?" on the TV. Google TV:
   Wireless debugging > *Pair device with pairing code*; a paired TV shows no prompt. On each TV:
   open the Dashboard and the app list, use the remote, open More > Emergency recovery, then
   force-stop the app, reopen it, pick the TV under "Which TV?" and confirm it connects with no new
   prompt. Any failure is the blocker to fix before anyone else sees the app: send the agent the
   time it happened and a reviewed report from Support and privacy on the connection screen, or from More.
3. **Run the purchase script as a license tester** (about 30 minutes): RELEASE.md §6, in its order.
   Add your account under Console Settings > License testing first. While you own Pro, turn off one
   unneeded app on a TV and turn it back on (Disable is a Pro action), before the script's refund.
4. **Share the group link in §2.** The closed track already carries every tagged build, so there
   is nothing to promote by hand.

The rest of the phone/TV matrix, Fast-remote Phase 4, the app-restart report (#105) and the 16 KB
emulator check happen during the 14 days. Record a successful runtime receipt for the exact
signed candidate using RELEASE.md. Production access needs 12 testers opted in
for 14 days in a row, so the clock starts only once 12 have joined.

## 2. How people get it: one Google Group, two links

Closed testing only admits listed Google accounts. An email list means collecting every
address and pasting it into the Console. A Google Group that anyone can join turns the invite
into links you can post anywhere.

**One-time setup (about 10 minutes, owner):**

1. Done 2026-11-21: <https://groups.google.com/g/atv-optimizer-testers>
   (`atv-optimizer-testers@googlegroups.com`). For a new group: <https://groups.google.com>,
   **Create group**.
2. Settings: *Who can search for group*: anyone on the web. *Who can join group*: anyone can join.
   *Who can view conversations*: anyone on the web. Only managers can post, so there is nothing
   private to see, and with a narrower setting the group's page shows non-members "Content
   unavailable" instead of a **Join group** button (seen 2026-11-21). *Who can post*: group
   managers, so nobody can spam testers. *Who can view members*: group managers.
3. Play Console > ATV Optimizer > Test and release > Testing > Closed testing > Manage track >
   **Testers** > **Google Groups** > add the group's email. Set *Feedback URL or email address* to
   `bryanroscoe+atv@gmail.com`. Save. This has to be done in Console: the Play Developer API
   refuses tester changes on this track ("upgraded to use open or closed testing", 2026-11-21).
4. Console takes email lists *or* Google Groups, not both; choosing the group dropped the old
   one-address owner list on 2026-11-21. Anyone who needs closed access joins the group.

**What you share** (the same for everyone):

1. Join the group: <https://groups.google.com/g/atv-optimizer-testers> > **Join group**.
2. Become a tester: <https://play.google.com/apps/testing/com.atvoptimizer.mobile> >
   **Become a tester** > **Download it on Google Play**.

The opt-in page works only after the closed release is published, and only for an account in the
group. Google doesn't say how long a new group member takes to reach Play, and after the first
publish the opt-in link itself can take hours to work. After production launch, the only link you
need is <https://play.google.com/store/apps/details?id=com.atvoptimizer.mobile>.

**Invite message** (paste into email, Discord, Reddit or Ko-fi; it carries no private data):

> I'm testing the Android phone version of ATV Optimizer. It cleans up and manages an Android TV,
> Google TV or NVIDIA Shield from your phone over Wi-Fi, with no PC. Fire TV isn't supported.
>
> 1. Join the tester group: https://groups.google.com/g/atv-optimizer-testers
> 2. Then open https://play.google.com/apps/testing/com.atvoptimizer.mobile on the same Google
>    account, tap **Become a tester**, and install from the Play Store.
> 3. Please stay opted in for 14 days. Google needs 12 testers for 14 days in a row before the app
>    can go public.
>
> The app is free, and the first 12 testers get Pro free: after you install it, email
> bryanroscoe+atv@gmail.com from the Google account you joined with and I'll send a Google Play
> code. Don't buy Pro to help test. Setup steps and what to try:
> https://bryanroscoe.com/atv-optimizer/testing/
> Problems or ideas: bryanroscoe+atv@gmail.com

**Free Pro** (owner decision, 2026-11-21). Joining the closed test does not make Pro free: Play
charges real money unless the account is on License testing (kept for the owner's purchase tests)
or redeems a promo code. So:

- **The first 12 testers** who email from the account they joined with get a one-time-use promo
  code for `atvopt_pro`. Count them in order of their first email.
- **Ko-fi supporters** get one regardless (README, "Support the project"); they don't count toward
  the 12. When the twelfth tester code goes out, remove the offer from README.md (banner and
  Android section), the invite above, TESTER-GUIDE.md and the website, and tell later emailers the
  free places are taken.
- Don't add anyone who gets a code to License testing: Google doesn't document how a promo code
  behaves on a license-test account (PLAY-CONSOLE-WALKTHROUGH.md 3.4).

Create codes in Play Console > Monetize with Play > Promo codes (one-time-use, product
`atvopt_pro`, purchase option `buy`; 500 per quarter across products; PLAY-CONSOLE-WALKTHROUGH.md
Phase 5). A redeemed code is a real Play entitlement on that account and carries over to the public
release. Send each code privately with `https://play.google.com/redeem?code=<CODE>`; the tester
then opens the app within three days (or taps More > Restore purchases) so the app acknowledges
the purchase; Play refunds unacknowledged purchases after three days. Keep codes, the list of who got one, and
every tester address out of this repository.

## 3. Helping people

Ask every reporter for four things first. Most problems resolve from them alone.

- The app version: Android Settings > Apps > ATV Optimizer (at the bottom), or the first line of
  More > Bug report.
- Phone model and Android version; TV model and Android version.
- What they tapped, what they expected and the exact message on screen (a screenshot is best).
- When it matters: More > **Bug report** (they review it, then it copies to the clipboard; nothing
  is sent), More > **Show debug log** > Copy, and More > Privacy > **Copy Support ID**. The
  Support ID exists only while reports are on.

Never ask for a pairing code, a password or a Google order number in public. Move anything with
TV names, addresses or serial numbers to email.

### Can't install, or the test link says "not available"

1. Not live yet: the link only works for a closed release Google has approved, and for the group
   change itself (in review since 2026-11-21).
2. Wrong Google account: they must join the group and open the link on the account their Play
   Store uses. On a phone with several accounts, switch accounts in the Play Store.
3. Just joined: group membership isn't instant. Wait a while, then reopen the link.
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

- **"Allow debugging?" never shows on the TV.** Read the error. "Refused", "unreachable" or
  "Connection timed out" with nothing answering mean the phone never reached the TV: check the
  address and port, and that debugging is still on. "Timed out during setup" means it reached the
  TV and waited 30 seconds for approval: look at the TV, choose *Always allow*, then *Allow*. Some
  Android 11 Shields hold a stale prompt and never show the phone's. The app advises restarting the
  TV; whether that clears it hasn't been confirmed on hardware yet, so report it if it doesn't.
- **Pairing failed or wrong code.** Each code allows one attempt and dies when the TV's pairing
  dialog closes, and the pairing port differs from the connect port. Reopen *Pair device with
  pairing code* on the TV for a fresh code and use the code and port it shows now.

### Connected, then lost

The banner with *Retry* and *Switch TV* appears when the TV stops answering: a reboot, a network
change, or sleep on TVs that drop debugging while asleep (a Shield keeps it up). Wake the TV and tap Retry. If it keeps happening, ask for the debug log and the time.

### "I turned something off and now the TV misbehaves"

1. More > **Emergency recovery** turns every disabled app back on. It is always free.
2. Home screen gone: Launcher, or Emergency recovery, from the phone while it is still connected.
   Don't restart the TV first: Wireless debugging can switch itself off after a restart, and with
   no Home screen it is hard to turn back on.
3. Emergency recovery re-enables apps only. It doesn't undo settings changes or reinstall an app
   that was uninstalled. An uninstalled store app comes back from the Play Store. One that came
   with the TV may come back through **Reinstall**: a recognised app stays in Apps as not installed
   with Reinstall, and any other app offers it in its sheet right after the uninstall.

### Pro and payments

- Bought but still Free: More > **Restore purchases**, on the same Google account that paid.
  A pending payment unlocks once Google completes it.
- Refund: Play Console > Order management > the order > Refund, and tick removing the
  entitlement. Users can also ask Google directly through their Play order history.
- Promo code: they redeem it in the Play Store (profile menu > Redeem code) or with
  `https://play.google.com/redeem?code=<CODE>`, then reopen the app. Promo redemptions don't
  appear in Order management.
- Testers never need to pay. If a tester's payment sheet offers only a real card, they aren't on
  the License testing list: tell them to cancel.

### Delete my data

Ask for the Support ID (More > Privacy > Copy Support ID; it shows only while reports are on, so
they should copy it before turning reports off). Then follow the deletion steps under
[Retention in `../telemetry-worker/README.md`](../telemetry-worker/README.md#retention). The public
policy is <https://atv-optimizer-telemetry.bryanroscoe.workers.dev/privacy>.

### Where reports go

Reproducible bugs become GitHub issues with private details removed; mention the TV model and
app version in the title. The weekly "Device support report" issue is built from telemetry alone;
a tester's report is a reason to look, not evidence, so add context as a comment there and never
edit its body (`docs/DEVICE-SUPPORT.md`). Answer every tester within a day or two during the 14 days:
engaged testers are what Google's production-access review asks about.
