//! Extra connections to the connected TV, so one slow command (`dumpsys
//! meminfo` takes about three seconds on a Shield) does not hold up the rest.
//! adb's server runs commands side by side over its one connection; the
//! vendored client reads one stream at a time, so commands share the main
//! connection and a few extra ones: each takes whichever is free first.
//!
//! Every extra connection authenticates with signatures only (it never asks
//! the TV for approval), must report the hardware id the main connection
//! read at connect before it runs anything, and belongs to that main
//! connection: once it is replaced or dropped, they are closed too. When the
//! TV refuses one, commands wait for the main connection as before.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use adb_client::tcp::{ADBTcpDevice, BoundedShellControl, PreferredKeys};
use adb_client::RustADBError;
use atv_optimizer_core::adb::{AdbError, AdbResult};

use super::{
    is_fatal_transport_error, lock_recovering, probe_alive, query_hardware_id,
    request_error_message, Identity,
};

/// The longest a command spends opening an extra connection before it
/// waits for the main one instead. A TV on the same network answers in a
/// fraction of this.
const OPEN_LIMIT: Duration = Duration::from_secs(2);
/// An extra connection idle for longer is closed instead of reused.
const IDLE_LIMIT: Duration = Duration::from_secs(120);
/// After the TV refuses or fails to open an extra connection, commands use
/// the main connection alone for this long.
const RETRY_AFTER: Duration = Duration::from_secs(60);
/// An extra connection idle for longer is checked before it is reused, so
/// one the TV closed meanwhile never fails a command that would have run.
const CHECK_AFTER: Duration = Duration::from_secs(10);
/// The longest a waiting command sleeps before looking again. A connection
/// handed back wakes it at once; the main connection, which other code
/// also holds, is noticed within this.
const RECHECK: Duration = Duration::from_millis(10);

/// The main connection the extra ones belong to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Owner {
    pub(crate) request_id: u64,
    pub(crate) address: SocketAddr,
    pub(crate) hardware_id: String,
}

/// What a command finding the main connection busy may do.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Take<D> {
    /// Run on this free extra connection, idle for the given time.
    Idle(D, Duration),
    /// Open one more for `Owner`, then hand it back with
    /// [`Lanes::put_back`] or give the room up with [`Lanes::discard`] or
    /// [`Lanes::refused`].
    Open(Owner),
    /// Every extra connection is in use: wait for any connection.
    Busy,
    /// There are none to use (none allowed, another or no main connection,
    /// or a recent refusal): wait for the main connection.
    Unavailable,
}

/// [`Lanes::acquire`]'s answer.
pub(crate) enum Acquired {
    Lane(u64, Box<ADBTcpDevice>),
    Busy,
    Unavailable,
}

struct State<D> {
    owner: Option<Owner>,
    idle: Vec<(D, Instant)>,
    /// Idle, in use and being opened.
    open: usize,
    retry_at: Option<Instant>,
}

pub(crate) struct Lanes<D = ADBTcpDevice> {
    limit: usize,
    state: Mutex<State<D>>,
    /// Signalled whenever a connection is handed back or given up.
    freed: Condvar,
}

impl<D> Lanes<D> {
    pub(crate) fn new(limit: usize) -> Self {
        Self {
            limit,
            state: Mutex::new(State {
                owner: None,
                idle: Vec::new(),
                open: 0,
                retry_at: None,
            }),
            freed: Condvar::new(),
        }
    }

    pub(crate) const fn enabled(&self) -> bool {
        self.limit > 0
    }

    /// Belong to `owner` from now on, closing every connection that
    /// belonged to the one before. `None` closes them all.
    pub(crate) fn bind(&self, owner: Option<Owner>) {
        let closed = {
            let mut state = lock_recovering(&self.state);
            state.owner = owner;
            state.open = 0;
            state.retry_at = None;
            std::mem::take(&mut state.idle)
        };
        self.freed.notify_all();
        if !closed.is_empty() {
            tracing::info!(closed = closed.len(), "closed extra connections to the TV");
        }
        drop(closed);
    }

    /// Whether connections taken for `request_id` still belong to the
    /// current main connection.
    pub(crate) fn owns(&self, request_id: u64) -> bool {
        lock_recovering(&self.state)
            .owner
            .as_ref()
            .is_some_and(|owner| owner.request_id == request_id)
    }

    pub(crate) fn clear(&self) {
        self.bind(None);
    }

    /// What a command for the main connection `request_id` may use while
    /// that connection is busy.
    pub(crate) fn take(&self, request_id: u64, now: Instant) -> Take<D> {
        let mut expired = Vec::new();
        let take = {
            let mut state = lock_recovering(&self.state);
            match state.owner.clone() {
                Some(owner) if owner.request_id == request_id && self.enabled() => {
                    let mut found = None;
                    while let Some((device, since)) = state.idle.pop() {
                        let idle = now.saturating_duration_since(since);
                        if idle < IDLE_LIMIT {
                            found = Some((device, idle));
                            break;
                        }
                        state.open = state.open.saturating_sub(1);
                        expired.push(device);
                    }
                    match found {
                        Some((device, idle)) => Take::Idle(device, idle),
                        // A recent refusal stops new ones, not the open ones.
                        None if state.retry_at.is_some_and(|at| now < at) => Take::Unavailable,
                        None if state.open < self.limit => {
                            state.open += 1;
                            Take::Open(owner)
                        }
                        None => Take::Busy,
                    }
                }
                _ => Take::Unavailable,
            }
        };
        drop(expired);
        take
    }

    /// Return a connection that is still in step with the TV.
    pub(crate) fn put_back(&self, request_id: u64, device: D, now: Instant) {
        let mut state = lock_recovering(&self.state);
        if state
            .owner
            .as_ref()
            .is_some_and(|owner| owner.request_id == request_id)
        {
            state.idle.push((device, now));
            drop(state);
        } else {
            drop(state);
            drop(device);
        }
        self.freed.notify_one();
    }

    /// Give up a connection that was closed or never opened.
    pub(crate) fn discard(&self, request_id: u64) {
        let mut state = lock_recovering(&self.state);
        if state
            .owner
            .as_ref()
            .is_some_and(|owner| owner.request_id == request_id)
        {
            state.open = state.open.saturating_sub(1);
        }
        drop(state);
        self.freed.notify_one();
    }

    /// The TV refused a new connection or it did not open in time: give up
    /// its room and open none for a while.
    pub(crate) fn refused(&self, request_id: u64, now: Instant) {
        let mut state = lock_recovering(&self.state);
        if state
            .owner
            .as_ref()
            .is_some_and(|owner| owner.request_id == request_id)
        {
            state.open = state.open.saturating_sub(1);
            state.retry_at = Some(now + RETRY_AFTER);
        }
        drop(state);
        self.freed.notify_all();
    }

    /// Wake one command waiting for a connection: the main one was released.
    pub(crate) fn main_released(&self) {
        self.freed.notify_one();
    }

    /// Sleep until a connection is handed back, for at most `wait`.
    pub(crate) fn wait_for_free(&self, wait: Duration) {
        let state = lock_recovering(&self.state);
        let _ = self
            .freed
            .wait_timeout(state, wait.min(RECHECK))
            .map_err(|poisoned| poisoned.into_inner());
    }
}

impl Lanes<ADBTcpDevice> {
    /// A connection to run on while the main one is busy: a free one, or a
    /// new one opened within [`OPEN_LIMIT`], for a command with `budget` left.
    pub(crate) fn acquire(
        &self,
        identity: &Mutex<Option<Identity>>,
        serial: &str,
        key: &Path,
        preferred: &PreferredKeys,
        budget: Duration,
    ) -> Acquired {
        if !self.enabled() {
            return Acquired::Unavailable;
        }
        let Some(request_id) = lock_recovering(identity)
            .as_ref()
            .filter(|current| current.serial == serial)
            .map(|current| current.request_id)
        else {
            return Acquired::Unavailable;
        };
        loop {
            match self.take(request_id, Instant::now()) {
                Take::Idle(mut device, idle) => {
                    if idle < CHECK_AFTER || probe_alive(&mut device) {
                        return Acquired::Lane(request_id, Box::new(device));
                    }
                    drop(device);
                    self.discard(request_id);
                    tracing::info!(serial = %serial, "an idle extra connection to the TV had closed");
                }
                Take::Busy => return Acquired::Busy,
                Take::Unavailable => return Acquired::Unavailable,
                // A command too close to its deadline to open one waits
                // instead, so a slow open is never mistaken for a refusal.
                Take::Open(_) if budget < OPEN_LIMIT => {
                    self.discard(request_id);
                    return Acquired::Busy;
                }
                Take::Open(owner) => {
                    return match open(&owner, key, preferred, OPEN_LIMIT) {
                        Ok(device) => {
                            tracing::info!(serial = %serial, "opened an extra connection to the TV");
                            Acquired::Lane(request_id, Box::new(device))
                        }
                        Err(reason) => {
                            self.refused(request_id, Instant::now());
                            tracing::info!(
                                serial = %serial,
                                reason = %reason,
                                "extra connection to the TV refused; no new one for a minute"
                            );
                            Acquired::Unavailable
                        }
                    };
                }
            }
        }
    }

    /// Run `run` on an extra connection, keeping it for the next command
    /// unless it fell out of step with the TV. `on_lane` tells the caller's
    /// deadline handling that the main connection was not involved.
    pub(crate) fn run<T, F>(
        &self,
        request_id: u64,
        mut device: ADBTcpDevice,
        control: &BoundedShellControl,
        on_lane: &Arc<AtomicBool>,
        run: F,
    ) -> AdbResult<T>
    where
        F: FnOnce(&mut ADBTcpDevice, BoundedShellControl) -> Result<T, RustADBError>,
    {
        if control.remaining().is_err() {
            self.put_back(request_id, device, Instant::now());
            return Err(AdbError::Transport(super::QUEUED_DEADLINE.into()));
        }
        // Disconnected or replaced while this one was taken or opened: run
        // nothing on a connection the TV's current session does not own.
        if !self.owns(request_id) {
            drop(device);
            return Err(AdbError::Transport("Not connected to a device.".into()));
        }
        on_lane.store(true, Ordering::Release);
        let error = match run(&mut device, control.clone()) {
            Ok(value) => {
                self.put_back(request_id, device, Instant::now());
                return Ok(value);
            }
            Err(error) => error,
        };
        if !control.was_registered() {
            self.put_back(request_id, device, Instant::now());
            return Err(AdbError::Transport(request_error_message(&error)));
        }
        if is_fatal_transport_error(&error) || !probe_alive(&mut device) {
            drop(device);
            self.discard(request_id);
            tracing::warn!(error = %error, "extra connection to the TV failed — closed it");
            Err(AdbError::Transport(format!(
                "The TV stopped answering: {error}. The command's outcome is unknown; check the TV before retrying."
            )))
        } else {
            self.put_back(request_id, device, Instant::now());
            Err(AdbError::Transport(request_error_message(&error)))
        }
    }
}

/// Connect to `owner`'s address with signatures only and check that the TV
/// there reports `owner`'s hardware id before anything runs on it.
fn open(
    owner: &Owner,
    key: &Path,
    preferred: &PreferredKeys,
    budget: Duration,
) -> Result<ADBTcpDevice, String> {
    let control = BoundedShellControl::new(budget).map_err(|error| error.to_string())?;
    let _cancel = crate::bounded_shell::CancelOnDrop(control.clone());
    let mut device = ADBTcpDevice::new_with_keys_and_control_signing_only(
        owner.address,
        key,
        &preferred.clone().signing_only(),
        true,
        control.clone(),
    )
    .map_err(|error| error.to_string())?;
    let remaining = control.remaining().map_err(|error| error.to_string())?;
    let reported = query_hardware_id(&mut device, remaining).map_err(|error| error.to_string())?;
    if reported.as_deref() != Some(owner.hardware_id.as_str()) {
        return Err("the TV at this address did not report the connected TV's hardware id".into());
    }
    device
        .finish_connection()
        .map_err(|error| error.to_string())?;
    Ok(device)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(request_id: u64) -> Owner {
        Owner {
            request_id,
            address: "192.0.2.1:5555".parse().unwrap(),
            hardware_id: "1324619053514".into(),
        }
    }

    #[test]
    fn nothing_is_offered_before_a_main_connection_owns_the_lanes() {
        let lanes = Lanes::<u32>::new(2);
        assert_eq!(lanes.take(1, Instant::now()), Take::Unavailable);
        lanes.bind(Some(owner(1)));
        assert_eq!(
            lanes.take(2, Instant::now()),
            Take::Unavailable,
            "another generation"
        );
        assert_eq!(lanes.take(1, Instant::now()), Take::Open(owner(1)));
    }

    #[test]
    fn at_most_the_limit_are_open_and_a_returned_one_is_reused() {
        let lanes = Lanes::<u32>::new(2);
        lanes.bind(Some(owner(1)));
        let now = Instant::now();
        assert_eq!(lanes.take(1, now), Take::Open(owner(1)));
        assert_eq!(lanes.take(1, now), Take::Open(owner(1)));
        assert_eq!(lanes.take(1, now), Take::Busy, "both are in use");
        lanes.put_back(1, 7, now);
        assert_eq!(lanes.take(1, now), Take::Idle(7, Duration::ZERO));
        assert_eq!(lanes.take(1, now), Take::Busy);
        lanes.discard(1);
        assert_eq!(lanes.take(1, now), Take::Open(owner(1)));
    }

    #[test]
    fn a_new_main_connection_closes_the_old_ones_and_ignores_their_return() {
        let lanes = Lanes::<u32>::new(1);
        lanes.bind(Some(owner(1)));
        let now = Instant::now();
        assert_eq!(lanes.take(1, now), Take::Open(owner(1)));
        lanes.bind(Some(owner(2)));
        lanes.put_back(1, 7, now);
        assert_eq!(lanes.take(2, now), Take::Open(owner(2)));
        assert_eq!(
            lanes.take(2, now),
            Take::Busy,
            "the old one never came back"
        );
        lanes.clear();
        assert_eq!(lanes.take(2, now), Take::Unavailable);
    }

    #[test]
    fn a_refusal_pauses_new_connections_for_a_while() {
        let lanes = Lanes::<u32>::new(2);
        lanes.bind(Some(owner(1)));
        let now = Instant::now();
        assert_eq!(lanes.take(1, now), Take::Open(owner(1)));
        lanes.refused(1, now);
        assert_eq!(lanes.take(1, now + RETRY_AFTER / 2), Take::Unavailable);
        assert_eq!(lanes.take(1, now + RETRY_AFTER), Take::Open(owner(1)));
    }

    #[test]
    fn a_refusal_leaves_the_open_ones_in_use() {
        let lanes = Lanes::<u32>::new(2);
        lanes.bind(Some(owner(1)));
        let now = Instant::now();
        assert_eq!(lanes.take(1, now), Take::Open(owner(1)));
        lanes.put_back(1, 7, now);
        assert_eq!(lanes.take(1, now), Take::Idle(7, Duration::ZERO));
        assert_eq!(lanes.take(1, now), Take::Open(owner(1)));
        lanes.refused(1, now);
        lanes.put_back(1, 7, now);
        assert_eq!(lanes.take(1, now), Take::Idle(7, Duration::ZERO));
        assert_eq!(
            lanes.take(1, now),
            Take::Unavailable,
            "no new one is opened"
        );
    }

    #[test]
    fn a_connection_reports_how_long_it_was_idle_and_its_owner() {
        let lanes = Lanes::<u32>::new(1);
        lanes.bind(Some(owner(1)));
        let now = Instant::now();
        assert_eq!(lanes.take(1, now), Take::Open(owner(1)));
        lanes.put_back(1, 7, now);
        assert_eq!(lanes.take(1, now + CHECK_AFTER), Take::Idle(7, CHECK_AFTER));
        assert!(lanes.owns(1));
        lanes.bind(Some(owner(2)));
        assert!(!lanes.owns(1));
    }

    #[test]
    fn an_idle_connection_past_the_limit_is_closed_and_replaced() {
        let lanes = Lanes::<u32>::new(1);
        lanes.bind(Some(owner(1)));
        let now = Instant::now();
        assert_eq!(lanes.take(1, now), Take::Open(owner(1)));
        lanes.put_back(1, 7, now);
        assert_eq!(
            lanes.take(1, now + IDLE_LIMIT),
            Take::Open(owner(1)),
            "the stale one was closed, which freed its room"
        );
    }

    #[test]
    fn disabled_lanes_offer_nothing() {
        let lanes = Lanes::<u32>::new(0);
        assert!(!lanes.enabled());
        lanes.bind(Some(owner(1)));
        assert_eq!(lanes.take(1, Instant::now()), Take::Unavailable);
    }

    #[test]
    fn a_handed_back_connection_wakes_a_waiting_command() {
        let lanes = Arc::new(Lanes::<u32>::new(1));
        lanes.bind(Some(owner(1)));
        let now = Instant::now();
        assert_eq!(lanes.take(1, now), Take::Open(owner(1)));
        let waiter = {
            let lanes = lanes.clone();
            std::thread::spawn(move || {
                let started = Instant::now();
                loop {
                    if let Take::Idle(device, _) = lanes.take(1, Instant::now()) {
                        return (device, started.elapsed());
                    }
                    lanes.wait_for_free(Duration::from_secs(5));
                }
            })
        };
        std::thread::sleep(Duration::from_millis(30));
        lanes.put_back(1, 7, Instant::now());
        let (device, waited) = waiter.join().unwrap();
        assert_eq!(device, 7);
        assert!(waited < Duration::from_secs(1), "{waited:?}");
    }
}
