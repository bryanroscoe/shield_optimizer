// Shared session store — a runes singleton holding the live connection, the
// entitlement, connection liveness, and a small cache of the health report so
// the Dashboard and Diagnostics screens don't each re-fetch it. Screens import
// `session` instead of receiving prop-drilled `any`.
//
// The one canonical `deviceLabel` lives here (via deviceLabelOf) — the three
// old divergent derivations in Onboarding/Dashboard/Remote are gone.

import { api } from "./api";
import { onConnectionLost, setConnectionGeneration } from "./connectionEvents";
import { cachedDeviceName, rememberDevice, setAutoConnect } from "./savedDevices";
import { deviceLabelOf } from "./types";
import type {
  ConnectResult,
  Device,
  DiscoveryResult,
  Entitlement,
  HealthReport,
} from "./types";

export type Liveness = "idle" | "connecting" | "live" | "reconnecting" | "lost";

class Session {
  private connectionGeneration = Date.now() * 1000;
  private healthGeneration = 0;
  private bloatGeneration = 0;
  private recovering: { generation: number; promise: Promise<boolean> } | null = null;
  private recoveryAttempted = false;

  get generation(): number {
    return this.connectionGeneration;
  }

  private nextGeneration(): number {
    this.connectionGeneration = Math.max(this.connectionGeneration + 1, Date.now() * 1000);
    setConnectionGeneration(this.connectionGeneration);
    return this.connectionGeneration;
  }

  connectedDevice = $state<Device | null>(null);
  host = $state("");
  connectPort = $state(5555);
  entitlement = $state<Entitlement>("free");
  liveness = $state<Liveness>("idle");
  /// True while Optimize is applying a plan; tabs lock so the loop can't be
  /// orphaned by navigating away.
  applyInProgress = $state(false);

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
  /// Size of the recommended-debloat set the count is measured against.
  bloatTotal = $state(0);
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
    const reported = this.connectedDevice?.properties?.friendly_name?.trim();
    if (reported) return reported;
    const cached = cachedDeviceName(
      this.host,
      this.connectedDevice?.properties?.serial_number?.trim() || undefined,
    );
    if (cached) return cached;
    return deviceLabelOf(this.connectedDevice);
  }

  get isConnected(): boolean {
    return this.connectedDevice != null && this.liveness === "live";
  }

  /// Wire the api-level "connection lost" signal. Called once from App.
  attachConnectionWatch(): () => void {
    return onConnectionLost((generation) => {
      if (generation === this.connectionGeneration) void this.recoverOrMarkLost();
    });
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
    if (this.applyInProgress) return { ok: false, message: "Stop the current operation before switching TVs." };
    const generation = this.nextGeneration();
    this.recoveryAttempted = false;
    this.liveness = "connecting";
    try {
      const result = await api.wirelessConnect(host, port, generation);
      if (generation !== this.connectionGeneration) return { ok: false, message: "Connection attempt canceled." };
      if (!result.ok) {
        await this.restoreCurrentLiveness(generation);
        return result;
      }
      // The backend has replaced its connection, so whatever device object we
      // held describes a socket that no longer exists. Drop it before the new
      // host is published so no render can pair the old name with the new IP.
      this.connectedDevice = null;
      this.clearDeviceData();
      this.host = host;
      this.connectPort = port;
      try {
        await this.refreshDevices(generation);
      } catch (error) {
        if (generation === this.connectionGeneration) this.liveness = "lost";
        throw error;
      }
      if (generation !== this.connectionGeneration) return { ok: false, message: "Connection attempt canceled." };
      this.liveness = this.connectedDevice ? "live" : "lost";
      if (this.connectedDevice) {
        // Remember this TV so the next launch can offer it, and allow a
        // silent redial of it because the user chose it explicitly.
        this.rememberCurrentDevice();
        setAutoConnect(true);
      }
      return this.connectedDevice ? result : { ok: false, message: "The TV disconnected before its profile could be loaded." };
    } catch (error) {
      if (generation === this.connectionGeneration) {
        await this.restoreCurrentLiveness(generation);
      }
      throw error;
    }
  }

  /// Re-read the device list. Results are discarded when a newer connection
  /// attempt started in the meantime.
  async refreshDevices(generation = this.connectionGeneration): Promise<void> {
    const list = await api.listDevices();
    if (generation !== this.connectionGeneration) return;
    const expected = `${this.host}:${this.connectPort}`;
    this.connectedDevice = list.find((d) => d.serial === expected && d.status === "device") ?? null;
  }

  /// Explicit user disconnect. Also turns off launch auto-dial so a process
  /// kill can't resurrect the connection the user just ended.
  async disconnect(): Promise<boolean> {
    if (this.applyInProgress) return false;
    const generation = this.nextGeneration();
    setAutoConnect(false);
    try {
      await api.wirelessDisconnect(generation);
    } catch {
      // best-effort; we're tearing down regardless
    }
    if (generation !== this.connectionGeneration) return false;
    this.reset();
    return true;
  }

  async cancelConnect(): Promise<void> {
    const canceledRequestId = this.connectionGeneration;
    const generation = this.nextGeneration();
    this.liveness = this.connectedDevice ? "lost" : "idle";
    try {
      await api.wirelessCancelConnect(generation, canceledRequestId);
    } catch (e) {
      if (generation === this.connectionGeneration) throw e;
    }
    if (generation === this.connectionGeneration) await this.restoreCurrentLiveness(generation);
  }

  async reconnect(): Promise<ConnectResult> {
    return this.connect(this.host, this.connectPort);
  }

  rememberCurrentDevice(): void {
    if (!this.host) return;
    rememberDevice(this.host, this.connectPort, this.connectedDevice);
  }

  reset(): void {
    this.connectedDevice = null;
    this.host = "";
    this.connectPort = 5555;
    this.liveness = "idle";
    this.clearDeviceData();
  }

  private clearDeviceData(): void {
    ++this.healthGeneration;
    ++this.bloatGeneration;
    this.health = null;
    this.healthLoaded = false;
    this.healthLoading = false;
    this.healthError = "";
    this.bloatCount = 0;
    this.bloatTotal = 0;
    this.bloatLoaded = false;
    this.bloatLoading = false;
    this.bloatError = "";
  }

  private async restoreCurrentLiveness(generation: number): Promise<void> {
    try {
      await this.refreshDevices(generation);
      if (generation !== this.connectionGeneration) return;
      this.liveness = this.connectedDevice ? "live" : "idle";
    } catch {
      if (generation !== this.connectionGeneration) return;
      this.liveness = this.connectedDevice ? "lost" : "idle";
    }
  }

  // ---- Liveness ----

  /// Cheap probe. When the TV stops answering we try one silent reconnect to
  /// the same TV the user chose; only if that fails do we flip to 'lost' and
  /// show the reconnect banner.
  async checkLiveness(): Promise<void> {
    if (!this.connectedDevice) return;
    if (this.liveness !== "live") return;
    const generation = this.connectionGeneration;
    let alive = false;
    try {
      alive = (await api.wirelessStatus()).connected;
    } catch {
      alive = false;
    }
    // A newer connect/disconnect owns the state now; this probe is history.
    if (generation !== this.connectionGeneration || !this.connectedDevice) return;
    if (alive) {
      this.liveness = "live";
      return;
    }
    await this.recoverOrMarkLost();
  }

  /// One automatic reconnect to the current host. Concurrent callers share the
  /// same attempt. Resolves true when the connection is live again.
  async recoverOrMarkLost(): Promise<boolean> {
    if (!this.connectedDevice || !this.host) return false;
    if (this.recovering?.generation === this.connectionGeneration) return this.recovering.promise;
    if (this.liveness === "connecting" || this.recoveryAttempted) return false;
    this.recoveryAttempted = true;
    const generation = this.nextGeneration();
    const promise = (async () => {
      this.liveness = "reconnecting";
      try {
        const result = await api.wirelessConnect(this.host, this.connectPort, generation);
        if (generation !== this.connectionGeneration) return false;
        if (result.ok) {
          await this.refreshDevices(generation);
          if (generation !== this.connectionGeneration) return false;
          const live = this.connectedDevice != null;
          this.liveness = live ? "live" : "lost";
          if (live) this.recoveryAttempted = false;
          return live;
        }
      } catch {
        // fall through to lost
      }
      if (generation === this.connectionGeneration) this.liveness = "lost";
      return false;
    })();
    const recovery = { generation, promise };
    this.recovering = recovery;
    try {
      return await promise;
    } finally {
      if (this.recovering === recovery) this.recovering = null;
    }
  }

  async finishReboot(generation: number): Promise<boolean> {
    if (generation !== this.connectionGeneration) return false;
    return this.disconnect();
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
    const serial = this.serial;
    const generation = ++this.healthGeneration;
    this.healthLoading = true;
    this.healthError = "";
    try {
      const health = await api.healthReport(serial);
      if (generation !== this.healthGeneration || serial !== this.serial) return;
      this.health = health;
    } catch (e) {
      if (generation !== this.healthGeneration || serial !== this.serial) return;
      this.healthError = String(e);
    } finally {
      if (generation === this.healthGeneration && serial === this.serial) {
        this.healthLoaded = true;
        this.healthLoading = false;
      }
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
    const serial = this.serial;
    const deviceType = this.connectedDevice.device_type;
    const generation = ++this.bloatGeneration;
    this.bloatLoading = true;
    this.bloatError = "";
    try {
      const catalog = await api.appListForDevice(deviceType);
      const defaults = catalog.filter((a) => a.default_optimize);
      let count = 0;
      if (defaults.length > 0) {
        const states = await api.packageStates(
          serial,
          defaults.map((a) => a.package),
        );
        count = Object.values(states).filter(
          (s) => s === "enabled",
        ).length;
      }
      if (generation !== this.bloatGeneration || serial !== this.serial) return;
      this.bloatCount = count;
      this.bloatTotal = defaults.length;
    } catch (e) {
      if (generation !== this.bloatGeneration || serial !== this.serial) return;
      this.bloatError = String(e);
    } finally {
      if (generation === this.bloatGeneration && serial === this.serial) {
        this.bloatLoaded = true;
        this.bloatLoading = false;
      }
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
