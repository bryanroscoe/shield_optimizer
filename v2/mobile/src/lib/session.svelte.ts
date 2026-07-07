// Shared session store — a runes singleton holding the live connection, the
// entitlement, connection liveness, and a small cache of the health report so
// the Dashboard and Diagnostics screens don't each re-fetch it. Screens import
// `session` instead of receiving prop-drilled `any`.
//
// The one canonical `deviceLabel` lives here (via deviceLabelOf) — the three
// old divergent derivations in Onboarding/Dashboard/Remote are gone.

import { api } from "./api";
import { rememberDevice } from "./savedDevices";
import { deviceLabelOf } from "./types";
import type {
  ConnectResult,
  Device,
  DiscoveryResult,
  Entitlement,
  HealthReport,
} from "./types";

export type Liveness = "idle" | "connecting" | "live" | "lost";

class Session {
  connectedDevice = $state<Device | null>(null);
  host = $state("");
  connectPort = $state(5555);
  entitlement = $state<Entitlement>("free");
  liveness = $state<Liveness>("idle");

  // Shared health cache. `healthLoaded` tracks a load *attempt* (an errored
  // load still counts as loaded so we render the error, not a spinner forever).
  // Mutating actions call `invalidateHealth()` which keeps stale data on screen
  // but lets the next screen visit refetch — no empty flash.
  health = $state<HealthReport | null>(null);
  healthLoaded = $state(false);
  healthLoading = $state(false);
  healthError = $state("");

  // Count of still-active recommended-debloat packages (real signal from
  // package_states), cached like health. Powers the dashboard score/summary.
  bloatCount = $state(0);
  bloatLoaded = $state(false);
  bloatLoading = $state(false);
  bloatError = $state("");

  get serial(): string {
    return this.connectedDevice?.serial ?? "";
  }

  get isPro(): boolean {
    return this.entitlement === "pro";
  }

  get deviceLabel(): string {
    return deviceLabelOf(this.connectedDevice);
  }

  get isConnected(): boolean {
    return this.connectedDevice != null && this.liveness !== "lost";
  }

  // ---- Discovery / connect flow (called by Onboarding) ----

  async scan(): Promise<DiscoveryResult> {
    return api.wirelessDiscover();
  }

  async pair(host: string, port: number, code: string): Promise<ConnectResult> {
    return api.wirelessPair(host, port, code);
  }

  /// Connect and, on success, resolve the real device via list_devices and go
  /// live. Returns the raw ConnectResult so the caller can surface errors.
  async connect(host: string, port: number): Promise<ConnectResult> {
    this.liveness = "connecting";
    const result = await api.wirelessConnect(host, port);
    if (result.ok) {
      this.host = host;
      this.connectPort = port;
      await this.refreshDevices();
      this.liveness = "live";
      // Remember this TV so §1.0 can offer a one-tap reconnect next launch.
      rememberDevice(host, port, this.connectedDevice);
    } else {
      this.liveness = "idle";
    }
    return result;
  }

  async refreshDevices(): Promise<void> {
    const list = await api.listDevices();
    // Prefer an authorized device; keep the current serial if it's still there.
    const current = list.find((d) => d.serial === this.serial);
    this.connectedDevice =
      current ?? list.find((d) => d.status === "device") ?? list[0] ?? null;
  }

  async disconnect(): Promise<void> {
    try {
      await api.wirelessDisconnect();
    } catch {
      // best-effort; we're tearing down regardless
    }
    this.reset();
  }

  async reconnect(): Promise<ConnectResult> {
    return this.connect(this.host, this.connectPort);
  }

  reset(): void {
    this.connectedDevice = null;
    this.liveness = "idle";
    this.health = null;
    this.healthLoaded = false;
    this.healthError = "";
  }

  // ---- Liveness ----

  /// Cheap probe. On connected:false we flip to 'lost' so the UI can show a
  /// reconnect banner instead of pretending we're still connected.
  async checkLiveness(): Promise<void> {
    if (!this.connectedDevice) return;
    try {
      const status = await api.wirelessStatus();
      this.liveness = status.connected ? "live" : "lost";
    } catch {
      this.liveness = "lost";
    }
  }

  // ---- Entitlement ----

  async loadEntitlement(): Promise<void> {
    try {
      this.entitlement = await api.getEntitlement();
    } catch {
      // Leave as-is (defaults to free) if the command isn't available yet.
    }
  }

  async activatePro(key: string): Promise<Entitlement> {
    this.entitlement = await api.activateLicense(key);
    return this.entitlement;
  }

  // ---- Shared health cache ----

  async loadHealth(force = false): Promise<void> {
    if (this.healthLoading) return;
    if (this.healthLoaded && !force && this.healthError === "") return;
    if (!this.serial) return;
    this.healthLoading = true;
    this.healthError = "";
    try {
      this.health = await api.healthReport(this.serial);
    } catch (e) {
      this.healthError = String(e);
    } finally {
      this.healthLoaded = true;
      this.healthLoading = false;
    }
  }

  /// Mark health stale after a mutating action (trim, reboot, enable/disable)
  /// without clearing the on-screen data — the next visit refetches.
  invalidateHealth(): void {
    this.healthLoaded = false;
  }

  /// Count enabled recommended-debloat apps for the connected device. Real
  /// data (app_list_for_device + package_states). On failure sets bloatError
  /// so the dashboard shows "—" rather than an invented score.
  async loadBloat(force = false): Promise<void> {
    if (this.bloatLoading) return;
    if (this.bloatLoaded && !force && this.bloatError === "") return;
    if (!this.connectedDevice) return;
    this.bloatLoading = true;
    this.bloatError = "";
    try {
      const catalog = await api.appListForDevice(
        this.connectedDevice.device_type,
      );
      const defaults = catalog.filter((a) => a.default_optimize);
      if (defaults.length === 0) {
        this.bloatCount = 0;
      } else {
        const states = await api.packageStates(
          this.serial,
          defaults.map((a) => a.package),
        );
        this.bloatCount = Object.values(states).filter(
          (s) => s === "enabled",
        ).length;
      }
    } catch (e) {
      this.bloatError = String(e);
    } finally {
      this.bloatLoaded = true;
      this.bloatLoading = false;
    }
  }

  /// After a mutation that changes installed/enabled state (disable/enable/
  /// uninstall/optimize), drop both caches so the next screen visit refetches.
  invalidateAll(): void {
    this.healthLoaded = false;
    this.bloatLoaded = false;
  }
}

export const session = new Session();
