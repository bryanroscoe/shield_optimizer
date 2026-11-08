import type { Device, ScanConnectionProgress } from "$lib/types";

/// "not_allowed": the TV answered but hasn't allowed this computer; nothing
/// was shown on it. "approval": Request approval is waiting for the person to
/// choose Allow on the TV.
export type DeviceRow = Device & {
  discoveryState?: "connecting" | "approval" | "not_allowed" | "failed" | "offline";
  discoveryError?: string;
};

/// Rows for the built-in engine: the listed devices plus every endpoint a scan
/// or request is working on, each under its exact endpoint. Nothing is merged
/// on an address: an unauthorized TV has no verified hardware id.
export function nativeScanRows(devices: Device[], progress: ScanConnectionProgress): DeviceRow[] {
  const rows = new Map<string, DeviceRow>(devices.map(device => [device.serial,
    device.status === "offline" && device.connection === "network" && !device.properties
      ? { ...device, discoveryState: "offline" }
      : device,
  ]));
  const add = (serial: string, state: DeviceRow["discoveryState"], message?: string) => {
    const existing = rows.get(serial);
    if (existing?.status === "device") return;
    // A quiet retry of a TV that hasn't allowed this computer keeps its row.
    if (state === "connecting" && existing?.status === "unauthorized") return;
    const unauthorized = state === "approval" || state === "not_allowed";
    rows.set(serial, {
      id: 0, serial, name: serial, model: "", device_type: "unknown", tv_evidence: "unknown",
      connection: "network", properties: null, ...existing,
      status: unauthorized ? "unauthorized" : "offline",
      discoveryState: state, discoveryError: message,
    });
  };
  for (const failure of progress.failed) add(failure.address, "failed", failure.message);
  for (const address of progress.pending) add(address, "connecting");
  for (const address of progress.not_allowed ?? []) add(address, "not_allowed");
  for (const address of progress.awaiting_authorization ?? []) add(address, "approval");
  return [...rows.values()];
}
