//! Playback-capability model — what the device can actually decode and output.
//!
//! Pure per architectural commitment #1: every function here is
//! `&str`/slice in, typed value out. The host layer (`commands::health`)
//! fetches the strings — `media_codecs*.xml`, `dumpsys display`, two
//! `settings get` reads — and hands them over.
//!
//! Two layers live here and they are deliberately separate:
//!
//! * **Derived** — [`parse_media_codecs`], [`video_formats`],
//!   [`surround_mode`] report only what the device itself advertises. They
//!   work on any Android TV and never guess.
//! * **Curated** — [`device_notes`] adds per-family knowledge that no device
//!   reports about itself (a Shield will not tell you its SoC has no AV1
//!   block). Notes are additive: they annotate a derived verdict, never
//!   replace or contradict it.
//!
//! Keeping the split explicit is what stops the curated half from rotting
//! into a second, unverifiable source of truth.

use serde::{Deserialize, Serialize};

use super::detection::DeviceType;

/// One decoder entry from `media_codecs*.xml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decoder {
    /// Component name, e.g. `OMX.Nvidia.h265.decode` / `c2.android.av1.decoder`.
    pub name: String,
    /// MIME type it decodes, e.g. `video/hevc`.
    pub mime: String,
    /// False for Android's bundled software components. Android reserves the
    /// `OMX.google.` and `c2.android.` prefixes for them, so the name is the
    /// canonical signal — the XML carries no explicit hardware flag.
    pub hardware: bool,
}

impl Decoder {
    /// Android's reserved prefixes for the platform's own software codecs.
    /// Anything else is a vendor component backed by silicon.
    fn is_software_name(name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        lower.starts_with("omx.google.")
            || lower.starts_with("c2.android.")
            || lower.starts_with("omx.ffmpeg.")
    }
}

/// A video format rolled up across every decoder that advertises it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoFormat {
    /// Display label, e.g. "HEVC / H.265".
    pub label: String,
    pub mime: String,
    /// At least one vendor (silicon-backed) decoder advertises it.
    pub hardware: bool,
    /// At least one platform software decoder advertises it.
    pub software: bool,
}

impl VideoFormat {
    /// Neither hardware nor software decoder present.
    pub fn unsupported(&self) -> bool {
        !self.hardware && !self.software
    }
}

/// One entry from `dumpsys display`'s `supportedModes`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayModeEntry {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// The mode the display is currently running.
    pub active: bool,
}

impl DisplayModeEntry {
    /// Whether this mode can present 24p film at its native cadence. Covers
    /// both 23.976 (NTSC-pulled 24) and true 24.000, which is what film
    /// sources actually ship as.
    pub fn is_film_rate(&self) -> bool {
        (23.9..=24.1).contains(&self.fps)
    }
}

/// `settings get global encoded_surround_output`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurroundMode {
    /// `0` — negotiate with the connected sink over HDMI/eARC.
    Auto,
    /// `1` — never pass encoded audio through; decode everything to PCM.
    Never,
    /// `2` — always pass through, whatever the sink reports.
    Always,
    /// `3` — pass through only the formats in the enabled-formats list.
    Manual,
    /// Key absent — Android treats this as Auto.
    Unset,
}

impl SurroundMode {
    pub fn from_raw(raw: Option<&str>) -> Self {
        match raw.map(str::trim) {
            Some("0") => Self::Auto,
            Some("1") => Self::Never,
            Some("2") => Self::Always,
            Some("3") => Self::Manual,
            _ => Self::Unset,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Never => "Never",
            Self::Always => "Always",
            Self::Manual => "Manual",
            Self::Unset => "Auto (unset)",
        }
    }
}

/// Encoded-audio passthrough configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioPassthrough {
    pub mode: SurroundMode,
    /// Friendly names of the formats in `encoded_surround_output_enabled_formats`.
    /// Only meaningful when `mode` is `Manual`.
    pub enabled_formats: Vec<String>,
    /// The raw comma-separated setting value, kept so the UI can show what is
    /// actually on the device rather than only our interpretation.
    pub raw_formats: Option<String>,
}

/// Severity of a verdict line. Drives colour only — no behavior depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictLevel {
    /// Working as it should.
    Good,
    /// Costs quality or will surprise the user. Actionable.
    Warn,
    /// Worth knowing, nothing to fix.
    Info,
}

/// One line of the playback verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub level: VerdictLevel,
    pub title: String,
    pub detail: String,
    /// Curated per-device knowledge appended to a derived verdict. Rendered
    /// visually apart so the user can tell "your device reports this" from
    /// "we know this about this device family".
    pub note: Option<String>,
}

/// Everything the Media tab renders, in one payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaCapabilities {
    pub video: Vec<VideoFormat>,
    /// From `mSupportedHdrTypes` — what the *display chain* accepts, which is
    /// not the same question as what the device can decode.
    pub hdr_types: Vec<String>,
    pub modes: Vec<DisplayModeEntry>,
    pub audio: AudioPassthrough,
    /// `secure.match_content_frame_rate` — `0` Never / `1` Seamless / `2` Always.
    pub match_content_frame_rate: Option<String>,
    pub verdicts: Vec<Verdict>,
}

/// Video MIME types we roll up, in the order the UI shows them. Anything the
/// device advertises outside this list is ignored rather than surfaced with a
/// raw MIME string — the tab answers "can it play my library", not "dump every
/// codec".
const KNOWN_VIDEO: &[(&str, &str)] = &[
    ("video/avc", "H.264 / AVC"),
    ("video/hevc", "HEVC / H.265"),
    ("video/x-vnd.on2.vp9", "VP9"),
    ("video/av01", "AV1"),
    ("video/dolby-vision", "Dolby Vision"),
    ("video/mpeg2", "MPEG-2"),
];

/// Scan `media_codecs*.xml` for decoder entries.
///
/// Deliberately a tolerant line scanner rather than a real XML parse: the file
/// is concatenated from several vendor/system copies before it reaches us (one
/// `cat` over a glob), so the input is not a single well-formed document and
/// any strict parser would reject it outright. Encoder entries are skipped by
/// tracking which `<Decoders>` / `<Encoders>` block we are inside.
pub fn parse_media_codecs(xml: &str) -> Vec<Decoder> {
    let mut out: Vec<Decoder> = Vec::new();
    // None until the first block marker — some vendor files list `<MediaCodec>`
    // before any block wrapper, and those are decoders in practice.
    let mut in_encoders = false;

    for line in xml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<Encoders") {
            in_encoders = true;
            continue;
        }
        if trimmed.starts_with("<Decoders") || trimmed.starts_with("</Encoders") {
            in_encoders = false;
            continue;
        }
        if in_encoders || !trimmed.starts_with("<MediaCodec") {
            continue;
        }
        let (Some(name), Some(mime)) = (attr(trimmed, "name"), attr(trimmed, "type")) else {
            continue;
        };
        // A `type` attribute can carry several MIMEs separated by `,`.
        for mime in mime.split(',') {
            let mime = mime.trim().to_ascii_lowercase();
            if mime.is_empty() {
                continue;
            }
            let decoder = Decoder {
                hardware: !Decoder::is_software_name(&name),
                name: name.clone(),
                mime,
            };
            if !out.contains(&decoder) {
                out.push(decoder);
            }
        }
    }
    out
}

/// Read `key="value"` out of a tag line. Returns `None` when the attribute is
/// absent or unterminated.
fn attr(tag: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Roll decoders up into the fixed [`KNOWN_VIDEO`] list. Formats the device
/// never mentions are still returned, flagged unsupported — "no AV1" is the
/// answer the user came for, and an absent row would not say it.
pub fn video_formats(decoders: &[Decoder]) -> Vec<VideoFormat> {
    KNOWN_VIDEO
        .iter()
        .map(|(mime, label)| {
            let matching: Vec<&Decoder> = decoders.iter().filter(|d| d.mime == *mime).collect();
            VideoFormat {
                label: (*label).to_string(),
                mime: (*mime).to_string(),
                hardware: matching.iter().any(|d| d.hardware),
                software: matching.iter().any(|d| !d.hardware),
            }
        })
        .collect()
}

/// `AudioFormat.ENCODING_*` constants, as written into
/// `encoded_surround_output_enabled_formats`. Only the encodings that can
/// appear in a surround passthrough list are mapped; anything else renders as
/// its raw number rather than being dropped, so an unmapped value stays
/// visible instead of silently vanishing from the UI.
fn audio_format_label(code: &str) -> String {
    match code.trim() {
        "5" => "Dolby Digital (AC-3)".to_string(),
        "6" => "Dolby Digital Plus (E-AC-3)".to_string(),
        "7" => "DTS".to_string(),
        "8" => "DTS-HD".to_string(),
        "14" => "Dolby TrueHD".to_string(),
        "17" => "Dolby AC-4".to_string(),
        "18" => "Dolby Atmos over DD+ (E-AC-3 JOC)".to_string(),
        "19" => "Dolby MAT".to_string(),
        "26" => "DTS:X (DTS-UHD)".to_string(),
        other => format!("Format {other}"),
    }
}

/// Build the passthrough view from the two `settings get` reads.
pub fn surround_mode(mode_raw: Option<&str>, formats_raw: Option<&str>) -> AudioPassthrough {
    let raw_formats = formats_raw
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "null")
        .map(str::to_string);
    let enabled_formats = raw_formats
        .as_deref()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(audio_format_label)
                .collect()
        })
        .unwrap_or_default();
    AudioPassthrough {
        mode: SurroundMode::from_raw(mode_raw),
        enabled_formats,
        raw_formats,
    }
}

/// Assemble the full capability payload, verdicts included.
pub fn build_capabilities(
    decoders: &[Decoder],
    hdr_types: Vec<String>,
    modes: Vec<DisplayModeEntry>,
    audio: AudioPassthrough,
    match_content_frame_rate: Option<String>,
    device_type: DeviceType,
) -> MediaCapabilities {
    let video = video_formats(decoders);
    let verdicts = verdicts(
        &video,
        &hdr_types,
        &modes,
        &audio,
        match_content_frame_rate.as_deref(),
        device_type,
    );
    MediaCapabilities {
        video,
        hdr_types,
        modes,
        audio,
        match_content_frame_rate,
        verdicts,
    }
}

fn find<'a>(video: &'a [VideoFormat], mime: &str) -> Option<&'a VideoFormat> {
    video.iter().find(|v| v.mime == mime)
}

/// Curated per-device-family knowledge for one verdict topic. Returns `None`
/// when we have nothing to add beyond what the device reported — the common
/// case, and the reason this stays a small table rather than a data file.
fn device_notes(topic: &str, device_type: DeviceType) -> Option<String> {
    match (device_type, topic) {
        (DeviceType::Shield, "av1") => Some(
            "The Shield's Tegra X1/X1+ has no AV1 decode block, and no firmware update can \
             add one. AV1 streams fall back to software decoding — fine at 1080p, \
             unreliable above it."
                .into(),
        ),
        (DeviceType::Shield, "dolby_vision") => Some(
            "Profiles 5 and 8 play natively. Profile 7 — the dual-layer format UHD Blu-ray \
             remuxes use — plays the base layer only: the enhancement layer is discarded, so \
             FEL titles render from a base grade that was never meant to be shown alone. \
             Converting Profile 7 to 8.1 before playback avoids that."
                .into(),
        ),
        _ => None,
    }
}

/// Derive the verdict lines. Order is deliberate — the things most likely to
/// be silently costing quality come first.
fn verdicts(
    video: &[VideoFormat],
    hdr_types: &[String],
    modes: &[DisplayModeEntry],
    audio: &AudioPassthrough,
    match_content_frame_rate: Option<&str>,
    device_type: DeviceType,
) -> Vec<Verdict> {
    let mut out = Vec::new();

    // --- 24p cadence. The single most common silent quality loss.
    let film_mode = modes.iter().find(|m| m.is_film_rate());
    match (film_mode, match_content_frame_rate) {
        (None, _) if modes.is_empty() => {}
        (None, _) => out.push(Verdict {
            level: VerdictLevel::Warn,
            title: "No 24p output mode".into(),
            detail: "The display chain advertises no 23.976 / 24 Hz mode, so film content is \
                     rate-converted to the panel's rate. That is where 3:2 judder comes from."
                .into(),
            note: None,
        }),
        (Some(mode), Some("2")) => out.push(Verdict {
            level: VerdictLevel::Good,
            title: format!("24p handled ({:.3} Hz mode available)", mode.fps),
            detail: "Match Content Frame Rate is set to Always, so film switches to its native \
                     cadence instead of being pulled to the panel rate."
                .into(),
            note: None,
        }),
        (Some(mode), Some("1")) => out.push(Verdict {
            level: VerdictLevel::Info,
            title: format!("24p available ({:.3} Hz), seamless switches only", mode.fps),
            detail: "Match Content Frame Rate is Seamless only: the device switches rate solely \
                     when it can do so without a black frame. On links that cannot, film stays \
                     at the panel rate and judders."
                .into(),
            note: None,
        }),
        (Some(mode), _) => out.push(Verdict {
            level: VerdictLevel::Warn,
            title: format!("24p mode exists ({:.3} Hz) but is never used", mode.fps),
            detail: "Match Content Frame Rate is Never or unset, so every film is converted to \
                     the panel's refresh rate — 3:2 judder on all 24p content. Set it to Always \
                     on the Tweaks tab."
                .into(),
            note: None,
        }),
    }

    // --- Encoded audio passthrough.
    match audio.mode {
        SurroundMode::Never => out.push(Verdict {
            level: VerdictLevel::Warn,
            title: "Encoded surround passthrough is off".into(),
            detail: "Every soundtrack is decoded to PCM on the device, so lossless formats never \
                     reach the receiver — a TrueHD or DTS-HD track arrives downmixed."
                .into(),
            note: None,
        }),
        SurroundMode::Manual => out.push(Verdict {
            level: VerdictLevel::Info,
            title: "Surround passthrough is on a manual allow-list".into(),
            detail: if audio.enabled_formats.is_empty() {
                "Mode is Manual but the allow-list is empty, so nothing is passed through."
                    .to_string()
            } else {
                format!(
                    "Only these pass through: {}. Anything else is decoded on the device.",
                    audio.enabled_formats.join(", ")
                )
            },
            note: None,
        }),
        SurroundMode::Always => out.push(Verdict {
            level: VerdictLevel::Info,
            title: "Surround passthrough is forced".into(),
            detail: "Encoded audio is sent regardless of what the sink reports supporting. If the \
                     receiver cannot decode a format, that track plays silent."
                .into(),
            note: None,
        }),
        SurroundMode::Auto | SurroundMode::Unset => out.push(Verdict {
            level: VerdictLevel::Good,
            title: "Surround passthrough negotiates automatically".into(),
            detail: "The device sends whatever the connected receiver or TV reports it can \
                     decode. Switch to Manual only if a sink misreports its support."
                .into(),
            note: None,
        }),
    }

    // Whether the device gave us a decoder list at all. `media_codecs*.xml`
    // is world-readable on every build we know of, but a restricted or
    // relocated vendor partition would leave us with nothing — and "we could
    // not read it" must never render as "the format is unsupported". Every
    // codec-derived verdict below is gated on this.
    let codecs_known = video.iter().any(|v| !v.unsupported());
    if !codecs_known {
        out.push(Verdict {
            level: VerdictLevel::Info,
            title: "Decoder list unavailable".into(),
            detail: "The device did not return a readable media_codecs XML, so codec support \
                     could not be determined. This is not the same as a format being \
                     unsupported — nothing is being claimed either way."
                .into(),
            note: None,
        });
    }

    // --- Dolby Vision.
    let dv_decoder =
        codecs_known && find(video, "video/dolby-vision").is_some_and(|v| !v.unsupported());
    let dv_display = hdr_types
        .iter()
        .any(|h| h.eq_ignore_ascii_case("dolby vision"));
    if dv_decoder || dv_display {
        out.push(Verdict {
            level: VerdictLevel::Good,
            title: "Dolby Vision available".into(),
            detail: match (dv_decoder, dv_display) {
                (true, true) => "The device advertises a Dolby Vision decoder and the display \
                                 chain accepts Dolby Vision."
                    .into(),
                (true, false) => "The device advertises a Dolby Vision decoder, but the current \
                                  display chain does not report accepting it — check the TV \
                                  input and the cable."
                    .into(),
                _ => "The display chain accepts Dolby Vision. The device reports no dedicated \
                      Dolby Vision decoder entry, which is normal on some builds."
                    .into(),
            },
            note: device_notes("dolby_vision", device_type),
        });
    } else if codecs_known {
        out.push(Verdict {
            level: VerdictLevel::Info,
            title: "No Dolby Vision".into(),
            detail: "Neither a Dolby Vision decoder nor Dolby Vision display support is reported. \
                     Dolby Vision titles fall back to their HDR10 layer where one exists."
                .into(),
            note: None,
        });
    }

    // --- AV1.
    if let Some(av1) = find(video, "video/av01").filter(|_| codecs_known) {
        if av1.hardware {
            out.push(Verdict {
                level: VerdictLevel::Good,
                title: "AV1 decoded in hardware".into(),
                detail: "AV1 streams play without loading the CPU.".into(),
                note: None,
            });
        } else {
            out.push(Verdict {
                level: VerdictLevel::Warn,
                title: if av1.software {
                    "AV1 in software only".into()
                } else {
                    "No AV1 support".into()
                },
                detail: if av1.software {
                    "No hardware AV1 decoder is advertised. AV1 falls back to CPU decoding, which \
                     stutters above 1080p on TV-class silicon."
                        .into()
                } else {
                    "No AV1 decoder is advertised at all. AV1 sources will not play; services \
                     that offer AV1 need to be steered to an H.264 or HEVC ladder."
                        .into()
                },
                note: device_notes("av1", device_type),
            });
        }
    }

    // --- HEVC, the format the library is actually in.
    if let Some(hevc) = find(video, "video/hevc").filter(|_| codecs_known) {
        if !hevc.hardware {
            out.push(Verdict {
                level: VerdictLevel::Warn,
                title: "HEVC not hardware-accelerated".into(),
                detail: "No vendor HEVC decoder is advertised. High-bitrate 4K HEVC will not play \
                         reliably through software decoding."
                    .into(),
                note: None,
            });
        }
    }

    // --- HDR, last: it describes the current link, not a fixed capability.
    if hdr_types.is_empty() && !modes.is_empty() {
        out.push(Verdict {
            level: VerdictLevel::Info,
            title: "Display chain reports SDR only".into(),
            detail: "No HDR formats are advertised on the current link. This tracks whatever the \
                     device is connected to right now — a powered-off TV or a receiver in the \
                     middle commonly reads this way."
                .into(),
            note: None,
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Shaped like a real Shield `media_codecs.xml`: vendor decoders, an
    /// encoder block that must be ignored, and Android's software fallbacks.
    const SHIELD_XML: &str = r#"
<MediaCodecs>
    <Decoders>
        <MediaCodec name="OMX.Nvidia.h264.decode" type="video/avc">
            <Limit name="size" min="32x32" max="4096x4096" />
        </MediaCodec>
        <MediaCodec name="OMX.Nvidia.h265.decode" type="video/hevc" />
        <MediaCodec name="OMX.Nvidia.vp9.decode" type="video/x-vnd.on2.vp9" />
        <MediaCodec name="OMX.Nvidia.mpeg2v.decode" type="video/mpeg2" />
        <MediaCodec name="OMX.dolby.vision.decoder" type="video/dolby-vision" />
        <MediaCodec name="c2.android.av1.decoder" type="video/av01" />
        <MediaCodec name="c2.android.avc.decoder" type="video/avc" />
    </Decoders>
    <Encoders>
        <MediaCodec name="OMX.Nvidia.h264.encoder" type="video/avc" />
        <MediaCodec name="c2.android.av1.encoder" type="video/av01" />
    </Encoders>
</MediaCodecs>
"#;

    fn shield_video() -> Vec<VideoFormat> {
        video_formats(&parse_media_codecs(SHIELD_XML))
    }

    fn modes_with_film() -> Vec<DisplayModeEntry> {
        vec![
            DisplayModeEntry {
                width: 3840,
                height: 2160,
                fps: 59.94,
                active: true,
            },
            DisplayModeEntry {
                width: 3840,
                height: 2160,
                fps: 23.976,
                active: false,
            },
        ]
    }

    fn auto_audio() -> AudioPassthrough {
        surround_mode(Some("0"), None)
    }

    fn titles(v: &[Verdict]) -> Vec<&str> {
        v.iter().map(|x| x.title.as_str()).collect()
    }

    #[test]
    fn parses_decoders_and_skips_encoders() {
        let decoders = parse_media_codecs(SHIELD_XML);
        assert!(decoders
            .iter()
            .any(|d| d.name == "OMX.Nvidia.h265.decode" && d.mime == "video/hevc"),);
        // The encoder block declares AV1 too — only the decoder entry counts.
        let av1: Vec<&Decoder> = decoders.iter().filter(|d| d.mime == "video/av01").collect();
        assert_eq!(av1.len(), 1);
        assert_eq!(av1[0].name, "c2.android.av1.decoder");
        assert!(!decoders.iter().any(|d| d.name.contains("encoder")));
    }

    #[test]
    fn classifies_the_component_names_a_real_shield_advertises() {
        // Verbatim from a Shield TV Pro (mdarcy, Android 11). The `.secure`
        // DRM variants sit on the same silicon and must not read as software,
        // and `OMX.google.*` fallbacks must not read as hardware — that pair
        // is what makes the AV1 and HEVC verdicts come out right.
        let xml = r#"<Decoders>
<MediaCodec name="OMX.Nvidia.h265.decode" type="video/hevc" />
<MediaCodec name="OMX.Nvidia.h265.decode.secure" type="video/hevc" />
<MediaCodec name="OMX.Nvidia.DOVI.decode" type="video/dolby-vision" />
<MediaCodec name="OMX.Nvidia.vp9.decode" type="video/x-vnd.on2.vp9" />
<MediaCodec name="OMX.Nvidia.mpeg2v.decode" type="video/mpeg2" />
<MediaCodec name="OMX.google.hevc.decoder" type="video/hevc" />
<MediaCodec name="OMX.google.h264.decoder" type="video/avc" />
</Decoders>"#;
        let video = video_formats(&parse_media_codecs(xml));
        let by_mime = |m: &str| video.iter().find(|v| v.mime == m).unwrap().clone();

        let hevc = by_mime("video/hevc");
        assert!(hevc.hardware && hevc.software);
        assert!(by_mime("video/dolby-vision").hardware);
        assert!(by_mime("video/x-vnd.on2.vp9").hardware);
        // H.264 only has the Google software fallback in this excerpt.
        let avc = by_mime("video/avc");
        assert!(!avc.hardware && avc.software);
        // The device advertises no AV1 decoder of any kind.
        assert!(by_mime("video/av01").unsupported());
    }

    #[test]
    fn vendor_prefixes_are_hardware_and_platform_prefixes_are_not() {
        let decoders = parse_media_codecs(SHIELD_XML);
        let by_name = |n: &str| decoders.iter().find(|d| d.name == n).unwrap().hardware;
        assert!(by_name("OMX.Nvidia.h265.decode"));
        assert!(by_name("OMX.dolby.vision.decoder"));
        assert!(!by_name("c2.android.av1.decoder"));
        assert!(!by_name("c2.android.avc.decoder"));
    }

    #[test]
    fn a_format_with_both_decoder_kinds_reports_both() {
        // H.264 has a vendor decoder *and* the platform fallback.
        let avc = shield_video()
            .into_iter()
            .find(|v| v.mime == "video/avc")
            .unwrap();
        assert!(avc.hardware);
        assert!(avc.software);
        assert!(!avc.unsupported());
    }

    #[test]
    fn multi_mime_type_attribute_expands_to_one_decoder_each() {
        let decoders = parse_media_codecs(
            r#"<MediaCodec name="OMX.vendor.multi" type="video/avc,video/hevc" />"#,
        );
        assert_eq!(decoders.len(), 2);
        assert!(decoders.iter().all(|d| d.hardware));
        assert_eq!(decoders[0].mime, "video/avc");
        assert_eq!(decoders[1].mime, "video/hevc");
    }

    #[test]
    fn concatenated_files_do_not_duplicate_or_desync_the_encoder_block() {
        // `cat` over a glob yields several documents back to back. The second
        // file's `<Decoders>` has to re-open decoder scanning after the first
        // file's `<Encoders>` block.
        let doubled = format!("{SHIELD_XML}\n{SHIELD_XML}");
        assert_eq!(parse_media_codecs(&doubled), parse_media_codecs(SHIELD_XML));
    }

    #[test]
    fn malformed_entries_are_skipped_not_fatal() {
        let xml = r#"
            <Decoders>
                <MediaCodec name="OMX.broken.decode"
                <MediaCodec type="video/hevc" />
                <MediaCodec name="OMX.ok.decode" type="video/hevc" />
            </Decoders>"#;
        let decoders = parse_media_codecs(xml);
        assert_eq!(decoders.len(), 1);
        assert_eq!(decoders[0].name, "OMX.ok.decode");
    }

    #[test]
    fn an_unreadable_codec_file_yields_no_decoders_rather_than_panicking() {
        assert!(parse_media_codecs("").is_empty());
        assert!(parse_media_codecs("cat: /vendor/etc/media_codecs.xml: No such file").is_empty());
    }

    #[test]
    fn formats_the_device_never_mentions_are_reported_unsupported() {
        // The point of the tab: "no AV1" has to be a visible row, not an
        // absent one. Strip AV1 from the fixture and it must still appear.
        let decoders: Vec<Decoder> = parse_media_codecs(SHIELD_XML)
            .into_iter()
            .filter(|d| d.mime != "video/av01")
            .collect();
        let av1 = video_formats(&decoders)
            .into_iter()
            .find(|v| v.mime == "video/av01")
            .unwrap();
        assert!(av1.unsupported());
    }

    #[test]
    fn shield_av1_is_software_only_and_carries_the_soc_note() {
        let v = verdicts(
            &shield_video(),
            &["HDR10".into()],
            &modes_with_film(),
            &auto_audio(),
            Some("2"),
            DeviceType::Shield,
        );
        let av1 = v.iter().find(|x| x.title.contains("AV1")).unwrap();
        assert_eq!(av1.level, VerdictLevel::Warn);
        assert_eq!(av1.title, "AV1 in software only");
        assert!(av1.note.as_ref().unwrap().contains("Tegra"));
    }

    #[test]
    fn curated_notes_are_scoped_to_the_device_family() {
        // Same hardware facts, unknown device: derived verdict identical,
        // curated note absent. This is the guardrail on the curated layer.
        let args = (shield_video(), vec!["HDR10".to_string()], modes_with_film());
        let shield = verdicts(
            &args.0,
            &args.1,
            &args.2,
            &auto_audio(),
            Some("2"),
            DeviceType::Shield,
        );
        let other = verdicts(
            &args.0,
            &args.1,
            &args.2,
            &auto_audio(),
            Some("2"),
            DeviceType::GoogleTv,
        );
        assert_eq!(titles(&shield), titles(&other));
        let av1_note = |v: &[Verdict]| {
            v.iter()
                .find(|x| x.title.contains("AV1"))
                .unwrap()
                .note
                .clone()
        };
        assert!(av1_note(&shield).is_some());
        assert_eq!(av1_note(&other), None);
    }

    #[test]
    fn dolby_vision_note_names_the_profile_7_base_layer_behavior() {
        let v = verdicts(
            &shield_video(),
            &["Dolby Vision".into(), "HDR10".into()],
            &modes_with_film(),
            &auto_audio(),
            Some("2"),
            DeviceType::Shield,
        );
        let dv = v.iter().find(|x| x.title.contains("Dolby Vision")).unwrap();
        assert_eq!(dv.level, VerdictLevel::Good);
        let note = dv.note.as_ref().unwrap();
        assert!(note.contains("Profile 7"));
        assert!(note.contains("base layer only"));
    }

    #[test]
    fn film_rate_verdict_tracks_the_match_content_setting() {
        let level_for = |setting: Option<&str>| {
            let v = verdicts(
                &shield_video(),
                &["HDR10".into()],
                &modes_with_film(),
                &auto_audio(),
                setting,
                DeviceType::Shield,
            );
            v[0].level
        };
        assert_eq!(level_for(Some("2")), VerdictLevel::Good);
        assert_eq!(level_for(Some("1")), VerdictLevel::Info);
        assert_eq!(level_for(Some("0")), VerdictLevel::Warn);
        // Unset behaves like Never — Android does not switch rate by default.
        assert_eq!(level_for(None), VerdictLevel::Warn);
    }

    #[test]
    fn a_device_with_no_film_rate_mode_is_flagged_even_when_matching_is_on() {
        let modes = vec![DisplayModeEntry {
            width: 3840,
            height: 2160,
            fps: 60.0,
            active: true,
        }];
        let v = verdicts(
            &shield_video(),
            &["HDR10".into()],
            &modes,
            &auto_audio(),
            Some("2"),
            DeviceType::Shield,
        );
        assert_eq!(v[0].level, VerdictLevel::Warn);
        assert_eq!(v[0].title, "No 24p output mode");
    }

    #[test]
    fn true_24_counts_as_film_rate_alongside_23_976() {
        assert!(DisplayModeEntry {
            width: 1,
            height: 1,
            fps: 24.0,
            active: false
        }
        .is_film_rate());
        assert!(DisplayModeEntry {
            width: 1,
            height: 1,
            fps: 23.976,
            active: false
        }
        .is_film_rate());
        assert!(!DisplayModeEntry {
            width: 1,
            height: 1,
            fps: 25.0,
            active: false
        }
        .is_film_rate());
        assert!(!DisplayModeEntry {
            width: 1,
            height: 1,
            fps: 30.0,
            active: false
        }
        .is_film_rate());
    }

    #[test]
    fn no_display_modes_at_all_produces_no_cadence_claim() {
        // A truncated `dumpsys display` must not be read as "no 24p mode".
        let v = verdicts(
            &shield_video(),
            &[],
            &[],
            &auto_audio(),
            Some("2"),
            DeviceType::Shield,
        );
        assert!(!v.iter().any(|x| x.title.contains("24p")));
        // …and it must not claim the link is SDR either.
        assert!(!v.iter().any(|x| x.title.contains("SDR only")));
    }

    #[test]
    fn surround_modes_map_from_the_raw_setting() {
        assert_eq!(SurroundMode::from_raw(Some("0")), SurroundMode::Auto);
        assert_eq!(SurroundMode::from_raw(Some("1")), SurroundMode::Never);
        assert_eq!(SurroundMode::from_raw(Some("2")), SurroundMode::Always);
        assert_eq!(SurroundMode::from_raw(Some("3")), SurroundMode::Manual);
        assert_eq!(SurroundMode::from_raw(None), SurroundMode::Unset);
        assert_eq!(SurroundMode::from_raw(Some("null")), SurroundMode::Unset);
    }

    #[test]
    fn manual_format_list_decodes_to_friendly_names() {
        let audio = surround_mode(Some("3"), Some("5,6,7,8,14,18"));
        assert_eq!(
            audio.enabled_formats,
            [
                "Dolby Digital (AC-3)",
                "Dolby Digital Plus (E-AC-3)",
                "DTS",
                "DTS-HD",
                "Dolby TrueHD",
                "Dolby Atmos over DD+ (E-AC-3 JOC)",
            ]
        );
        assert_eq!(audio.raw_formats.as_deref(), Some("5,6,7,8,14,18"));
    }

    #[test]
    fn unmapped_audio_codes_stay_visible_rather_than_disappearing() {
        let audio = surround_mode(Some("3"), Some("14,999"));
        assert_eq!(audio.enabled_formats, ["Dolby TrueHD", "Format 999"]);
    }

    #[test]
    fn passthrough_off_is_a_warning_and_names_the_consequence() {
        let v = verdicts(
            &shield_video(),
            &["HDR10".into()],
            &modes_with_film(),
            &surround_mode(Some("1"), None),
            Some("2"),
            DeviceType::Shield,
        );
        let audio = v.iter().find(|x| x.title.contains("passthrough")).unwrap();
        assert_eq!(audio.level, VerdictLevel::Warn);
        assert!(audio.detail.contains("TrueHD"));
    }

    #[test]
    fn manual_mode_with_an_empty_allow_list_says_nothing_passes_through() {
        let v = verdicts(
            &shield_video(),
            &["HDR10".into()],
            &modes_with_film(),
            &surround_mode(Some("3"), None),
            Some("2"),
            DeviceType::Shield,
        );
        let audio = v.iter().find(|x| x.title.contains("allow-list")).unwrap();
        assert!(audio.detail.contains("empty"));
    }

    #[test]
    fn build_capabilities_carries_every_input_through() {
        let caps = build_capabilities(
            &parse_media_codecs(SHIELD_XML),
            vec!["Dolby Vision".into(), "HDR10".into()],
            modes_with_film(),
            surround_mode(Some("0"), None),
            Some("2".into()),
            DeviceType::Shield,
        );
        assert_eq!(caps.video.len(), KNOWN_VIDEO.len());
        assert_eq!(caps.hdr_types, ["Dolby Vision", "HDR10"]);
        assert_eq!(caps.modes.len(), 2);
        assert_eq!(caps.match_content_frame_rate.as_deref(), Some("2"));
        assert!(!caps.verdicts.is_empty());
    }

    #[test]
    fn a_device_that_reports_nothing_still_produces_a_usable_payload() {
        // Everything unreadable: no codecs, no modes, no HDR, no settings.
        // The tab must render rather than blow up or invent claims.
        let caps = build_capabilities(
            &[],
            vec![],
            vec![],
            surround_mode(None, None),
            None,
            DeviceType::Unknown,
        );
        assert!(caps.video.iter().all(VideoFormat::unsupported));
        // The audio setting is the one fact still known; everything codec-
        // derived must abstain rather than assert absence.
        assert_eq!(
            titles(&caps.verdicts),
            [
                "Surround passthrough negotiates automatically",
                "Decoder list unavailable"
            ]
        );
    }

    #[test]
    fn an_unreadable_codec_list_never_reads_as_unsupported() {
        // The distinction this whole gate exists for: `unsupported()` on the
        // row means "not advertised", but with no list at all the verdicts
        // must not turn that into a claim about AV1, HEVC or Dolby Vision.
        let caps = build_capabilities(
            &[],
            vec!["HDR10".into()],
            modes_with_film(),
            auto_audio(),
            Some("2".into()),
            DeviceType::Shield,
        );
        for banned in [
            "No AV1 support",
            "AV1 in software only",
            "HEVC not hardware-accelerated",
            "No Dolby Vision",
        ] {
            assert!(
                !titles(&caps.verdicts).contains(&banned),
                "must not claim {banned:?} without a decoder list: {:?}",
                titles(&caps.verdicts)
            );
        }
        assert!(caps
            .verdicts
            .iter()
            .any(|v| v.title == "Decoder list unavailable"));
        // Facts from other sources are unaffected.
        assert!(caps
            .verdicts
            .iter()
            .any(|v| v.title.contains("24p handled")));
    }
}
