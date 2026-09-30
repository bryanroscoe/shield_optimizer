/// The hardware serial (`ro.serialno`) a Wireless debugging mDNS instance name
/// embeds: `adb-<serial>-<random suffix>`. Same rule as the Rust
/// `instance_serial`: anything else, or an empty or `unknown` serial, is null.
export function instanceSerial(instance: string | null | undefined): string | null {
  if (!instance || !instance.startsWith("adb-")) return null;
  const rest = instance.slice(4);
  const cut = rest.lastIndexOf("-");
  if (cut <= 0 || cut === rest.length - 1) return null;
  const serial = rest.slice(0, cut).trim();
  if (!serial || serial.toLowerCase() === "unknown") return null;
  return serial;
}
