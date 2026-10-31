// Google Play billing state for the UI. The backend decides everything: it
// verifies purchases, owns the entitlement and enforces every Pro gate. This
// store only mirrors `billing_status` and forwards Buy / Restore requests, and
// it copies the backend's effective entitlement into the session.

import { api } from "./api";
import { session } from "./session.svelte";
import type { BillingStatus, PurchaseOutcome } from "./types";

export const FREE_RECOVERY_PROMISE = "Re-enabling apps and emergency recovery are always free.";

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

/// While a purchase is pending, re-check this often (the backend rate-limits to
/// the same interval). Play delivers a completed pending payment to the app
/// only as an unsolicited update the adapter doesn't forward, so without this a
/// user who keeps the app open would stay Free until they leave and return.
const PENDING_POLL_MS = 30_000;

class BillingStore {
  status = $state<BillingStatus | null>(null);
  purchasing = $state(false);
  restoring = $state(false);
  message = $state("");
  tone = $state<Tone>("info");

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
        if (document.visibilityState === "visible") void this.refresh();
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
        this.say("No Pro purchase found for the Google account on this phone.", "info");
      else if (play.state === "error") this.say(play.message, "error");
      else if (play.state === "unavailable") this.say(play.reason, "error");
    } finally {
      this.restoring = false;
    }
  }

  async purchase(): Promise<PurchaseOutcome | null> {
    if (this.purchasing || this.restoring) return null;
    this.purchasing = true;
    this.say("", "info");
    try {
      const outcome = await api.billingPurchase();
      this.apply(outcome?.status);
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
    }
  }
}

export const billing = new BillingStore();
