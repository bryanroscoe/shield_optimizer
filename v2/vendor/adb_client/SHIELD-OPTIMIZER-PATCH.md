# Shield Optimizer patch

This directory starts from the exact `adb_client` 3.2.2 crates.io source, whose upstream commit is
recorded in `.cargo_vcs_info.json`. It remains MIT-licensed; `LICENSE` is copied from the upstream
repository because crates.io did not include it in the published archive.

Shield Optimizer adds one generic API: `adb_client::tcp::ADBTcpService`. It opens an arbitrary
device service over a dedicated authenticated connection and implements `std::io::Read` and
`std::io::Write` with correct ADB `OPEN`, `WRTE`, `OKAY`, and `CLSE` framing. The implementation is
generic and contains no scrcpy- or product-specific behavior.

Changed source files:

- `src/message_devices/adb_service.rs` — logical-stream framing and protocol tests
- `src/message_devices/adb_message_device.rs` — internal raw-service opener
- `src/message_devices/mod.rs` — internal module registration
- `src/message_devices/tcp/adb_tcp_service.rs` — public dedicated TCP service stream
- `src/message_devices/tcp/tcp_transport.rs` — physical-EOF handling for bounded stream failure
- `src/message_devices/tcp/mod.rs` — public export

The workspace pins this directory through `[patch.crates-io]` in `v2/Cargo.toml`. Before replacing
it with a newer upstream release, verify that the release exposes an equivalent owned raw-service
stream, then rerun the tests and Android cross-compile documented in `mobile/FAST-REMOTE-PLAN.md`.
