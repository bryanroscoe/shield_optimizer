/// Plain-language guidance for the ways `adb pair` and `adb connect` fail.
///
/// adb reports these as terse transport errors ("protocol fault (couldn't read
/// status message): Undefined error: 0" is what a mistyped IP looks like), so
/// the raw text stays available behind a Details disclosure and this supplies
/// the sentence a person can act on. An error it does not recognise gets no
/// guidance rather than a guess.

export type AdbOperation = "pair" | "connect";

/// What the macOS Local Network probe (`local_network_access`) last saw.
export type LocalNetworkStatus = "allowed" | "blocked" | "unknown" | "no_network" | "not_applicable";

/// Whether a failure should point at macOS Local Network privacy: "blocked"
/// when the probe saw macOS block this app, "possible" when it is a likely
/// cause the probe could not confirm or rule out, null when it does not apply.
export type LocalNetworkHint = "blocked" | "possible" | null;

export interface ExplainedFailure {
  /// What to do about it, or null when the failure is not one we recognise.
  summary: string | null;
  /// adb's own words, trimmed.
  raw: string;
  /// Show the "Open Local Network settings" button beside it.
  localNetwork: LocalNetworkHint;
}

export const LOCAL_NETWORK_BLOCKED = "macOS is blocking ATV Optimizer from your local network.";

const NO_ROUTE = /no route to host|host is unreachable|ehostunreach/i;

export function isNoRoute(raw: string): boolean {
  return NO_ROUTE.test(String(raw ?? ""));
}

/// "No route to host" is what a macOS app without the Local Network privilege
/// gets for every LAN address. Only the probe's own evidence makes it
/// "blocked". A probe that sent fine, found no network, or isn't on macOS
/// (or hasn't answered) adds nothing.
export function localNetworkHint(raw: string, status: LocalNetworkStatus | null): LocalNetworkHint {
  if (!isNoRoute(raw)) return null;
  if (status === "blocked") return "blocked";
  if (status === "unknown") return "possible";
  return null;
}

const LOCAL_NETWORK_SENTENCE: Record<"blocked" | "possible", string> = {
  blocked: `${LOCAL_NETWORK_BLOCKED} Turn ATV Optimizer on under Local Network in System Settings, then quit and reopen the app.`,
  possible:
    "macOS may also be blocking local network access for ATV Optimizer, or a VPN or firewall " +
    "app may be in the way.",
};

/// The sentence to add for a hint, or null for none.
export function localNetworkSentence(hint: LocalNetworkHint): string | null {
  return hint ? LOCAL_NETWORK_SENTENCE[hint] : null;
}

const WRONG_PIN = /wrong password|incorrect (pin|code|password)|bad (pin|code)/i;
const DIALOG_GONE =
  /connection was dropped|unable to start pairing client|pairing (was )?(cancel|aborted|expired)|connection reset|broken pipe/i;
const UNREACHABLE =
  /protocol fault|connection refused|no route to host|network is unreachable|host is (down|unreachable)|timed out|timeout|couldn't read status|could not connect|cannot connect|failed to connect|unable to connect|no such host|nodename nor servname|name or service not known/i;

/// Strip the wrapper the desktop command adds ("adb pair: adb process failed
/// (exit code Some(1)): ") so Details shows what adb actually said first.
function clean(raw: string): string {
  return raw
    .replace(/^adb (pair|connect): /i, "")
    .replace(/^adb process failed \(exit code [^)]*\)\)?: /i, "")
    .trim();
}

export function explainAdbFailure(
  op: AdbOperation,
  target: string,
  raw: string,
  localNetwork: LocalNetworkStatus | null = null,
): ExplainedFailure {
  const text = clean(String(raw ?? ""));
  const hint = localNetworkHint(text, localNetwork);
  const where = target.trim() || "that address";
  let summary: string | null = null;

  if (op === "pair" && WRONG_PIN.test(text)) {
    summary =
      "The PIN didn't match, or the pairing dialog closed before it was entered. Check the " +
      "6-digit code on the device, or open the pairing dialog again for a fresh code and port.";
  } else if (op === "pair" && DIALOG_GONE.test(text)) {
    summary =
      "The pairing dialog closed or expired. Open \"Pair device with pairing code\" on the " +
      "device again and use the new port and PIN it shows.";
  } else if (UNREACHABLE.test(text)) {
    summary =
      op === "pair"
        ? `Couldn't reach ${where}. Check the IP and pairing port match the pairing dialog, ` +
          "that it's still open, and that both are on the same Wi-Fi."
        : `Couldn't reach ${where}. Check the TV's current IP and that both devices are on the same local network. ` +
          "On SHIELD, enable Network debugging. For Wireless debugging, use the IP and port from the " +
          "main Wireless debugging screen (not the pairing dialog).";
  }

  if (hint === "blocked") {
    summary = `Couldn't reach ${where}. ${LOCAL_NETWORK_SENTENCE.blocked}`;
  } else if (hint === "possible" && summary) {
    summary = `${summary} ${LOCAL_NETWORK_SENTENCE.possible}`;
  }

  return { summary, raw: text, localNetwork: hint };
}

function ipv4Octets(host: string): number[] | null {
  const parts = host.trim().split(".");
  if (parts.length !== 4) return null;
  const octets = parts.map((p) => (/^\d{1,3}$/.test(p) ? Number(p) : NaN));
  return octets.every((o) => Number.isInteger(o) && o >= 0 && o <= 255) ? octets : null;
}

/// The host part of `host:port`, or the input itself when there is no port.
export function hostOf(address: string): string {
  const a = address.trim();
  if (a.startsWith("[")) {
    const end = a.indexOf("]");
    return end > 0 ? a.slice(0, end + 1) : a;
  }
  const i = a.lastIndexOf(":");
  return i > 0 ? a.slice(0, i) : a;
}

/// A gentle "typo?" when the typed IPv4 host is not on the /24 of the local
/// interface this computer would use to reach it. Only a warning: VPNs and
/// routed networks are real, so it never blocks. Returns null when either
/// side is not a readable IPv4 address, because unknown claims nothing.
export function subnetWarning(host: string, localAddress: string | null): string | null {
  const target = ipv4Octets(host);
  const local = localAddress ? ipv4Octets(localAddress) : null;
  if (!target || !local) return null;
  if (target[0] === local[0] && target[1] === local[1] && target[2] === local[2]) return null;
  const net = `${local[0]}.${local[1]}.${local[2]}.x`;
  return `This computer is on ${net}, and ${host.trim()} isn't on it. Typo?`;
}
