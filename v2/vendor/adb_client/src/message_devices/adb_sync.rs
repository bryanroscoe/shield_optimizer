// Derived in part from Kadb (Apache-2.0): stream/AdbSyncStream.kt.

//! The `sync:` file-transfer service, parsed as a byte stream the way AOSP's
//! `file_sync_client` does. adbd does not align sync replies with ADB packet
//! boundaries, so nothing here assumes one reply per WRTE.

use std::io::{ErrorKind, Read, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use byteorder::{ByteOrder, LittleEndian};

use crate::{
    ADBListItem, ADBListItemType, AdbStatResponse, ConnectionInfo, Result, RustADBError,
    message_devices::{
        adb_message_device::ADBMessageDevice,
        adb_message_transport::ADBMessageTransport,
        adb_service::{ADBService, adb_error},
    },
    models::ADBLocalCommand,
};

type SyncId = [u8; 4];

const ID_STAT: SyncId = *b"STAT";
const ID_STAT_V2: SyncId = *b"STA2";
const ID_LSTAT_V2: SyncId = *b"LST2";
const ID_LIST: SyncId = *b"LIST";
const ID_LIST_V2: SyncId = *b"LIS2";
const ID_DENT: SyncId = *b"DENT";
const ID_DENT_V2: SyncId = *b"DNT2";
const ID_SEND: SyncId = *b"SEND";
const ID_SEND_V2: SyncId = *b"SND2";
const ID_RECV: SyncId = *b"RECV";
const ID_RECV_V2: SyncId = *b"RCV2";
const ID_DATA: SyncId = *b"DATA";
const ID_DONE: SyncId = *b"DONE";
const ID_OKAY: SyncId = *b"OKAY";
const ID_FAIL: SyncId = *b"FAIL";
const ID_QUIT: SyncId = *b"QUIT";

/// AOSP caps one DATA chunk at 64 KiB in both directions.
const SYNC_DATA_MAX: usize = 64 * 1024;
/// AOSP refuses longer request paths.
const SYNC_PATH_MAX: usize = 1024;
/// Linux `NAME_MAX`; a directory entry name can be no longer.
const SYNC_NAME_MAX: usize = 255;
const DATA_HEADER_LEN: usize = 8;
/// `sync_stat_v2` after its id.
const STAT_V2_LEN: usize = 68;
/// `sync_dent` after its id: mode, size, mtime, name length.
const DENT_V1_LEN: usize = 16;
/// `sync_dent_v2` after its id, ending with the name length.
const DENT_V2_LEN: usize = 72;
/// How long to wait for the service to close after QUIT.
const QUIT_DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const QUIT_DRAIN_LIMIT: usize = 64 * 1024;

/// Mode for files this host pushes: a regular file, `rw-r--r--`.
pub(crate) const PUSH_FILE_MODE: u32 = 0o100_644;

/// Which v2 sync requests both sides negotiated.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SyncFeatures {
    pub(crate) stat_v2: bool,
    pub(crate) ls_v2: bool,
    pub(crate) sendrecv_v2: bool,
}

impl SyncFeatures {
    pub(crate) fn negotiated(info: &ConnectionInfo) -> Self {
        Self {
            stat_v2: info.has_feature("stat_v2"),
            ls_v2: info.has_feature("ls_v2"),
            sendrecv_v2: info.has_feature("sendrecv_v2"),
        }
    }
}

/// Seconds since the Unix epoch, for the DONE that ends a push.
pub(crate) fn now_mtime() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u32::try_from(elapsed.as_secs()).unwrap_or(u32::MAX)
        })
}

fn le_u32(bytes: &[u8]) -> u32 {
    LittleEndian::read_u32(bytes)
}

fn le_u64(bytes: &[u8]) -> u64 {
    LittleEndian::read_u64(bytes)
}

/// A v2 timestamp is signed; one before 1970 is reported as 0.
fn le_time(bytes: &[u8]) -> u64 {
    u64::try_from(LittleEndian::read_i64(bytes)).unwrap_or(0)
}

fn unexpected(operation: &str, id: SyncId) -> RustADBError {
    RustADBError::UnknownResponseType(format!(
        "sync {operation}: unexpected reply {:?}",
        String::from_utf8_lossy(&id)
    ))
}

/// The wire errno AOSP sends in a v2 stat reply (Linux numbering).
fn errno_text(code: u32) -> String {
    let text = match code {
        1 => "Operation not permitted",
        2 => "No such file or directory",
        13 => "Permission denied",
        20 => "Not a directory",
        21 => "Is a directory",
        40 => "Too many levels of symbolic links",
        _ => return format!("error {code}"),
    };
    text.to_string()
}

fn entry_name(raw: Vec<u8>) -> Result<String> {
    let name = String::from_utf8(raw)?;
    // AOSP's client refuses a listed name with a separator in it: it could
    // point a recursive pull outside the directory being listed.
    if name.contains('/') || name.is_empty() {
        return Err(RustADBError::UnknownResponseType(format!(
            "sync list: invalid entry name {name:?}"
        )));
    }
    Ok(name)
}

/// One `sync:` stream. Requests are batched into WRTEs of up to the
/// negotiated payload size; replies are read as bytes.
#[derive(Debug)]
pub(crate) struct SyncStream<T: ADBMessageTransport> {
    service: ADBService<T>,
    features: SyncFeatures,
    outgoing: Vec<u8>,
}

impl<T: ADBMessageTransport> ADBMessageDevice<T> {
    pub(crate) fn open_sync(&mut self, read_timeout: Duration) -> Result<SyncStream<T>> {
        let features = SyncFeatures::negotiated(self.connection_info());
        let service = self.open_service(&ADBLocalCommand::Sync, read_timeout)?;
        Ok(SyncStream::new(service, features))
    }
}

impl<T: ADBMessageTransport> SyncStream<T> {
    pub(crate) const fn new(service: ADBService<T>, features: SyncFeatures) -> Self {
        Self {
            service,
            features,
            outgoing: Vec::new(),
        }
    }

    fn queue(&mut self, bytes: &[u8]) -> Result<()> {
        self.outgoing.extend_from_slice(bytes);
        let max_write = self.service.max_write();
        if self.outgoing.len() >= max_write {
            let whole = self.outgoing.len() / max_write * max_write;
            self.service
                .write_all(&self.outgoing[..whole])
                .map_err(adb_error)?;
            self.outgoing.drain(..whole);
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if self.outgoing.is_empty() {
            return Ok(());
        }
        let result = self.service.write_all(&self.outgoing).map_err(adb_error);
        self.outgoing.clear();
        result
    }

    fn request(&mut self, id: SyncId, path: &str) -> Result<()> {
        if path.len() > SYNC_PATH_MAX {
            return Err(RustADBError::ADBRequestFailed(format!(
                "sync path is longer than {SYNC_PATH_MAX} bytes"
            )));
        }
        self.queue(&id)?;
        self.queue(&u32::try_from(path.len())?.to_le_bytes())?;
        self.queue(path.as_bytes())
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<()> {
        self.service
            .read_exact(buf)
            .map_err(|error| self.service.read_error(error, "the sync stream"))
    }

    fn read_id(&mut self) -> Result<SyncId> {
        let mut id = [0_u8; 4];
        self.read_exact(&mut id)?;
        Ok(id)
    }

    fn read_u32(&mut self) -> Result<u32> {
        let mut value = [0_u8; 4];
        self.read_exact(&mut value)?;
        Ok(u32::from_le_bytes(value))
    }

    fn read_vec(&mut self, len: usize) -> Result<Vec<u8>> {
        let mut bytes = vec![0_u8; len];
        self.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    /// Read the message of a FAIL whose id and length were already read.
    fn fail(&mut self, operation: &str, len: u32) -> RustADBError {
        let len = len as usize;
        if len > SYNC_DATA_MAX {
            return RustADBError::UnknownResponseType(format!(
                "sync {operation}: FAIL message of {len} bytes"
            ));
        }
        match self.read_vec(len) {
            Ok(message) => RustADBError::SyncFailed(format!(
                "{operation}: {}",
                String::from_utf8_lossy(&message).trim_end()
            )),
            Err(error) => error,
        }
    }

    /// `stat` the path, following symlinks when `stat_v2` is negotiated. The
    /// legacy `STAT` request is an lstat and carries a 32-bit size.
    pub(crate) fn stat(&mut self, path: &str) -> Result<AdbStatResponse> {
        if self.features.stat_v2 {
            return self.stat_v2(path);
        }
        self.request(ID_STAT, path)?;
        self.flush()?;
        let id = self.read_id()?;
        if id != ID_STAT {
            return Err(unexpected("stat", id));
        }
        let mut body = [0_u8; 12];
        self.read_exact(&mut body)?;
        // AOSP's client reads an all-zero legacy reply as a failed lstat.
        if body.iter().all(|&byte| byte == 0) {
            return Err(RustADBError::SyncFailed(
                "stat: no such file or directory, or it cannot be read".to_string(),
            ));
        }
        Ok(body.into())
    }

    fn stat_v2(&mut self, path: &str) -> Result<AdbStatResponse> {
        self.request(ID_STAT_V2, path)?;
        self.flush()?;
        let id = self.read_id()?;
        if id == ID_FAIL {
            let len = self.read_u32()?;
            return Err(self.fail("stat", len));
        }
        if id != ID_STAT_V2 && id != ID_LSTAT_V2 {
            return Err(unexpected("stat", id));
        }
        let mut body = [0_u8; STAT_V2_LEN];
        self.read_exact(&mut body)?;
        let error = le_u32(&body[0..4]);
        if error != 0 {
            return Err(RustADBError::SyncFailed(format!(
                "stat: {}",
                errno_text(error)
            )));
        }
        Ok(AdbStatResponse {
            file_perm: le_u32(&body[20..24]),
            file_size: le_u64(&body[36..44]),
            mod_time: le_time(&body[52..60]),
        })
    }

    /// List a directory, with 64-bit sizes when `ls_v2` is negotiated.
    pub(crate) fn list(&mut self, path: &str) -> Result<Vec<ADBListItemType>> {
        let v2 = self.features.ls_v2;
        self.request(if v2 { ID_LIST_V2 } else { ID_LIST }, path)?;
        self.flush()?;
        let mut entries = Vec::new();
        loop {
            let id = self.read_id()?;
            match id {
                ID_DENT if !v2 => {
                    let mut body = [0_u8; DENT_V1_LEN];
                    self.read_exact(&mut body)?;
                    let mode = le_u32(&body[0..4]);
                    let size = u64::from(le_u32(&body[4..8]));
                    let time = u64::from(le_u32(&body[8..12]));
                    let name = self.read_name(le_u32(&body[12..16]))?;
                    entries.push(list_entry(mode, size, time, name));
                }
                ID_DENT_V2 if v2 => {
                    let mut body = [0_u8; DENT_V2_LEN];
                    self.read_exact(&mut body)?;
                    let name = self.read_name(le_u32(&body[68..72]))?;
                    let error = le_u32(&body[0..4]);
                    if error != 0 {
                        // adbd could not lstat this entry. Its metadata is
                        // unknown, so it is left out rather than shown as zeros.
                        log::warn!(
                            "sync list: skipping an entry the device could not read ({})",
                            errno_text(error)
                        );
                        continue;
                    }
                    let mode = le_u32(&body[20..24]);
                    let size = le_u64(&body[36..44]);
                    let time = le_time(&body[52..60]);
                    entries.push(list_entry(mode, size, time, name));
                }
                ID_DONE => {
                    // DONE is sent as a whole directory entry.
                    let tail = if v2 { DENT_V2_LEN } else { DENT_V1_LEN };
                    self.read_vec(tail)?;
                    return Ok(entries);
                }
                ID_FAIL => {
                    let len = self.read_u32()?;
                    return Err(self.fail("list", len));
                }
                other => return Err(unexpected("list", other)),
            }
        }
    }

    fn read_name(&mut self, len: u32) -> Result<String> {
        let len = len as usize;
        if len > SYNC_NAME_MAX {
            return Err(RustADBError::UnknownResponseType(format!(
                "sync list: entry name of {len} bytes"
            )));
        }
        entry_name(self.read_vec(len)?)
    }

    /// Push `source` to `path`. Success is the device's `OKAY` after DONE;
    /// a `FAIL` becomes [`RustADBError::SyncFailed`] with its message.
    pub(crate) fn send(
        &mut self,
        source: &mut dyn Read,
        path: &str,
        mode: u32,
        mtime: u32,
    ) -> Result<()> {
        if self.features.sendrecv_v2 {
            self.request(ID_SEND_V2, path)?;
            // sync_send_v2 { id, mode, flags }; no compression.
            self.queue(&ID_SEND_V2)?;
            self.queue(&mode.to_le_bytes())?;
            self.queue(&0_u32.to_le_bytes())?;
        } else {
            self.request(ID_SEND, &format!("{path},{mode}"))?;
        }
        let answered_early = match self.send_body(source, mtime) {
            Ok(answered_early) => answered_early,
            Err(error) => {
                // adbd answers FAIL as soon as it cannot write the file, then
                // discards data until DONE or closes the stream. That answer,
                // not our failed write, says what went wrong.
                if self.service.closed_by_peer_during_write(&error) {
                    self.send_status(true)?;
                }
                return Err(error);
            }
        };
        self.send_status(answered_early)
    }

    /// Send the file and DONE. Returns whether the device answered before
    /// DONE went out, which can only be a refusal.
    fn send_body(&mut self, source: &mut dyn Read, mtime: u32) -> Result<bool> {
        // Each DATA chunk fits in one WRTE on its own; several share a WRTE
        // when the negotiated payload allows.
        let chunk_len = SYNC_DATA_MAX
            .min(self.service.max_write().saturating_sub(DATA_HEADER_LEN))
            .max(1);
        let mut chunk = vec![0_u8; chunk_len];
        // Data from the device before DONE can only be an early FAIL; stop
        // sending the file and let DONE end the device's discard loop.
        while !self.service.has_pending() {
            let read = match source.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => read,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(error) => return Err(RustADBError::LocalIo(error)),
            };
            self.queue(&ID_DATA)?;
            self.queue(&u32::try_from(read)?.to_le_bytes())?;
            self.queue(&chunk[..read])?;
        }
        let answered_early = self.service.has_pending();
        self.queue(&ID_DONE)?;
        self.queue(&mtime.to_le_bytes())?;
        self.flush()?;
        Ok(answered_early)
    }

    /// Read the device's verdict on a push. adbd sends `OKAY` only after
    /// DONE, so one that arrived before DONE went out (`answered_early`) is
    /// a broken peer, never a completed upload.
    fn send_status(&mut self, answered_early: bool) -> Result<()> {
        let id = self.read_id()?;
        let arg = self.read_u32()?;
        match id {
            ID_OKAY if answered_early => Err(RustADBError::UnknownResponseType(
                "sync push: premature OKAY before the upload finished".to_string(),
            )),
            ID_OKAY => Ok(()),
            ID_FAIL => Err(self.fail("push", arg)),
            other => Err(unexpected("push", other)),
        }
    }

    /// Pull `path` into `sink`. Ends at the device's DONE; a `FAIL` becomes
    /// [`RustADBError::SyncFailed`], and a failing `sink` is
    /// [`RustADBError::LocalIo`].
    pub(crate) fn recv(&mut self, path: &str, sink: &mut dyn Write) -> Result<()> {
        if self.features.sendrecv_v2 {
            self.request(ID_RECV_V2, path)?;
            // sync_recv_v2 { id, flags }; no compression.
            self.queue(&ID_RECV_V2)?;
            self.queue(&0_u32.to_le_bytes())?;
        } else {
            self.request(ID_RECV, path)?;
        }
        self.flush()?;

        let mut chunk = Vec::new();
        loop {
            let id = self.read_id()?;
            let arg = self.read_u32()?;
            match id {
                ID_DATA => {
                    let len = arg as usize;
                    if len > SYNC_DATA_MAX {
                        return Err(RustADBError::UnknownResponseType(format!(
                            "sync pull: DATA chunk of {len} bytes exceeds {SYNC_DATA_MAX}"
                        )));
                    }
                    chunk.resize(len, 0);
                    self.read_exact(&mut chunk)?;
                    sink.write_all(&chunk).map_err(RustADBError::LocalIo)?;
                }
                ID_DONE => break,
                ID_FAIL => return Err(self.fail("pull", arg)),
                other => return Err(unexpected("pull", other)),
            }
        }
        sink.flush().map_err(RustADBError::LocalIo)
    }

    /// End the session with QUIT and wait briefly for the service to close,
    /// as AOSP's client does, so its CLSE does not trail into the next
    /// request. The operation already succeeded, so failures here are only
    /// logged; a broken connection fails the next request instead.
    pub(crate) fn finish(mut self) {
        let quit = self
            .queue(&ID_QUIT)
            .and_then(|()| self.queue(&0_u32.to_le_bytes()))
            .and_then(|()| self.flush());
        if let Err(error) = quit {
            log::debug!("sync QUIT was not sent: {error}");
            return;
        }
        self.service
            .drain_until_closed(QUIT_DRAIN_TIMEOUT, QUIT_DRAIN_LIMIT);
    }
}

fn list_entry(mode: u32, size: u64, time: u64, name: String) -> ADBListItemType {
    ADBListItemType::from_mode_and_entry(
        mode,
        ADBListItem {
            name,
            time,
            permissions: mode & 0o777,
            size,
        },
    )
}
