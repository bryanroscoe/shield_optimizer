use super::{validate_requested_serial, CountedStream, WirelessAdb};
use adb_client::tcp::{open_bound_service, BoundServiceError, BoundedShellControl};
use shield_optimizer_core::adb::{
    AdbDeviceBinding, AdbError, AdbOwnedService, AdbResult, AdbServiceCancellation,
};
use std::{sync::Arc, time::Duration};

const MAX_LIFETIME: Duration = Duration::from_secs(15 * 60);
struct Cancellation(BoundedShellControl);
impl AdbServiceCancellation for Cancellation {
    fn cancel(&self) {
        self.0.cancel();
    }
}
struct Opening {
    control: BoundedShellControl,
    complete: bool,
}
impl Drop for Opening {
    fn drop(&mut self) {
        if !self.complete {
            self.control.cancel();
        }
    }
}

impl WirelessAdb {
    pub(crate) async fn device_binding_impl(&self, serial: &str) -> AdbResult<AdbDeviceBinding> {
        let hardware_id = self
            .verified_owned_hardware_id(serial, Duration::from_secs(5))
            .await?;
        Ok(AdbDeviceBinding {
            serial: serial.into(),
            hardware_id,
        })
    }
    pub(crate) async fn open_bound_device_service_impl(
        &self,
        binding: &AdbDeviceBinding,
        service: &str,
        lifetime: Duration,
    ) -> AdbResult<AdbOwnedService> {
        if lifetime.is_zero() || lifetime > MAX_LIFETIME {
            return Err(AdbError::Transport(
                "Bound service lifetime must be greater than zero and at most fifteen minutes."
                    .into(),
            ));
        }
        if service.is_empty() || service.len() > 4096 || service.as_bytes().contains(&0) {
            return Err(AdbError::Transport(
                "Invalid bounded device service.".into(),
            ));
        }
        let expected =
            shield_optimizer_core::commands::devices::verified_hardware_id(&binding.hardware_id)
                .ok_or_else(|| {
                    AdbError::Transport("A verified hardware binding is required.".into())
                })?
                .to_owned();
        let Some((active, host, port)) = self.info() else {
            return Err(AdbError::Transport("Not connected to a device.".into()));
        };
        validate_requested_serial(&active, &binding.serial).map_err(AdbError::Transport)?;
        let deadline = tokio::time::Instant::now() + lifetime;
        let actual = self
            .verified_owned_hardware_id(&binding.serial, lifetime.min(Duration::from_secs(5)))
            .await?;
        if actual != expected {
            return Err(AdbError::Transport(
                "Selected TV hardware identity changed; no service was opened.".into(),
            ));
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(AdbError::Transport(
                "Bound service setup deadline expired.".into(),
            ));
        }
        let address = format!("{host}:{port}")
            .parse()
            .map_err(|e| AdbError::Transport(format!("invalid device address: {e}")))?;
        let operation = self.work().map_err(AdbError::Transport)?;
        let control = BoundedShellControl::new(remaining)?;
        let key = self.key_path.clone();
        let service = service.to_owned();
        open_worker(operation, control, move |control| {
            open_bound_service(address, &key, &expected, &service, control)
                .map(|stream| {
                    Box::new(stream) as Box<dyn shield_optimizer_core::adb::AdbByteStream>
                })
                .map_err(|error| match error {
                    BoundServiceError::BeforeOpen(error) => AdbError::Transport(error.to_string()),
                    BoundServiceError::OpenUncertain(error) => AdbError::Uncertain {
                        operation: "device service OPEN",
                        detail: error.to_string(),
                    },
                })
        })
        .await
    }
}

async fn open_worker<F>(
    operation: Arc<shield_optimizer_core::adb::Lease>,
    control: BoundedShellControl,
    open: F,
) -> AdbResult<AdbOwnedService>
where
    F: FnOnce(BoundedShellControl) -> AdbResult<Box<dyn shield_optimizer_core::adb::AdbByteStream>>
        + Send
        + 'static,
{
    let mut opening = Opening {
        control: control.clone(),
        complete: false,
    };
    let result = tokio::task::spawn_blocking(move || {
        let cancellation = Arc::new(Cancellation(control.clone()));
        let stream = open(control)?;
        Ok::<_, AdbError>(AdbOwnedService {
            stream: Box::new(CountedStream {
                stream,
                _operation: operation,
            }),
            cancellation,
        })
    })
    .await
    .map_err(|e| AdbError::Transport(format!("Bound service worker failed: {e}")))?;
    if result.is_ok() {
        opening.complete = true;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use shield_optimizer_core::adb::{AdbDriver, Leases};
    #[tokio::test]
    async fn invalid_bound_service_requests_fail_before_any_identity_or_network_work() {
        let operations = Leases::new();
        let driver = WirelessAdb::with_existing_key("not-created".into(), operations.clone());
        let unknown = AdbDeviceBinding {
            serial: "fixture".into(),
            hardware_id: "unknown".into(),
        };
        assert!(driver
            .open_bound_device_service(&unknown, "localabstract:fixture", Duration::from_secs(1))
            .await
            .is_err());
        let valid = AdbDeviceBinding {
            serial: "fixture".into(),
            hardware_id: "known-fixture".into(),
        };
        for (service, lifetime) in [
            ("", Duration::from_secs(1)),
            ("bad\0service", Duration::from_secs(1)),
            ("localabstract:fixture", Duration::ZERO),
            (
                "localabstract:fixture",
                MAX_LIFETIME + Duration::from_secs(1),
            ),
        ] {
            assert!(driver
                .open_bound_device_service(&valid, service, lifetime)
                .await
                .is_err());
        }
        assert_eq!(operations.active(), 0);
    }
    struct FixtureStream {
        control: BoundedShellControl,
        dropped: Arc<std::sync::atomic::AtomicBool>,
    }
    impl std::io::Read for FixtureStream {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Ok(0)
        }
    }
    impl std::io::Write for FixtureStream {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Drop for FixtureStream {
        fn drop(&mut self) {
            self.control.cancel();
            self.dropped
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }
    #[tokio::test]
    async fn canceled_service_open_retains_worker_lease_and_drops_late_result() {
        let operations = Leases::new();
        let operation = Arc::new(operations.try_acquire().unwrap());
        let (started, started_rx) = tokio::sync::oneshot::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let captured = dropped.clone();
        let worker = tokio::spawn(open_worker(
            operation,
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            move |control| {
                started.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(Box::new(FixtureStream {
                    control,
                    dropped: captured,
                }))
            },
        ));
        started_rx.await.unwrap();
        worker.abort();
        assert!(matches!(worker.await, Err(error) if error.is_cancelled()));
        assert_eq!(operations.active(), 1);
        operations.close();
        assert!(operations.try_acquire().is_none());
        release.send(()).unwrap();
        assert!(
            operations
                .wait_idle(tokio::time::Instant::now() + Duration::from_secs(2))
                .await
        );
        assert!(dropped.load(std::sync::atomic::Ordering::Acquire));
    }
    #[tokio::test]
    async fn service_stream_owns_lease_but_a_retained_cancel_handle_does_not() {
        let operations = Leases::new();
        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let captured = dropped.clone();
        let service = open_worker(
            Arc::new(operations.try_acquire().unwrap()),
            BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
            move |control| {
                Ok(Box::new(FixtureStream {
                    control,
                    dropped: captured,
                }))
            },
        )
        .await
        .unwrap();
        let cancel = service.cancellation.clone();
        assert_eq!(operations.active(), 1);
        drop(service);
        assert!(dropped.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(operations.active(), 0);
        cancel.cancel();
    }
}
