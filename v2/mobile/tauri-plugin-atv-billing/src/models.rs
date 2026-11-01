use serde::{Deserialize, Serialize};

/// Play Billing `BillingResponseCode` values the app distinguishes. Anything
/// else is treated as a generic, retryable failure.
pub mod response_code {
    pub const SERVICE_TIMEOUT: i32 = -3;
    pub const FEATURE_NOT_SUPPORTED: i32 = -2;
    pub const SERVICE_DISCONNECTED: i32 = -1;
    pub const OK: i32 = 0;
    pub const USER_CANCELED: i32 = 1;
    pub const SERVICE_UNAVAILABLE: i32 = 2;
    pub const BILLING_UNAVAILABLE: i32 = 3;
    pub const ITEM_UNAVAILABLE: i32 = 4;
    pub const DEVELOPER_ERROR: i32 = 5;
    pub const ERROR: i32 = 6;
    pub const ITEM_ALREADY_OWNED: i32 = 7;
    pub const ITEM_NOT_OWNED: i32 = 8;
    pub const NETWORK_ERROR: i32 = 12;
}

/// A purchase exactly as Play delivered it: the signed JSON and its base64
/// signature. Nothing the Kotlin side parsed is trusted; Rust verifies the
/// signature and reads every field back out of `original_json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPurchase {
    pub original_json: String,
    pub signature: String,
}

/// Result of `queryPurchasesAsync(INAPP)` or of a purchase flow
/// (`onPurchasesUpdated`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchasesResponse {
    pub response_code: i32,
    #[serde(default)]
    pub debug_message: String,
    #[serde(default)]
    pub purchases: Vec<RawPurchase>,
}

/// Display data from Play's `ProductDetails` for a one-time product. The
/// price is Play's localized, formatted string; the app never hard-codes one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductInfo {
    pub product_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub formatted_price: String,
    #[serde(default)]
    pub price_amount_micros: i64,
    #[serde(default)]
    pub price_currency_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductResponse {
    pub response_code: i32,
    #[serde(default)]
    pub debug_message: String,
    #[serde(default)]
    pub product: Option<ProductInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BillingResultResponse {
    pub response_code: i32,
    #[serde(default)]
    pub debug_message: String,
}

#[cfg(mobile)]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductArgs<'a> {
    pub product_id: &'a str,
}

#[cfg(mobile)]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AcknowledgeArgs<'a> {
    pub purchase_token: &'a str,
}
