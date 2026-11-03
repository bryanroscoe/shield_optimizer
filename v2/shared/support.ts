/// Device support tiers, mirroring `crates/core/src/engine/support.rs`.
///
/// A tier is informational. It never relaxes or bypasses a safety gate, and
/// `not_supported` never blocks recovery or re-enabling an app: it only adds
/// a stronger warning before a destructive action. Copy here must not claim
/// more than the tier does: `unknown` means nobody has verified the TV yet,
/// not that it works and not that it doesn't.

export type SupportTier = "unknown" | "not_supported" | "beta" | "supported" | "fully_supported";

export interface SupportInfo {
  tier: SupportTier;
  note: string | null;
}

const TIERS: readonly SupportTier[] = [
  "unknown",
  "not_supported",
  "beta",
  "supported",
  "fully_supported",
];

export const UNKNOWN_SUPPORT: SupportInfo = { tier: "unknown", note: null };

/// Anything malformed or missing (an older backend, an unauthorized TV) is
/// `unknown`, which claims nothing.
export function parseSupport(raw: unknown): SupportInfo {
  if (!raw || typeof raw !== "object") return UNKNOWN_SUPPORT;
  const r = raw as { tier?: unknown; note?: unknown };
  const tier = TIERS.includes(r.tier as SupportTier) ? (r.tier as SupportTier) : "unknown";
  const note = typeof r.note === "string" && r.note.trim() ? r.note.trim() : null;
  return { tier, note: tier === "unknown" ? null : note };
}

export const TIER_LABEL: Record<SupportTier, string> = {
  unknown: "Not verified",
  not_supported: "Not supported",
  beta: "Beta",
  supported: "Supported",
  fully_supported: "Fully supported",
};

/// One line for the device header or card.
export function supportLine(info: SupportInfo): string {
  switch (info.tier) {
    case "not_supported":
      return `This TV isn't supported: ${info.note ?? "the app can't make its changes on it."}`;
    case "beta":
      return (
        "Beta: this TV may not work fully. " +
        (info.note ? `${info.note} ` : "") +
        "Reports from it help us improve support."
      );
    case "supported":
      return info.note ?? "This TV is supported.";
    case "fully_supported":
      return info.note ?? "This TV is fully supported.";
    default:
      return "We haven't verified this TV model yet.";
  }
}

/// Added to an error message when the TV's tier could explain the failure.
/// Supported tiers add nothing: blaming the TV there would be a guess.
export function errorTierHint(info: SupportInfo | null | undefined): string | null {
  switch (info?.tier) {
    case "not_supported":
      return `This TV isn't supported${info.note ? ` (${info.note.replace(/\.$/, "")})` : ""}, so failures like this are expected.`;
    case "beta":
      return "This TV has beta support, so this may be a compatibility problem.";
    case "unknown":
      return "This TV model hasn't been verified yet, so this may be a compatibility problem.";
    default:
      return null;
  }
}

/// Destructive actions (disable, uninstall, optimize, snapshot apply,
/// launcher changes) add this warning on an unsupported TV. Never shown on
/// enable/restore paths, which must stay unobstructed.
export function destructiveWarning(info: SupportInfo | null | undefined): string | null {
  if (info?.tier !== "not_supported") return null;
  return (
    "This TV isn't supported" +
    (info.note ? `: ${info.note}` : ".") +
    " Changes may fail or behave unexpectedly. Continue only if you know how to undo them."
  );
}
