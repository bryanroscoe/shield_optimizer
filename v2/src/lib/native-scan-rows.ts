import type { Device, ScanConnectionProgress } from "$lib/types";

export type DeviceRow = Device & {
  discoveryState?: "connecting" | "approval" | "failed";
  discoveryError?: string;
};

export function nativeScanRows(devices: Device[], progress: ScanConnectionProgress): DeviceRow[] {
  const rows = new Map<string, DeviceRow>(devices.map(device => [device.serial, device]));
  const approvals = new Set(progress.awaiting_authorization ?? []);
  const add = (serial: string, state: DeviceRow["discoveryState"], message?: string) => {
    const existing = rows.get(serial);
    if (existing?.status === "device") return;
    rows.set(serial, {
      id: 0, serial, name: serial, model: "", device_type: "unknown", tv_evidence: "unknown",
      connection: "network", properties: null, ...existing,
      status: state === "approval" ? "unauthorized" : "offline",
      discoveryState: state, discoveryError: message,
    });
  };
  for (const failure of progress.failed) add(failure.address, "failed", failure.message);
  for (const address of progress.pending) add(address, approvals.has(address) ? "approval" : "connecting");
  return [...rows.values()];
}
