# Desktop device logs

The Logs tab addresses #162 with finite, read-only logcat snapshots. It is not a lossless
stream or a persistent recording. The most recent snapshot replaces the preceding one;
entries can repeat between reads and high-volume traffic can roll out of the selected window.

- Reads start only on an explicit Read logs or Auto-refresh click.
- Severity, one exact tag and an optional Android package constrain the device-side read.
  Package filtering resolves the running main PID for each snapshot. Missing, ambiguous or
  unsupported PID resolution fails without requesting unfiltered logs. Helper processes are
  excluded; Android PID reuse means historical entries can belong to an earlier process.
- Every shell invocation uses the existing bounded driver: 30 seconds and 256 KiB per stream.
  App filtering adds a separate bounded PID lookup. Only one read is pending per view.
- Auto-refresh waits three seconds after a completed read. It stops on error, tab departure,
  device-page reset, document hiding or explicit Stop. A native read already in flight may
  finish within its bounds, but its late result is discarded. Returning does not resume it.
- A tag cannot start with a hyphen or contain shell/filter syntax. The typed command exposes
  no log-clearing, buffer resizing, system-setting or arbitrary-shell option.
- Raw text is escaped on screen. Find filters only the displayed snapshot. This feature does
  not save, upload or append logs to bug reports; the shared bounded driver redacts payloads
  from its own debug/session records. Review sensitive text before manually sharing it.
- Timeout/output-cap results are labeled partial. Android versions may reject logcat flags;
  device errors remain visible rather than falling back to broader capture.

Validation covers pure filter/command construction, mocked driver failures and PID scope,
polling lifecycle, and browser interaction. Those checks do not substitute for a physical-TV
compatibility pass. The screenshot gallery also needs regeneration before release because
the device navigation includes a new tab. Mobile has not been wired to this command.
