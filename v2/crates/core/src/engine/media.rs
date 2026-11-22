//! Observations from device codec configuration, display modes, and audio settings.
use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decoder {
    pub name: String,
    pub mime: String,
    pub software: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoFormat {
    pub label: String,
    pub mime: String,
    pub advertised: bool,
    pub software: bool,
    pub acceleration_unknown: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayModeEntry {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub active: bool,
}

impl DisplayModeEntry {
    pub fn is_film_rate(&self) -> bool {
        (23.9..=24.1).contains(&self.fps)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurroundMode {
    Auto,
    Never,
    Always,
    Manual,
    Unset,
    Unknown,
}

impl SurroundMode {
    pub fn from_raw(raw: Option<&str>) -> Self {
        match raw.map(str::trim) {
            Some("0") => Self::Auto,
            Some("1") => Self::Never,
            Some("2") => Self::Always,
            Some("3") => Self::Manual,
            None | Some("" | "null") => Self::Unset,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioPassthrough {
    pub mode: SurroundMode,
    pub enabled_formats: Vec<String>,
    pub raw_formats: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictLevel {
    Good,
    Warn,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub level: VerdictLevel,
    pub title: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaCapabilities {
    pub video: Vec<VideoFormat>,
    pub hdr_types: Vec<String>,
    pub modes: Vec<DisplayModeEntry>,
    pub audio: AudioPassthrough,
    pub match_content_frame_rate: Option<String>,
    pub verdicts: Vec<Verdict>,
}

const KNOWN_VIDEO: &[(&str, &str)] = &[
    ("video/avc", "H.264 / AVC"),
    ("video/hevc", "HEVC / H.265"),
    ("video/x-vnd.on2.vp9", "VP9"),
    ("video/av01", "AV1"),
    ("video/dolby-vision", "Dolby Vision"),
    ("video/mpeg2", "MPEG-2"),
];

fn attribute(tag: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    tag.attributes()
        .filter_map(Result::ok)
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

fn add_decoder(out: &mut Vec<Decoder>, name: &str, mime: &str, software: bool) {
    for mime in mime.split(',').map(str::trim).filter(|m| !m.is_empty()) {
        let decoder = Decoder {
            name: name.to_string(),
            mime: mime.to_ascii_lowercase(),
            software,
        };
        if !out.contains(&decoder) {
            out.push(decoder);
        }
    }
}

/// The input may contain several concatenated XML documents. Missing entries
/// are not proof of runtime codec absence: vendor includes can be unreadable.
pub fn parse_media_codecs(xml: &str) -> Vec<Decoder> {
    let mut reader = Reader::from_str(xml);
    let mut out = Vec::new();
    let mut in_decoders = false;
    let mut current: Option<(String, bool)> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(tag) | Event::Empty(tag)) => match tag.name().as_ref() {
                b"Decoders" => in_decoders = true,
                b"Encoders" => {
                    in_decoders = false;
                    current = None;
                }
                b"MediaCodec" if in_decoders => {
                    current = attribute(&tag, b"name").map(|name| {
                        let lower = name.to_ascii_lowercase();
                        let software = lower.starts_with("omx.google.")
                            || lower.starts_with("c2.android.")
                            || attribute(&tag, b"software-codec").as_deref() == Some("true");
                        (name, software)
                    });
                    if let (Some((name, software)), Some(mime)) =
                        (&current, attribute(&tag, b"type"))
                    {
                        add_decoder(&mut out, name, &mime, *software);
                    }
                }
                b"Type" if in_decoders => {
                    if let (Some((name, software)), Some(mime)) =
                        (&current, attribute(&tag, b"name"))
                    {
                        add_decoder(&mut out, name, &mime, *software);
                    }
                }
                _ => {}
            },
            Ok(Event::End(tag)) => match tag.name().as_ref() {
                b"MediaCodec" => current = None,
                b"Decoders" | b"Encoders" => {
                    in_decoders = false;
                    current = None;
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

pub fn video_formats(decoders: &[Decoder]) -> Vec<VideoFormat> {
    KNOWN_VIDEO
        .iter()
        .map(|(mime, label)| {
            let matching: Vec<_> = decoders.iter().filter(|d| d.mime == *mime).collect();
            VideoFormat {
                label: (*label).into(),
                mime: (*mime).into(),
                advertised: !matching.is_empty(),
                software: matching.iter().any(|d| d.software),
                acceleration_unknown: matching.iter().any(|d| !d.software),
            }
        })
        .collect()
}

fn audio_format_label(code: &str) -> String {
    match code.trim() {
        "5" => "Dolby Digital (AC-3)".into(),
        "6" => "Dolby Digital Plus (E-AC-3)".into(),
        "7" => "DTS".into(),
        "8" => "DTS-HD".into(),
        "14" => "Dolby TrueHD".into(),
        "17" => "Dolby AC-4".into(),
        "18" => "Dolby Atmos over DD+ (E-AC-3 JOC)".into(),
        "19" => "Dolby MAT".into(),
        "26" => "MPEG-H LC L4".into(),
        "27" => "DTS UHD P1".into(),
        other => format!("Format {other}"),
    }
}

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

pub fn build_capabilities(
    decoders: &[Decoder],
    hdr_types: Vec<String>,
    modes: Vec<DisplayModeEntry>,
    audio: AudioPassthrough,
    match_content_frame_rate: Option<String>,
) -> MediaCapabilities {
    let mut verdicts = vec![Verdict {
        level: VerdictLevel::Info,
        title: "Configuration, not a playback test".into(),
        detail: "Codec entries describe available configuration. They do not verify runtime registration, acceleration, profiles, DRM or playback performance.".into(),
    }];
    if decoders.is_empty() {
        verdicts.push(Verdict {
            level: VerdictLevel::Info,
            title: "Decoder list unavailable".into(),
            detail: "No decoder entries were read. Codec support could not be determined.".into(),
        });
    }
    if let Some(mode) = modes.iter().find(|m| m.is_film_rate()) {
        verdicts.push(Verdict {
            level: VerdictLevel::Info,
            title: format!("24p mode reported ({:.3} Hz)", mode.fps),
            detail: "A matching display mode is available. Actual switching depends on the player, device and display; this setting alone does not guarantee film-rate output.".into(),
        });
    }
    if audio.mode == SurroundMode::Never {
        verdicts.push(Verdict {
            level: VerdictLevel::Info,
            title: "Encoded surround passthrough is off".into(),
            detail: "The setting disables encoded surround output. PCM channel count and quality depend on the app and output path; this does not by itself imply downmixing.".into(),
        });
    }
    MediaCapabilities {
        video: video_formats(decoders),
        hdr_types,
        modes,
        audio,
        match_content_frame_rate,
        verdicts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_multiline_nested_types_comments_and_concatenated_documents() {
        let xml = r#"<?xml version="1.0"?><MediaCodecs><Decoders>
        <!-- <MediaCodec name="fake" type="video/av01"/> -->
        <MediaCodec
          name="vendor.decoder">
          <Type name="video/hevc"><Limit name="size" max="4096x4096"/></Type>
          <Type name="video/avc"/>
        </MediaCodec></Decoders><Encoders>
        <MediaCodec name="vendor.encoder" type="video/av01"/>
        </Encoders></MediaCodecs>
        <?xml version="1.0"?><MediaCodecs><Decoders>
        <MediaCodec name="c2.android.av1.decoder" type="video/av01"/>
        </Decoders></MediaCodecs>"#;
        let decoders = parse_media_codecs(xml);
        assert_eq!(decoders.len(), 3);
        assert!(!decoders[0].software);
        assert!(decoders[2].software);
        assert!(!decoders
            .iter()
            .any(|d| d.name.contains("encoder") || d.name == "fake"));
    }

    #[test]
    fn vendor_name_does_not_prove_hardware_acceleration() {
        let xml = r#"<Decoders><MediaCodec name="vendor.decoder" type="video/hevc"/></Decoders>"#;
        let video = video_formats(&parse_media_codecs(xml));
        let hevc = video.iter().find(|v| v.mime == "video/hevc").unwrap();
        assert!(hevc.advertised && hevc.acceleration_unknown && !hevc.software);
        let av1 = video.iter().find(|v| v.mime == "video/av01").unwrap();
        assert!(!av1.advertised);
    }

    #[test]
    fn software_metadata_and_mixed_decoders_preserve_presence() {
        let xml = r#"<Decoders>
        <MediaCodec name="vendor.soft" type="video/hevc" software-codec="true"/>
        <MediaCodec name="vendor.unknown" type="video/hevc"/>
        <MediaCodec name="OMX.google.avc.decoder" type="video/avc"/>
        </Decoders>"#;
        let video = video_formats(&parse_media_codecs(xml));
        let hevc = video.iter().find(|v| v.mime == "video/hevc").unwrap();
        assert!(hevc.advertised && hevc.software && hevc.acceleration_unknown);
    }

    #[test]
    fn duplicate_documents_do_not_duplicate_decoders() {
        let xml = r#"<Decoders><MediaCodec name="vendor" type="video/hevc,video/avc"/></Decoders>"#;
        assert_eq!(
            parse_media_codecs(&format!("{xml}{xml}")),
            parse_media_codecs(xml)
        );
    }

    #[test]
    fn empty_and_malformed_xml_do_not_panic_or_invent_support() {
        assert!(parse_media_codecs("").is_empty());
        assert!(parse_media_codecs("cat: permission denied").is_empty());
        assert!(parse_media_codecs("<Decoders><MediaCodec name=\"unterminated").is_empty());
    }

    #[test]
    fn audio_codes_preserve_unknowns_and_do_not_confuse_mpegh_with_dts() {
        let audio = surround_mode(Some("3"), Some("26,27,999"));
        assert_eq!(
            audio.enabled_formats,
            ["MPEG-H LC L4", "DTS UHD P1", "Format 999"]
        );
        assert_eq!(audio.raw_formats.as_deref(), Some("26,27,999"));
        assert_eq!(SurroundMode::from_raw(Some("99")), SurroundMode::Unknown);
        assert_eq!(SurroundMode::from_raw(None), SurroundMode::Unset);
    }

    #[test]
    fn absent_frame_setting_and_display_data_stay_unknown() {
        let caps = build_capabilities(&[], vec![], vec![], surround_mode(None, None), None);
        assert_eq!(caps.match_content_frame_rate, None);
        assert!(caps
            .verdicts
            .iter()
            .any(|v| v.title == "Decoder list unavailable"));
        assert!(!caps
            .verdicts
            .iter()
            .any(|v| v.title.contains("SDR") || v.title.contains("Never")));
    }
}
