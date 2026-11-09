//! A discovered TV's display name, read from its Cast setup endpoint.
//!
//! Android below 14 browses only adb's mDNS services (legacy NsdManager has one
//! resolve slot and cannot cancel a slow Cast lookup), and those adverts are
//! `adb-<serial>`, so nothing names the TV. Cast TVs also answer
//! `GET /setup/eureka_info` on port 8008 with JSON carrying the name the owner
//! gave the TV. That JSON also holds MAC, BSSID and location fields, so only
//! `name` is ever deserialized. The name is display text, never identity.
//!
//! Pure: the request text, the response parser and the address allow-list. The
//! socket lives in [`crate::net`].

use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};

use serde::Deserialize;

pub const CAST_SETUP_PORT: u16 = 8008;
pub const MAX_NAME_CHARS: usize = 64;

/// What the bytes received so far amount to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CastNameRead {
    /// The response is not complete yet.
    NeedMore,
    /// The response is complete, or can never yield a name.
    Done(Option<String>),
}

/// The TVs answer HTTP/1.0 with an empty reply and ignore `Connection: close`,
/// so this is HTTP/1.1 and the reader stops at `Content-Length` rather than EOF.
pub fn request(addr: SocketAddrV4) -> String {
    format!(
        "GET /setup/eureka_info?params=name HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )
}

/// Reads the name from a raw HTTP/1.x response. `eof` means no more bytes will
/// arrive (closed, or the size cap was hit); with it set this never asks for more.
pub fn read_response(raw: &[u8], eof: bool) -> CastNameRead {
    match body(raw) {
        Body::Ready(body) => CastNameRead::Done(name_from_json(&body)),
        Body::Invalid => CastNameRead::Done(None),
        Body::UntilClose(body) if eof => CastNameRead::Done(name_from_json(body)),
        Body::NeedMore if eof => CastNameRead::Done(None),
        Body::NeedMore | Body::UntilClose(_) => CastNameRead::NeedMore,
    }
}

/// [`read_response`] on a response that is known to be complete.
pub fn parse_response(raw: &[u8]) -> Option<String> {
    match read_response(raw, true) {
        CastNameRead::Done(name) => name,
        CastNameRead::NeedMore => None,
    }
}

/// The address to ask, from a discovered host string (`192.168.1.20`, or
/// bracketed IPv6). Only a LAN IPv4 address qualifies.
pub fn lookup_target(host: &str) -> Option<Ipv4Addr> {
    let host = host.trim();
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    match host.parse::<IpAddr>().ok()? {
        IpAddr::V4(ip) if is_lan_ipv4(ip) => Some(ip),
        _ => None,
    }
}

/// The address to ask for a network adb serial (`192.168.1.20:5555`). An
/// mDNS transport key or a USB serial names no address, so it has none.
pub fn serial_lookup_target(serial: &str) -> Option<Ipv4Addr> {
    let (host, port) = serial.trim().rsplit_once(':')?;
    port.parse::<u16>().ok()?;
    lookup_target(host)
}

/// RFC 1918, carrier-grade NAT (100.64/10) and link-local (169.254/16). A
/// discovery result naming any other address is never sent a request.
pub fn is_lan_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_private() || ip.is_link_local() || (a == 100 && (64..128).contains(&b))
}

enum Body<'a> {
    NeedMore,
    Invalid,
    Ready(std::borrow::Cow<'a, [u8]>),
    /// No length or chunking: the body ends when the connection closes.
    UntilClose(&'a [u8]),
}

fn body(raw: &[u8]) -> Body<'_> {
    let Some(end) = find(raw, b"\r\n\r\n") else {
        return Body::NeedMore;
    };
    let Ok(head) = std::str::from_utf8(&raw[..end]) else {
        return Body::Invalid;
    };
    let rest = &raw[end + 4..];
    let mut lines = head.split("\r\n");
    let mut status = lines.next().unwrap_or_default().split_ascii_whitespace();
    let version_ok = status.next().is_some_and(|v| v.starts_with("HTTP/1."));
    if !version_ok || status.next() != Some("200") {
        return Body::Invalid;
    }
    let mut length: Option<usize> = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            return Body::Invalid;
        };
        let (name, value) = (name.trim(), value.trim());
        if name.eq_ignore_ascii_case("content-length") {
            match (value.parse::<usize>(), length) {
                (Ok(n), None) => length = Some(n),
                (Ok(n), Some(seen)) if n == seen => {}
                _ => return Body::Invalid,
            }
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            let last = value.rsplit(',').next().unwrap_or_default().trim();
            if !last.eq_ignore_ascii_case("chunked") {
                return Body::Invalid;
            }
            chunked = true;
        }
    }
    if chunked {
        dechunk(rest)
    } else if let Some(length) = length {
        match rest.get(..length) {
            Some(body) => Body::Ready(body.into()),
            None => Body::NeedMore,
        }
    } else {
        Body::UntilClose(rest)
    }
}

fn dechunk(mut rest: &[u8]) -> Body<'static> {
    let mut out = Vec::new();
    loop {
        let Some(line_end) = find(rest, b"\r\n") else {
            return Body::NeedMore;
        };
        let Some(size) = std::str::from_utf8(&rest[..line_end])
            .ok()
            .map(|line| line.split(';').next().unwrap_or_default().trim())
            .filter(|size| !size.is_empty())
            .and_then(|size| usize::from_str_radix(size, 16).ok())
        else {
            return Body::Invalid;
        };
        rest = &rest[line_end + 2..];
        // Trailers after the last chunk carry nothing the name needs.
        if size == 0 {
            return Body::Ready(out.into());
        }
        let Some(needed) = size.checked_add(2) else {
            return Body::Invalid;
        };
        if rest.len() < needed {
            return Body::NeedMore;
        }
        if &rest[size..needed] != b"\r\n" {
            return Body::Invalid;
        }
        out.extend_from_slice(&rest[..size]);
        rest = &rest[needed..];
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn name_from_json(body: &[u8]) -> Option<String> {
    #[derive(Deserialize)]
    struct EurekaName {
        name: Option<String>,
    }
    let raw = serde_json::from_slice::<EurekaName>(body).ok()?.name?;
    if raw.chars().any(char::is_control) {
        return None;
    }
    let name = raw.trim();
    (!name.is_empty() && name.chars().count() <= MAX_NAME_CHARS).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: &str, headers: &[&str], body: &str) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 {status}\r\n");
        for header in headers {
            out.push_str(header);
            out.push_str("\r\n");
        }
        out.push_str("\r\n");
        out.push_str(body);
        out.into_bytes()
    }

    fn sized(body: &str) -> Vec<u8> {
        response(
            "200 OK",
            &[
                "Access-Control-Allow-Headers:Content-Type",
                "Cache-Control:no-cache",
                &format!("Content-Length:{}", body.len()),
                "Content-Type:application/json",
            ],
            body,
        )
    }

    // Shape of a real Shield's reply, with the other fields replaced by dummies.
    const EUREKA: &str = r#"{"bssid":"","build_version":"446070","connected":true,"hotspot_bssid":"00:00:5E:00:53:00","ip_address":"192.0.2.10","locale":"en-US","location":{"country_code":"US","latitude":0.0,"longitude":0.0},"mac_address":"00:00:5E:00:53:01","name":"Living Room TV","ssid":"","uptime":1.5}"#;

    #[test]
    fn a_200_with_a_name_yields_the_name() {
        assert_eq!(
            parse_response(&sized(EUREKA)),
            Some("Living Room TV".to_string())
        );
        assert_eq!(
            read_response(&sized(EUREKA), false),
            CastNameRead::Done(Some("Living Room TV".to_string()))
        );
    }

    #[test]
    fn the_name_is_trimmed() {
        assert_eq!(
            parse_response(&sized(r#"{"name":"  Bedroom Shield  "}"#)),
            Some("Bedroom Shield".to_string())
        );
    }

    #[test]
    fn a_non_200_status_yields_nothing() {
        for status in [
            "404 Not Found",
            "500 Internal Server Error",
            "301 Moved",
            "204 No Content",
        ] {
            let raw = response(status, &["Content-Length:23"], r#"{"name":"Living Room"}"#);
            assert_eq!(
                read_response(&raw, false),
                CastNameRead::Done(None),
                "{status}"
            );
        }
        let raw = b"HTTP/2 200\r\nContent-Length:2\r\n\r\n{}".to_vec();
        assert_eq!(parse_response(&raw), None);
        let raw = b"SSH-2.0-OpenSSH\r\n\r\n".to_vec();
        assert_eq!(parse_response(&raw), None);
    }

    #[test]
    fn malformed_json_yields_nothing() {
        for body in [
            "",
            "not json",
            r#"{"name":"Living Room""#,
            r#"{"name":"Living Room"} trailing"#,
            "<html>name</html>",
        ] {
            assert_eq!(parse_response(&sized(body)), None, "{body}");
        }
    }

    #[test]
    fn missing_empty_overlong_or_control_names_yield_nothing() {
        let at_limit = "x".repeat(MAX_NAME_CHARS);
        let over_limit = "x".repeat(MAX_NAME_CHARS + 1);
        let padded_at_limit = format!("  {at_limit}  ");
        for (body, want) in [
            ("{}".to_string(), None),
            (r#"{"name":null}"#.to_string(), None),
            (r#"{"name":42}"#.to_string(), None),
            (r#"{"name":""}"#.to_string(), None),
            (r#"{"name":"   "}"#.to_string(), None),
            (r#"{"name":"Living\nRoom"}"#.to_string(), None),
            (r#"{"name":"TV\u0007"}"#.to_string(), None),
            (r#"{"name":"TV\u0000"}"#.to_string(), None),
            (r#"{"name":"TV\u009b"}"#.to_string(), None),
            (format!(r#"{{"name":"{over_limit}"}}"#), None),
            (
                format!(r#"{{"name":"{at_limit}"}}"#),
                Some(at_limit.clone()),
            ),
            (
                format!(r#"{{"name":"{padded_at_limit}"}}"#),
                Some(at_limit.clone()),
            ),
            // Counted in characters, not bytes.
            (
                format!(r#"{{"name":"{}"}}"#, "é".repeat(MAX_NAME_CHARS)),
                Some("é".repeat(MAX_NAME_CHARS)),
            ),
        ] {
            assert_eq!(parse_response(&sized(&body)), want, "{body}");
        }
    }

    #[test]
    fn headers_end_at_the_first_blank_line() {
        // Nothing until the blank line arrives, however much else has.
        let partial = b"HTTP/1.1 200 OK\r\nContent-Length:21\r\n".to_vec();
        assert_eq!(read_response(&partial, false), CastNameRead::NeedMore);
        assert_eq!(read_response(&partial, true), CastNameRead::Done(None));
        // A bare-LF response never splits, so it never yields a name.
        let lf = b"HTTP/1.1 200 OK\nContent-Length:15\n\n{\"name\":\"Den\"}".to_vec();
        assert_eq!(parse_response(&lf), None);
        // A blank line inside the body is body, not headers.
        let body = "{\r\n\r\n\"name\":\"Den\"}";
        assert_eq!(parse_response(&sized(body)), Some("Den".to_string()));
        // A header line with no colon is not HTTP.
        let raw = response("200 OK", &["Content-Length 15"], r#"{"name":"Den"}"#);
        assert_eq!(parse_response(&raw), None);
    }

    #[test]
    fn a_sized_body_waits_for_every_byte_and_ignores_what_follows() {
        let full = sized(EUREKA);
        let short = &full[..full.len() - 1];
        assert_eq!(read_response(short, false), CastNameRead::NeedMore);
        assert_eq!(read_response(short, true), CastNameRead::Done(None));
        let mut extra = full.clone();
        extra.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
        assert_eq!(parse_response(&extra), Some("Living Room TV".to_string()));
        let conflicting = response(
            "200 OK",
            &["Content-Length:15", "Content-Length:16"],
            r#"{"name":"Den"}"#,
        );
        assert_eq!(parse_response(&conflicting), None);
        let bad = response("200 OK", &["Content-Length:-1"], r#"{"name":"Den"}"#);
        assert_eq!(parse_response(&bad), None);
    }

    #[test]
    fn a_body_without_a_length_ends_at_close() {
        let raw = response(
            "200 OK",
            &["Content-Type:application/json"],
            r#"{"name":"Den"}"#,
        );
        assert_eq!(read_response(&raw, false), CastNameRead::NeedMore);
        assert_eq!(
            read_response(&raw, true),
            CastNameRead::Done(Some("Den".to_string()))
        );
        let http10 = b"HTTP/1.0 200 OK\r\n\r\n{\"name\":\"Den\"}".to_vec();
        assert_eq!(parse_response(&http10), Some("Den".to_string()));
    }

    #[test]
    fn a_chunked_body_is_reassembled() {
        let chunked = |chunks: &str| response("200 OK", &["Transfer-Encoding: chunked"], chunks);
        let raw = chunked("8\r\n{\"name\":\r\n6;ext=1\r\n\"Den\"}\r\n0\r\n\r\n");
        assert_eq!(
            read_response(&raw, false),
            CastNameRead::Done(Some("Den".to_string()))
        );
        let unfinished = chunked("8\r\n{\"name\":\r\n6\r\n\"Den\"}\r\n");
        assert_eq!(read_response(&unfinished, false), CastNameRead::NeedMore);
        assert_eq!(read_response(&unfinished, true), CastNameRead::Done(None));
        let short_chunk = chunked("8\r\n{\"name\"");
        assert_eq!(read_response(&short_chunk, false), CastNameRead::NeedMore);
        for bad in [
            "zz\r\n{}\r\n0\r\n\r\n",
            "\r\n{}\r\n0\r\n\r\n",
            "2\r\n{}XX0\r\n\r\n",
            "ffffffffffffffffffff\r\n",
        ] {
            assert_eq!(
                read_response(&chunked(bad), false),
                CastNameRead::Done(None),
                "{bad}"
            );
        }
        let gzip = response("200 OK", &["Transfer-Encoding: gzip"], "\x1f\u{8b}");
        assert_eq!(read_response(&gzip, false), CastNameRead::Done(None));
    }

    #[test]
    fn only_lan_ipv4_hosts_are_looked_up() {
        for (host, want) in [
            ("192.168.42.71", Some(Ipv4Addr::new(192, 168, 42, 71))),
            (" 10.0.0.5 ", Some(Ipv4Addr::new(10, 0, 0, 5))),
            ("172.16.0.1", Some(Ipv4Addr::new(172, 16, 0, 1))),
            ("172.31.255.254", Some(Ipv4Addr::new(172, 31, 255, 254))),
            ("100.64.0.1", Some(Ipv4Addr::new(100, 64, 0, 1))),
            ("100.127.255.254", Some(Ipv4Addr::new(100, 127, 255, 254))),
            ("169.254.3.4", Some(Ipv4Addr::new(169, 254, 3, 4))),
            ("[192.168.1.20]", Some(Ipv4Addr::new(192, 168, 1, 20))),
            ("8.8.8.8", None),
            ("172.32.0.1", None),
            ("100.63.255.255", None),
            ("100.128.0.1", None),
            ("127.0.0.1", None),
            ("0.0.0.0", None),
            ("224.0.0.251", None),
            ("255.255.255.255", None),
            ("[2001:db8:0:0:0:0:0:5]", None),
            ("[fe80:0:0:0:1c2d:3e4f:5a6b:7c8d%3]", None),
            ("[::ffff:192.168.1.20]", None),
            ("tv.local", None),
            ("192.168.1.20:5555", None),
            ("", None),
        ] {
            assert_eq!(lookup_target(host), want, "{host}");
        }
    }

    #[test]
    fn the_request_asks_http_1_1_for_the_setup_endpoint() {
        let request = request(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 20), 8008));
        assert_eq!(
            request,
            "GET /setup/eureka_info?params=name HTTP/1.1\r\nHost: 192.168.1.20:8008\r\nConnection: close\r\n\r\n"
        );
    }

    #[test]
    fn only_a_lan_ip_and_port_serial_names_an_address() {
        assert_eq!(
            serial_lookup_target("192.168.42.143:5555"),
            Some(Ipv4Addr::new(192, 168, 42, 143))
        );
        for serial in [
            "192.168.42.143",
            "8.8.8.8:5555",
            "adb-1323719049001._adb-tls-connect._tcp",
            "1323719049001",
            "[fe80::1]:5555",
            "192.168.42.143:port",
        ] {
            assert_eq!(serial_lookup_target(serial), None, "{serial}");
        }
    }
}
