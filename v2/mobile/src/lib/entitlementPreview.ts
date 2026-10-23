// Isolated simulation: never connected to real session entitlement or payment APIs.
export type Phase = "idle" | "buying" | "restoring" | "pending" | "verifying" | "verified" | "canceled" | "error" | "revoked";
export type Preview = { request: number; phase: Phase; legacyPro: boolean; playPro: boolean; acknowledged: boolean };
export type Event = { type: "begin"; request: number; operation: "buy" | "restore" } | { type: "pending" | "purchased" | "verified" | "acknowledged" | "canceled" | "error"; request: number } | { type: "revoked" } | { type: "legacy"; valid: boolean };
export const initialPreview = (): Preview => ({ request: 0, phase: "idle", legacyPro: false, playPro: false, acknowledged: false });
export const previewIsPro = (s: Preview): boolean => s.legacyPro || s.playPro;
export function reducePreview(s: Preview, e: Event): Preview {
  if (e.type === "legacy") return { ...s, legacyPro: e.valid };
  // A future authenticated verifier, not a store/client callback, owns these verdicts.
  if (e.type === "revoked") return { ...s, request: s.request + 1, phase: "revoked", playPro: false, acknowledged: false };
  if (e.type === "begin") return e.request <= s.request ? s : { ...s, request: e.request, phase: e.operation === "buy" ? "buying" : "restoring" };
  if (e.request !== s.request) return s;
  if (e.type === "pending" && ["buying", "restoring"].includes(s.phase)) return { ...s, phase: "pending" };
  if (e.type === "purchased" && ["buying", "restoring", "pending"].includes(s.phase)) return { ...s, phase: "verifying" };
  if (e.type === "verified" && s.phase === "verifying") return { ...s, phase: "verified", playPro: true, acknowledged: false };
  if (e.type === "acknowledged" && s.phase === "verified" && s.playPro) return { ...s, acknowledged: true };
  if (["canceled", "error"].includes(e.type) && ["buying", "restoring", "pending", "verifying"].includes(s.phase)) return { ...s, phase: e.type as Phase };
  return s;
}
export function previewMessage(s: Preview): string {
  if (s.phase === "pending") return "Payment pending — no new access granted.";
  if (s.phase === "verifying") return "Verifying purchase — client success alone does not unlock Pro.";
  if (s.phase === "error") return "Request failed. Existing verified access is unchanged.";
  if (s.phase === "canceled") return "Purchase canceled. No new access granted.";
  if (s.phase === "revoked") return "Play access revoked. Free recovery remains available.";
  if (previewIsPro(s)) return s.playPro && !s.acknowledged ? "Pro verified — acknowledgment pending." : "Pro verified.";
  return "Free compatibility checks and recovery are available.";
}
