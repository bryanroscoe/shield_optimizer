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
        send(&mut socket, b"CNXN", 0x01000000, 1048576, b"device::\0");
        let (command, local, payload) = receive(&mut socket);
        assert_eq!(command, b"OPEN");
        assert_eq!(payload, b"shell,v2,raw:echo ok\0");
        send(&mut socket, b"OKAY", 1, local, &[]);
        for (channel, payload) in [(1u8, b"ok\n".as_slice()), (3u8, &[0][..])] {
            let mut frame = vec![channel];
            frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            frame.extend_from_slice(payload);
            send(&mut socket, b"WRTE", 1, local, &frame);
            assert_eq!(receive(&mut socket).0, b"OKAY");
        }
        send(&mut socket, b"CLSE", 1, local, &[]);
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
    assert!(error.contains("Pair PIN"), "{error}");
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

fn download_stat(socket: &mut TcpStream) -> u32 {
    let (command, local, payload) = receive(socket);
    assert_eq!(command, b"OPEN");
    assert_eq!(payload, b"sync:");
    send(socket, b"OKAY", 1, local, &[]);
    for _ in 0..2 {
        assert_eq!(receive(socket).0, b"WRTE");
        send(socket, b"OKAY", 1, local, &[]);
    }
    let mut stat = b"STAT".to_vec();
    for value in [0o100_644_u32, 3, 0] {
        stat.extend_from_slice(&value.to_le_bytes());
    }
    send(socket, b"WRTE", 1, local, &stat);
    assert_eq!(receive(socket).0, b"OKAY");
    local
}

fn finish_download_sync(socket: &mut TcpStream, local: u32) {
    assert!(receive(socket).2.starts_with(b"QUIT"));
    send(socket, b"OKAY", 1, local, &[]);
    send(socket, b"CLSE", 1, local, &[]);
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
                send(&mut selected, b"CNXN", 0x01000000, 1048576, b"device::\0");
                serve_download_identity(&mut selected);
                let mut transfer = accept(&listener);
                assert_eq!(receive(&mut transfer).0, b"CNXN");
                send(&mut transfer, b"CNXN", 0x01000000, 1048576, b"device::\0");
                serve_download_identity(&mut transfer);
                let local = download_stat(&mut transfer);
                finish_download_sync(&mut transfer, local);
                let local = download_stat(&mut transfer);
                for _ in 0..2 {
                    assert_eq!(receive(&mut transfer).0, b"WRTE");
                    send(&mut transfer, b"OKAY", 1, local, &[]);
                }
                let mut response = b"DATA".to_vec();
                response.extend_from_slice(&3u32.to_le_bytes());
                response.extend_from_slice(b"abc");
                if rejected {
                    response.extend_from_slice(b"FAIL");
                    response.extend_from_slice(&17u32.to_le_bytes());
                    response.extend_from_slice(b"Permission denied");
                } else {
                    response.extend_from_slice(b"DONE");
                    response.extend_from_slice(&0u32.to_le_bytes());
                }
                for bytes in response.chunks(5) {
                    send(&mut transfer, b"WRTE", 1, local, bytes);
                    assert_eq!(receive(&mut transfer).0, b"OKAY");
                }
                if !rejected {
                    finish_download_sync(&mut transfer, local);
                }
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
