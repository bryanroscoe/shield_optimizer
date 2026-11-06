//! Local-network lookups that the pure engine describes but cannot make.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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

pub(crate) type NameLookup =
    Arc<dyn Fn(Ipv4Addr) -> Pin<Box<dyn Future<Output = Option<String>> + Send>> + Send + Sync>;

const LOOKUP_TIMEOUT: Duration = Duration::from_millis(1_200);
const LOOKUPS_BUDGET: Duration = Duration::from_millis(1_500);
/// Short too: the address may be handed to another TV, and a stale name there
/// would point the setup help at the wrong screen.
const NAME_TTL: Duration = Duration::from_secs(60);
/// Short, so a TV that was asleep or still booting gets asked again soon.
const MISS_TTL: Duration = Duration::from_secs(30);

/// Cast names for TVs adb lists but cannot query yet (unauthorized),
/// cached so a device list refreshed every few seconds doesn't wait on the
/// network each time. Display text only: never identity, and never compared.
pub struct CastNames {
    lookup: NameLookup,
    cache: Mutex<HashMap<Ipv4Addr, (Instant, Option<String>)>>,
}

impl CastNames {
    pub fn live() -> Self {
        Self::with_lookup(Arc::new(|ip| {
            Box::pin(cast_display_name(IpAddr::V4(ip), LOOKUP_TIMEOUT))
        }))
    }

    pub(crate) fn with_lookup(lookup: NameLookup) -> Self {
        Self {
            lookup,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Names for `hosts`, from cache or asked concurrently within one short
    /// budget. A host missing from the result has no name to show.
    pub async fn names(&self, hosts: &[Ipv4Addr]) -> HashMap<Ipv4Addr, String> {
        let now = Instant::now();
        let mut names = HashMap::new();
        let mut ask: Vec<Ipv4Addr> = Vec::new();
        {
            let cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
            for &ip in hosts {
                match cache.get(&ip) {
                    Some((at, name)) if now.duration_since(*at) < ttl(name) => {
                        if let Some(name) = name {
                            names.insert(ip, name.clone());
                        }
                    }
                    _ => ask.push(ip),
                }
            }
        }
        ask.sort_unstable();
        ask.dedup();
        if ask.is_empty() {
            return names;
        }
        let mut lookups = tokio::task::JoinSet::new();
        for ip in ask {
            let lookup = self.lookup.clone();
            lookups.spawn(async move { (ip, lookup(ip).await) });
        }
        let mut answered = Vec::new();
        let collect = async {
            while let Some(joined) = lookups.join_next().await {
                if let Ok(answer) = joined {
                    answered.push(answer);
                }
            }
        };
        // Dropping the set aborts any lookup still running; those hosts are
        // asked again on the next refresh.
        let _ = tokio::time::timeout(LOOKUPS_BUDGET, collect).await;
        let mut cache = self.cache.lock().unwrap_or_else(|p| p.into_inner());
        let at = Instant::now();
        for (ip, name) in answered {
            if let Some(name) = &name {
                names.insert(ip, name.clone());
            }
            cache.insert(ip, (at, name));
        }
        names
    }
}

fn ttl(name: &Option<String>) -> Duration {
    if name.is_some() {
        NAME_TTL
    } else {
        MISS_TTL
    }
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

    fn counting(answer: Option<&'static str>) -> (CastNames, Arc<std::sync::atomic::AtomicUsize>) {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = calls.clone();
        let names = CastNames::with_lookup(Arc::new(move |_ip| {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async move { answer.map(str::to_string) })
        }));
        (names, calls)
    }

    #[tokio::test]
    async fn a_found_name_is_cached_and_not_asked_again() {
        let (names, calls) = counting(Some("Living Room TV"));
        let ip = Ipv4Addr::new(192, 168, 42, 143);
        for _ in 0..3 {
            let got = names.names(&[ip, ip]).await;
            assert_eq!(got.get(&ip).map(String::as_str), Some("Living Room TV"));
        }
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_miss_is_cached_briefly_and_yields_no_name() {
        let (names, calls) = counting(None);
        let ip = Ipv4Addr::new(192, 168, 42, 25);
        assert!(names.names(&[ip]).await.is_empty());
        assert!(names.names(&[ip]).await.is_empty());
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_slow_lookup_is_abandoned_within_the_budget() {
        let names = CastNames::with_lookup(Arc::new(|_ip| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(30)).await;
                Some("late".to_string())
            })
        }));
        let started = Instant::now();
        assert!(names.names(&[Ipv4Addr::new(10, 0, 0, 9)]).await.is_empty());
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
