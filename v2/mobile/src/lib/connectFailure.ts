// What a failed connect can and can't mean for the TV's approval prompt.

import { classifyError, errorText, type ErrorCode } from "../../../shared/errors";

/// Failures where the phone never reached the TV's ADB service: nothing
/// answered, there was no route, or the port refused. No "Allow debugging?"
/// prompt can be showing then, so the screen must not send the user to the TV
/// to look for one.
const NEVER_REACHED: ReadonlySet<ErrorCode> = new Set(["connection_refused", "unreachable", "timeout"]);

export function approvalPromptMayExplain(error: unknown): boolean {
  return !NEVER_REACHED.has(classifyError(error));
}

export function connectFailureGuidance(error: unknown): string {
  const text = errorText(error).trim();
  return text.startsWith("The TV connection timed out during setup.") ? text : "";
}
