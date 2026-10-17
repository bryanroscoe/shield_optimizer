export const playbackFixture = {
  video: [
    { label: "AV1", mime: "video/av01", advertised: true, software: false, acceleration_unknown: true },
    { label: "HEVC / H.265 with a deliberately long manufacturer configuration label", mime: "video/hevc", advertised: false, software: false, acceleration_unknown: false },
    { label: "VP9", mime: "video/x-vnd.on2.vp9", advertised: true, software: true, acceleration_unknown: false },
  ],
  hdr_types: ["HDR10", "HLG", "Dolby Vision", "UnrecognizedVendorHDRConfiguration012345678901234567890123456789"],
  modes: [{ width: 3840, height: 2160, fps: 59.94006, active: true }, { width: 1920, height: 1080, fps: 23.976024, active: false }, { width: 1280, height: 720, fps: 50, active: false }],
  audio: { mode: "manual", enabled_formats: ["Dolby Digital (AC-3)", "MPEG-H LC L4", "DTS UHD P1", "Encoding 999"], raw_formats: "5,26,27,999" },
  match_content_frame_rate: "vendor-policy-999",
  verdicts: [
    { level: "warn", title: "Passthrough depends on the full playback chain", detail: "Advertised configuration is not a playback test. A receiver, display, app or content mismatch can still prevent passthrough. Unknown formats must remain visible rather than being treated as supported." },
    { level: "info", title: "Long configuration identifier", detail: "VendorConfiguration012345678901234567890123456789012345678901234567890123456789" },
  ],
};
export const resourceFixture = {
  cpu_percent: 22.55, interval_ms: 1123,
  interfaces: [
    { name: "wlan0", rx_bytes_per_s: 2048, tx_bytes_per_s: 1024 },
    { name: "tun0-vendor-interface-012345678901234567890123456789", rx_bytes_per_s: null, tx_bytes_per_s: 0 },
    { name: "eth0", rx_bytes_per_s: 1024 * 123456789.25, tx_bytes_per_s: 512 },
  ],
};
