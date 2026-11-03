use tauri::{
    plugin::{PluginApi, PluginHandle},
    AppHandle, Runtime,
};

use crate::error::Result;
use crate::models::*;

/// Must match the Kotlin plugin's package (the Android library namespace).
const PLUGIN_IDENTIFIER: &str = "app.tauri.atvbilling";

pub fn init<R: Runtime, C: serde::de::DeserializeOwned>(
    _app: &AppHandle<R>,
    api: PluginApi<R, C>,
) -> Result<Billing<R>> {
    let handle = api.register_android_plugin(PLUGIN_IDENTIFIER, "BillingPlugin")?;
    Ok(Billing(handle))
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
}
