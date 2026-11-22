// Shared session store — a runes singleton holding the live connection, the
// entitlement, connection liveness, and a small cache of the health report so
// the Dashboard and Diagnostics screens don't each re-fetch it. Screens import
// `session` instead of receiving prop-drilled `any`.
//
// The one canonical `deviceLabel` lives here (via deviceLabelOf).

import { api } from "./api";
import { onConnectionLost, setConnectionGeneration } from "./connectionEvents";
import { normalizeHardwareId } from "./identity";
import { cachedDeviceName, rememberDevice, setAutoConnect } from "./savedDevices";
import { deviceLabelOf } from "./types";
import { parseSupport, type SupportInfo } from "../../../shared/support";
import type {
  ConnectResult,
  Device,
  DiscoveryResult,
  Entitlement,
  HealthReport,
  SavedDevice,
} from "./types";

export type Liveness = "idle" | "connecting" | "live" | "reconnecting" | "lost";

function identityFailure(expectedId: string, device: Device): string {
  const actualId = normalizeHardwareId(device.properties?.serial_number);
  if (actualId === expectedId) return "";
  return actualId
    ? "A different TV answered at this address. Scan for TVs or enter its address to connect as a new TV."
    : "Couldn't verify the saved TV's identity. Scan for TVs or enter its address to connect as a new TV.";
}

class Session {
  private connectionGeneration = $state(Date.now() * 1000);
  private healthGeneration = 0;
  private healthLoadingSerial = "";
  private bloatGeneration = 0;
  private bloatConnectionGeneration = 0;
  private recovering: { generation: number; promise: Promise<boolean> } | null = null;
  private recoveryAttempted = false;
  private reconnectIdentityChanged = false;

  get generation(): number {
    return this.connectionGeneration;
  }

  private nextGeneration(): number {
    this.connectionGeneration = Math.max(this.connectionGeneration + 1, Date.now() * 1000);
    setConnectionGeneration(this.connectionGeneration);
    return this.connectionGeneration;
  }

  connectedDevice = $state<Device | null>(null);
  /// Which TV the session is on. `generation` moves on every connect, silent
  /// redial and disconnect; this moves only when the TV may have changed: a
  /// different serial, a hardware id that differs or that either side can't
  /// report (sameness is never inferred from the address alone), or no TV at
  /// all. A silent reconnect to the same verified TV keeps it, so a screen can
  /// keep what the person was doing (text typed into Send Text) across a
  /// liveness blip, while still refusing to send over a dead link.
  deviceEpoch = $state(0);
  host = $state("");
  connectPort = $state(5555);
  entitlement = $state<Entitlement>("free");
  liveness = $state<Liveness>("idle");
  /// True while Optimize is applying a plan; tabs lock so the loop can't be
  /// orphaned by navigating away.
  applyInProgress = $state(false);
  /// Set when the last `rememberCurrentDevice()` found a different device at
  /// this row's address than the one saved (see savedDevices.ts). Screens
  /// read and clear it once to show an honest "different device" note rather
  /// than silently keeping the old TV's name.
  identityNote = $state("");
  reconnectFailure = $state("");

  private setReconnectFailure(message: string, changed = false): void {
    this.reconnectFailure = message;
    this.reconnectIdentityChanged = changed;
  }

  // Shared health cache. `healthLoaded` tracks a load *attempt* (an errored
  // load still counts as loaded so we render the error, not a spinner forever).
  // Mutating actions call `invalidateHealth()` which keeps stale data on screen
  // but lets the next screen visit refetch — no empty flash.
  health = $state<HealthReport | null>(null);
  healthLoaded = $state(false);
  healthLoading = $state(false);
  healthError = $state("");

  // Enabled and installed counts for the recommended catalog, cached like
  // health. Missing packages contribute to neither count.
  bloatCount = $state(0);
  /// Installed recommended packages (enabled + disabled), not catalog size.
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
      this.connectPort,
      this.connectedDevice?.properties?.serial_number ?? undefined,
    );
    if (cached) return cached;
    return deviceLabelOf(this.connectedDevice);
  }

  /// The connected TV's support tier. `unknown` (claims nothing) whenever no
  /// profile was read; never derived from a saved row or an address.
  get support(): SupportInfo {
    return parseSupport(this.connectedDevice?.support);
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
  /// `onAttempt` receives this attempt's generation, the only value
  /// `cancelConnect` accepts, so a screen can never cancel a connection it
  /// did not start.
  async connect(
    rawHost: string,
    port: number,
    onAttempt?: (generation: number) => void,
    expectedId?: string,
  ): Promise<ConnectResult> {
    if (this.applyInProgress) return { ok: false, message: "Stop the current operation before switching TVs." };
    // The backend trims too; the stored host has to match the serial it lists.
    const host = rawHost.trim();
    const generation = this.nextGeneration();
    this.liveness = "connecting";
    this.identityNote = "";
    onAttempt?.(generation);
    const previousSerial = this.serial;
    const previousId = normalizeHardwareId(this.connectedDevice?.properties?.serial_number);
    let replaced = false;
    try {
      const result = await api.wirelessConnect(host, port, generation, expectedId);
      if (generation !== this.connectionGeneration) return { ok: false, message: "Connection attempt canceled." };
      if (!result.ok) {
        await this.restoreCurrentLiveness(generation);
        if (generation === this.connectionGeneration && result.identity_mismatch && this.connectedDevice && !this.isConnected &&
          this.host === host && this.connectPort === port) {
          this.setReconnectFailure(result.message, result.identity_mismatch === "changed");
        }
        return result;
      }
      replaced = true;
      const found = await this.deviceAtEndpoint(host, port);
      if (generation !== this.connectionGeneration) return { ok: false, message: "Connection attempt canceled." };
      const refusal = found && expectedId ? identityFailure(expectedId, found) : "";
      if (refusal) {
        if (await this.rejectConnection(generation)) {
          if (this.connectedDevice && this.host === host && this.connectPort === port) {
            this.setReconnectFailure(refusal, !!normalizeHardwareId(found?.properties?.serial_number));
          }
          return { ok: false, message: refusal };
        }
        return { ok: false, message: "Connection attempt canceled." };
      }
      // The backend has replaced its connection, so whatever device object we
      // held describes a socket that no longer exists. Drop it before the new
      // host is published so no render can pair the old name with the new IP.
      this.connectedDevice = null;
      this.clearDeviceData();
      this.host = host;
      this.connectPort = port;
      this.connectedDevice = found;
      this.liveness = this.connectedDevice ? "live" : "lost";
      let mismatch = false;
      if (this.connectedDevice) {
        this.setReconnectFailure("");
        this.recoveryAttempted = false;
        // Remember this TV so the next launch can offer it, and allow a
        // silent redial of it because the user chose it explicitly.
        mismatch = this.rememberCurrentDevice();
        setAutoConnect(true);
      }
      this.settleDevice(previousSerial, previousId, mismatch);
      return this.connectedDevice ? result : { ok: false, message: "The TV disconnected before its profile could be loaded." };
    } catch (error) {
      if (generation === this.connectionGeneration) {
        if (replaced) {
          this.connectedDevice = null;
          this.clearDeviceData();
          this.host = host;
          this.connectPort = port;
          this.liveness = "lost";
          this.deviceEpoch += 1;
        } else {
          await this.restoreCurrentLiveness(generation);
        }
      }
      throw error;
    }
  }

  connectSaved(device: SavedDevice, onAttempt?: (generation: number) => void): Promise<ConnectResult> {
    return this.connect(device.host, device.connectPort, onAttempt, normalizeHardwareId(device.hardwareId));
  }

  private async deviceAtEndpoint(host: string, port: number): Promise<Device | null> {
    const list = await api.listDevices();
    const expected = `${host}:${port}`;
    return list.find((device) => device.serial === expected && device.status === "device") ?? null;
  }

  private async rejectConnection(expected: number): Promise<boolean> {
    if (expected !== this.connectionGeneration) return false;
    const generation = this.nextGeneration();
    try {
      // Retire only the rejected connection. Never tear down a newer choice.
      await api.wirelessCancelConnect(generation, expected);
    } catch {
      // Keep the interface offline even if backend cleanup failed.
    }
    if (generation !== this.connectionGeneration) return false;
    if (this.connectedDevice) this.deviceEpoch += 1;
    this.liveness = this.connectedDevice ? "lost" : "idle";
    return true;
  }

  /// Re-read the device list. Results are discarded when a newer connection
  /// attempt started in the meantime. Resolves true when the backend still
  /// lists this connection. When it doesn't, the last profile is kept so the
  /// banner can name the TV, Retry and the network-return redial still have a
  /// target, and a live session turns lost.
  async refreshDevices(generation = this.connectionGeneration): Promise<boolean> {
    const found = await this.deviceAtEndpoint(this.host, this.connectPort);
    if (generation !== this.connectionGeneration) return false;
    if (found) {
      const expectedId = normalizeHardwareId(this.connectedDevice?.properties?.serial_number);
      if (!expectedId || !identityFailure(expectedId, found)) {
        this.connectedDevice = found;
        return true;
      }
    }
    if (this.connectedDevice && this.liveness === "live") this.liveness = "lost";
    return false;
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

  /// Cancel the connect attempt `expected` names. Anything else -- a newer
  /// attempt, a silent redial, or the live connection itself -- is left alone.
  async cancelConnect(expected: number): Promise<void> {
    if (expected !== this.connectionGeneration || this.liveness !== "connecting") return;
    const generation = this.nextGeneration();
    if (!this.connectedDevice) this.liveness = "idle";
    let failure: unknown = null;
    try {
      await api.wirelessCancelConnect(generation, expected);
    } catch (e) {
      failure = e ?? "Couldn't cancel the connection attempt.";
    }
    if (generation !== this.connectionGeneration) return;
    await this.restoreCurrentLiveness(generation);
    if (failure !== null && generation === this.connectionGeneration) throw failure;
  }

  async reconnect(): Promise<ConnectResult> {
    const expectedId = normalizeHardwareId(this.connectedDevice?.properties?.serial_number);
    return this.connect(this.host, this.connectPort, undefined, expectedId);
  }

  /// Returns true when the saved row at this address belongs to a different
  /// device than the one now connected.
  rememberCurrentDevice(): boolean {
    if (!this.host) return false;
    const result = rememberDevice(this.host, this.connectPort, this.connectedDevice);
    if (result.mismatch) this.identityNote = "A different device is now at this address.";
    return result.mismatch;
  }

  reset(): void {
    if (this.connectedDevice) this.deviceEpoch += 1;
    this.connectedDevice = null;
    this.host = "";
    this.connectPort = 5555;
    this.liveness = "idle";
    this.setReconnectFailure("");
    this.clearDeviceData();
  }

  /// After a connect or silent redial settles: keep `deviceEpoch` only when
  /// the live TV is verifiably the one that was there before.
  private settleDevice(previousSerial: string, previousId: string | undefined, mismatch: boolean): void {
    const device = this.connectedDevice;
    const id = normalizeHardwareId(device?.properties?.serial_number);
    const same = !mismatch && device !== null && device.serial === previousSerial && previousId !== undefined && id === previousId;
    if (!same) this.deviceEpoch += 1;
  }

  private clearDeviceData(): void {
    ++this.healthGeneration;
    ++this.bloatGeneration;
    this.bloatConnectionGeneration = 0;
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

  /// After a connect that didn't replace the connection: live if the backend
  /// still holds the previous TV, lost (keeping its profile) if it was
  /// evicted, idle if there was none.
  private async restoreCurrentLiveness(generation: number): Promise<void> {
    try {
      const listed = await this.refreshDevices(generation);
      if (generation !== this.connectionGeneration) return;
      this.liveness = listed ? "live" : this.connectedDevice ? "lost" : "idle";
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

  /// A settled change of the phone's default network (lib/networkWatch.ts).
  /// A live session is probed. A lost one gets one more silent redial once a
  /// network is back, because its first redial usually ran while there was no
  /// route to the TV; a loss alone never spends one.
  async onNetworkChange(reason: unknown): Promise<void> {
    if (!this.connectedDevice) return;
    if (this.liveness !== "lost") return this.checkLiveness();
    if (reason === "lost" || this.reconnectIdentityChanged) return;
    this.recoveryAttempted = false;
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
      const lostSerial = this.serial;
      const lostId = normalizeHardwareId(this.connectedDevice?.properties?.serial_number);
      try {
        const result = await api.wirelessConnect(this.host, this.connectPort, generation, lostId);
        if (generation !== this.connectionGeneration) return false;
        if (result.identity_mismatch) {
          this.setReconnectFailure(result.message, result.identity_mismatch === "changed");
          this.deviceEpoch += 1;
        }
        if (result.ok) {
          const live = await this.deviceAtEndpoint(this.host, this.connectPort);
          if (generation !== this.connectionGeneration) return false;
          if (live) {
            const refusal = lostId ? identityFailure(lostId, live) : "";
            if (refusal) {
              if (await this.rejectConnection(generation)) {
                this.setReconnectFailure(refusal, !!normalizeHardwareId(live.properties?.serial_number));
              }
              return false;
            }
            this.connectedDevice = live;
            // A silent recovery redials the same host:port, exactly like an
            // explicit reconnect -- so it needs the same identity check before
            // being trusted as live. Without it, a different id-less TV that
            // took over this address mid-session would be silently treated as
            // "the" saved TV reconnecting (#154).
            const mismatch = this.rememberCurrentDevice();
            const recoveredId = normalizeHardwareId(this.connectedDevice?.properties?.serial_number);
            // Cached stats describe the TV that was lost; another TV at this
            // address must not show them as its own.
            if (mismatch || recoveredId !== lostId) this.clearDeviceData();
            this.settleDevice(lostSerial, lostId, mismatch);
            this.liveness = "live";
            this.setReconnectFailure("");
            this.recoveryAttempted = false;
            return true;
          }
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

  /// Drop the connection to a TV that was just told to reboot. A TV usually
  /// drops the socket as it goes down, which starts a silent redial and moves
  /// `generation`, so this is keyed on the TV (serial and `deviceEpoch`): a
  /// newer session on another TV is never disconnected.
  async finishReboot(target: { serial: string; epoch: number }): Promise<boolean> {
    if (target.serial !== this.serial || target.epoch !== this.deviceEpoch) return false;
    return this.disconnect();
  }

  // ---- Entitlement ----

  async loadEntitlement(): Promise<void> {
    try {
      this.entitlement = await api.getEntitlement();
    } catch {
      // Leave as-is (defaults to free) if the command fails.
    }
  }

  async activatePro(key: string): Promise<Entitlement> {
    this.entitlement = await api.activateLicense(key);
    return this.entitlement;
  }

  // ---- Shared health cache ----

  async loadHealth(force = false): Promise<void> {
    // Only a load for the current device can make another one redundant. A
    // load whose device vanished mid-flight is history and must not block.
    if (this.healthLoading && this.healthLoadingSerial === this.serial) return;
    if (this.healthLoaded && !force && this.healthError === "") return;
    if (!this.serial) return;
    const serial = this.serial;
    const generation = ++this.healthGeneration;
    this.healthLoadingSerial = serial;
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
      if (generation === this.healthGeneration) {
        this.healthLoading = false;
        if (serial === this.serial) this.healthLoaded = true;
      }
    }
  }

  /// Mark health stale after a mutating action (trim, reboot, enable/disable)
  /// without clearing the on-screen data — the next visit refetches.
  invalidateHealth(): void {
    this.healthLoaded = false;
  }

  /// Count enabled and installed recommended apps for the connected device.
  /// Every requested package must have a valid state before either count is
  /// published; incomplete reads are unavailable, never a successful zero.
  async loadBloat(force = false): Promise<void> {
    if (this.bloatLoading && this.bloatConnectionGeneration === this.generation) return;
    if (this.bloatLoaded && !force && this.bloatError === "") return;
    if (!this.connectedDevice) return;
    const serial = this.serial;
    const deviceType = this.connectedDevice.device_type;
    const connectionGeneration = this.generation;
    const request = ++this.bloatGeneration;
    this.bloatConnectionGeneration = connectionGeneration;
    this.bloatLoading = true;
    this.bloatError = "";
    try {
      const catalog = await api.appListForDevice(deviceType);
      const defaults = catalog.filter((a) => a.default_optimize);
      let enabled = 0;
      let installed = 0;
      if (defaults.length > 0) {
        const packages = defaults.map((entry) => entry.package);
        const states = await api.packageStates(
          serial,
          packages,
        );
        const complete = packages.every((pkg) => {
          if (!Object.prototype.hasOwnProperty.call(states, pkg)) return false;
          const state = states[pkg];
          return state === "enabled" || state === "disabled" || state === "missing";
        });
        if (!complete) throw new Error("Couldn't read status for all recommended apps. Try again.");
        enabled = packages.filter((pkg) => states[pkg] === "enabled").length;
        installed = packages.filter(
          (pkg) => states[pkg] === "enabled" || states[pkg] === "disabled",
        ).length;
      }
      if (
        request !== this.bloatGeneration ||
        connectionGeneration !== this.generation ||
        serial !== this.serial
      ) return;
      this.bloatCount = enabled;
      this.bloatTotal = installed;
    } catch (e) {
      if (
        request !== this.bloatGeneration ||
        connectionGeneration !== this.generation ||
        serial !== this.serial
      ) return;
      this.bloatError = String(e);
    } finally {
      // A load abandoned by a reconnect still has to release the flag, or the
      // card says "Checking…" forever with Retry hidden. Only a newer load
      // owns it.
      if (request === this.bloatGeneration) {
        this.bloatLoading = false;
        if (connectionGeneration === this.generation && serial === this.serial) this.bloatLoaded = true;
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
