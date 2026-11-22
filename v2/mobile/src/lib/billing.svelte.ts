// Google Play billing state for the UI. The backend decides everything: it
// verifies purchases, owns the entitlement and enforces every Pro gate. This
// store only mirrors `billing_status` and forwards Buy / Restore requests, and
// it copies the backend's effective entitlement into the session.

import { api, sendTelemetry } from "./api";
import { recordAction } from "../../../shared/telemetry";
import { session } from "./session.svelte";
import type { BillingStatus, PlayOffer, PurchaseOutcome, PurchaseResultKind } from "./types";

export const FREE_RECOVERY_PROMISE = "Re-enabling apps and emergency recovery are always free.";

/// Shown beside Restore purchases: a purchase or promo code redeemed outside
/// the app (on the Play Store website, say) can reach the phone late.
export const RESTORE_HINT = "Already bought Pro, or redeemed a promo code on Google Play? Tap Restore purchases.";

/// Matches the Kotlin plugin's event (tauri-plugin-atv-billing, PURCHASES_UPDATED_EVENT).
export const PURCHASES_UPDATED_EVENT = "purchases-updated";

function isStatus(value: unknown): value is BillingStatus {
  return (
    !!value &&
    typeof value === "object" &&
    "play" in value &&
    "entitlement" in value &&
    typeof (value as BillingStatus).play === "object"
  );
}

type Tone = "success" | "error" | "info";

/// Anonymous report of how a Buy attempt ended: the outcome kind only, never
/// the order id, purchase token, price, product or license data. A canceled
/// sheet or a pending payment is not a result yet, so it isn't reported.
export function reportPurchaseOutcome(kind: PurchaseResultKind | undefined): void {
  if (kind === "canceled" || kind === "pending") return;
  const ok = kind === "purchased" || kind === "restored" || kind === "already_owned";
  recordAction(sendTelemetry, "other", null, ok ? { ok } : { ok, code: `billing_${kind ?? "failed"}` });
}

/// While a purchase is pending, re-check this often (the backend rate-limits to
/// the same interval). Play delivers a completed pending payment to the app
/// only as an unsolicited update the adapter doesn't forward, so without this a
/// user who keeps the app open would stay Free until they leave and return.
const PENDING_POLL_MS = 30_000;

class BillingStore {
  status = $state<BillingStatus | null>(null);
  purchasing = $state(false);
  /// The product whose purchase sheet is open, so only its button says so.
  purchasingId = $state<string | null>(null);
  restoring = $state(false);
  message = $state("");
  tone = $state<Tone>("info");
  /// Set when a check the user didn't start (launch, resume, Play reporting a
  /// purchase made outside the app) newly finds Pro owned on Google Play.
  unlockNote = $state("");

  /// Offline key entry is a sideload/dev unlock. Unknown status hides it, the
  /// policy-safe default; the backend refuses keys in Play builds regardless.
  get licenseKeyEntry(): boolean {
    return this.status?.license_key_entry ?? false;
  }

  get available(): boolean {
    return this.status?.available ?? false;
  }

  get ownsViaPlay(): boolean {
    return this.status?.sources.includes("play") ?? false;
  }

  get supportOffers(): PlayOffer[] {
    return this.status?.support_offers ?? [];
  }

  get pending(): boolean {
    return this.status?.play.state === "pending";
  }

  get unavailableReason(): string {
    const play = this.status?.play;
    if (play?.state === "unavailable") return play.reason;
    return "Purchases aren't available right now.";
  }

  private pendingTimer: ReturnType<typeof setInterval> | undefined;

  private apply(status: unknown): status is BillingStatus {
    if (!isStatus(status)) return false;
    this.status = status;
    session.entitlement = status.entitlement;
    this.watchPending(status.play.state === "pending");
    return true;
  }

  private watchPending(pending: boolean) {
    if (pending && this.pendingTimer === undefined) {
      this.pendingTimer = setInterval(() => {
        if (document.visibilityState === "visible") void this.checkOwned();
      }, PENDING_POLL_MS);
    } else if (!pending && this.pendingTimer !== undefined) {
      clearInterval(this.pendingTimer);
      this.pendingTimer = undefined;
    }
  }

  private say(message: string, tone: Tone) {
    this.message = message;
    this.tone = tone;
  }

  async load(): Promise<void> {
    try {
      this.apply(await api.billingStatus());
    } catch {
      // Leave the last known status; nothing here can grant Pro.
    }
  }

  /// Launch / resume / paywall open. Rate-limited in the backend unless forced.
  async refresh(force = false): Promise<BillingStatus | null> {
    try {
      const status = await api.billingRefresh(force);
      return this.apply(status) ? status : null;
    } catch {
      return null;
    }
  }

  /// Launch, resume, and Play reporting a purchase made outside the app (a
  /// promo code redeemed on the Play Store). Says so when that unlocks Pro.
  /// The stored status is read first, so an owner launching the app again
  /// isn't told Pro "unlocked".
  async checkOwned(force = false): Promise<void> {
    if (this.status === null) await this.load();
    const before = this.ownsViaPlay;
    const status = await this.refresh(force);
    if (status && !before && this.ownsViaPlay) this.unlockNote = "Pro unlocked from your Google Play purchase.";
  }

  async restore(): Promise<void> {
    if (this.restoring || this.purchasing) return;
    this.restoring = true;
    this.say("", "info");
    try {
      const status = await this.refresh(true);
      const play = status?.play;
      if (!play) this.say("Couldn't check purchases. Try again.", "error");
      else if (play.state === "owned") this.say("Pro restored from Google Play.", "success");
      else if (play.state === "pending")
        this.say("Your purchase is still pending. Pro unlocks once Google Play confirms the payment.", "info");
      else if (play.state === "not_owned")
        this.say(
          "No Pro purchase found for the Google account on this phone. A code redeemed on the Play Store website can take a few minutes to reach the phone: check it was the same Google account, then try again shortly.",
          "info",
        );
      else if (play.state === "error") this.say(play.message, "error");
      else if (play.state === "unavailable") this.say(play.reason, "error");
    } finally {
      this.restoring = false;
    }
  }

  /// Buy Pro. A support tier's product id buys the same Pro at its higher price.
  async purchase(productId?: string): Promise<PurchaseOutcome | null> {
    if (this.purchasing || this.restoring) return null;
    this.purchasing = true;
    this.purchasingId = productId ?? this.status?.offer?.product_id ?? null;
    this.say("", "info");
    try {
      const outcome = await api.billingPurchase(productId);
      this.apply(outcome?.status);
      reportPurchaseOutcome(outcome?.outcome);
      switch (outcome?.outcome) {
        case "purchased":
        case "restored":
        case "already_owned":
          this.say(outcome.message, "success");
          break;
        case "pending":
          this.say(outcome.message, "info");
          break;
        case "canceled":
          break;
        default:
          this.say(outcome?.message || "Google Play reported an error. Try again.", "error");
      }
      return outcome ?? null;
    } catch (e) {
      this.say(String(e).replace(/^.*Error:\s*/, "") || "Google Play reported an error. Try again.", "error");
      return null;
    } finally {
      this.purchasing = false;
      this.purchasingId = null;
    }
  }
}

export const billing = new BillingStore();
