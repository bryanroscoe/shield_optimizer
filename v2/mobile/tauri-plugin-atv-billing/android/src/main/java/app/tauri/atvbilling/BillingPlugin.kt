package app.tauri.atvbilling

import android.app.Activity
import android.util.Log
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.android.billingclient.api.AcknowledgePurchaseParams
import com.android.billingclient.api.BillingClient
import com.android.billingclient.api.BillingClient.BillingResponseCode
import com.android.billingclient.api.BillingClient.ProductType
import com.android.billingclient.api.BillingClientStateListener
import com.android.billingclient.api.BillingFlowParams
import com.android.billingclient.api.BillingResult
import com.android.billingclient.api.PendingPurchasesParams
import com.android.billingclient.api.ProductDetails
import com.android.billingclient.api.Purchase
import com.android.billingclient.api.PurchasesUpdatedListener
import com.android.billingclient.api.QueryProductDetailsParams
import com.android.billingclient.api.QueryPurchasesParams
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONArray
import java.util.concurrent.ConcurrentHashMap
import kotlin.coroutines.resume

private const val TAG = "AtvBilling"
private const val CONNECT_TIMEOUT_MS = 15_000L

@InvokeArg
internal class ProductArgs {
  var productId: String = ""
}

@InvokeArg
internal class AcknowledgeArgs {
  var purchaseToken: String = ""
}

/**
 * Thin adapter over the Play Billing Library. It decides nothing: every
 * response carries Play's response code, and purchases go back to Rust as the
 * signed `originalJson` plus `signature`, which Rust verifies against the
 * app's Play license key before any entitlement changes. Consumption is never
 * called; Pro is a non-consumable one-time product.
 */
@TauriPlugin
class BillingPlugin(private val activity: Activity) : Plugin(activity), PurchasesUpdatedListener {
  // Billing callbacks arrive on the main thread; keeping the plugin's own work
  // there too means `pendingPurchase` is only ever touched from one thread.
  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
  private val connectLock = Mutex()
  private val products = ConcurrentHashMap<String, ProductDetails>()
  private var pendingPurchase: Invoke? = null

  private val client: BillingClient by lazy {
    BillingClient.newBuilder(activity.applicationContext)
      .setListener(this)
      .enablePendingPurchases(
        PendingPurchasesParams.newBuilder().enableOneTimeProducts().build()
      )
      .build()
  }

  @Command
  fun queryProduct(invoke: Invoke) {
    scope.launch {
      try {
        val args = invoke.parseArgs(ProductArgs::class.java)
        val connection = connect()
        if (connection.responseCode != BillingResponseCode.OK) {
          invoke.resolve(resultJson(connection))
          return@launch
        }
        val (result, details) = queryDetails(args.productId)
        val out = resultJson(result)
        val offer = details?.let { purchaseOffer(it) }
        if (details != null && offer != null) {
          val product = JSObject().apply {
            put("productId", details.productId)
            put("name", details.name)
            put("title", details.title)
            put("description", details.description)
            put("formattedPrice", offer.formattedPrice)
            put("priceAmountMicros", offer.priceAmountMicros)
            put("priceCurrencyCode", offer.priceCurrencyCode)
          }
          out.put("product", product)
        }
        invoke.resolve(out)
      } catch (error: Throwable) {
        invoke.reject(error.message ?: error.javaClass.simpleName)
      }
    }
  }

  @Command
  fun queryPurchases(invoke: Invoke) {
    scope.launch {
      try {
        val connection = connect()
        if (connection.responseCode != BillingResponseCode.OK) {
          invoke.resolve(resultJson(connection))
          return@launch
        }
        val params = QueryPurchasesParams.newBuilder().setProductType(ProductType.INAPP).build()
        val (result, purchases) = suspendCancellableCoroutine { cont ->
          client.queryPurchasesAsync(params) { result, purchases ->
            if (cont.isActive) cont.resume(result to purchases)
          }
        }
        invoke.resolve(purchasesJson(result, purchases))
      } catch (error: Throwable) {
        invoke.reject(error.message ?: error.javaClass.simpleName)
      }
    }
  }

  @Command
  fun launchPurchase(invoke: Invoke) {
    scope.launch {
      try {
        val args = invoke.parseArgs(ProductArgs::class.java)
        val connection = connect()
        if (connection.responseCode != BillingResponseCode.OK) {
          invoke.resolve(resultJson(connection))
          return@launch
        }
        val details = products[args.productId] ?: queryDetails(args.productId).second
        if (details == null) {
          invoke.resolve(
            resultJson(
              BillingResult.newBuilder()
                .setResponseCode(BillingResponseCode.ITEM_UNAVAILABLE)
                .setDebugMessage("Product ${args.productId} was not returned by Play")
                .build()
            )
          )
          return@launch
        }
        // Rust serializes purchases, so a waiter still parked here belongs to a
        // flow whose caller already gave up (an activity recreated mid-sheet,
        // for instance). Release it rather than refusing every later purchase.
        pendingPurchase?.reject("Superseded by a newer purchase request")
        pendingPurchase = invoke
        val productParams = BillingFlowParams.ProductDetailsParams.newBuilder().setProductDetails(details)
        purchaseOffer(details)?.offerToken?.takeIf { it.isNotBlank() }?.let { productParams.setOfferToken(it) }
        val flow = BillingFlowParams.newBuilder()
          .setProductDetailsParamsList(listOf(productParams.build()))
          .build()
        val launch = client.launchBillingFlow(activity, flow)
        if (launch.responseCode != BillingResponseCode.OK && pendingPurchase === invoke) {
          pendingPurchase = null
          invoke.resolve(resultJson(launch))
        }
      } catch (error: Throwable) {
        if (pendingPurchase === invoke) pendingPurchase = null
        invoke.reject(error.message ?: error.javaClass.simpleName)
      }
    }
  }

  @Command
  fun acknowledge(invoke: Invoke) {
    scope.launch {
      try {
        val args = invoke.parseArgs(AcknowledgeArgs::class.java)
        val connection = connect()
        if (connection.responseCode != BillingResponseCode.OK) {
          invoke.resolve(resultJson(connection))
          return@launch
        }
        val params = AcknowledgePurchaseParams.newBuilder().setPurchaseToken(args.purchaseToken).build()
        val result = suspendCancellableCoroutine { cont ->
          client.acknowledgePurchase(params) { result -> if (cont.isActive) cont.resume(result) }
        }
        invoke.resolve(resultJson(result))
      } catch (error: Throwable) {
        invoke.reject(error.message ?: error.javaClass.simpleName)
      }
    }
  }

  override fun onPurchasesUpdated(result: BillingResult, purchases: MutableList<Purchase>?) {
    val waiting = pendingPurchase
    pendingPurchase = null
    if (waiting == null) {
      // An update outside an in-app flow (a promo code redeemed in the Play
      // Store, a pending purchase completing). Rust picks it up from the next
      // owned-purchase query, which runs on launch and on every resume.
      Log.i(TAG, "purchase update with no waiting flow (code=${result.responseCode})")
      return
    }
    waiting.resolve(purchasesJson(result, purchases.orEmpty()))
  }

  private suspend fun connect(): BillingResult = connectLock.withLock {
    if (client.isReady) return@withLock okResult()
    withTimeoutOrNull(CONNECT_TIMEOUT_MS) {
      suspendCancellableCoroutine { cont ->
        client.startConnection(object : BillingClientStateListener {
          override fun onBillingSetupFinished(result: BillingResult) {
            if (cont.isActive) cont.resume(result)
          }

          override fun onBillingServiceDisconnected() {
            if (cont.isActive) {
              cont.resume(
                BillingResult.newBuilder()
                  .setResponseCode(BillingResponseCode.SERVICE_DISCONNECTED)
                  .setDebugMessage("Play Billing service disconnected")
                  .build()
              )
            }
          }
        })
      }
    } ?: BillingResult.newBuilder()
      .setResponseCode(BillingResponseCode.SERVICE_TIMEOUT)
      .setDebugMessage("Timed out connecting to Google Play")
      .build()
  }

  private suspend fun queryDetails(productId: String): Pair<BillingResult, ProductDetails?> {
    val params = QueryProductDetailsParams.newBuilder()
      .setProductList(
        listOf(
          QueryProductDetailsParams.Product.newBuilder()
            .setProductId(productId)
            .setProductType(ProductType.INAPP)
            .build()
        )
      )
      .build()
    val (result, list) = suspendCancellableCoroutine { cont ->
      client.queryProductDetailsAsync(params) { result, details ->
        if (cont.isActive) cont.resume(result to details.productDetailsList)
      }
    }
    val match = list.firstOrNull { it.productId == productId }
    if (match != null) products[productId] = match
    return result to match
  }

  // Products with several purchase options or offers only list them in
  // oneTimePurchaseOfferDetailsList, and the purchase must name one by token.
  // The price shown is always the offer that the purchase sheet will charge.
  private fun purchaseOffer(details: ProductDetails): ProductDetails.OneTimePurchaseOfferDetails? =
    details.oneTimePurchaseOfferDetailsList?.firstOrNull() ?: details.oneTimePurchaseOfferDetails

  private fun okResult(): BillingResult =
    BillingResult.newBuilder().setResponseCode(BillingResponseCode.OK).build()

  private fun resultJson(result: BillingResult): JSObject = JSObject().apply {
    put("responseCode", result.responseCode)
    put("debugMessage", result.debugMessage)
  }

  private fun purchasesJson(result: BillingResult, purchases: List<Purchase>): JSObject {
    val array = JSONArray()
    purchases.forEach { purchase ->
      array.put(
        JSObject().apply {
          put("originalJson", purchase.originalJson)
          put("signature", purchase.signature)
        }
      )
    }
    return resultJson(result).apply { put("purchases", array) }
  }
}
