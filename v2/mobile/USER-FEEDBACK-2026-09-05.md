# Mobile feedback — 2026-09-05

Reported by Bryan after installing the debug build from `main` at `44d2d66` on a
Pixel 10 Pro. These are user observations and requested improvements, not verified
root causes. All acceptance items remain open; this document records TODOs and any
explicitly labeled local WIP status. It does not authorize broader implementation or
claim physical-device or released-build validation.

## 1. Dashboard summary feels confusing and too passive

Reported wording, approximately: “0 active, system optimized. Only 13 flow apps are
recommended.” The meaning of “flow apps” in the reported wording needs checking
against the actual screen. The overall message feels odd and does not guide the user
through useful next steps.

- [ ] Check the exact displayed copy and what each count represents.
- [ ] Make the distinction between known recommended bloat and other optional apps
  clear; zero recommended apps active should not imply there is nothing left to review.
- [ ] Provide a more hands-on next step into a guided review of installed apps.

## 2. Optimize needs a guided optional-app review

Optimize does not ask what the user uses or suggest optional apps they may not need.
Amazon Prime Video appeared to be running but was not suggested for removal. Bryan
wants a review of known “semi-bloated” apps, not only the existing recommended list.

- [ ] Investigate why Prime Video was absent from recommendations on the selected TV;
  distinguish running, installed, enabled, and recommended states.
- [ ] Design a guided review that asks about usage/preferences and offers optional
  apps or app categories to keep or disable.
- [ ] Explain why each candidate is offered and distinguish disable from uninstall.
- [ ] Keep optional choices explicit: being installed or running alone does not make
  an app unwanted. Preserve core safety gates and JSON-backed package definitions.

## 3. Apps needs a system-app visibility filter

- [ ] Add a clear show/hide system apps control to the Apps list.
- [ ] Make the active filter and its effect on search/results understandable.

## 4. Match content frame rate has an odd “30fps” indicator

- [ ] Reproduce and inspect the “30fps” text beside the tweak.
- [ ] Clarify whether it represents an actual reading, an option, or decorative copy;
  correct misleading text and explain the setting's behavior.

## 5. Previously connected TVs should open Dashboard directly

Selecting a TV that has connected before should not require an extra “Open
dashboard” button after a successful connection.

Local implementation note (unpublished WIP `f989bdc`; test and documentation
refinements are uncommitted): the saved-TV path now opens Dashboard only after the
current connection attempt succeeds and its device profile is confirmed. Browser-only
regression coverage in the worktree exercises success, failure, cancellation, stale
responses, and pending-versus-completed teardown. This has not been validated on a
physical TV or included in a released build, so the acceptance items remain open.

- [ ] After a confirmed successful connection to a previously connected TV, navigate
  directly to Dashboard.
- [ ] Preserve connecting, cancellation, authorization, and error handling; do not
  navigate on a failed connection or a stale response from another attempt.

## 6. Backup scope and app selection are confusing

The wording sounds like it will back up all apps, but the action appears to update
only one. The list also includes many system-type apps the user would probably
never restore. Neither the actual action scope nor a backend defect is confirmed.

- [ ] Reproduce the flow and align the description, button labels, selection, and
  completion feedback with the actual single-app or multi-app scope.
- [ ] Clearly distinguish an APK backup from a backup of app data/settings or the
  entire device; describe restore limitations.
- [ ] Make useful restore candidates easier to find and provide a clear way to hide
  or reveal system apps, with appropriate restore warnings.

## 7. Briefly leaving and reopening the app restarts the experience

Bryan reports that briefly closing the app and reopening it restarts the whole
thing. It is not yet known whether this means background/resume, an activity or
WebView recreation, process death, or explicit dismissal from Recents.

- [ ] Capture exact reproduction steps and lifecycle/log evidence on the Pixel.
- [ ] Investigate loss of screen/session state and unnecessary onboarding or reconnect
  after a brief interruption.
- [ ] Resume the prior screen/session when valid, and recover gracefully when Android
  actually killed the process or the TV connection was lost; never show a false
  connected state or automatically replay mutations.
