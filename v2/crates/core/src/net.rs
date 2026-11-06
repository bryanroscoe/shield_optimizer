//! Local-network lookups that the pure engine describes but cannot make.

use std::net::{IpAddr, SocketAddr, SocketAddrV4};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::engine::cast_name::{self, CastNameRead, CAST_SETUP_PORT};

/// The setup JSON is about 1.3 KB; anything this large is not that endpoint.
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// The name a TV reports on its Cast setup endpoint (see
/// [`engine::cast_name`](crate::engine::cast_name)). `None` when the address is
/// not a LAN IPv4 address, nothing answers within `timeout`, or the answer has
/// no usable name. Display text only, never identity.
pub async fn cast_display_name(ip: IpAddr, timeout: Duration) -> Option<String> {
    fetch_cast_name(lookup_addr(ip)?, timeout).await
}

fn lookup_addr(ip: IpAddr) -> Option<SocketAddrV4> {
    match ip {
        IpAddr::V4(v4) if cast_name::is_lan_ipv4(v4) => {
            Some(SocketAddrV4::new(v4, CAST_SETUP_PORT))
        }
        _ => None,
    }
}

/// No address check of its own: everything outside tests goes through
/// [`cast_display_name`].
async fn fetch_cast_name(addr: SocketAddrV4, timeout: Duration) -> Option<String> {
    let exchange = async {
        let mut stream = TcpStream::connect(SocketAddr::V4(addr)).await.ok()?;
        stream
            .write_all(cast_name::request(addr).as_bytes())
            .await
            .ok()?;
        let mut response = Vec::with_capacity(4096);
        let mut chunk = [0u8; 4096];
        loop {
            let n = stream.read(&mut chunk).await.ok()?;
            let room = MAX_RESPONSE_BYTES - response.len();
            response.extend_from_slice(&chunk[..n.min(room)]);
            // The TVs keep the connection open after answering, so the parser
            // decides when the response is complete; EOF and the cap only end it.
            let finished = n == 0 || response.len() >= MAX_RESPONSE_BYTES;
            if let CastNameRead::Done(name) = cast_name::read_response(&response, finished) {
                return name;
            }
        }
    };
    tokio::time::timeout(timeout, exchange).await.ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;
    use std::time::Instant;
    use tokio::net::TcpListener;

    async fn local_server() -> (TcpListener, SocketAddrV4) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let SocketAddr::V4(addr) = listener.local_addr().unwrap() else {
            unreachable!("bound to an IPv4 address");
        };
        (listener, addr)
    }

    async fn read_request(sock: &mut TcpStream) -> String {
        let mut request = Vec::new();
        let mut chunk = [0u8; 1024];
        while !request.ends_with(b"\r\n\r\n") {
            let n = sock.read(&mut chunk).await.unwrap();
            assert!(n > 0, "client closed before finishing its request");
            request.extend_from_slice(&chunk[..n]);
        }
        String::from_utf8(request).unwrap()
    }

    fn eureka_reply(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nCache-Control:no-cache\r\nContent-Length:{}\r\nContent-Type:application/json\r\n\r\n{body}",
            body.len()
        )
    }

    #[tokio::test]
    async fn reads_the_name_without_waiting_for_the_tv_to_close() {
        let (listener, addr) = local_server().await;
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let request = read_request(&mut sock).await;
            let reply = eureka_reply(
                r#"{"mac_address":"00:00:5E:00:53:01","name":"Living Room TV","ssid":""}"#,
            );
            // Split mid-body, then held open like the real TVs.
            let (first, second) = reply.split_at(reply.len() - 10);
            sock.write_all(first.as_bytes()).await.unwrap();
            sock.flush().await.unwrap();
            tokio::time::sleep(Duration::from_millis(50)).await;
            sock.write_all(second.as_bytes()).await.unwrap();
            let mut rest = [0u8; 64];
            let after = sock.read(&mut rest).await.unwrap_or(0);
            (request, after)
        });
        let started = Instant::now();
        let name = fetch_cast_name(addr, Duration::from_secs(5)).await;
        assert_eq!(name.as_deref(), Some("Living Room TV"));
        assert!(started.elapsed() < Duration::from_secs(2));
        let (request, after) = server.await.unwrap();
        assert_eq!(request, cast_name::request(addr));
        assert_eq!(after, 0, "the client closes without sending more");
    }

    #[tokio::test]
    async fn a_closed_or_silent_endpoint_yields_nothing() {
        // The TVs answer an HTTP/1.0 request by closing without a reply.
        let (listener, addr) = local_server().await;
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            read_request(&mut sock).await;
        });
        assert_eq!(fetch_cast_name(addr, Duration::from_secs(5)).await, None);
        server.await.unwrap();

        let (listener, addr) = local_server().await;
        let server = tokio::spawn(async move {
            let (_held, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let started = Instant::now();
        assert_eq!(
            fetch_cast_name(addr, Duration::from_millis(200)).await,
            None
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        server.abort();
    }

    #[tokio::test]
    async fn an_endless_response_stops_at_the_size_cap() {
        let (listener, addr) = local_server().await;
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            read_request(&mut sock).await;
            let _ = sock.write_all(b"HTTP/1.1 200 OK\r\n").await;
            let filler = [b'x'; 4096];
            while sock.write_all(&filler).await.is_ok() {}
        });
        let started = Instant::now();
        assert_eq!(fetch_cast_name(addr, Duration::from_secs(5)).await, None);
        assert!(started.elapsed() < Duration::from_secs(2));
        server.await.unwrap();
    }

    #[test]
    fn only_lan_ipv4_addresses_are_asked() {
        assert_eq!(
            lookup_addr(IpAddr::V4(Ipv4Addr::new(192, 168, 42, 71))),
            Some(SocketAddrV4::new(Ipv4Addr::new(192, 168, 42, 71), 8008))
        );
        for ip in [
            "8.8.8.8",
            "192.0.2.1",
            "127.0.0.1",
            "::1",
            "fe80::1",
            "2001:db8::5",
        ] {
            assert_eq!(lookup_addr(ip.parse().unwrap()), None, "{ip}");
        }
    }

    #[tokio::test]
    async fn a_public_address_is_refused_without_connecting() {
        // TEST-NET-1 is never routed, so a connection attempt would sit until
        // the timeout instead of returning at once.
        let started = Instant::now();
        let name = cast_display_name("192.0.2.1".parse().unwrap(), Duration::from_secs(3)).await;
        assert_eq!(name, None);
        assert!(started.elapsed() < Duration::from_millis(500));
    }
}
