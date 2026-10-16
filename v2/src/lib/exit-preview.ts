import { invoke } from "@tauri-apps/api/core";

export const exitPreviewEnabled = import.meta.env.DEV && import.meta.env.VITE_ADB_EXIT_PREVIEW === "1";
export type ExitPolicy = "auto" | "always_stop" | "always_leave_running";
export type ExitPreviewAction =
  | { type: "read" | "request_quit" | "cancel" | "restart_simulation" | "updater_restart" }
  | { type: "save"; policy: ExitPolicy }
  | { type: "confirm"; policy: ExitPolicy; remember: boolean };
export type ExitPreviewSnapshot = {
  preference: { version: number; policy: ExitPolicy; remembered: boolean };
  phase: string;
  prompt_generation: number;
  prompt_open: boolean;
  admission_open: boolean;
  stop_available: false;
  message: string;
  storage_warning: string | null;
};
export const previewPolicies: { value: ExitPolicy; title: string; detail: string }[] = [
  { value: "always_leave_running", title: "Always leave running", detail: "Default. Keep shared ADB available to other apps and devices." },
  { value: "auto", title: "Auto", detail: "Shared ownership cannot be verified, so Auto leaves ADB running." },
  { value: "always_stop", title: "Always stop — unavailable", detail: "Stopping shared ADB disconnects other apps and all attached devices. The stop executor is not enabled; this choice cannot be saved." },
];
export function exitPreview(action: ExitPreviewAction): Promise<ExitPreviewSnapshot> {
  if (!exitPreviewEnabled) return Promise.reject(new Error("Development preview is disabled."));
  return invoke<ExitPreviewSnapshot>("exit_settings_preview", { action });
}
