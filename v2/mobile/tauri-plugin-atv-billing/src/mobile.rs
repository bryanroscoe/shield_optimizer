use tauri::{
    ipc::Channel,
    plugin::{PluginApi, PluginHandle},
    AppHandle, Emitter, Runtime,
};

use crate::error::Result;
use crate::models::*;
use crate::PURCHASES_UPDATED_EVENT;

/// Must match the Kotlin plugin's package (the Android library namespace).
const PLUGIN_IDENTIFIER: &str = "app.tauri.atvbilling";

pub fn init<R: Runtime, C: serde::de::DeserializeOwned>(
    app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> Result<Billing<R>> {
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "BillingPlugin")?;
    // Without the watch a purchase made outside the app still arrives on the
    // next launch or resume; this only makes it arrive while the app is open.
    if let Err(error) = watch_purchases(app, &handle) {
        eprintln!("atv-billing: purchase watch unavailable: {error}");
    }
    Ok(Billing(handle))
}

/// Starts the Kotlin purchase watch and re-emits each ping to the frontend as
/// [`PURCHASES_UPDATED_EVENT`]. The ping's payload is ignored on purpose.
fn watch_purchases<R: Runtime>(app: &AppHandle<R>, handle: &PluginHandle<R>) -> Result<()> {
    let app = app.clone();
    let channel: Channel = Channel::new(move |_| {
        let _ = app.emit(PURCHASES_UPDATED_EVENT, ());
        Ok(())
    });
    handle.run_mobile_plugin::<()>("watchPurchases", WatchPurchasesPayload { channel })?;
    Ok(())
}

/// Rust-side handle to the Kotlin `BillingPlugin`. Every call blocks on the
/// JNI hop (a purchase blocks until the Play sheet closes), so async callers
/// must wrap it in `spawn_blocking`.
pub struct Billing<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Billing<R> {
    pub fn query_product(&self, product_id: &str) -> Result<ProductResponse> {
        Ok(self
            .0
            .run_mobile_plugin("queryProduct", ProductArgs { product_id })?)
    }

    pub fn query_purchases(&self) -> Result<PurchasesResponse> {
        Ok(self
            .0
            .run_mobile_plugin("queryPurchases", serde_json::json!({}))?)
    }

    pub fn launch_purchase(&self, product_id: &str) -> Result<PurchasesResponse> {
        Ok(self
            .0
            .run_mobile_plugin("launchPurchase", ProductArgs { product_id })?)
    }

    pub fn acknowledge(&self, purchase_token: &str) -> Result<BillingResultResponse> {
        Ok(self
            .0
            .run_mobile_plugin("acknowledge", AcknowledgeArgs { purchase_token })?)
    }

    /// Warm up Play Integrity's token provider, so a later
    /// [`Billing::integrity_token`] answers quickly. `error` says why not.
    pub fn prepare_integrity(&self, cloud_project_number: i64) -> Result<IntegrityTokenResponse> {
        Ok(self.0.run_mobile_plugin(
            "prepareIntegrity",
            IntegrityArgs {
                cloud_project_number,
                request_hash: None,
            },
        )?)
    }

    /// A Standard Play Integrity token bound to `request_hash`.
    pub fn integrity_token(
        &self,
        cloud_project_number: i64,
        request_hash: &str,
    ) -> Result<IntegrityTokenResponse> {
        Ok(self.0.run_mobile_plugin(
            "integrityToken",
            IntegrityArgs {
                cloud_project_number,
                request_hash: Some(request_hash),
            },
        )?)
    }
}
