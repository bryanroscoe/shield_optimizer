/// The canonical safety verdict, mirroring `crates/core/src/engine/safety.rs`.
///
/// `safe` only ever comes from the reviewed app list — never as a fallback for
/// something we failed to recognise. That distinction is the point: a verdict
/// that defaults to safe is worthless, and one that defaults to unknown for
/// curated apps is equally worthless in the other direction, because then
/// nothing stands out when a genuinely unrecognised package appears.
export type SafetyKind = "never_disable" | "caution" | "safe" | "unknown";

/// Which reviewed source produced the verdict, so the UI can say where an
/// answer came from instead of asking the reader to trust a bare word.
export type SafetySource =
  | "protected_list"
  | "caution_list"
  | "reviewed_catalog"
  | "no_record";

export type Safety = { kind: SafetyKind; reason: string; source: SafetySource };

const unavailableMessage =
  "Safety information is unavailable. Retry before disabling or uninstalling.";

const KINDS: readonly SafetyKind[] = ["never_disable", "caution", "safe", "unknown"];
const SOURCES: readonly SafetySource[] = [
  "protected_list",
  "caution_list",
  "reviewed_catalog",
  "no_record",
];

/// Where a verdict came from, in words a person can act on.
export function safetySourceLabel(source: SafetySource): string {
  switch (source) {
    case "protected_list":
      return "On the do-not-disable list";
    case "caution_list":
      return "On the caution list";
    case "reviewed_catalog":
      return "From the reviewed app list";
    case "no_record":
      return "Not in any reviewed list";
  }
}

export function parseSafety(value: unknown): Safety {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(unavailableMessage);
  }

  const payload = value as Record<string, unknown>;
  const { kind, reason, source } = payload;
  if (
    !KINDS.includes(kind as SafetyKind) ||
    typeof reason !== "string" ||
    reason.trim().length === 0
  ) {
    throw new Error(unavailableMessage);
  }
  // Tolerate a missing source rather than failing closed on it: an absent
  // provenance degrades the detail panel, while refusing the whole verdict
  // would block a removal the backend already judged safe.
  const parsedSource = SOURCES.includes(source as SafetySource)
    ? (source as SafetySource)
    : "no_record";

  return { kind: kind as SafetyKind, reason, source: parsedSource };
}
