export type HistoryKey = Pick<
  KeyboardEvent,
  "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "isComposing" | "defaultPrevented"
>;

/// Back/forward as in a browser: Cmd or Ctrl with an arrow or bracket, or Alt
/// with an arrow. In a text field those keys move the caret, and with a dialog
/// open the page underneath stays put, so both return 0.
export function historyStep(e: HistoryKey, inTextField: boolean, dialogOpen: boolean): -1 | 0 | 1 {
  if (e.defaultPrevented || e.shiftKey || e.isComposing || inTextField || dialogOpen) return 0;
  const mod = e.metaKey || e.ctrlKey;
  if ((mod && (e.key === "ArrowLeft" || e.key === "[")) || (e.altKey && e.key === "ArrowLeft")) return -1;
  if ((mod && (e.key === "ArrowRight" || e.key === "]")) || (e.altKey && e.key === "ArrowRight")) return 1;
  return 0;
}
