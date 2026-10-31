use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::error::{Error, Result};
use crate::models::*;

pub fn init<R: Runtime, C: serde::de::DeserializeOwned>(
    _app: &AppHandle<R>,
    _api: PluginApi<R, C>,
) -> Result<Billing<R>> {
    Ok(Billing(std::marker::PhantomData))
}

/// Host-build stub. There is no Play Store off Android, so every call reports
/// `Unsupported`; it never fabricates a product or a purchase.
pub struct Billing<R: Runtime>(std::marker::PhantomData<fn() -> R>);

impl<R: Runtime> Billing<R> {
    pub fn query_product(&self, _product_id: &str) -> Result<ProductResponse> {
        Err(Error::Unsupported)
    }

    pub fn query_purchases(&self) -> Result<PurchasesResponse> {
        Err(Error::Unsupported)
    }

    pub fn launch_purchase(&self, _product_id: &str) -> Result<PurchasesResponse> {
        Err(Error::Unsupported)
    }

    pub fn acknowledge(&self, _purchase_token: &str) -> Result<BillingResultResponse> {
        Err(Error::Unsupported)
    }
}
