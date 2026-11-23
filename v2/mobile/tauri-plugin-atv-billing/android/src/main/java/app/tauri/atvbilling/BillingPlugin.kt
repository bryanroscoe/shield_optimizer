package app.tauri.atvbilling

import android.app.Activity
import android.util.Log
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Channel
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
import com.google.android.play.core.integrity.IntegrityManagerFactory
import com.google.android.play.core.integrity.StandardIntegrityException
import com.google.android.play.core.integrity.StandardIntegrityManager
import com.google.android.play.core.integrity.StandardIntegrityManager.PrepareIntegrityTokenRequest
import com.google.android.play.core.integrity.StandardIntegrityManager.StandardIntegrityTokenProvider
import com.google.android.play.core.integrity.StandardIntegrityManager.StandardIntegrityTokenRequest
import com.google.android.play.core.integrity.model.StandardIntegrityErrorCode
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONArray
import java.util.concurrent.ConcurrentHashMap
import kotlin.coroutines.resume

private const val TAG = "AtvBilling"
private const val CONNECT_TIMEOUT_MS = 15_000L
private const val INTEGRITY_TIMEOUT_MS = 10_000L

@InvokeArg
internal class ProductArgs {
  var productId: String = ""
}

@InvokeArg
internal class AcknowledgeArgs {
  var purchaseToken: String = ""
}

@InvokeArg
internal class IntegrityArgs {
  var cloudProjectNumber: Long = 0
  var requestHash: String? = null
}

@InvokeArg
internal class WatchPurchasesArgs {
  lateinit var channel: Channel
}

/**
 * Thin adapter over the Play Billing Library. It decides nothing: every
 * response carries Play's response code, and purchases go back to Rust as the
 * signed `originalJson` plus `signature`, which Rust verifies against the
 * app's Play license key before any entitlement changes. Consumption is never
 * called; Pro is a non-consumable one-time product. It also asks Play
 * Integrity for Standard tokens, which it hands to Rust unread.
 */
@TauriPlugin
class BillingPlugin(private val activity: Activity) : Plugin(activity), PurchasesUpdatedListener {
  // Billing callbacks arrive on the main thread; keeping the plugin's own work
  // there too means `pendingPurchase` is only ever touched from one thread.
  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
  private val products = ConcurrentHashMap<String, ProductDetails>()
  private var pendingPurchase: Invoke? = null
  private var connecting: CompletableDeferred<BillingResult>? = null
  /// Told when Play reports purchases outside an in-app flow. Main thread only.
  private var purchaseWatch: Channel? = null
  /// Play Integrity's Standard token provider, prepared once and reused until
  /// Play says it is invalid. Main thread only, like the billing state.
  private var integrityProvider: StandardIntegrityTokenProvider? = null
  private var integrityPreparing: CompletableDeferred<Pair<StandardIntegrityTokenProvider?, String?>>? = null
  private var integrityManager: StandardIntegrityManager? = null

  private val client: BillingClient by lazy {
    BillingClient.newBuilder(activity.applicationContext)
      .setListener(this)
      .enablePendingPurchases(
        PendingPurchasesParams.newBuilder().enableOneTimeProducts().build()
      )
      .build()
  }

  /// Registers the channel `onPurchasesUpdated` pings when Play reports
  /// purchases outside an in-app flow. The ping carries only the response
  /// code, never a purchase: Rust re-queries owned purchases and verifies them.
  @Command
  fun watchPurchases(invoke: Invoke) {
    scope.launch {
      try {
        purchaseWatch = invoke.parseArgs(WatchPurchasesArgs::class.java).channel
        invoke.resolve()
      } catch (error: Throwable) {
        invoke.reject(error.message ?: error.javaClass.simpleName)
      }
    }
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

  /// Prepares the Play Integrity token provider ahead of a request. Resolves
  /// with `error` set when Play can't (no Play Store or Play services, an
  /// emulator, an unlinked Cloud project); never rejects for that.
  @Command
  fun prepareIntegrity(invoke: Invoke) {
    scope.launch {
      try {
        val args = invoke.parseArgs(IntegrityArgs::class.java)
        val (_, error) = integrityProvider(args.cloudProjectNumber)
        invoke.resolve(JSObject().apply { if (error != null) put("error", error) })
      } catch (error: Throwable) {
        invoke.resolve(JSObject().put("error", error.javaClass.simpleName))
      }
    }
  }

  /// A Standard Play Integrity token bound to `requestHash`, or `error`. The
  /// token is opaque here: it goes to the developer's server, which records
  /// Google's verdict and never acts on it. No verdict gates anything.
  @Command
  fun integrityToken(invoke: Invoke) {
    scope.launch {
      try {
        val args = invoke.parseArgs(IntegrityArgs::class.java)
        val hash = args.requestHash
        if (hash.isNullOrEmpty()) {
          invoke.resolve(JSObject().put("error", "no_request_hash"))
          return@launch
        }
        val (provider, prepareError) = integrityProvider(args.cloudProjectNumber)
        if (provider == null) {
          invoke.resolve(JSObject().put("error", prepareError ?: "unavailable"))
          return@launch
        }
        val request = StandardIntegrityTokenRequest.builder().setRequestHash(hash).build()
        val outcome = withTimeoutOrNull(INTEGRITY_TIMEOUT_MS) {
          suspendCancellableCoroutine<Pair<String?, String?>> { cont ->
            provider.request(request)
              .addOnSuccessListener { token -> if (cont.isActive) cont.resume(token.token() to null) }
              .addOnFailureListener { error ->
                if (integrityErrorCode(error) == StandardIntegrityErrorCode.INTEGRITY_TOKEN_PROVIDER_INVALID &&
                  integrityProvider === provider
                ) {
                  integrityProvider = null
                }
                if (cont.isActive) cont.resume(null to integrityError(error))
              }
          }
        } ?: (null to "timeout")
        invoke.resolve(
          JSObject().apply {
            outcome.first?.let { put("token", it) }
            outcome.second?.let { put("error", it) }
          }
        )
      } catch (error: Throwable) {
        invoke.resolve(JSObject().put("error", error.javaClass.simpleName))
      }
    }
  }

  // One preparation at a time; later callers wait on it. A failed one is
  // forgotten, so the next request tries again.
  private suspend fun integrityProvider(cloudProjectNumber: Long): Pair<StandardIntegrityTokenProvider?, String?> {
    integrityProvider?.let { return it to null }
    val attempt = integrityPreparing ?: CompletableDeferred<Pair<StandardIntegrityTokenProvider?, String?>>().also { attempt ->
      integrityPreparing = attempt
      try {
        val manager = integrityManager
          ?: IntegrityManagerFactory.createStandard(activity.applicationContext).also { integrityManager = it }
        val request = PrepareIntegrityTokenRequest.builder().setCloudProjectNumber(cloudProjectNumber).build()
        manager.prepareIntegrityToken(request)
          .addOnSuccessListener { provider ->
            integrityProvider = provider
            if (integrityPreparing === attempt) integrityPreparing = null
            attempt.complete(provider to null)
          }
          .addOnFailureListener { error ->
            if (integrityPreparing === attempt) integrityPreparing = null
            attempt.complete(null to integrityError(error))
          }
      } catch (error: Throwable) {
        if (integrityPreparing === attempt) integrityPreparing = null
        attempt.complete(null to error.javaClass.simpleName)
      }
    }
    return withTimeoutOrNull(INTEGRITY_TIMEOUT_MS) { attempt.await() } ?: (null to "timeout")
  }

  private fun integrityErrorCode(error: Throwable): Int? = (error as? StandardIntegrityException)?.errorCode

  private fun integrityError(error: Throwable): String =
    integrityErrorCode(error)?.let { "integrity_$it" } ?: error.javaClass.simpleName

  override fun onPurchasesUpdated(result: BillingResult, purchases: MutableList<Purchase>?) {
    val waiting = pendingPurchase
    pendingPurchase = null
    if (waiting == null) {
      // An update outside an in-app flow (a promo code redeemed in the Play
      // Store, a pending purchase completing). The app re-queries owned
      // purchases when told, as it also does on launch and on every resume.
      Log.i(TAG, "purchase update with no waiting flow (code=${result.responseCode})")
      purchaseWatch?.send(JSObject().put("code", result.responseCode))
      return
    }
    waiting.resolve(purchasesJson(result, purchases.orEmpty()))
  }

  // A timed-out caller stops waiting but the attempt carries on, and calling
  // startConnection again while it does makes Play answer DEVELOPER_ERROR
  // ("already in the process of connecting"). So there is at most one attempt:
  // later callers wait on it until Play answers. Main thread only, like every
  // other caller and the billing callbacks, so no lock is needed.
  private suspend fun connect(): BillingResult {
    if (client.isReady) return okResult()
    val attempt = connecting ?: startConnecting()
    return withTimeoutOrNull(CONNECT_TIMEOUT_MS) { attempt.await() }
      ?: BillingResult.newBuilder()
        .setResponseCode(BillingResponseCode.SERVICE_TIMEOUT)
        .setDebugMessage("Timed out connecting to Google Play")
        .build()
  }

  private fun startConnecting(): CompletableDeferred<BillingResult> {
    val attempt = CompletableDeferred<BillingResult>()
    // Set before startConnection: Play may answer synchronously.
    connecting = attempt
    try {
      client.startConnection(object : BillingClientStateListener {
        override fun onBillingSetupFinished(result: BillingResult) {
          if (connecting === attempt) connecting = null
          attempt.complete(result)
        }

        override fun onBillingServiceDisconnected() {
          if (connecting === attempt) connecting = null
          attempt.complete(
            BillingResult.newBuilder()
              .setResponseCode(BillingResponseCode.SERVICE_DISCONNECTED)
              .setDebugMessage("Play Billing service disconnected")
              .build()
          )
        }
      })
    } catch (error: Throwable) {
      if (connecting === attempt) connecting = null
      throw error
    }
    return attempt
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
