# Legacy v1 TV Recommendations default correction

The active `Get-AppListForDevice` → `Run-Task` path reads the SHIELD catalog's
DefaultOptimize value. Both interactive default selection and Apply all defaults
previously chose DISABLE for `com.google.android.tvrecommendations`. This entry now
uses DefaultOptimize=N, High Risk and original consequence wording about reported
SHIELD TV phone-app key input loss after reboot. Explicit choices, method DISABLE,
DefaultRestore=Y, settings, connection and release behavior remain unchanged.

The [external source report](https://github.com/Gathin23/cleanyourtv/blob/30626a8e4714f86bf837384b109b5c73ee736917/docs/TROUBLESHOOTING.md#shield-tv-phone-app-connects-but-no-key-does-anything)
concerns darcy/foster_e Android11 devices. Hardware behavior and recovery were not
reproduced here; other models and firmware remain unverified. Upstream code/prose
was not copied, and no strict clean-room certification is asserted. The inspected
upstream license is MIT. This PR is separate from the v2 correction in PR215.

`tests/TVRecommendations.Tests.ps1` parses the real shipped data assignment,
rejects executable commands in that fixture, and imports only the relevant local
functions. It verifies the warning/defaults and exercises Run-Task with a fake
package-list function and mocked mutation boundary for review-each and apply-all
modes. A sentinel stops execution at the performance-settings header after the
package loop. Both paths must skip the package, display the consequence and issue
zero mutation commands. No ADB binary, device or production top-level script is run.

PowerShell is unavailable on the local Mac; no installation was performed. The
existing four-job v1 CI (three Pester platforms and syntax check) is the validation
gate. Screenshots/video are inapplicable to this console data correction; the
regression asserts the displayed consequence at Write-Dim. No desktop/mobile UI or
gallery is changed, and no release/tag/deployment is performed.
