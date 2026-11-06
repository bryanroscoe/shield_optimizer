use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

fn fixture_key() -> tempfile::NamedTempFile {
    // Reuse the vendor's public test key; never read a user's ADB identity.
    let source =
        include_str!("../../../vendor/adb_client/src/message_devices/models/adb_rsa_key.rs");
    let start = source.find("-----BEGIN PRIVATE KEY-----").unwrap();
    let end = start
        + source[start..].find("-----END PRIVATE KEY-----").unwrap()
        + "-----END PRIVATE KEY-----".len();
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&source.as_bytes()[start..end]).unwrap();
    file
}

fn receive(socket: &mut TcpStream) -> (Vec<u8>, u32, Vec<u8>) {
    let mut header = [0; 24];
    socket.read_exact(&mut header).unwrap();
    let size = u32::from_le_bytes(header[12..16].try_into().unwrap());
    assert!(size < 65536);
    let mut payload = vec![0; size as usize];
    socket.read_exact(&mut payload).unwrap();
    (
        header[..4].to_vec(),
        u32::from_le_bytes(header[4..8].try_into().unwrap()),
        payload,
    )
}

fn send(socket: &mut TcpStream, command: &[u8; 4], arg0: u32, arg1: u32, payload: &[u8]) {
    let command = u32::from_le_bytes(*command);
    for field in [
        command,
        arg0,
        arg1,
        payload.len() as u32,
        payload.iter().map(|b| u32::from(*b)).sum(),
        command ^ u32::MAX,
    ] {
        socket.write_all(&field.to_le_bytes()).unwrap();
    }
    socket.write_all(payload).unwrap();
}

/// A device banner that negotiates shell v2, as every TV since Android 7 does.
const SHELL_V2_DEVICE: &[u8] = b"device::features=shell_v2\0";

/// Serve one shell v2 command: each `(channel, payload)` is one shell packet
/// in its own WRTE, then the device closes the stream.
fn serve_shell(socket: &mut TcpStream, command: &str, packets: &[(u8, &[u8])]) {
    let (kind, local, payload) = receive(socket);
    assert_eq!(kind, b"OPEN");
    assert_eq!(payload, format!("shell,v2,raw:{command}\0").as_bytes());
    send(socket, b"OKAY", 1, local, &[]);
    for (channel, payload) in packets {
        let mut frame = vec![*channel];
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(payload);
        send(socket, b"WRTE", 1, local, &frame);
        assert_eq!(receive(socket).0, b"OKAY");
    }
    send(socket, b"CLSE", 1, local, &[]);
}

struct Connected {
    driver: WirelessAdb,
    serial: String,
    peer: std::thread::JoinHandle<()>,
    _key: tempfile::NamedTempFile,
}

/// A driver connected to a fresh loopback peer that runs `serve` after CNXN.
async fn connected_driver<F>(serve: F) -> Connected
where
    F: FnOnce(TcpStream, TcpListener) + Send + 'static,
{
    let key = fixture_key();
    let driver = WirelessAdb::with_existing_key(key.path().into(), Leases::new());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer = std::thread::spawn(move || {
        let mut socket = accept(&listener);
        assert_eq!(receive(&mut socket).0, b"CNXN");
        send(&mut socket, b"CNXN", 0x01000000, 1048576, SHELL_V2_DEVICE);
        serve(socket, listener);
    });
    driver.begin_request(1).unwrap();
    driver.connect("127.0.0.1", port, 1).await.unwrap();
    Connected {
        driver,
        serial: format!("127.0.0.1:{port}"),
        peer,
        _key: key,
    }
}

fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let socket = loop {
        match listener.accept() {
            Ok((socket, _)) => break socket,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("fixture accept deadline: {error}"),
        }
    };
    socket.set_nonblocking(false).unwrap();
    socket.set_nodelay(true).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    socket
}

fn start_tls(socket: &mut TcpStream) {
    send(socket, b"STLS", 0x01000000, 0, &[]);
    assert_eq!(receive(socket).0, b"STLS");
    let mut header = [0; 5];
    socket.read_exact(&mut header).unwrap();
    assert_eq!(header[0], 22); // TLS Handshake record, containing ClientHello.
    let size = u16::from_be_bytes([header[3], header[4]]) as usize;
    socket.read_exact(&mut vec![0; size]).unwrap();
}

fn assert_closed(socket: &mut TcpStream) {
    match socket.read(&mut [0]) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
            ) => {}
        result => panic!("canceled handshake still owns its socket: {result:?}"),
    }
}

#[tokio::test]
async fn canceling_handshake_closes_socket_before_auth_timeout_and_releases_lease() {
    for stage in [0, 1, 2] {
        let key = fixture_key();
        let operations = Leases::new();
        let driver = Arc::new(WirelessAdb::with_existing_key(
            key.path().into(),
            operations.clone(),
        ));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (waiting, ready) = tokio::sync::oneshot::channel();
        let peer = std::thread::spawn(move || {
            let mut socket = accept(&listener);
            assert_eq!(receive(&mut socket).0, b"CNXN");
            if stage == 1 {
                send(&mut socket, b"AUTH", 1, 0, &[42; 20]);
                let signature = receive(&mut socket);
                assert_eq!(
                    (signature.0.as_slice(), signature.1),
                    (b"AUTH".as_slice(), 2)
                );
                send(&mut socket, b"AUTH", 1, 0, &[43; 20]);
                let public_key = receive(&mut socket);
                assert_eq!(
                    (public_key.0.as_slice(), public_key.1),
                    (b"AUTH".as_slice(), 3)
                );
            }
            if stage == 2 {
                start_tls(&mut socket);
            }
            waiting.send(()).unwrap();
            assert_closed(&mut socket);
        });
        driver.begin_request(1).unwrap();
        let connecting = driver.clone();
        let task = tokio::spawn(async move { connecting.connect("127.0.0.1", port, 1).await });
        tokio::time::timeout(Duration::from_secs(2), ready)
            .await
            .unwrap()
            .unwrap();
        if stage == 1 {
            tokio::time::timeout(Duration::from_secs(2), async {
                while !driver.awaiting_authorization() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            // The mobile cancel/replacement path invalidates a still-running request.
            driver.begin_request(2).unwrap();
            assert!(tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap()
                .is_err());
            assert!(lock_recovering(&driver.requests).check(2).is_ok());
        } else {
            // Desktop Forget/timeout drops the asynchronous connection future.
            task.abort();
            assert!(task.await.unwrap_err().is_cancelled());
        }
        assert!(
            operations
                .wait_idle(tokio::time::Instant::now() + Duration::from_secs(2))
                .await
        );
        assert!(!driver.awaiting_authorization());
        assert!(driver.info().is_none());
        peer.join().unwrap();
    }
}

#[test]
fn total_handshake_deadline_closes_a_silent_peer() {
    let key = fixture_key();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let peer = std::thread::spawn(move || {
        let mut socket = accept(&listener);
        assert_eq!(receive(&mut socket).0, b"CNXN");
        assert_closed(&mut socket);
    });
    let control = adb_client::tcp::BoundedShellControl::new(Duration::from_secs(1)).unwrap();
    let start = std::time::Instant::now();
    assert!(ADBTcpDevice::new_with_control_and_auth_observer(
        address,
        key.path(),
        true,
        control,
        &|| panic!("no AUTH challenge was received")
    )
    .is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    peer.join().unwrap();
}

#[tokio::test]
async fn completed_handshake_keeps_socket_for_liveness_after_attempt_guard_drops() {
    let key = fixture_key();
    let driver = Arc::new(WirelessAdb::with_existing_key(
        key.path().into(),
        Leases::new(),
    ));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer = std::thread::spawn(move || {
        let mut socket = accept(&listener);
        assert_eq!(receive(&mut socket).0, b"CNXN");
        send(&mut socket, b"CNXN", 0x01000000, 1048576, SHELL_V2_DEVICE);
        serve_shell(&mut socket, "echo ok", &[(1, b"ok\n"), (3, &[0])]);
    });
    driver.begin_request(1).unwrap();
    driver.connect("127.0.0.1", port, 1).await.unwrap();
    assert!(lock_recovering(&driver.requests).control.is_none());
    assert!(driver.status().await.connected);
    peer.join().unwrap();
}

#[tokio::test]
async fn cancel_after_authentication_closes_socket_while_publication_is_blocked() {
    let key = fixture_key();
    let operations = Leases::new();
    let driver = Arc::new(WirelessAdb::with_existing_key(
        key.path().into(),
        operations.clone(),
    ));
    let connection = driver.conn.clone();
    let (held, held_rx) = tokio::sync::oneshot::channel();
    let (release, release_rx) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        let _connection = lock_recovering(&connection);
        held.send(()).unwrap();
        // Dropping the sender also releases the lock if an assertion fails.
        let _ = release_rx.recv_timeout(Duration::from_secs(5));
    });
    held_rx.await.unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let (approve, approved) = std::sync::mpsc::channel();
    let (closed, closed_rx) = tokio::sync::oneshot::channel();
    let peer = std::thread::spawn(move || {
        let mut socket = accept(&listener);
        assert_eq!(receive(&mut socket).0, b"CNXN");
        send(&mut socket, b"AUTH", 1, 0, &[42; 20]);
        assert_eq!(receive(&mut socket).1, 2);
        send(&mut socket, b"AUTH", 1, 0, &[43; 20]);
        assert_eq!(receive(&mut socket).1, 3);
        approved.recv_timeout(Duration::from_secs(2)).unwrap();
        send(&mut socket, b"CNXN", 0x01000000, 1048576, b"device::\0");
        assert_closed(&mut socket);
        closed.send(()).unwrap();
    });
    driver.begin_request(1).unwrap();
    let connecting = driver.clone();
    let task = tokio::spawn(async move { connecting.connect("127.0.0.1", port, 1).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while !driver.awaiting_authorization() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    approve.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while driver.awaiting_authorization() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!task.is_finished());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(2), closed_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(driver.info().is_none());
    assert!(
        operations
            .wait_idle(tokio::time::Instant::now() + Duration::from_secs(2))
            .await
    );
    // The canceled worker must drain even while the prior command keeps its lock.
    release.send(()).unwrap();
    assert!(driver.info().is_none());
    holder.join().unwrap();
    peer.join().unwrap();
}

#[tokio::test]
async fn tls_identity_rejection_explains_pairing_without_claiming_a_legacy_prompt() {
    let key = fixture_key();
    let operations = Leases::new();
    let driver = WirelessAdb::with_existing_key(key.path().into(), operations.clone());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer = std::thread::spawn(move || {
        let mut socket = accept(&listener);
        assert_eq!(receive(&mut socket).0, b"CNXN");
        start_tls(&mut socket);
        // Fatal CertificateUnknown alert, as sent by an unpaired Android TLS peer.
        socket.write_all(&[21, 3, 3, 0, 2, 2, 46]).unwrap();
    });
    driver.begin_request(1).unwrap();
    let error = driver.connect("127.0.0.1", port, 1).await.unwrap_err();
    assert!(error.contains("hasn't paired"), "{error}");
    assert!(error.contains("Pair device with pairing code"));
    assert!(error.contains("pairing port"));
    assert!(error.contains("separate port"));
    assert!(!error.contains("Allow debugging"));
    assert!(!driver.awaiting_authorization());
    assert!(driver.info().is_none());
    assert_eq!(operations.active(), 0);
    peer.join().unwrap();
}

fn serve_download_identity(socket: &mut TcpStream) {
    let (command, local, payload) = receive(socket);
    assert_eq!(command, b"OPEN");
    assert_eq!(payload, b"shell,v2,raw:getprop ro.serialno\0");
    send(socket, b"OKAY", 1, local, &[]);
    assert_eq!(receive(socket).2, [4, 0, 0, 0, 0]);
    for (channel, payload) in [(1u8, b"fixture-tv\n".as_slice()), (3, &[0][..])] {
        let mut frame = vec![channel];
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(payload);
        send(socket, b"WRTE", 1, local, &frame);
        assert_eq!(receive(socket).0, b"OKAY");
    }
    send(socket, b"CLSE", 1, local, &[]);
}

/// A legacy sync service (no `stat_v2`/`sendrecv_v2` negotiated) serving
/// one three-byte file. Requests are parsed from the byte stream, so it does
/// not depend on how the client batches them into WRTEs.
fn serve_sync(socket: &mut TcpStream, rejected: bool) {
    let (command, local, payload) = receive(socket);
    assert_eq!(command, b"OPEN");
    assert_eq!(payload, b"sync:\0");
    send(socket, b"OKAY", 1, local, &[]);
    let mut pending = Vec::new();
    loop {
        let (command, _, payload) = receive(socket);
        match command.as_slice() {
            b"WRTE" => {
                send(socket, b"OKAY", 1, local, &[]);
                pending.extend_from_slice(&payload);
            }
            b"OKAY" => continue,
            b"CLSE" => return,
            other => panic!("unexpected {other:?} on the sync stream"),
        }
        while pending.len() >= 8 {
            let size = u32::from_le_bytes(pending[4..8].try_into().unwrap()) as usize;
            if pending.len() < 8 + size {
                break;
            }
            let request: Vec<u8> = pending.drain(..8 + size).collect();
            let mut reply = Vec::new();
            match &request[..4] {
                b"STAT" => {
                    assert_eq!(&request[8..], b"/fixture");
                    reply.extend_from_slice(b"STAT");
                    for value in [0o100_644_u32, 3, 0] {
                        reply.extend_from_slice(&value.to_le_bytes());
                    }
                }
                b"RECV" => {
                    assert_eq!(&request[8..], b"/fixture");
                    reply.extend_from_slice(b"DATA");
                    reply.extend_from_slice(&3u32.to_le_bytes());
                    reply.extend_from_slice(b"abc");
                    if rejected {
                        reply.extend_from_slice(b"FAIL");
                        reply.extend_from_slice(&17u32.to_le_bytes());
                        reply.extend_from_slice(b"Permission denied");
                    } else {
                        reply.extend_from_slice(b"DONE");
                        reply.extend_from_slice(&0u32.to_le_bytes());
                    }
                }
                b"QUIT" => {
                    send(socket, b"CLSE", 1, local, &[]);
                    return;
                }
                other => panic!("unexpected sync request {other:?}"),
            }
            for bytes in reply.chunks(5) {
                send(socket, b"WRTE", 1, local, bytes);
            }
        }
    }
}

#[tokio::test]
async fn native_download_publishes_only_success_and_keeps_selected_tv_connected() {
    for rejected in [false, true] {
        for existing in [false, true] {
            let key = fixture_key();
            let driver = WirelessAdb::with_existing_key(key.path().into(), Leases::new());
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let peer = std::thread::spawn(move || {
                let mut selected = accept(&listener);
                assert_eq!(receive(&mut selected).0, b"CNXN");
                send(&mut selected, b"CNXN", 0x01000000, 1048576, SHELL_V2_DEVICE);
                serve_download_identity(&mut selected);
                let mut transfer = accept(&listener);
                assert_eq!(receive(&mut transfer).0, b"CNXN");
                send(&mut transfer, b"CNXN", 0x01000000, 1048576, SHELL_V2_DEVICE);
                serve_download_identity(&mut transfer);
                serve_sync(&mut transfer, rejected);
                // The original session must still accept a fresh read after a
                // dedicated transfer fails, without reconnecting or replaying it.
                serve_download_identity(&mut selected);
            });
            driver.begin_request(1).unwrap();
            driver.connect("127.0.0.1", port, 1).await.unwrap();
            let serial = format!("127.0.0.1:{port}");
            let dir = tempfile::tempdir().unwrap();
            let local = dir.path().join("fixture");
            if existing {
                std::fs::write(&local, b"previous download").unwrap();
            }
            let result = driver
                .raw_transfer(&["-s", &serial, "pull", "/fixture", local.to_str().unwrap()])
                .await;
            if rejected {
                assert!(result
                    .unwrap_err()
                    .to_string()
                    .contains("Permission denied"));
                if existing {
                    assert_eq!(std::fs::read(&local).unwrap(), b"previous download");
                } else {
                    assert!(!local.exists());
                }
            } else {
                result.unwrap();
                assert_eq!(std::fs::read(&local).unwrap(), b"abc");
            }
            assert_eq!(
                std::fs::read_dir(dir.path()).unwrap().count(),
                usize::from(!rejected || existing)
            );
            assert_eq!(
                driver
                    .verified_owned_hardware_id(&serial, Duration::from_secs(2))
                    .await
                    .unwrap(),
                "fixture-tv"
            );
            peer.join().unwrap();
        }
    }
}

#[tokio::test]
async fn a_peer_silent_before_auth_does_not_claim_the_user_missed_approval() {
    // This peer never sends AUTH, so no approval request is established.
    // It pins the vendored transport's handshake-timeout wording that
    // `is_handshake_timeout` relies on.
    let key = fixture_key();
    let driver = WirelessAdb::with_existing_key(key.path().into(), Leases::new());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer = std::thread::spawn(move || {
        let mut socket = accept(&listener);
        assert_eq!(receive(&mut socket).0, b"CNXN");
        assert_closed(&mut socket);
    });
    driver.begin_request(1).unwrap();
    let error = driver
        .connect_within("127.0.0.1", port, 1, Duration::from_millis(300))
        .await
        .unwrap_err();
    assert!(
        error.contains("timed out") && !error.contains("didn't approve"),
        "{error}"
    );
    assert!(driver.info().is_none());
    peer.join().unwrap();
}

#[tokio::test]
async fn a_closed_port_is_a_refusal_not_an_approval_wait() {
    let port = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let key = fixture_key();
    let driver = WirelessAdb::with_existing_key(key.path().into(), Leases::new());
    driver.begin_request(1).unwrap();
    let error = driver.connect("127.0.0.1", port, 1).await.unwrap_err();
    assert!(error.starts_with("Connection refused"), "{error}");
    assert!(!error.contains("Allow"), "{error}");
}

#[tokio::test]
async fn a_shell_without_an_exit_status_is_unknown_and_keeps_the_tv_connected() {
    let Connected {
        driver,
        serial,
        peer,
        _key,
    } = connected_driver(|mut socket, _| {
        // Output, then the stream closes before the exit packet.
        serve_shell(
            &mut socket,
            "pm disable-user com.example",
            &[(1, b"partial")],
        );
        // The post-error liveness probe still succeeds.
        serve_shell(&mut socket, "echo ok", &[(1, b"ok\n"), (3, &[0])]);
    })
    .await;
    let error = driver
        .shell(&serial, "pm disable-user com.example")
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("unknown"), "{error}");
    assert!(!error.contains(CONNECTION_LOST_PREFIX), "{error}");
    assert_eq!(driver.info().unwrap().0, serial);
    peer.join().unwrap();
}

#[tokio::test]
async fn a_reported_exit_status_is_the_shell_result() {
    let Connected {
        driver,
        serial,
        peer,
        _key,
    } = connected_driver(|mut socket, _| {
        serve_shell(
            &mut socket,
            "pm path com.example",
            &[(1, b"out"), (2, b"err"), (3, &[7])],
        );
    })
    .await;
    let output = driver.shell(&serial, "pm path com.example").await.unwrap();
    assert_eq!(
        (
            output.stdout.as_str(),
            output.stderr.as_str(),
            output.exit_code
        ),
        ("out", "err", Some(7))
    );
    peer.join().unwrap();
}

#[tokio::test]
async fn lost_framing_drops_the_connection_and_the_next_connect_redials() {
    let Connected {
        driver,
        serial,
        peer,
        _key,
    } = connected_driver(|mut socket, listener| {
        let (kind, local, _) = receive(&mut socket);
        assert_eq!(kind, b"OPEN");
        send(&mut socket, b"OKAY", 1, local, &[]);
        // A WRTE header whose magic does not match its command: the client
        // can no longer trust where the next message starts.
        let command = u32::from_le_bytes(*b"WRTE");
        for field in [command, 1, local, 0, 0, command] {
            socket.write_all(&field.to_le_bytes()).unwrap();
        }
        assert_closed(&mut socket);
        let mut again = accept(&listener);
        assert_eq!(receive(&mut again).0, b"CNXN");
        send(&mut again, b"CNXN", 0x01000000, 1048576, SHELL_V2_DEVICE);
    })
    .await;
    let error = driver
        .shell(&serial, "echo hi")
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(CONNECTION_LOST_PREFIX), "{error}");
    assert!(driver.info().is_none());
    assert!(driver
        .shell(&serial, "echo hi")
        .await
        .unwrap_err()
        .to_string()
        .contains("Not connected"));
    let (host, port) = serial.rsplit_once(':').unwrap();
    driver.begin_request(2).unwrap();
    driver
        .connect(host, port.parse().unwrap(), 2)
        .await
        .unwrap();
    assert_eq!(driver.info().unwrap().0, serial);
    peer.join().unwrap();
}

/// A second RSA 2048 key for the preferred-key tests, PKCS#1 like an old
/// platform-tools `adbkey` could be, generated once per test binary.
fn preferred_pkcs1_pem() -> &'static str {
    static PEM: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PEM.get_or_init(|| {
        use rsa::pkcs1::{EncodeRsaPrivateKey, LineEnding};
        rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048)
            .unwrap()
            .to_pkcs1_pem(LineEnding::LF)
            .unwrap()
            .to_string()
    })
}

/// The AUTH signature `key` makes over `token`. PKCS#1 v1.5 is
/// deterministic, so the device's view can be compared byte for byte.
fn auth_signature(key: &rsa::RsaPrivateKey, token: &[u8; 20]) -> Vec<u8> {
    const SHA1_DIGEST_INFO: [u8; 15] = [
        0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04, 0x14,
    ];
    let mut digest_info = SHA1_DIGEST_INFO.to_vec();
    digest_info.extend_from_slice(token);
    key.sign(rsa::Pkcs1v15Sign::new_unprefixed(), &digest_info)
        .unwrap()
}

/// Serve AUTH: each `(token, expected)` is one token and the signature or
/// public key kind (2 or 3) and payload the host must answer with.
fn serve_auth(listener: TcpListener, rounds: Vec<([u8; 20], u32, Option<Vec<u8>>)>) {
    let mut socket = accept(&listener);
    assert_eq!(receive(&mut socket).0, b"CNXN");
    for (token, kind, expected) in rounds {
        send(&mut socket, b"AUTH", 1, 0, &token);
        let (command, arg0, payload) = receive(&mut socket);
        assert_eq!((command.as_slice(), arg0), (b"AUTH".as_slice(), kind));
        if let Some(expected) = expected {
            assert_eq!(payload, expected, "AUTH answered with the wrong key");
        }
    }
    send(&mut socket, b"CNXN", 0x01000000, 1048576, SHELL_V2_DEVICE);
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let _ = std::io::copy(&mut socket, &mut std::io::sink());
}

#[tokio::test]
async fn preferred_keys_sign_before_the_own_key() {
    use rsa::pkcs1::DecodeRsaPrivateKey;
    use rsa::pkcs8::DecodePrivateKey;
    let own = fixture_key();
    let own_before = std::fs::read(own.path()).unwrap();
    let own_private =
        rsa::RsaPrivateKey::from_pkcs8_pem(&std::fs::read_to_string(own.path()).unwrap()).unwrap();
    let preferred_private = rsa::RsaPrivateKey::from_pkcs1_pem(preferred_pkcs1_pem()).unwrap();
    let preferred = load_external_identity_pem(preferred_pkcs1_pem().as_bytes()).unwrap();
    let driver = WirelessAdb::with_existing_key(own.path().into(), Leases::new())
        .with_preferred_keys(preferred);
    assert_eq!(driver.preferred_key_count(), 1);

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let rounds = vec![
        (
            [7; 20],
            2,
            Some(auth_signature(&preferred_private, &[7; 20])),
        ),
        ([8; 20], 2, Some(auth_signature(&own_private, &[8; 20]))),
    ];
    let peer = std::thread::spawn(move || serve_auth(listener, rounds));
    driver.begin_request(1).unwrap();
    driver.connect("127.0.0.1", port, 1).await.unwrap();
    assert!(!driver.awaiting_authorization());
    drop(driver);
    peer.join().unwrap();
    // The own key was only read.
    assert_eq!(std::fs::read(own.path()).unwrap(), own_before);
}

#[tokio::test]
async fn without_preferred_keys_the_own_key_signs_and_is_the_one_offered() {
    use rsa::pkcs8::DecodePrivateKey;
    let own = fixture_key();
    let own_private =
        rsa::RsaPrivateKey::from_pkcs8_pem(&std::fs::read_to_string(own.path()).unwrap()).unwrap();
    let driver = WirelessAdb::with_existing_key(own.path().into(), Leases::new());
    assert_eq!(driver.preferred_key_count(), 0);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let rounds = vec![
        ([7; 20], 2, Some(auth_signature(&own_private, &[7; 20]))),
        ([8; 20], 3, None),
    ];
    let peer = std::thread::spawn(move || serve_auth(listener, rounds));
    driver.begin_request(1).unwrap();
    driver.connect("127.0.0.1", port, 1).await.unwrap();
    drop(driver);
    peer.join().unwrap();
}

#[test]
fn external_identities_load_from_pkcs8_or_pkcs1_and_refuse_anything_else() {
    use rsa::pkcs1::DecodeRsaPrivateKey;
    use rsa::pkcs8::{EncodePrivateKey, LineEnding};
    let pkcs1 = preferred_pkcs1_pem();
    let pkcs8 = rsa::RsaPrivateKey::from_pkcs1_pem(pkcs1)
        .unwrap()
        .to_pkcs8_pem(LineEnding::LF)
        .unwrap();
    assert_eq!(
        load_external_identity_pem(pkcs1.as_bytes()).unwrap().len(),
        1
    );
    assert_eq!(
        load_external_identity_pem(pkcs8.as_bytes()).unwrap().len(),
        1
    );

    let mut oversized = pkcs8.as_bytes().to_vec();
    oversized.resize(32769, b'\n');
    let small = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 1024)
        .unwrap()
        .to_pkcs8_pem(LineEnding::LF)
        .unwrap();
    for (bytes, why) in [
        (oversized.as_slice(), "over 32 KiB"),
        (&[0xff, 0xfe, 0x00][..], "not UTF-8"),
        (small.as_bytes(), "1024-bit"),
        (
            b"-----BEGIN OPENSSH PRIVATE KEY-----\nAAAA\n".as_slice(),
            "OpenSSH",
        ),
        (&pkcs1.as_bytes()[..400], "truncated"),
        (b"".as_slice(), "empty"),
    ] {
        let error = load_external_identity_pem(bytes).expect_err(why);
        assert!(
            !error.contains("MII") && !error.contains("AAAA"),
            "{why}: {error}"
        );
    }
}
