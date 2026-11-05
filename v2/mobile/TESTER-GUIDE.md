# ATV Optimizer — Android closed-test guide

This guide is for invited testers once the closed test is published. A draft or an approved-but-unpublished release cannot be installed through the closed-test link. The developer is preparing the test; invitations have not been sent.

## Before you start

- Use an Android phone with Google Play and the Google account you gave the developer.
- Have an Android TV or Google TV you own or are authorized to manage, on the same Wi-Fi/local network as the phone. Fire TV is unsupported.
- Keep the TV remote nearby. Know how to reach the TV's settings before changing anything.
- Start with connection, diagnostics and remote checks. Do not change system apps or your Home app just to complete a test.

## Join and install

1. Wait for the developer to confirm that the closed test is published and your account has access.
2. Open https://play.google.com/apps/testing/com.atvoptimizer.mobile while signed into that Google account. If a Google Group is used, join that group first.
3. Opt in, then follow the Google Play download link. Install or update **ATV Optimizer** from Google Play. If you previously joined the internal test, tell the developer: an internal-test enrollment can take precedence over the closed track, and the developer must verify your closed-test enrollment before counting you.
4. If the page says unavailable, confirm the Google account first, then send the developer the exact message. Do not install an APK from a third-party site.
5. Open the app and record the installed app version, phone model and Android version with your first report.

Joining the email list is not enrollment. You must opt in using the test link, use the app and stay opted in for at least 14 consecutive days. Google evaluates the closed test before production access; completing 14 days is not an automatic public launch.

## Connect your TV

1. Enable Developer options on the TV (typically Settings → About → select the build number repeatedly).
2. Enable the TV's available Network debugging or Wireless debugging setting. Names and support vary by TV/firmware.
3. Open ATV Optimizer and select the TV. Check the model before continuing. Approve the debugging authorization prompt **on the TV**.
4. If Android's code-pairing screen is required, use the pairing code and pairing port currently shown on the TV; the connect port can differ from the pairing port. Do not post these codes in a public issue.
5. If discovery finds nothing, use the app's connection help. Confirm both devices are on the same local network and that guest-network/client isolation is not blocking them. Report failure honestly rather than marking connection as passed.

Wireless debugging grants control of your TV. Enable it only on a trusted network, authorize only devices you recognize, and disable debugging when you no longer need it.

## First useful checks

- **Connection:** connect, return to the phone home screen, reopen the app and confirm it still shows the intended TV.
- **Apps and diagnostics:** load the app list and diagnostics. Verify that data belongs to the selected TV and that Unknown packages are not presented as Safe.
- **Remote:** try navigation and Back while watching the TV. Avoid power/reboot controls until ordinary controls work.
- **Recovery:** find More → Emergency recovery before changing app state. Recovery re-enables apps; it does not reverse every setting or Home-app change.
- **Reconnect:** reconnect after an app relaunch and a normal network interruption. Report app restarts, lost selection, unexpected reconnects or stale results.

Only if you intentionally test a change: review the warning and preview, change one reversible nonessential item at a time, then verify the result and undo it. Never disable the last working Home app. Stop immediately if the TV becomes unstable or the selected TV is ambiguous.

## Pro and payments

The app is a free download. Pro is an optional one-time Google Play purchase, with no subscription. Do not pay money merely to participate in the test.

The developer must separately add your Google account to **License testing** before asking you to exercise purchases. Closed-test membership alone does not make purchases free. In Google's payment sheet, use only an explicitly named **test** payment instrument. If the sheet offers only your real card or another real payment method, cancel and contact the developer.

For purchase testers, follow the developer's ordered test script in [RELEASE.md](RELEASE.md#purchase-test-script-license-tester-closed-or-internal-track-build-installed-from-play): cancellation/decline and pending purchase before successful ownership, then acknowledgment, restore and refund/revocation checks. Never report a purchase/restore as passed from a paywall screenshot alone.

A supporter promo code is single-use and grants Pro to the redeeming Google account. Do not share it publicly. In the app, More → Restore purchases refreshes ownership. The reviewer access code is reserved for Google and must not be used for tester purchases.

## Send useful feedback

Email **bryanroscoe+atv@gmail.com** with:

- App version/build, phone model/Android version, TV model/Android version.
- What you tried, what you expected, what actually happened, and whether it repeats.
- The time and time zone of the problem. For connection issues, say discovery, authorization, code pairing or reconnect.
- Relevant screenshots and a redacted debug log if available. Include the app's Support ID when useful for optional report/deletion support.

Remove private TV names, network addresses, serial numbers, pairing codes, Google-account details and payment/order information from public issues. Send only the minimum needed through the private support channel. Never send passwords or verification codes.

Privacy and data deletion: https://atv-optimizer-telemetry.bryanroscoe.workers.dev/privacy

Project information: https://bryanroscoe.com/atv-optimizer/

## Developer launch gate

Before distributing the closed test: verify the exact signed Play build, real phone/TV acceptance, purchase/restore/acknowledgment/pending/refund behavior, Google review outcome, selected tester access and the opt-in/install link. Keep managed publishing on until the owner is ready to begin. This guide is preparation, not a claim that those checks passed.
