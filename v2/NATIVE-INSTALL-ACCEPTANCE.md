# Native APK installation slice

This local stack starts at reviewed native parity head `18f69b7`. It targets the
existing desktop single-APK sideload action before discovery: the vendored direct
transport already implements streamed package-manager installation, while desktop
currently refuses `install`. Reusing that protocol requires no discovery library,
subprocess, LAN scan or persistent identity onboarding.

The first slice supports one local APK and the existing replacement choice (`-r`).
It does not claim split-APK, restore/clone, Android-version or hardware parity.
Unsupported options must fail before opening an install service. No command is
replayed with platform-tools or retried automatically after a transport failure.

## Required evidence

- Open and validate the local input before device installation; stream bounded
  chunks from the held file, not an entire APK buffered in memory. Preserve exact
  declared length and replacement semantics without interpolating local paths into
  a remote shell expression.
- Use one dedicated required-existing-identity connection, with a finite total
  network deadline and cancellation scoped to the registered socket. Hold the operation lease
  until worker cleanup finishes, including when the awaiting task is canceled.
- Read a verified hardware ID from the currently owned selected control session,
  then require that exact ID on the new connection before installation starts.
  Both identity queries require bounded, successful shell-v2 results. Unknown,
  unreadable or mismatched IDs refuse installation; a shared key or matching IP
  address is not identity evidence. Normal probe completion preserves the existing
  control socket; cancellation before a borrowed probe starts must not close it.
  An active borrowed probe may close its selected control socket on timeout or
  cancellation, without closing another TV's session or a shared server.
- Bound response buffering during transfer acknowledgements as well as final
  result reading. Continuous early output cannot bypass the cap by withholding an
  acknowledgement.
- Distinguish explicit installation success, explicit package-manager rejection
  and a lost/ambiguous result. Interruption after installation begins must not be
  described as proof that nothing installed. Preserve actionable rejection codes.
- Test transfer interruption, completion/commit response loss, cancellation,
  malformed or excessive replies, exact bytes/flags and wrong-target isolation.
  Use deterministic fake peers and the existing in-memory public test fixture;
  never copy its PEM into another file or generate an identity.
- Verify command callers cannot turn indeterminate outcomes or text merely
  containing the word “Success” into a successful installation.

No APK is downloaded or executed on the host, and no test installs an APK on a
real device. Byte fixtures exercise transport framing only. Passing them does not
establish package-manager compatibility or a successful TV installation.

The watchdog interrupts registered network sockets, not a synchronous local file
read blocked inside the operating system. The worker remains tracked until that
read and cleanup return; passing network deadline tests does not establish a
wall-clock bound for pathological local filesystem failures.

The shipped backend remains platform-tools. Rust-native is still explicitly
selected and feature-gated; platform-tools is an explicit backup selection, never
an automatic second attempt. Normal identity setup, discovery, split installs,
fast Remote and packaged-app/TV acceptance remain separate gates.
