import { describe, expect, it } from "vitest";
import { MESSAGE_MAX, redactMessage, redactShortField } from "../src/redact";

describe("redactMessage", () => {
  const cases: Array<[string, string, string]> = [
    ["IPv4", "failed to connect to 192.168.42.71", "failed to connect to [ip]"],
    ["IPv4 with port", "connect 10.0.0.5:5555 refused", "connect [ip] refused"],
    ["IPv6", "route fe80::1c2d:3e4f:5a6b:7c8d unreachable", "route [ip] unreachable"],
    ["IPv6 full", "peer 2001:0db8:85a3:0000:0000:8a2e:0370:7334 gone", "peer [ip] gone"],
    ["IPv6 loopback", "bound to ::1 only", "bound to [ip] only"],
    ["host:port", "adb server localhost:5037 not running", "adb server [host] not running"],
    ["MAC", "wlan0 a4:5e:60:c1:22:9b down", "wlan0 [mac] down"],
    ["MAC dashed", "eth A4-5E-60-C1-22-9B", "eth [mac]"],
    ["email", "licensed to someone@example.com", "licensed to [email]"],
    ["serial digits", "device 0323220012345 offline", "device [serial] offline"],
    ["serial mixed", "unauthorized 3A091FDJH000AB", "unauthorized [serial]"],
    ["adb mDNS name", "found adb-0323220012345-AbCdEf._adb-tls-connect._tcp", "found [host]"],
    ["mdns host", "resolved living-room-tv.local ok", "resolved [host] ok"],
    ["POSIX path", "cannot open /Users/bryan/Library/adb/key", "cannot open [path]"],
    ["home path", "wrote ~/snapshots/a.json", "wrote [path]"],
    ["Windows path", "missing C:\\Users\\Bryan\\AppData\\adb.exe", "missing [path]"],
    ["UNC path", "share \\\\NAS\\backups\\tv", "share [path]"],
    ["URL", "GET https://example.com/a?b=c failed", "GET [url] failed"],
  ];
  for (const [name, input, expected] of cases) {
    it(`redacts ${name}`, () => expect(redactMessage(input)).toBe(expected));
  }

  it("keeps ordinary error text and a single package name", () => {
    expect(redactMessage("disable failed for com.google.android.tvlauncher: exit 1")).toBe(
      "disable failed for com.google.android.tvlauncher: exit 1",
    );
    expect(redactMessage("timed out after 30s waiting for adb")).toBe("timed out after 30s waiting for adb");
  });

  it("treats three or more package names as a package list", () => {
    expect(redactMessage("installed: com.a.one, com.b.two, org.c.three")).toBe(
      "installed: [package], [package], [package]",
    );
    expect(redactMessage("installed: com.foo, org.bar, tv.baz")).toBe("installed: [package], [package], [package]");
  });

  it("strips control characters and clamps to the limit", () => {
    expect(redactMessage("line1\nline2\u0000")).toBe("line1 line2");
    const long = "x ".repeat(1000);
    expect(Array.from(redactMessage(long)).length).toBeLessThanOrEqual(MESSAGE_MAX);
  });

  it("does not split surrogate pairs when clamping", () => {
    const out = redactMessage("📺".repeat(600));
    expect(Array.from(out)).toHaveLength(MESSAGE_MAX);
    expect(out.endsWith("📺")).toBe(true);
  });
});

describe("redactShortField", () => {
  it("keeps real device strings", () => {
    for (const v of ["NVIDIA", "SHIELD Android TV", "mdarcy", "AFTMM", "QM7L Pro", "macos 15.1", "TCL"]) {
      expect(redactShortField(v)).toBe(v);
    }
  });
  it("replaces values that carry identifiers", () => {
    expect(redactShortField("192.168.1.20:5555")).toBe("[redacted]");
    expect(redactShortField("0323220012345")).toBe("[redacted]");
    expect(redactShortField("bryan@example.com")).toBe("[redacted]");
    expect(redactShortField("/home/bryan")).toBe("[redacted]");
  });
});
