/** Shapes and checks for `read_device_logs`, shared by the desktop Logs tab and the phone's Logs screen. */
export interface DeviceLogOptions {
  priority: "verbose" | "debug" | "info" | "warning" | "error" | "fatal";
  lines: number;
  tag: string | null;
  package: string | null;
}

export interface DeviceLogResult {
  output: {
    stdout: string;
    stderr: string;
    exit_code: number | null;
    termination: "completed" | "output_limit" | "timeout";
  };
  package: string | null;
  pid: number | null;
}

export const LOG_PRIORITIES: { value: DeviceLogOptions["priority"]; label: string }[] = [
  { value: "verbose", label: "Verbose" },
  { value: "debug", label: "Debug" },
  { value: "info", label: "Info" },
  { value: "warning", label: "Warning" },
  { value: "error", label: "Error" },
  { value: "fatal", label: "Fatal" },
];

export const LOG_LINE_CHOICES = [100, 200, 500, 1000] as const;

/** Trimmed request options; blank filters mean "none". */
export function logOptions(priority: DeviceLogOptions["priority"], lines: number, tag: string, pkg: string): DeviceLogOptions {
  return { priority, lines, tag: tag.trim() || null, package: pkg.trim() || null };
}

/**
 * A completed read that exited non-zero is a failure, not an empty snapshot.
 * A timed-out or output-limited read is partial and still shown, labelled.
 */
export function checkLogRead(data: DeviceLogResult): DeviceLogResult {
  if (data.output.exit_code !== 0 && data.output.termination === "completed") {
    throw new Error(data.output.stderr || `logcat exited with code ${data.output.exit_code ?? "unknown"}`);
  }
  return data;
}

/** Local, case-insensitive line filter over one snapshot. */
export function filterLogLines(text: string, search: string): string {
  if (!search) return text;
  const needle = search.toLowerCase();
  return text.split("\n").filter(line => line.toLowerCase().includes(needle)).join("\n");
}

/** Why a snapshot is partial, or null when it is complete. */
export function partialReason(data: DeviceLogResult): string | null {
  switch (data.output.termination) {
    case "completed": return null;
    case "timeout": return "the read timed out";
    default: return "the output limit was reached";
  }
}
