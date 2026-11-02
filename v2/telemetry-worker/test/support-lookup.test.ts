import { describe, expect, it } from "vitest";
import { formatSupportReport, type Report } from "../scripts/support-lookup-lib";

describe("support lookup report", () => {
  it("prints the install, its TVs, uploads, diagnostics and log excerpts", () => {
    const report: Report = {
      query: "7D0B-0B9E",
      matches: 1,
      truncated: false,
      installs: [
        {
          install_id: "7d0b0b9e-1f43-4e43-9b47-3a5d6f0f9c11",
          short: "7D0B-0B9E",
          first_seen: "2026-11-01T10:00:00.000Z",
          last_seen: "2026-11-02T14:00:03.000Z",
          events: 2,
          kinds: { device_seen: 1, diagnostic: 1 },
          apps: ["desktop"],
          app_versions: ["2.4.2"],
          os: ["windows"],
          devices: [{ manufacturer: "NVIDIA", model: "SHIELD Android TV", android_release: "11", profile: "shield", events: 1 }],
          error_codes: [{ code: "scan_empty", count: 1 }],
          recent: [
            {
              received_at: "2026-11-02T14:00:03.000Z",
              app: "desktop",
              app_version: "2.4.2",
              os: "windows",
              kind: "diagnostic",
              device: null,
              action: "scan",
              ok: null,
              error_code: "scan_empty",
              message: null,
              diag: { sweep: { probed: 254, open: 0 } },
              log_excerpt: "line one\nline two",
            },
          ],
          uploads: [{ id: 12, received_at: "2026-11-02T14:05:00.000Z", app_version: "2.4.2", os: "windows", reports_on: false, bytes: 2048 }],
        },
      ],
    };
    const text = formatSupportReport(report);
    expect(text).toContain("Support ID 7D0B-0B9E: 1 install matched");
    expect(text).toContain("Install 7d0b0b9e-1f43-4e43-9b47-3a5d6f0f9c11 (7D0B-0B9E)");
    expect(text).toContain("TV          NVIDIA SHIELD Android TV, Android 11, shield (1 events)");
    expect(text).toContain("Upload #12  2026-11-02 14:05:00Z  2.0 KB  2.4.2 on windows, reports off");
    expect(text).toContain("scripts/support-lookup.sh --upload 12");
    expect(text).toContain("2026-11-02 14:00:03Z  diagnostic  scan  scan_empty  (desktop 2.4.2, windows)");
    expect(text).toContain('diag: {"sweep":{"probed":254,"open":0}}');
    expect(text).toContain("        | line two");
  });

  it("says so when nothing matched", () => {
    const text = formatSupportReport({ query: "AAAA-BBBB", matches: 0, truncated: false, installs: [] });
    expect(text).toContain("0 installs matched");
    expect(text).toContain("Nothing stored under that ID");
  });
});
