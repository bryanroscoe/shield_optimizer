export type Safety =
  | { kind: "never_disable"; reason: string }
  | { kind: "caution"; reason: string }
  | { kind: "unknown"; reason: string };

const unavailableMessage =
  "Safety information is unavailable. Retry before disabling or uninstalling.";

export function parseSafety(value: unknown): Safety {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(unavailableMessage);
  }

  const payload = value as Record<string, unknown>;
  const { kind, reason } = payload;
  if (
    (kind !== "never_disable" && kind !== "caution" && kind !== "unknown") ||
    typeof reason !== "string" ||
    reason.trim().length === 0
  ) {
    throw new Error(unavailableMessage);
  }

  return { kind, reason };
}
