// Error codes are constants like pm_denied or adb_exit_255. More than four
// digits means a serial, IP or timestamp was folded in (serial_0323220012345,
// ip_192_168_1_5); license words mean licence data. Such codes are replaced
// with redacted_code, since codes are kept forever in agg_error_daily.
export const REDACTED_CODE = "redacted_code";

export function plausibleErrorCode(code: string): boolean {
  return (code.match(/\d/g) ?? []).length <= 4 && !/licen[cs]/.test(code);
}
