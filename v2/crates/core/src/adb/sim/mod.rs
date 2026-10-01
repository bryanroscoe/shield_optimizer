//! `SimulatedAdb` — a stateful, test-only model of the adb server and the
//! devices behind it, implementing [`AdbDriver`].
//!
//! It is the driver the E2E harness (`src-tauri/src/bin/e2e_server.rs`) runs
//! the real command layer against. Unlike the substring `MockAdb` in
//! `commands::test_support`, it keeps state: disabling a package removes it
//! from `pm list packages -e`, a `set-home-activity` moves Home, a disconnect
//! drops the transport and (if the device is paired and advertising) adb
//! brings it back. Shell strings are parsed and executed command by command,
//! so batched reads and their exit status behave as on a device.
//!
//! Commands it has no model for answer with an error naming the command and
//! are recorded in [`World::gaps`], so a coverage hole is visible instead of
//! quietly succeeding.
//!
//! Compiled only for tests and the `test-support` feature; never shipped.

mod device;
pub mod faults;
pub mod profile;
pub mod replay;
mod shell;
mod world;

use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;

pub use device::{Device, HomeComponent, HomePolicy, HomeState, Network, Package, Wireless};
pub use faults::{FaultEffect, FaultRule, FaultScope};
pub use profile::{captured_transports, fixtures_dir, list_profiles, load_profile, synthesize};
pub use world::{Invocation, Transport, TransportState, World};

use super::driver::{process_output, BoundedShellOutput, ShellTermination};
use super::{AdbDriver, AdbError, AdbOutput, AdbResult};
use world::Reply;

/// Seconds the simulator reports for a timed-out call, matching the desktop
/// driver's command timeout.
const TIMEOUT_SECONDS: u64 = 30;

#[derive(Clone)]
pub struct SimulatedAdb {
    world: Arc<Mutex<World>>,
}

impl SimulatedAdb {
    pub fn new(world: World) -> Self {
        Self {
            world: Arc::new(Mutex::new(world)),
        }
    }

    /// An empty world that knows the catalog's stock launchers.
    pub fn empty() -> Self {
        Self::new(World::new(stock_launchers()))
    }

    /// Lock the world for inspection or setup.
    pub fn world(&self) -> MutexGuard<'_, World> {
        self.world.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn handle(&self, args: &[&str]) -> Reply {
        self.world().handle(args)
    }
}

pub fn stock_launchers() -> Vec<String> {
    crate::commands::loader::launchers()
        .stock
        .iter()
        .map(|e| e.package.clone())
        .collect()
}

async fn settle(reply: Reply) -> Result<world::ReplyOut, AdbError> {
    match reply {
        Reply::Out(o) => Ok(world::ReplyOut::Text(o)),
        Reply::Delayed(o, ms) => {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            Ok(world::ReplyOut::Text(o))
        }
        Reply::Bytes(b) => Ok(world::ReplyOut::Bytes(b)),
        Reply::Timeout { delay_ms } => {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            Err(AdbError::Timeout {
                seconds: TIMEOUT_SECONDS,
            })
        }
    }
}

#[async_trait]
impl AdbDriver for SimulatedAdb {
    async fn raw(&self, args: &[&str]) -> AdbResult<AdbOutput> {
        match settle(self.handle(args)).await? {
            world::ReplyOut::Text(o) => process_output(o.stdout, o.stderr, Some(o.code)),
            world::ReplyOut::Bytes(b) => process_output(
                String::from_utf8_lossy(&b).into_owned(),
                String::new(),
                Some(0),
            ),
        }
    }

    async fn shell(&self, serial: &str, command: &str) -> AdbResult<AdbOutput> {
        self.raw(&["-s", serial, "shell", command]).await
    }

    async fn shell_bounded(&self, serial: &str, command: &str) -> AdbResult<BoundedShellOutput> {
        match self.handle(&["-s", serial, "shell", command]) {
            Reply::Timeout { .. } => Ok(BoundedShellOutput {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: None,
                termination: ShellTermination::Timeout,
            }),
            other => match settle(other).await? {
                world::ReplyOut::Text(o) => Ok(BoundedShellOutput {
                    stdout: o.stdout,
                    stderr: o.stderr,
                    exit_code: Some(o.code),
                    termination: ShellTermination::Completed,
                }),
                world::ReplyOut::Bytes(_) => Err(AdbError::Unsupported {
                    operation: "shell_bounded bytes",
                }),
            },
        }
    }

    async fn raw_bytes(&self, args: &[&str]) -> AdbResult<Vec<u8>> {
        match settle(self.handle(args)).await? {
            world::ReplyOut::Bytes(b) => Ok(b),
            world::ReplyOut::Text(o) => {
                process_output(o.stdout, o.stderr, Some(o.code)).map(|o| o.stdout.into_bytes())
            }
        }
    }
}

#[cfg(test)]
mod tests;
