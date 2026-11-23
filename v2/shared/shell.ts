/// The expert shell, shared by the desktop Shell tab and the phone's Shell
/// screen: the result shape `run_shell` returns, the read-only presets, and
/// the rules for stored bookmarks. Both apps run the same core command, so the
/// presets and the way a result is read must not drift between them.

export interface ShellRunResult {
  stdout: string;
  stderr: string;
  exit_code: number | null;
  termination: "completed" | "output_limit" | "timeout";
  /// The safety gate refused it; nothing was sent to the device.
  blocked: boolean;
  blocked_reason: string | null;
}

export interface ShellBookmark {
  label: string;
  command: string;
}

/// Presets only fill the command box; nothing runs until Run is pressed.
export const SHELL_PRESETS: readonly ShellBookmark[] = [
  { label: "All packages", command: "pm list packages --user 0" },
  { label: "All packages with paths", command: "pm list packages -f --user 0" },
  { label: "System packages", command: "pm list packages -s --user 0" },
  { label: "Enabled packages", command: "pm list packages -e --user 0" },
  { label: "Disabled packages", command: "pm list packages -d --user 0" },
  { label: "Third-party packages", command: "pm list packages -3 --user 0" },
  { label: "Uninstall a package", command: "pm uninstall --user 0 PACKAGE_NAME" },
  { label: "Current launcher", command: "cmd shortcut get-default-launcher" },
  { label: "Device properties", command: "getprop | grep ro.product" },
  { label: "Display state", command: "dumpsys display | grep -E 'mBaseDisplayInfo|modeId'" },
  { label: "Running processes", command: "ps -A -o NAME,PID,RSS --sort=-RSS | head -20" },
  { label: "Uptime", command: "uptime" },
];

/// Stored bookmarks reach this unchecked (hand-edited or older payloads), so
/// each row is validated rather than trusted into the UI.
export function parseShellBookmarks(raw: unknown): ShellBookmark[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .filter(
      (b): b is ShellBookmark =>
        !!b && typeof b.label === "string" && typeof b.command === "string",
    )
    .map((b) => ({ label: b.label.trim(), command: b.command.trim() }))
    .filter((b) => b.label && b.command);
}

/// How many lines came back across both streams, for the output footer.
export function shellOutputLines(result: ShellRunResult): number {
  if (result.blocked) return 0;
  const count = (text: string) => (text.trim() ? text.trimEnd().split("\n").length : 0);
  return count(result.stdout) + count(result.stderr);
}

/// What the person must know when a run was cut short: the output is partial
/// and the command may already have changed the TV. Null for a finished run.
export function shellStopNotice(result: ShellRunResult): string | null {
  if (result.blocked || result.termination === "completed") return null;
  const why = result.termination === "timeout"
    ? "Stopped after 30 seconds."
    : "Stopped at the output limit.";
  return `${why} Partial output is shown below. The command may already have changed the device.`;
}

/// Both streams, for Copy: on-device tools split their output across them.
export function shellOutputText(result: ShellRunResult): string {
  return [result.stdout, result.stderr].filter((part) => part.trim()).join("\n");
}
