//! Session, sync, shell and install behavior against a scripted fake adbd.

use std::{
    net::SocketAddr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::{
    ADBTcpDevice,
    test_support::{Peer, TempApk, fake_adbd, shared_test_key, shell_packet, sync_reply},
};
use crate::{
    ADBDeviceExt, ADBListItemType, RustADBError, ShellExit,
    message_devices::message_commands::MessageCommand,
};

const TIMEOUT: Duration = Duration::from_secs(5);

fn connect(address: SocketAddr) -> ADBTcpDevice {
    ADBTcpDevice::new_with_custom_private_key_and_auth_timeout(address, shared_test_key(), TIMEOUT)
        .unwrap()
}

fn run<F>(features: &'static str, max_payload: u32, script: F) -> (ADBTcpDevice, impl FnOnce())
where
    F: FnOnce(&mut Peer) + Send + 'static,
{
    let (address, server) = fake_adbd(move |stream| {
        let mut peer = Peer::connect(stream, features, max_payload);
        script(&mut peer);
        peer.finish();
    });
    let device = connect(address);
    (device, move || server.join().unwrap())
}

fn now() -> u32 {
    u32::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

fn fail(message: &str) -> Vec<u8> {
    sync_reply(
        *b"FAIL",
        u32::try_from(message.len()).unwrap(),
        message.as_bytes(),
    )
}

fn stat_v2_body(error: u32, mode: u32, size: u64, mtime: i64) -> Vec<u8> {
    let mut body = vec![0_u8; 68];
    body[0..4].copy_from_slice(&error.to_le_bytes());
    body[20..24].copy_from_slice(&mode.to_le_bytes());
    body[36..44].copy_from_slice(&size.to_le_bytes());
    body[52..60].copy_from_slice(&mtime.to_le_bytes());
    body
}

/// Read a pushed file's DATA chunks up to DONE; returns the data and mtime.
fn receive_pushed(peer: &mut Peer, stream: super::test_support::Stream) -> (Vec<u8>, u32) {
    let mut data = Vec::new();
    loop {
        let (id, arg) = peer.sync_header(stream);
        match &id {
            b"DATA" => {
                assert!(arg as usize <= 64 * 1024);
                data.extend(peer.read_host(stream, arg as usize));
            }
            b"DONE" => return (data, arg),
            other => panic!("unexpected {:?}", String::from_utf8_lossy(other)),
        }
    }
}

// ---- sync: push ----

#[test]
fn push_sends_a_real_mode_and_mtime_and_succeeds_on_okay() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        let (id, spec) = peer.sync_request(sync);
        assert_eq!(&id, b"SEND");
        // 0o100644, as decimal the way AOSP's host writes it.
        assert_eq!(spec, b"/sdcard/x.txt,33188");
        let (data, mtime) = receive_pushed(peer, sync);
        assert_eq!(data, b"hello");
        assert!(now().abs_diff(mtime) < 60, "mtime {mtime}");
        peer.send(sync, &sync_reply(*b"OKAY", 0, &[]), usize::MAX);
        peer.expect_quit(sync);
    });
    device.push(&mut &b"hello"[..], &"/sdcard/x.txt").unwrap();
    drop(device);
    join();
}

#[test]
fn a_push_the_device_refuses_reports_its_reason() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        peer.sync_request(sync);
        receive_pushed(peer, sync);
        peer.send(
            sync,
            &fail("couldn't create file: Read-only file system"),
            usize::MAX,
        );
        peer.close(sync);
        // The connection still works afterwards.
        let shell = peer.accept_open("shell:echo ok");
        peer.send(shell, b"ok\n", usize::MAX);
        peer.close(shell);
    });
    let error = device.push(&mut &b"data"[..], &"/system/x").unwrap_err();
    assert!(
        matches!(&error, RustADBError::SyncFailed(m) if m.contains("Read-only file system")),
        "{error:?}"
    );
    let mut out = Vec::new();
    device
        .shell_command_with_timeout(&"echo ok", Some(&mut out), None, TIMEOUT)
        .unwrap();
    assert_eq!(out, b"ok\n");
    drop(device);
    join();
}

#[test]
fn an_early_fail_and_close_mid_push_is_the_device_refusal_not_a_lost_connection() {
    let (mut device, join) = run("", 4096, |peer| {
        let sync = peer.accept_open("sync:");
        let first = peer.next();
        assert_eq!(first.header().command(), MessageCommand::Write);
        peer.raw(MessageCommand::Okay, sync.remote, sync.local, &[]);
        let refusal = fail("No space left on device");
        peer.raw(MessageCommand::Write, sync.remote, sync.local, &refusal);
        peer.close(sync);
    });
    let data = vec![7_u8; 20_000];
    let error = device.push(&mut &data[..], &"/sdcard/big").unwrap_err();
    assert!(
        matches!(&error, RustADBError::SyncFailed(m) if m.contains("No space left")),
        "{error:?}"
    );
    drop(device);
    join();
}

#[test]
fn an_early_fail_stops_the_upload_and_ends_it_with_done() {
    let (mut device, join) = run("", 4096, |peer| {
        let sync = peer.accept_open("sync:");
        let first = peer.next();
        assert_eq!(first.header().command(), MessageCommand::Write);
        // adbd reports the failure as soon as it happens, then discards data
        // until DONE, as `handle_send_file` does.
        let refusal = fail("couldn't create file: Permission denied");
        peer.raw(MessageCommand::Write, sync.remote, sync.local, &refusal);
        let ack = peer.next();
        assert_eq!(ack.header().command(), MessageCommand::Okay);
        peer.raw(MessageCommand::Okay, sync.remote, sync.local, &[]);
        // The first WRTE held the 21-byte SEND request and the start of a
        // 4088-byte DATA chunk. Only that chunk's last 21 bytes follow (adbd
        // reads a declared chunk in full), then DONE; no new DATA.
        let rest = peer.next();
        assert_eq!(rest.header().command(), MessageCommand::Write);
        assert_eq!(rest.payload().len(), 21 + 8);
        assert!(rest.payload()[..21].iter().all(|&byte| byte == 7));
        assert_eq!(&rest.payload()[21..25], b"DONE");
        peer.raw(MessageCommand::Okay, sync.remote, sync.local, &[]);
        peer.close(sync);
        let shell = peer.accept_open("shell:echo ok");
        peer.send(shell, b"ok\n", usize::MAX);
        peer.close(shell);
    });
    let data = vec![7_u8; 20_000];
    let error = device.push(&mut &data[..], &"/data/x").unwrap_err();
    assert!(
        matches!(&error, RustADBError::SyncFailed(m) if m.contains("Permission denied")),
        "{error:?}"
    );
    let mut out = Vec::new();
    device
        .shell_command_with_timeout(&"echo ok", Some(&mut out), None, TIMEOUT)
        .unwrap();
    assert_eq!(out, b"ok\n");
    drop(device);
    join();
}

#[test]
fn a_silent_close_mid_push_fails_the_request_but_not_the_connection() {
    let (mut device, join) = run("", 4096, |peer| {
        let sync = peer.accept_open("sync:");
        peer.next();
        peer.raw(MessageCommand::Okay, sync.remote, sync.local, &[]);
        peer.close(sync);
    });
    let data = vec![7_u8; 20_000];
    let error = device.push(&mut &data[..], &"/sdcard/big").unwrap_err();
    assert!(
        matches!(&error, RustADBError::ADBRequestFailed(m) if m.contains("closed the sync stream")),
        "{error:?}"
    );
    drop(device);
    join();
}

#[test]
fn push_splits_and_batches_writes_at_a_small_negotiated_payload() {
    let data: Vec<u8> = (0..20_000_u32).map(|n| (n % 251) as u8).collect();
    let expected = data.clone();
    let (mut device, join) = run("", 4096, move |peer| {
        let sync = peer.accept_open("sync:");
        peer.sync_request(sync);
        let (received, _) = receive_pushed(peer, sync);
        assert_eq!(received, expected);
        // Every WRTE fits the device's limit, and they are filled to it.
        assert!(peer.host_writes.iter().all(|&size| size <= 4096));
        assert_eq!(peer.host_writes[0], 4096);
        peer.send(sync, &sync_reply(*b"OKAY", 0, &[]), usize::MAX);
        peer.expect_quit(sync);
    });
    device.push(&mut &data[..], &"/sdcard/d").unwrap();
    drop(device);
    join();
}

/// Android 14's adbd acknowledges a WRTE once per partial flush into the
/// sync service. On a real Android TV 14 image a 5 MiB push drew two OKAYs
/// per 1 MiB WRTE and failed with "Unexpected OKAY while reading".
#[test]
fn push_survives_several_okays_for_each_write_as_android_14_sends() {
    let data: Vec<u8> = (0..20_000_u32).map(|n| (n % 251) as u8).collect();
    let expected = data.clone();
    let (mut device, join) = run("sendrecv_v2", 4096, move |peer| {
        peer.okays_per_write = 3;
        let sync = peer.accept_open("sync:");
        peer.sync_request(sync);
        peer.sync_header(sync);
        peer.read_host(sync, 4);
        let (received, _) = receive_pushed(peer, sync);
        assert_eq!(received, expected);
        peer.send(sync, &sync_reply(*b"OKAY", 0, &[]), usize::MAX);
        peer.expect_quit(sync);
        let shell = peer.accept_open("shell:echo ok");
        peer.send(shell, b"ok\n", usize::MAX);
        peer.close(shell);
    });
    device.push(&mut &data[..], &"/sdcard/big").unwrap();
    let mut out = Vec::new();
    device
        .shell_command_with_timeout(&"echo ok", Some(&mut out), None, TIMEOUT)
        .unwrap();
    assert_eq!(out, b"ok\n");
    drop(device);
    join();
}

#[test]
fn push_uses_send_v2_when_negotiated() {
    let (mut device, join) = run("sendrecv_v2", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        let (id, path) = peer.sync_request(sync);
        assert_eq!((&id, path.as_slice()), (b"SND2", &b"/sdcard/v2"[..]));
        let (id, mode) = peer.sync_header(sync);
        assert_eq!((&id, mode), (b"SND2", 0o100_644));
        assert_eq!(peer.read_host(sync, 4), 0_u32.to_le_bytes(), "flags");
        let (data, _) = receive_pushed(peer, sync);
        assert_eq!(data, b"v2");
        peer.send(sync, &sync_reply(*b"OKAY", 0, &[]), usize::MAX);
        peer.expect_quit(sync);
    });
    device.push(&mut &b"v2"[..], &"/sdcard/v2").unwrap();
    drop(device);
    join();
}

// ---- sync: pull, stat, list ----

#[test]
fn pull_parses_replies_split_anywhere_across_writes() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        assert_eq!(peer.sync_request(sync), (*b"STAT", b"/sdcard/f".to_vec()));
        let mut stat = b"STAT".to_vec();
        for value in [0o100_644_u32, 5, 1_700_000_000] {
            stat.extend(value.to_le_bytes());
        }
        peer.send(sync, &stat, 3);
        assert_eq!(peer.sync_request(sync), (*b"RECV", b"/sdcard/f".to_vec()));
        let mut reply = sync_reply(*b"DATA", 5, b"hello");
        reply.extend(sync_reply(*b"DONE", 0, &[]));
        peer.send(sync, &reply, 5);
        peer.expect_quit(sync);
    });
    let mut out = Vec::new();
    device.pull(&"/sdcard/f", &mut out).unwrap();
    assert_eq!(out, b"hello");
    drop(device);
    join();
}

#[test]
fn pull_uses_sta2_and_rcv2_when_negotiated() {
    let (mut device, join) = run("stat_v2,sendrecv_v2", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        assert_eq!(peer.sync_request(sync), (*b"STA2", b"/sdcard/f".to_vec()));
        let mut stat = b"STA2".to_vec();
        stat.extend(stat_v2_body(0, 0o100_644, 2, 1));
        peer.send(sync, &stat, usize::MAX);
        assert_eq!(peer.sync_request(sync), (*b"RCV2", b"/sdcard/f".to_vec()));
        // sync_recv_v2 { id, flags }; flags 0 asks for no compression.
        assert_eq!(peer.sync_header(sync), (*b"RCV2", 0));
        let mut reply = sync_reply(*b"DATA", 2, b"v2");
        reply.extend(sync_reply(*b"DONE", 0, &[]));
        peer.send(sync, &reply, usize::MAX);
        peer.expect_quit(sync);
    });
    let mut out = Vec::new();
    device.pull(&"/sdcard/f", &mut out).unwrap();
    assert_eq!(out, b"v2");
    drop(device);
    join();
}

#[test]
fn a_bounded_pull_checks_the_size_and_transfers_in_one_session() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        peer.sync_request(sync);
        let mut stat = b"STAT".to_vec();
        for value in [0o100_644_u32, 5, 1] {
            stat.extend(value.to_le_bytes());
        }
        peer.send(sync, &stat, usize::MAX);
        assert_eq!(peer.sync_request(sync), (*b"RECV", b"/sdcard/f".to_vec()));
        let mut reply = sync_reply(*b"DATA", 5, b"hello");
        reply.extend(sync_reply(*b"DONE", 0, &[]));
        peer.send(sync, &reply, usize::MAX);
        peer.expect_quit(sync);

        // Too large: the same session ends with QUIT, no RECV, no second OPEN.
        let sync = peer.accept_open("sync:");
        peer.sync_request(sync);
        let mut stat = b"STAT".to_vec();
        for value in [0o100_644_u32, 5000, 1] {
            stat.extend(value.to_le_bytes());
        }
        peer.send(sync, &stat, usize::MAX);
        peer.expect_quit(sync);
    });
    let mut out = Vec::new();
    device.pull_with_limit("/sdcard/f", &mut out, 5).unwrap();
    assert_eq!(out, b"hello");
    let mut out = Vec::new();
    let error = device
        .pull_with_limit("/sdcard/big", &mut out, 4096)
        .unwrap_err();
    assert!(
        matches!(error, RustADBError::FileTooLarge(5000, 4096)),
        "{error:?}"
    );
    assert!(out.is_empty());
    drop(device);
    join();
}

#[test]
fn a_recv_fail_is_reported_with_its_reason() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        peer.sync_request(sync);
        let mut stat = b"STAT".to_vec();
        for value in [0o100_600_u32, 5, 1] {
            stat.extend(value.to_le_bytes());
        }
        peer.send(sync, &stat, usize::MAX);
        peer.sync_request(sync);
        peer.send(sync, &fail("open failed: Permission denied"), usize::MAX);
        peer.close(sync);
    });
    let mut out = Vec::new();
    let error = device.pull(&"/data/secret", &mut out).unwrap_err();
    assert!(
        matches!(&error, RustADBError::SyncFailed(m) if m.contains("Permission denied")),
        "{error:?}"
    );
    assert!(out.is_empty());
    drop(device);
    join();
}

#[test]
fn a_missing_file_fails_at_stat_on_both_protocols() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        peer.sync_request(sync);
        let mut stat = b"STAT".to_vec();
        stat.extend([0_u8; 12]);
        peer.send(sync, &stat, usize::MAX);
    });
    let error = device.stat(&"/nope").unwrap_err();
    assert!(matches!(error, RustADBError::SyncFailed(_)), "{error:?}");
    drop(device);
    join();

    let (mut device, join) = run("stat_v2", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        assert_eq!(peer.sync_request(sync), (*b"STA2", b"/nope".to_vec()));
        let mut reply = b"STA2".to_vec();
        reply.extend(stat_v2_body(2, 0, 0, 0));
        peer.send(sync, &reply, usize::MAX);
    });
    let error = device.stat(&"/nope").unwrap_err();
    assert!(
        matches!(&error, RustADBError::SyncFailed(m) if m.contains("No such file")),
        "{error:?}"
    );
    drop(device);
    join();
}

#[test]
fn stat_v2_reports_sizes_past_four_gib() {
    let size = 5 * 1024 * 1024 * 1024_u64;
    let (mut device, join) = run("stat_v2,ls_v2,sendrecv_v2", 256 * 1024, move |peer| {
        let sync = peer.accept_open("sync:");
        assert_eq!(peer.sync_request(sync), (*b"STA2", b"/sdcard/big".to_vec()));
        let mut reply = b"STA2".to_vec();
        reply.extend(stat_v2_body(0, 0o100_644, size, 1_700_000_000));
        peer.send(sync, &reply, 11);
        peer.expect_quit(sync);
    });
    let stat = device.stat(&"/sdcard/big").unwrap();
    assert_eq!(stat.file_size, size);
    assert_eq!(stat.mod_time, 1_700_000_000);
    assert_eq!(stat.file_perm, 0o100_644);
    drop(device);
    join();
}

#[test]
fn list_v2_reads_wide_entries_and_skips_unreadable_ones() {
    let size = 6 * 1024 * 1024 * 1024_u64;
    let (mut device, join) = run("ls_v2", 256 * 1024, move |peer| {
        let sync = peer.accept_open("sync:");
        assert_eq!(peer.sync_request(sync), (*b"LIS2", b"/sdcard".to_vec()));
        let mut reply = Vec::new();
        for (error, name) in [(0_u32, "movie.mkv"), (13, "locked")] {
            reply.extend(b"DNT2");
            let mut body = stat_v2_body(error, 0o100_644, size, 1_700_000_000);
            body.extend(u32::try_from(name.len()).unwrap().to_le_bytes());
            reply.extend(body);
            reply.extend(name.as_bytes());
        }
        reply.extend(b"DONE");
        reply.extend([0_u8; 72]);
        peer.send(sync, &reply, 50);
        peer.expect_quit(sync);
    });
    let entries = device.list(&"/sdcard").unwrap();
    assert_eq!(entries.len(), 1);
    match &entries[0] {
        ADBListItemType::File(item) => {
            assert_eq!(item.name, "movie.mkv");
            assert_eq!(item.size, size);
            assert_eq!(item.permissions, 0o644);
        }
        other => panic!("{other:?}"),
    }
    drop(device);
    join();
}

#[test]
fn list_v1_ends_at_done_and_refuses_names_with_separators() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        assert_eq!(peer.sync_request(sync), (*b"LIST", b"/sdcard".to_vec()));
        let mut reply = b"DENT".to_vec();
        for value in [0o040_755_u32, 0, 1, 3] {
            reply.extend(value.to_le_bytes());
        }
        reply.extend(b"Dir");
        reply.extend(b"DONE");
        reply.extend([0_u8; 16]);
        peer.send(sync, &reply, usize::MAX);
        peer.expect_quit(sync);

        let sync = peer.accept_open("sync:");
        peer.sync_request(sync);
        let mut reply = b"DENT".to_vec();
        for value in [0o100_644_u32, 0, 1, 5] {
            reply.extend(value.to_le_bytes());
        }
        reply.extend(b"../up");
        peer.send(sync, &reply, usize::MAX);
    });
    let entries = device.list(&"/sdcard").unwrap();
    assert!(matches!(&entries[..], [ADBListItemType::Directory(item)] if item.name == "Dir"));
    assert!(device.list(&"/sdcard").is_err());
    drop(device);
    join();
}

// ---- shell ----

#[test]
fn shell_v2_is_chosen_by_feature_and_keeps_split_utf8_and_the_exit_code() {
    let (mut device, join) = run("shell_v2", 256 * 1024, |peer| {
        let shell = peer.accept_open("shell,v2,raw:printf caf");
        let mut packets = shell_packet(1, "café".as_bytes());
        packets.extend(shell_packet(2, b"warn"));
        packets.extend(shell_packet(3, &[3]));
        // Seven-byte WRTEs split packet headers and the two bytes of "é".
        peer.send(shell, &packets, 7);
        peer.close(shell);
    });
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let exit = device
        .shell_command_with_timeout(&"printf caf", Some(&mut out), Some(&mut err), TIMEOUT)
        .unwrap();
    assert_eq!(exit, ShellExit::Code(3));
    assert_eq!(String::from_utf8(out).unwrap(), "café");
    assert_eq!(err, b"warn");
    drop(device);
    join();
}

#[test]
fn a_v2_shell_that_closes_without_an_exit_status_is_an_error() {
    let (mut device, join) = run("shell_v2", 256 * 1024, |peer| {
        let shell = peer.accept_open("shell,v2,raw:pm list packages");
        peer.send(shell, &shell_packet(1, b"package:a\n"), usize::MAX);
        peer.close(shell);
    });
    let error = device
        .shell_command_with_timeout(&"pm list packages", Some(&mut Vec::new()), None, TIMEOUT)
        .unwrap_err();
    assert!(
        matches!(&error, RustADBError::ShellExitUnknown),
        "{error:?}"
    );
    drop(device);
    join();
}

#[test]
fn the_legacy_shell_is_used_without_the_feature_and_never_answers_clse() {
    let (mut device, join) = run("cmd", 256 * 1024, |peer| {
        let (first_id, destination) = peer.expect_open();
        assert_eq!(destination, "shell:echo hi");
        let shell = peer.accept(first_id);
        peer.send(shell, b"hi\n", usize::MAX);
        peer.close(shell);
        // Nothing answers the CLSE: the very next packet is the next OPEN,
        // on a fresh id.
        let open = peer.next();
        assert_eq!(open.header().command(), MessageCommand::Open);
        assert_eq!(open.payload(), b"shell:true\0");
        let second_id = open.header().arg0();
        assert!(second_id > first_id);
        let shell = peer.accept(second_id);
        peer.close(shell);
    });
    let mut out = Vec::new();
    let exit = device
        .shell_command_with_timeout(&"echo hi", Some(&mut out), None, TIMEOUT)
        .unwrap();
    assert_eq!(exit, ShellExit::Unreported);
    assert_eq!(out, b"hi\n");
    assert_eq!(
        device
            .shell_command_with_timeout(&"true", None, None, TIMEOUT)
            .unwrap(),
        ShellExit::Unreported
    );
    drop(device);
    join();
}

#[test]
fn a_refused_v2_open_falls_back_to_the_legacy_shell() {
    let (mut device, join) = run("shell_v2", 256 * 1024, |peer| {
        let (local, destination) = peer.expect_open();
        assert_eq!(destination, "shell,v2,raw:id");
        peer.raw(MessageCommand::Clse, 0, local, &[]);
        let shell = peer.accept_open("shell:id");
        peer.send(shell, b"uid=2000\n", usize::MAX);
        peer.close(shell);
    });
    let mut out = Vec::new();
    let exit = device
        .shell_command_with_timeout(&"id", Some(&mut out), None, TIMEOUT)
        .unwrap();
    assert_eq!(exit, ShellExit::Unreported);
    assert_eq!(out, b"uid=2000\n");
    drop(device);
    join();
}

#[test]
fn packets_for_other_streams_are_skipped_and_an_orphan_okay_is_closed() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let (local, _) = peer.expect_open();
        // Leftovers from streams this connection no longer has.
        peer.raw(MessageCommand::Clse, 99, local + 1000, &[]);
        peer.raw(MessageCommand::Write, 98, local + 1001, b"stale");
        peer.raw(MessageCommand::Okay, 97, local + 1002, &[]);
        let shell = peer.accept(local);
        let close = peer.next();
        assert_eq!(close.header().command(), MessageCommand::Clse);
        assert_eq!((close.header().arg0(), close.header().arg1()), (0, 97));
        peer.raw(MessageCommand::Okay, 96, local + 1003, &[]);
        peer.raw(MessageCommand::Write, shell.remote, shell.local, b"fresh");
        let close = peer.next();
        assert_eq!(
            (close.header().command(), close.header().arg1()),
            (MessageCommand::Clse, 96)
        );
        let okay = peer.next();
        assert_eq!(okay.header().command(), MessageCommand::Okay);
        peer.close(shell);
    });
    let mut out = Vec::new();
    device
        .shell_command_with_timeout(&"cat", Some(&mut out), None, TIMEOUT)
        .unwrap();
    assert_eq!(out, b"fresh");
    drop(device);
    join();
}

#[test]
fn legacy_output_past_its_bound_is_an_error_not_a_truncation() {
    let (mut device, join) = run("", 1024 * 1024, |peer| {
        let shell = peer.accept_open("shell:cat big");
        // 64 MiB in 1 MiB WRTEs, then one byte more.
        let chunk = vec![b'x'; 1024 * 1024];
        for _ in 0..64 {
            peer.send(shell, &chunk, usize::MAX);
        }
        peer.raw(MessageCommand::Write, shell.remote, shell.local, b"!");
    });
    let mut out = Vec::new();
    let error = device
        .shell_command_with_timeout(&"cat big", Some(&mut out), None, TIMEOUT)
        .unwrap_err();
    assert!(
        matches!(&error, RustADBError::ADBRequestFailed(m) if m.contains("more than")),
        "{error:?}"
    );
    drop(device);
    join();
}

#[test]
fn a_command_too_long_for_the_legacy_shell_is_refused_before_open() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        // The very next OPEN is the short command; the long one never left.
        let shell = peer.accept_open("shell:true");
        peer.close(shell);
    });
    let long = format!("echo {}", "x".repeat(5000));
    let error = device
        .shell_command_with_timeout(&long, None, None, TIMEOUT)
        .unwrap_err();
    assert!(
        matches!(&error, RustADBError::ADBRequestFailed(m) if m.contains("too long")),
        "{error:?}"
    );
    device
        .shell_command_with_timeout(&"true", None, None, TIMEOUT)
        .unwrap();
    drop(device);
    join();
}

#[test]
fn a_framebuffer_stream_closed_early_fails_the_request_not_the_connection() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let framebuffer = peer.accept_open("framebuffer:");
        // Version 1, then the device gives up before the header is complete.
        peer.send(framebuffer, &1_u32.to_le_bytes(), usize::MAX);
        peer.close(framebuffer);
        let shell = peer.accept_open("shell:echo ok");
        peer.send(shell, b"ok\n", usize::MAX);
        peer.close(shell);
    });
    let error = device.framebuffer_bytes().unwrap_err();
    assert!(
        matches!(&error, RustADBError::ADBRequestFailed(m) if m.contains("framebuffer")),
        "{error:?}"
    );
    let mut out = Vec::new();
    device
        .shell_command_with_timeout(&"echo ok", Some(&mut out), None, TIMEOUT)
        .unwrap();
    assert_eq!(out, b"ok\n");
    drop(device);
    join();
}

#[test]
fn binary_output_without_shell_v2_uses_exec_not_the_pty_shell() {
    let png = b"\x89PNG\r\n\x1a\n\0\x01\n".to_vec();
    let sent = png.clone();
    let (mut device, join) = run("cmd", 256 * 1024, move |peer| {
        let exec = peer.accept_open("exec:screencap -p");
        peer.send(exec, &sent, 4);
        peer.close(exec);
    });
    let mut out = Vec::new();
    let exit = device
        .exec_out_with_timeout("screencap -p", &mut out, None, TIMEOUT)
        .unwrap();
    assert_eq!(exit, ShellExit::Unreported);
    assert_eq!(out, png);
    drop(device);
    join();
}

// ---- install ----

#[test]
fn one_apk_streams_through_cmd_and_succeeds_on_success() {
    let apk = TempApk::new("base", b"PK\x03\x04apk-bytes");
    let (mut device, join) = run("cmd,shell_v2", 256 * 1024, |peer| {
        let install = peer.accept_open("exec:cmd 'package' 'install' '-r' '-S' '13'");
        assert_eq!(peer.read_host(install, 13), b"PK\x03\x04apk-bytes");
        peer.send(install, b"Success\n", usize::MAX);
        peer.close(install);
    });
    let output = device.install_apks(&[&apk.0], &["-r"], TIMEOUT).unwrap();
    assert_eq!(output, "Success\n");
    drop(device);
    join();
}

/// The same Android 14 acknowledgements on a streamed install of an APK
/// larger than one WRTE, which failed on the Android TV 14 image before the
/// reply could be read.
#[test]
fn a_multi_packet_install_survives_several_okays_for_each_write() {
    let apk = TempApk::new("base", &[7; 10_000]);
    let (mut device, join) = run("cmd", 4096, |peer| {
        peer.okays_per_write = 2;
        let install = peer.accept_open("exec:cmd 'package' 'install' '-S' '10000'");
        assert_eq!(peer.read_host(install, 10_000), vec![7; 10_000]);
        peer.send(install, b"Success\n", usize::MAX);
        peer.close(install);
    });
    let output = device.install_apks(&[&apk.0], &[], TIMEOUT).unwrap();
    assert_eq!(output, "Success\n");
    drop(device);
    join();
}

#[test]
fn a_streamed_install_failure_carries_the_package_manager_reason() {
    let apk = TempApk::new("base", &[1; 10_000]);
    let (mut device, join) = run("cmd", 4096, |peer| {
        let (local, _) = peer.expect_open();
        let install = peer.accept(local);
        // pm gives up after the first chunk and says why.
        let first = peer.next();
        assert_eq!(first.header().command(), MessageCommand::Write);
        peer.raw(MessageCommand::Okay, install.remote, install.local, &[]);
        peer.raw(
            MessageCommand::Write,
            install.remote,
            install.local,
            b"Failure [INSTALL_FAILED_INSUFFICIENT_STORAGE]\n",
        );
        peer.close(install);
    });
    let error = device
        .install_apks(&[&apk.0], &["-r"], TIMEOUT)
        .unwrap_err();
    assert!(
        matches!(&error, RustADBError::InstallFailed(m) if m == "Failure [INSTALL_FAILED_INSUFFICIENT_STORAGE]"),
        "{error:?}"
    );
    drop(device);
    join();
}

#[test]
fn a_silent_close_mid_upload_is_an_install_failure_not_a_lost_connection() {
    let apk = TempApk::new("base", &[1; 10_000]);
    let (mut device, join) = run("cmd", 4096, |peer| {
        let (local, _) = peer.expect_open();
        let install = peer.accept(local);
        peer.next();
        peer.raw(MessageCommand::Okay, install.remote, install.local, &[]);
        peer.close(install);
    });
    let error = device.install_apks(&[&apk.0], &[], TIMEOUT).unwrap_err();
    assert!(
        matches!(&error, RustADBError::InstallFailed(m) if m.contains("without a result")),
        "{error:?}"
    );
    drop(device);
    join();
}

#[test]
fn without_cmd_one_apk_is_pushed_installed_with_pm_and_removed() {
    let apk = TempApk::new("base", b"apk");
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        let (id, spec) = peer.sync_request(sync);
        assert_eq!(&id, b"SEND");
        let spec = String::from_utf8(spec).unwrap();
        let remote = spec.strip_suffix(",33188").unwrap().to_string();
        assert!(
            remote.starts_with("/data/local/tmp/adb-install-"),
            "{remote}"
        );
        assert_eq!(receive_pushed(peer, sync).0, b"apk");
        peer.send(sync, &sync_reply(*b"OKAY", 0, &[]), usize::MAX);
        peer.expect_quit(sync);

        let pm = peer.accept_open(&format!("shell:pm 'install' '-r' '{remote}'"));
        peer.send(
            pm,
            format!("\tpkg: {remote}\nSuccess\n").as_bytes(),
            usize::MAX,
        );
        peer.close(pm);
        let rm = peer.accept_open(&format!("shell:rm '{remote}' </dev/null"));
        peer.close(rm);
    });
    let output = device.install_apks(&[&apk.0], &["-r"], TIMEOUT).unwrap();
    assert!(output.contains("Success"));
    drop(device);
    join();
}

#[test]
fn a_failed_legacy_push_still_removes_the_temporary_copy() {
    let apk = TempApk::new("base", b"apk");
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        let (_, spec) = peer.sync_request(sync);
        let spec = String::from_utf8(spec).unwrap();
        let remote = spec.strip_suffix(",33188").unwrap().to_string();
        receive_pushed(peer, sync);
        peer.send(sync, &fail("No space left on device"), usize::MAX);
        peer.close(sync);
        let rm = peer.accept_open(&format!("shell:rm '{remote}' </dev/null"));
        peer.close(rm);
    });
    let error = device
        .install_apks(&[&apk.0], &["-r"], TIMEOUT)
        .unwrap_err();
    assert!(
        matches!(&error, RustADBError::SyncFailed(m) if m.contains("No space left")),
        "{error:?}"
    );
    drop(device);
    join();
}

#[test]
fn splits_install_through_an_abb_exec_session() {
    let base = TempApk::new("base", b"base!");
    let split = TempApk::new("split_config.en", b"en");
    let (mut device, join) = run("abb_exec,cmd,shell_v2", 256 * 1024, |peer| {
        let create = peer.accept_open("abb_exec:package\0install-create\0-r\0-S\x007");
        peer.send(
            create,
            b"Success: created install session [77]\n",
            usize::MAX,
        );
        peer.close(create);
        for (name, bytes) in [
            ("0_base.apk", &b"base!"[..]),
            ("1_split_config.en.apk", b"en"),
        ] {
            let write = peer.accept_open(&format!(
                "abb_exec:package\0install-write\0-S\0{}\x0077\0{name}\0-",
                bytes.len()
            ));
            assert_eq!(peer.read_host(write, bytes.len()), bytes);
            peer.send(write, b"Success: streamed\n", usize::MAX);
            peer.close(write);
        }
        let commit = peer.accept_open("abb_exec:package\0install-commit\x0077");
        peer.send(commit, b"Success\n", usize::MAX);
        peer.close(commit);
    });
    let output = device
        .install_apks(&[&base.0, &split.0], &["-r"], TIMEOUT)
        .unwrap();
    assert_eq!(output, "Success\n");
    drop(device);
    join();
}

#[test]
fn a_failed_split_write_abandons_the_cmd_session() {
    let base = TempApk::new("base", b"base!");
    let split = TempApk::new("split", b"xx");
    let (mut device, join) = run("cmd", 256 * 1024, |peer| {
        let create = peer.accept_open("exec:cmd 'package' 'install-create' '-r' '-S' '7'");
        peer.send(
            create,
            b"Success: created install session [5]\n",
            usize::MAX,
        );
        peer.close(create);
        let write =
            peer.accept_open("exec:cmd 'package' 'install-write' '-S' '5' '5' '0_base.apk' '-'");
        peer.read_host(write, 5);
        peer.send(write, b"Failure [INSTALL_FAILED_INVALID_APK]\n", usize::MAX);
        peer.close(write);
        let abandon = peer.accept_open("exec:cmd 'package' 'install-abandon' '5'");
        peer.send(abandon, b"Success\n", usize::MAX);
        peer.close(abandon);
    });
    let error = device
        .install_apks(&[&base.0, &split.0], &["-r"], TIMEOUT)
        .unwrap_err();
    assert!(
        matches!(&error, RustADBError::InstallFailed(m) if m.contains("INSTALL_FAILED_INVALID_APK")),
        "{error:?}"
    );
    drop(device);
    join();
}

#[test]
fn without_cmd_splits_use_a_pm_session_with_pushed_files() {
    let base = TempApk::new("base", b"b");
    let split = TempApk::new("split", b"s");
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let create = peer.accept_open("shell:pm 'install-create' '-r' '-S' '2'");
        peer.send(
            create,
            b"Success: created install session [9]\n",
            usize::MAX,
        );
        peer.close(create);
        for (index, name) in ["0_base.apk", "1_split.apk"].into_iter().enumerate() {
            let sync = peer.accept_open("sync:");
            let (_, spec) = peer.sync_request(sync);
            let spec = String::from_utf8(spec).unwrap();
            let remote = spec.strip_suffix(",33188").unwrap().to_string();
            assert!(remote.ends_with(&format!("-{index}.apk")), "{remote}");
            receive_pushed(peer, sync);
            peer.send(sync, &sync_reply(*b"OKAY", 0, &[]), usize::MAX);
            peer.expect_quit(sync);
            let write = peer.accept_open(&format!(
                "shell:pm 'install-write' '-S' '1' '9' '{name}' '{remote}'"
            ));
            peer.send(write, b"Success: streamed 1 bytes\n", usize::MAX);
            peer.close(write);
            let rm = peer.accept_open(&format!("shell:rm '{remote}' </dev/null"));
            peer.close(rm);
        }
        let commit = peer.accept_open("shell:pm 'install-commit' '9'");
        peer.send(commit, b"Success\n", usize::MAX);
        peer.close(commit);
    });
    let output = device
        .install_apks(&[&base.0, &split.0], &["-r"], TIMEOUT)
        .unwrap();
    assert!(output.starts_with("Success"));
    drop(device);
    join();
}

#[test]
fn uninstall_trusts_the_v2_exit_status() {
    let (mut device, join) = run("cmd,shell_v2", 256 * 1024, |peer| {
        let shell = peer.accept_open("shell,v2,raw:cmd package 'uninstall' 'com.example'");
        let mut packets = shell_packet(1, b"Failure [DELETE_FAILED_INTERNAL_ERROR]\n");
        packets.extend(shell_packet(3, &[1]));
        peer.send(shell, &packets, usize::MAX);
        peer.close(shell);
    });
    let error = device.uninstall(&"com.example", None).unwrap_err();
    assert!(
        error.to_string().contains("DELETE_FAILED_INTERNAL_ERROR"),
        "{error}"
    );
    drop(device);
    join();
}

// ---- one-shot services that restart adbd or the device ----

#[test]
fn root_succeeds_when_adbd_restarts_after_answering() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let root = peer.accept_open("root:");
        peer.send(root, b"restarting adbd as root\n", usize::MAX);
        peer.hang_up();
    });
    device.root().unwrap();
    drop(device);
    join();
}

#[test]
fn root_on_a_production_build_reports_the_refusal() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let root = peer.accept_open("root:");
        peer.send(
            root,
            b"adbd cannot run as root in production builds\n",
            usize::MAX,
        );
        peer.close(root);
    });
    let error = device.root().unwrap_err();
    assert!(
        matches!(&error, RustADBError::ADBRequestFailed(m) if m.contains("production builds")),
        "{error:?}"
    );
    drop(device);
    join();
}

#[test]
fn reboot_succeeds_once_adbd_accepts_even_if_the_connection_drops() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        peer.accept_open("reboot:");
        peer.hang_up();
    });
    device.reboot(crate::RebootType::System).unwrap();
    drop(device);
    join();
}

#[test]
fn remount_and_verity_keep_what_arrived_before_the_connection_dropped() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let remount = peer.accept_open("remount:");
        peer.send(remount, b"remount succeeded", usize::MAX);
        peer.hang_up();
    });
    assert!(device.remount().unwrap().is_empty());
    drop(device);
    join();

    let (mut device, join) = run("", 256 * 1024, |peer| {
        let verity = peer.accept_open("disable-verity:");
        peer.send(verity, b"Verity disabled\n", usize::MAX);
        peer.hang_up();
    });
    device.disable_verity().unwrap();
    drop(device);
    join();
}

#[test]
fn a_refused_root_open_is_still_a_failure() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let (local, _) = peer.expect_open();
        peer.raw(MessageCommand::Clse, 0, local, &[]);
    });
    let error = device.root().unwrap_err();
    assert!(
        matches!(error, RustADBError::ADBRequestFailed(_)),
        "{error:?}"
    );
    drop(device);
    join();
}

// ---- sync regressions ported from the desktop native-ADB work (#268) ----

/// After a refused request, the host may only close the stream: no new
/// OPEN, and no more data on any stream.
fn assert_no_replay(peer: &mut Peer) {
    loop {
        match peer.try_next(Duration::from_millis(500)) {
            Ok(message) if message.header().command() == MessageCommand::Clse => {}
            Ok(message) => panic!(
                "the host went on after a refusal: {}",
                message.header().command()
            ),
            Err(_) => return,
        }
    }
}

/// A device refusing an upload part-way, as adbd's `handle_send_file` does:
/// it answers at the `reject_at`th WRTE (0 holds SEND), with its reply split
/// into 3-byte WRTEs and the transport OKAY for that WRTE either after the
/// first fragment or after the whole reply, then discards data until DONE.
fn refuse_upload(peer: &mut Peer, reject_at: usize, reply: &[u8], interleave_ack: bool) {
    let sync = peer.accept_open("sync:");
    for _ in 0..reject_at {
        let message = peer.next();
        assert_eq!(message.header().command(), MessageCommand::Write);
        peer.raw(MessageCommand::Okay, sync.remote, sync.local, &[]);
    }
    let answered = peer.next();
    assert_eq!(answered.header().command(), MessageCommand::Write);
    let fragments = reply.chunks(3).count();
    for (index, part) in reply.chunks(3).enumerate() {
        peer.raw(MessageCommand::Write, sync.remote, sync.local, part);
        if index == 0 && interleave_ack {
            peer.raw(MessageCommand::Okay, sync.remote, sync.local, &[]);
        }
    }
    if !interleave_ack {
        peer.raw(MessageCommand::Okay, sync.remote, sync.local, &[]);
    }
    // The host acknowledges every fragment and, once its write was
    // acknowledged, sends the rest of what it had queued; in any order.
    let mut acknowledged = 0;
    let mut after = Vec::new();
    while acknowledged < fragments || !after.windows(4).any(|window| window == b"DONE") {
        let message = peer.next();
        match message.header().command() {
            MessageCommand::Okay => acknowledged += 1,
            MessageCommand::Write => {
                peer.raw(MessageCommand::Okay, sync.remote, sync.local, &[]);
                after.extend_from_slice(message.payload());
            }
            other => panic!("unexpected {other} during a refused upload"),
        }
    }
    assert_eq!(acknowledged, fragments);
    let done = after
        .windows(4)
        .position(|window| window == b"DONE")
        .unwrap();
    // Only the rest of the chunk in flight precedes DONE; no new DATA.
    assert!(!after[..done].windows(4).any(|window| window == b"DATA"));
    assert!(after[..done].iter().all(|&byte| byte == 7));
    assert_eq!(after.len(), done + 8, "nothing follows DONE");
    peer.close(sync);
}

#[test]
fn a_fragmented_fail_around_the_transport_ack_is_reported_at_send_and_any_data() {
    for reject_at in 0..=2 {
        for interleave_ack in [false, true] {
            let (mut device, join) = run("", 4096, move |peer| {
                refuse_upload(peer, reject_at, &fail("Permission denied"), interleave_ack);
                // The connection is still usable.
                let shell = peer.accept_open("shell:echo ok");
                peer.send(shell, b"ok\n", usize::MAX);
                peer.close(shell);
            });
            let data = vec![7_u8; 2 * 65535 + 1];
            let error = device.push(&mut &data[..], &"/fixture").unwrap_err();
            assert!(
                matches!(&error, RustADBError::SyncFailed(m) if m.contains("Permission denied")),
                "reject_at {reject_at}, interleave {interleave_ack}: {error:?}"
            );
            let mut out = Vec::new();
            device
                .shell_command_with_timeout(&"echo ok", Some(&mut out), None, TIMEOUT)
                .unwrap();
            assert_eq!(out, b"ok\n");
            drop(device);
            join();
        }
    }
}

#[test]
fn a_premature_sync_okay_is_never_a_completed_upload() {
    for reject_at in 0..=2 {
        for interleave_ack in [false, true] {
            let (mut device, join) = run("", 4096, move |peer| {
                refuse_upload(
                    peer,
                    reject_at,
                    &sync_reply(*b"OKAY", 0, &[]),
                    interleave_ack,
                );
            });
            let data = vec![7_u8; 2 * 65535 + 1];
            let error = device.push(&mut &data[..], &"/fixture").unwrap_err();
            assert!(
                error.to_string().contains("premature"),
                "reject_at {reject_at}, interleave {interleave_ack}: {error:?}"
            );
            drop(device);
            join();
        }
    }
}

#[test]
fn a_stalled_partial_rejection_ends_at_the_transfer_deadline_without_replay() {
    let (mut device, join) = run("", 4096, |peer| {
        let sync = peer.accept_open("sync:");
        peer.next();
        peer.send(sync, b"FAI", usize::MAX);
        // Neither the rest of the reply nor an OKAY ever comes.
        let next = peer.try_next(Duration::from_secs(3));
        assert!(
            next.is_err(),
            "the upload went on or was replayed: {next:?}"
        );
    });
    let control = super::BoundedShellControl::new(Duration::from_millis(300)).unwrap();
    let started = std::time::Instant::now();
    let data = vec![7_u8; 20_000];
    assert!(
        device
            .push_with_control(&mut &data[..], &"/fixture", control)
            .is_err()
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(device);
    join();
}

#[test]
fn a_controlled_upload_needs_a_fragmented_final_okay_and_never_replays_a_refusal() {
    for rejected in [false, true] {
        let (mut device, join) = run("", 256 * 1024, move |peer| {
            let sync = peer.accept_open("sync:");
            assert_eq!(
                peer.sync_request(sync),
                (*b"SEND", b"/fixture,33188".to_vec())
            );
            let (data, _) = receive_pushed(peer, sync);
            assert_eq!(data, b"fixture bytes");
            let status = if rejected {
                fail("Permission denied")
            } else {
                sync_reply(*b"OKAY", 0, &[])
            };
            peer.send(sync, &status, 3);
            if rejected {
                peer.close(sync);
                assert_no_replay(peer);
            } else {
                peer.expect_quit(sync);
            }
        });
        let control = super::BoundedShellControl::new(Duration::from_secs(2)).unwrap();
        let result = device.push_with_control(&mut &b"fixture bytes"[..], &"/fixture", control);
        if rejected {
            let error = result.unwrap_err();
            assert!(
                matches!(&error, RustADBError::SyncFailed(m) if m.contains("Permission denied")),
                "{error:?}"
            );
        } else {
            result.unwrap();
        }
        drop(device);
        join();
    }
}

#[test]
fn a_controlled_push_interrupts_a_stalled_open_or_data_ack_without_replay() {
    for accept_open in [false, true] {
        let (mut device, join) = run("", 256 * 1024, move |peer| {
            let (local, destination) = peer.expect_open();
            assert_eq!(destination, "sync:");
            if accept_open {
                peer.accept(local);
                assert_eq!(peer.next().header().command(), MessageCommand::Write);
            }
            let next = peer.try_next(Duration::from_secs(3));
            assert!(
                next.is_err(),
                "a stalled upload replayed its OPEN or DATA: {next:?}"
            );
        });
        let control = super::BoundedShellControl::new(Duration::from_millis(100)).unwrap();
        let started = std::time::Instant::now();
        assert!(
            device
                .push_with_control(&mut &vec![42_u8; 65536][..], &"/fixture", control)
                .is_err()
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(device);
        join();
    }
}

#[test]
fn done_like_bytes_inside_file_content_survive_every_split() {
    let content = b"file contents ending with DONE\0\0\0\0 and more";
    let mut reply = sync_reply(*b"DATA", u32::try_from(content.len()).unwrap(), content);
    reply.extend(sync_reply(*b"DATA", 4, b"tail"));
    reply.extend(sync_reply(*b"DONE", 0, &[]));
    for width in [1, 2, 3, 5, 7, 8, 13, reply.len()] {
        let reply = reply.clone();
        let (mut device, join) = run("", 256 * 1024, move |peer| {
            let sync = peer.accept_open("sync:");
            peer.sync_request(sync);
            let mut stat = b"STAT".to_vec();
            for value in [0o100_644_u32, 49, 1] {
                stat.extend(value.to_le_bytes());
            }
            peer.send(sync, &stat, width);
            peer.sync_request(sync);
            peer.send(sync, &reply, width);
            peer.expect_quit(sync);
        });
        let mut out = Vec::new();
        device.pull(&"/fixture", &mut out).unwrap();
        assert_eq!(out, [&content[..], b"tail"].concat(), "WRTE width {width}");
        drop(device);
        join();
    }
}

#[test]
fn a_fragmented_pull_refusal_is_never_written_as_file_data() {
    let refusal = fail("open failed: Permission denied");
    for width in [1, 3, 4, 8, 9, refusal.len()] {
        let refusal = refusal.clone();
        let (mut device, join) = run("", 256 * 1024, move |peer| {
            let sync = peer.accept_open("sync:");
            peer.sync_request(sync);
            let mut stat = b"STAT".to_vec();
            for value in [0o100_600_u32, 5, 1] {
                stat.extend(value.to_le_bytes());
            }
            peer.send(sync, &stat, usize::MAX);
            peer.sync_request(sync);
            peer.send(sync, &refusal, width);
            peer.close(sync);
        });
        let mut out = Vec::new();
        let error = device.pull(&"/data/secret", &mut out).unwrap_err();
        assert!(
            matches!(&error, RustADBError::SyncFailed(m) if m.contains("Permission denied")),
            "WRTE width {width}: {error:?}"
        );
        assert!(out.is_empty(), "WRTE width {width}: {out:?}");
        drop(device);
        join();
    }
}

#[test]
fn a_truncated_or_unknown_pull_reply_fails_the_request() {
    for reply in [
        // A DATA chunk cut short by the device closing the stream.
        b"DATA\x04\0\0\0abc".to_vec(),
        // A header cut short.
        b"DAT".to_vec(),
        // No DONE.
        sync_reply(*b"DATA", 3, b"abc"),
        // An id that is not DATA, DONE or FAIL.
        sync_reply(*b"WHAT", 0, &[]),
    ] {
        let (mut device, join) = run("", 256 * 1024, move |peer| {
            let sync = peer.accept_open("sync:");
            peer.sync_request(sync);
            let mut stat = b"STAT".to_vec();
            for value in [0o100_644_u32, 3, 1] {
                stat.extend(value.to_le_bytes());
            }
            peer.send(sync, &stat, usize::MAX);
            peer.sync_request(sync);
            peer.send(sync, &reply, usize::MAX);
            peer.close(sync);
        });
        let error = device.pull(&"/fixture", &mut Vec::new()).unwrap_err();
        assert!(
            matches!(
                error,
                RustADBError::ADBRequestFailed(_) | RustADBError::UnknownResponseType(_)
            ),
            "{error:?}"
        );
        drop(device);
        join();
    }
}

#[test]
fn a_pull_reply_from_the_wrong_remote_stream_is_refused_and_not_written() {
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        peer.sync_request(sync);
        let mut stat = b"STAT".to_vec();
        for value in [0o100_644_u32, 3, 1] {
            stat.extend(value.to_le_bytes());
        }
        peer.send(sync, &stat, usize::MAX);
        peer.sync_request(sync);
        let mut reply = sync_reply(*b"DATA", 3, b"abc");
        reply.extend(sync_reply(*b"DONE", 0, &[]));
        peer.raw(MessageCommand::Write, sync.remote + 1, sync.local, &reply);
    });
    let mut out = Vec::new();
    let error = device.pull(&"/fixture", &mut out).unwrap_err();
    assert!(matches!(error, RustADBError::ProtocolError(_)), "{error:?}");
    assert!(out.is_empty());
    drop(device);
    join();
}

#[test]
fn a_controlled_pull_keeps_bytes_split_anywhere_and_flushes_before_success() {
    struct Output {
        bytes: Vec<u8>,
        flushed: bool,
    }
    impl std::io::Write for Output {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.flushed = true;
            Ok(())
        }
    }
    let (mut device, join) = run("", 256 * 1024, |peer| {
        let sync = peer.accept_open("sync:");
        assert_eq!(peer.sync_request(sync), (*b"STAT", b"/fixture".to_vec()));
        let mut stat = b"STAT".to_vec();
        for value in [0o100_644_u32, 3, 0] {
            stat.extend(value.to_le_bytes());
        }
        peer.send(sync, &stat, 3);
        assert_eq!(peer.sync_request(sync), (*b"RECV", b"/fixture".to_vec()));
        let mut reply = sync_reply(*b"DATA", 3, b"abc");
        reply.extend(sync_reply(*b"DONE", 0, &[]));
        peer.send(sync, &reply, 5);
        peer.expect_quit(sync);
    });
    let mut output = Output {
        bytes: Vec::new(),
        flushed: false,
    };
    device
        .pull_with_control(
            &"/fixture",
            &mut output,
            super::BoundedShellControl::new(Duration::from_secs(2)).unwrap(),
        )
        .unwrap();
    assert_eq!(output.bytes, b"abc");
    assert!(output.flushed);
    drop(device);
    join();
}

#[test]
fn a_total_deadline_interrupts_a_stat_or_a_download_that_never_ends() {
    // (download, dedicated connection with the control from the start)
    for (download, dedicated) in [(false, true), (true, true), (true, false)] {
        let (address, server) = fake_adbd(move |stream| {
            let mut peer = Peer::connect(stream, "", 256 * 1024);
            let sync = peer.accept_open("sync:");
            peer.sync_request(sync);
            if !download {
                let next = peer.try_next(Duration::from_secs(3));
                assert!(next.is_err(), "{next:?}");
                return;
            }
            let mut stat = b"STAT".to_vec();
            for value in [0o100_644_u32, 1000, 0] {
                stat.extend(value.to_le_bytes());
            }
            peer.send(sync, &stat, usize::MAX);
            peer.sync_request(sync);
            for _ in 0..300 {
                // The host hangs up once its deadline passes; that ends the
                // download here, whichever of the write or the read sees it.
                if peer
                    .try_raw(
                        MessageCommand::Write,
                        sync.remote,
                        sync.local,
                        &sync_reply(*b"DATA", 1, b"x"),
                    )
                    .is_err()
                {
                    return;
                }
                match peer.try_next(Duration::from_secs(3)) {
                    Ok(message) if message.header().command() == MessageCommand::Okay => {}
                    _ => return,
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("a continuous download outlived its total deadline");
        });
        let key = shared_test_key();
        let (mut device, control) = if dedicated {
            let control = super::BoundedShellControl::new(Duration::from_millis(300)).unwrap();
            let device =
                ADBTcpDevice::new_with_required_key_and_control(address, key, &control).unwrap();
            (device, control)
        } else {
            let device = connect(address);
            (
                device,
                super::BoundedShellControl::new(Duration::from_millis(300)).unwrap(),
            )
        };
        let started = std::time::Instant::now();
        let mut out = Vec::new();
        let result = match (download, dedicated) {
            (false, _) => device.stat(&"/fixture").map(|_| ()),
            (true, true) => device.pull(&"/fixture", &mut out),
            (true, false) => device.pull_with_control(&"/fixture", &mut out, control),
        };
        assert!(result.is_err());
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
        if download {
            assert!(!out.is_empty());
        }
        drop(device);
        server.join().unwrap();
    }
}

// ---- install outcomes reported before the whole APK was sent ----

#[test]
fn a_success_before_the_whole_apk_was_sent_is_never_an_install() {
    // pm claims success after the first chunk and hangs up.
    let apk = TempApk::new("base", &[1; 10_000]);
    let (mut device, join) = run("cmd", 4096, |peer| {
        let (local, _) = peer.expect_open();
        let install = peer.accept(local);
        assert_eq!(peer.next().header().command(), MessageCommand::Write);
        peer.send(install, b"Success\n", usize::MAX);
        peer.close(install);
    });
    let error = device.install_apks(&[&apk.0], &[], TIMEOUT).unwrap_err();
    assert!(
        matches!(&error, RustADBError::ADBRequestFailed(m) if m.contains("not confirmed")),
        "{error:?}"
    );
    drop(device);
    join();

    // pm claims success early, then still takes the rest of the APK.
    let apk = TempApk::new("base", &[1; 10_000]);
    let (mut device, join) = run("cmd", 4096, |peer| {
        let (local, _) = peer.expect_open();
        let install = peer.accept(local);
        let first = peer.next();
        assert_eq!(first.header().command(), MessageCommand::Write);
        peer.send(install, b"Success\n", usize::MAX);
        peer.raw(MessageCommand::Okay, install.remote, install.local, &[]);
        peer.read_host(install, 10_000 - first.payload().len());
        peer.close(install);
    });
    let error = device.install_apks(&[&apk.0], &[], TIMEOUT).unwrap_err();
    assert!(
        matches!(&error, RustADBError::ADBRequestFailed(m) if m.contains("not confirmed")),
        "{error:?}"
    );
    drop(device);
    join();
}

#[test]
fn a_refusal_before_the_whole_apk_was_sent_stays_an_install_failure() {
    let apk = TempApk::new("base", &[1; 10_000]);
    let (mut device, join) = run("cmd", 4096, |peer| {
        let (local, _) = peer.expect_open();
        let install = peer.accept(local);
        assert_eq!(peer.next().header().command(), MessageCommand::Write);
        peer.send(
            install,
            b"Failure [INSTALL_FAILED_INVALID_APK]\n",
            usize::MAX,
        );
        peer.close(install);
    });
    let error = device.install_apks(&[&apk.0], &[], TIMEOUT).unwrap_err();
    assert!(
        matches!(&error, RustADBError::InstallFailed(m) if m == "Failure [INSTALL_FAILED_INVALID_APK]"),
        "{error:?}"
    );
    drop(device);
    join();
}

#[test]
fn a_success_after_the_last_byte_counts_even_if_its_okay_never_came() {
    let apk = TempApk::new("base", &[1; 10_000]);
    let (mut device, join) = run("cmd", 4096, |peer| {
        let (local, _) = peer.expect_open();
        let install = peer.accept(local);
        let mut received = 0;
        loop {
            let message = peer.next();
            assert_eq!(message.header().command(), MessageCommand::Write);
            received += message.payload().len();
            if received == 10_000 {
                break;
            }
            peer.raw(MessageCommand::Okay, install.remote, install.local, &[]);
        }
        // pm answers and closes before adbd acknowledged the last WRTE.
        peer.send(install, b"Success\n", usize::MAX);
        peer.close(install);
    });
    let output = device.install_apks(&[&apk.0], &[], TIMEOUT).unwrap();
    assert_eq!(output, "Success\n");
    drop(device);
    join();
}

// ---- controlled binary output ----

#[test]
fn controlled_exec_out_uses_shell_v2_when_negotiated_and_exec_otherwise() {
    let png = b"\x89PNG\r\n\x1a\n\0binary\r\n";
    let (mut device, join) = run("shell_v2", 256 * 1024, move |peer| {
        let shell = peer.accept_open("shell,v2,raw:screencap -p");
        let mut reply = shell_packet(1, png);
        reply.extend(shell_packet(2, b"warning"));
        reply.extend(shell_packet(3, &[0]));
        peer.send(shell, &reply, 7);
        peer.close(shell);
    });
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let control = super::BoundedShellControl::new(TIMEOUT).unwrap();
    let exit = device
        .exec_out_with_control(
            "screencap -p",
            &mut stdout,
            Some(&mut stderr),
            control.clone(),
        )
        .unwrap();
    assert_eq!(exit, ShellExit::Code(0));
    assert_eq!(stdout, png);
    assert_eq!(stderr, b"warning");
    assert!(control.was_registered());
    drop(device);
    join();

    let (mut device, join) = run("", 256 * 1024, move |peer| {
        let exec = peer.accept_open("exec:screencap -p");
        peer.send(exec, png, 5);
        peer.close(exec);
    });
    let mut stdout = Vec::new();
    let exit = device
        .exec_out_with_control(
            "screencap -p",
            &mut stdout,
            None,
            super::BoundedShellControl::new(TIMEOUT).unwrap(),
        )
        .unwrap();
    assert_eq!(exit, ShellExit::Unreported);
    assert_eq!(stdout, png);
    drop(device);
    join();
}

#[test]
fn controlled_exec_out_ends_at_its_deadline_and_a_spent_control_sends_nothing() {
    let (mut device, join) = run("shell_v2", 256 * 1024, |peer| {
        let shell = peer.accept_open("shell,v2,raw:screencap -p");
        peer.send(shell, &shell_packet(1, b"\x89PNG partial"), usize::MAX);
        let next = peer.try_next(Duration::from_secs(3));
        assert!(next.is_err(), "the socket outlived the deadline: {next:?}");
    });
    let spent = super::BoundedShellControl::new(TIMEOUT).unwrap();
    spent.cancel();
    assert!(
        device
            .exec_out_with_control("never", &mut Vec::new(), None, spent.clone())
            .is_err()
    );
    assert!(!spent.was_registered());
    let control = super::BoundedShellControl::new(Duration::from_millis(300)).unwrap();
    let started = std::time::Instant::now();
    let mut stdout = Vec::new();
    assert!(
        device
            .exec_out_with_control("screencap -p", &mut stdout, None, control)
            .is_err()
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(stdout, b"\x89PNG partial");
    drop(device);
    join();
}

// ---- a connection adbd closes before answering CNXN ----

/// Accepts one connection per script, in order, and counts them.
fn adbd_sequence(
    scripts: Vec<Box<dyn FnOnce(std::net::TcpStream) + Send>>,
) -> (SocketAddr, std::thread::JoinHandle<usize>) {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let mut scripts = scripts.into_iter();
        let mut accepted = 0;
        let quiet_until = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < quiet_until {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    accepted += 1;
                    match scripts.next() {
                        Some(script) => script(stream),
                        None => drop(stream),
                    }
                }
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        accepted
    });
    (address, handle)
}

fn hang_up_after_cnxn(mut stream: std::net::TcpStream) {
    let cnxn = super::test_support::read_message(&mut stream).unwrap();
    assert_eq!(cnxn.header().command(), MessageCommand::Cnxn);
}

#[test]
fn a_connection_closed_before_any_reply_is_tried_once_more() {
    let (address, server) = adbd_sequence(vec![
        Box::new(hang_up_after_cnxn),
        Box::new(|stream| {
            let mut peer = Peer::connect(stream, "shell_v2", 256 * 1024);
            let _ = peer.try_next(Duration::from_secs(1));
        }),
    ]);
    let started = std::time::Instant::now();
    let device = ADBTcpDevice::new_with_custom_private_key_and_auth_timeout(
        address,
        shared_test_key(),
        TIMEOUT,
    );
    assert!(device.is_ok(), "{:?}", device.err());
    assert!(started.elapsed() >= Duration::from_millis(150));
    drop(device);
    assert_eq!(server.join().unwrap(), 2);
}

#[test]
fn a_second_silent_close_is_returned_without_a_third_attempt() {
    let (address, server) = adbd_sequence(vec![
        Box::new(hang_up_after_cnxn),
        Box::new(hang_up_after_cnxn),
    ]);
    let device = ADBTcpDevice::new_with_custom_private_key_and_auth_timeout(
        address,
        shared_test_key(),
        TIMEOUT,
    );
    assert!(
        matches!(&device, Err(RustADBError::IOError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof),
        "{:?}",
        device.err()
    );
    assert_eq!(server.join().unwrap(), 2);
}

#[test]
fn a_close_after_the_device_answered_is_not_retried() {
    let (address, server) = adbd_sequence(vec![Box::new(|mut stream| {
        let _ = super::test_support::read_message(&mut stream).unwrap();
        // Ask for authentication, then hang up: the device did answer.
        super::test_support::write_message(&mut stream, MessageCommand::Auth, 1, 0, &[7; 20])
            .unwrap();
        let _ = super::test_support::read_message(&mut stream);
    })]);
    let device = ADBTcpDevice::new_with_custom_private_key_and_auth_timeout(
        address,
        shared_test_key(),
        TIMEOUT,
    );
    assert!(device.is_err());
    assert_eq!(server.join().unwrap(), 1);
}
