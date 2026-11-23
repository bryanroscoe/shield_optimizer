//! Google Play Billing adapter for ATV Optimizer.
//!
//! On Android this ships a thin Kotlin plugin (see `android/`) around the Play
//! Billing Library: product lookup, the purchase sheet, owned-purchase queries,
//! acknowledgment and consumption. It makes no entitlement decisions, and it
//! consumes only the purchase tokens Rust hands it (verified tips, never Pro). It hands Rust the
//! signed purchase JSON and signature exactly as Play delivered them, and the
//! app verifies those against its Play license key before anything unlocks.
//! Host builds get a stub that reports billing as unsupported.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

mod error;
mod models;

/// Emitted to the frontend when Play reports purchases outside an in-app
/// flow, such as a promo code redeemed in the Play Store while the app runs.
/// It carries nothing: the app re-queries owned purchases and verifies them.
pub const PURCHASES_UPDATED_EVENT: &str = "purchases-updated";

pub use error::{Error, Result};
pub use models::{
    response_code, BillingResultResponse, ProductInfo, ProductResponse, PurchasesResponse,
    RawPurchase, SaleInfo,
};

#[cfg(not(mobile))]
mod desktop;
#[cfg(mobile)]
mod mobile;

#[cfg(not(mobile))]
pub use desktop::Billing;
#[cfg(mobile)]
pub use mobile::Billing;

/// Access the billing adapter from any [`Manager`] (`app.billing()`).
pub trait BillingExt<R: Runtime> {
    fn billing(&self) -> &Billing<R>;
}

impl<R: Runtime, T: Manager<R>> BillingExt<R> for T {
    fn billing(&self) -> &Billing<R> {
        self.state::<Billing<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("atv-billing")
        .setup(|app, api| {
            #[cfg(mobile)]
            let billing = mobile::init(app, api)?;
            #[cfg(not(mobile))]
            let billing = desktop::init(app, api)?;
            app.manage(billing);
            Ok(())
        })
        .build()
}
