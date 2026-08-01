#![cfg_attr(not(target_os = "android"), allow(dead_code))]

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

pub const REMOTE_BACKGROUND_GRACE: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct RemoteLifecycle {
    epoch: AtomicU64,
    backgrounded: AtomicBool,
}

impl RemoteLifecycle {
    pub fn suspend(&self) -> u64 {
        self.backgrounded.store(true, Ordering::Release);
        self.epoch.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn resume(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.backgrounded.store(false, Ordering::Release);
    }

    pub fn claim_cleanup(&self, epoch: u64) -> bool {
        if self.epoch.load(Ordering::Acquire) != epoch {
            return false;
        }
        self.backgrounded
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

#[cfg(target_os = "android")]
pub fn handle_window_event(app: &tauri::AppHandle, event: &tauri::WindowEvent) {
    use shield_optimizer_core::commands::AppState;
    use tauri::Manager;

    match event {
        tauri::WindowEvent::Suspended => {
            let epoch = app.state::<RemoteLifecycle>().suspend();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(REMOTE_BACKGROUND_GRACE).await;
                if app.state::<RemoteLifecycle>().claim_cleanup(epoch) {
                    tracing::info!("background grace expired; closing remote sessions");
                    app.state::<AppState>().drop_all_remote_sessions().await;
                }
            });
        }
        tauri::WindowEvent::Resumed => {
            app.state::<RemoteLifecycle>().resume();
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteLifecycle;

    #[test]
    fn current_background_epoch_can_claim_cleanup_once() {
        let lifecycle = RemoteLifecycle::default();
        let epoch = lifecycle.suspend();

        assert!(lifecycle.claim_cleanup(epoch));
        assert!(!lifecycle.claim_cleanup(epoch));
    }

    #[test]
    fn resume_invalidates_pending_cleanup() {
        let lifecycle = RemoteLifecycle::default();
        let epoch = lifecycle.suspend();
        lifecycle.resume();

        assert!(!lifecycle.claim_cleanup(epoch));
    }

    #[test]
    fn newer_suspend_invalidates_an_older_timer() {
        let lifecycle = RemoteLifecycle::default();
        let old_epoch = lifecycle.suspend();
        let current_epoch = lifecycle.suspend();

        assert!(!lifecycle.claim_cleanup(old_epoch));
        assert!(lifecycle.claim_cleanup(current_epoch));
    }
}
