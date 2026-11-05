use thiserror::Error;

/// Custom Result type thrown by this crate.
pub type Result<T> = std::result::Result<T, RustADBError>;

/// Represents all error types that can be thrown by the crate.
#[derive(Error, Debug)]
pub enum RustADBError {
    /// Indicates that an error occurred with I/O.
    #[error(transparent)]
    IOError(#[from] std::io::Error),
    /// Indicates that an error occurred during ADB shell v2 parsing.
    #[error("ADB shell v2 parsing error: {0}")]
    ADBShellV2ParseError(String),
    /// Indicates that an error occurred when sending ADB request.
    #[error("ADB request failed - {0}")]
    ADBRequestFailed(String),
    /// Indicates that ADB server responded an unknown response type.
    #[error("Unknown response type {0}")]
    UnknownResponseType(String),
    /// Indicated that an unexpected command has been received
    #[error("Wrong response command received: {0}. Expected {1}")]
    WrongResponseReceived(String, String),
    /// Indicates that ADB server responses an unknown device state.
    #[error("Unknown device state {0}")]
    UnknownDeviceState(String),
    /// Indicates that an error occurred during UTF-8 parsing.
    #[error(transparent)]
    Utf8StrError(#[from] std::str::Utf8Error),
    /// Indicates that an error occurred during UTF-8 parsing.
    #[error(transparent)]
    Utf8StringError(#[from] std::string::FromUtf8Error),
    /// Indicates that the provided address is not a correct IP address.
    #[error(transparent)]
    AddrParseError(#[from] std::net::AddrParseError),
    /// Indicates an error with regexps.
    #[error(transparent)]
    RegexError(#[from] regex::Error),
    /// Indicates that parsing regex did not worked.
    #[error("Regex parsing error: missing field")]
    RegexParsingError,
    /// Indicates an error with the integer conversion.
    #[error(transparent)]
    ParseIntError(#[from] std::num::ParseIntError),
    /// Indicates that an error occurred when converting a value.
    #[error("Conversion error")]
    ConversionError,
    /// Indicates an error with the integer conversion.
    #[error(transparent)]
    IntegerConversionError(#[from] std::num::TryFromIntError),
    /// Remote ADB server does not support shell feature.
    #[error("Remote ADB server does not support shell feature")]
    ADBShellNotSupported,
    /// Desired device has not been found
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    /// Indicates that the device must be paired before attempting a connection over WI-FI
    #[error("Device not paired before attempting to connect")]
    ADBDeviceNotPaired,
    /// Indicates that remount operation failed
    #[error("Cannot remount filesystem: {0}")]
    RemountError(String),
    /// An error occurred when getting device's framebuffer image
    #[cfg(feature = "framebuffer")]
    #[error(transparent)]
    FramebufferImageError(#[from] image::error::ImageError),
    /// An error occurred when converting framebuffer content
    #[cfg(feature = "framebuffer")]
    #[error("Cannot convert framebuffer into image")]
    FramebufferConversionError,
    /// Unimplemented framebuffer image version
    #[error("Unimplemented framebuffer image version: {0}")]
    UnimplementedFramebufferImageVersion(u32),
    /// Cannot get home directory
    #[error("Cannot get home directory")]
    NoHomeDirectory,
    /// Generic USB error
    #[cfg(feature = "usb")]
    #[cfg_attr(docsrs, doc(cfg(feature = "usb")))]
    #[error("USB Error: {0}")]
    UsbError(#[from] rusb::Error),
    /// Selected device is busy.
    #[cfg(feature = "usb")]
    #[cfg_attr(docsrs, doc(cfg(feature = "usb")))]
    #[error("Device is busy. Is ADB server running?")]
    DeviceBusy,
    /// USB device not found
    #[error("USB Device not found: {0} {1}")]
    USBDeviceNotFound(u16, u16),
    /// No descriptor found
    #[error("No USB descriptor found")]
    USBNoDescriptorFound,
    /// Integrity of the received message cannot be validated
    #[error("Invalid integrity. Expected CRC32 {0}, got {1}")]
    InvalidIntegrity(u32, u32),
    /// The peer broke ADB framing or the connection handshake: a bad header
    /// magic, an unknown command, a payload longer than the negotiated limit,
    /// or an impossible CNXN. Also returned by every use of a connection after
    /// a read or write stopped part-way through a message. The connection
    /// cannot be trusted afterwards.
    #[error("ADB protocol error: {0}")]
    ProtocolError(String),
    /// The device's TLS server refused this client's certificate with an
    /// explicit TLS alert, which is how adbd reports that the key has not
    /// been paired with it.
    #[error("the device rejected this ADB key over TLS; pair it first ({0})")]
    TlsAuthRejected(String),
    /// The connection closed or reset during the TLS handshake or before the
    /// first message over TLS, without a TLS alert. A network drop does
    /// this, and so might a device refusing the key silently; which one it
    /// was is unknown.
    #[error("the device closed the secure connection: {0}")]
    TlsConnectionClosed(std::io::Error),
    /// A shell protocol v2 command ended (its stream closed) before the
    /// device reported an exit status. Whether the command ran, and how it
    /// ended, is unknown. The connection itself may be intact.
    #[error("the device closed the command before reporting a result; its outcome is unknown")]
    ShellExitUnknown,
    /// A file is larger than the caller allowed: its size, then the limit.
    /// Nothing was transferred, and the connection is intact.
    #[error("the file is {0} bytes, over the {1}-byte limit")]
    FileTooLarge(u64, u64),
    /// A message payload is larger than the connection's negotiated maximum.
    /// It was refused before anything was written, so the connection is intact.
    #[error("message payload of {0} bytes exceeds the negotiated maximum of {1}")]
    PayloadTooLarge(usize, usize),
    /// The device's file-sync service answered `FAIL`, for example for a
    /// missing path or a destination it cannot write. The connection is intact.
    #[error("the device refused the file operation: {0}")]
    SyncFailed(String),
    /// The device's package manager did not report `Success`. Carries what it
    /// printed, such as `Failure [INSTALL_FAILED_VERSION_DOWNGRADE]`. The
    /// connection is intact.
    #[error("install failed: {0}")]
    InstallFailed(String),
    /// Reading or writing the local end of a transfer (a file on this host,
    /// not on the device) failed. The connection is intact.
    #[error(transparent)]
    LocalIo(std::io::Error),
    /// Error while decoding base64 data
    #[error(transparent)]
    Base64DecodeError(#[from] base64::DecodeError),
    /// Error while encoding base64 data
    #[error(transparent)]
    Base64EncodeError(#[from] base64::EncodeSliceError),
    /// An error occurred with RSA engine
    #[error(transparent)]
    RSAError(#[from] rsa::errors::Error),
    /// Cannot convert given data from slice
    #[error(transparent)]
    TryFromSliceError(#[from] std::array::TryFromSliceError),
    /// Given path does not represent an APK
    #[error("wrong file extension: {0}")]
    WrongFileExtension(String),
    /// An error occurred with PKCS8 data
    #[error("error with pkcs8: {0}")]
    RsaPkcs8Error(#[from] rsa::pkcs8::Error),
    /// Error during certificate generation
    #[error(transparent)]
    CertificateGenerationError(#[from] rcgen::Error),
    /// TLS Error
    #[error(transparent)]
    TLSError(#[from] rustls::Error),
    /// PEM certificate error
    #[error(transparent)]
    PemCertError(#[from] rustls_pki_types::pem::Error),
    /// Error while locking mutex
    #[error("error while locking data")]
    PoisonError,
    /// Cannot upgrade connection from TCP to TLS
    #[error("upgrade error: {0}")]
    UpgradeError(String),
    /// An error occurred while getting mdns devices
    #[cfg(feature = "mdns")]
    #[cfg_attr(docsrs, doc(cfg(feature = "mdns")))]
    #[error(transparent)]
    MDNSError(#[from] mdns_sd::Error),
    /// An error occurred while sending data to channel
    #[error("error sending data to channel")]
    SendError,
    /// An unknown transport has been provided
    #[error("unknown transport: {0}")]
    UnknownTransport(String),
    /// An unknown file mode was encountered in list
    #[error("Unknown file mode {0}")]
    UnknownFileMode(u32),
    /// An error occured while parsing a date
    #[error(transparent)]
    ParseDateError(#[from] chrono::ParseError),
    /// An error occurred while parsing a stat extended response
    #[error("stat response error: {0}")]
    StatResponseError(String),
}

impl<T> From<std::sync::PoisonError<T>> for RustADBError {
    fn from(_err: std::sync::PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

impl RustADBError {
    /// Whether the device's TLS server explicitly refused this client's
    /// certificate, which is how adbd reports a key it has not paired with.
    /// Only a typed TLS alert counts: [`Self::TlsAuthRejected`], or the same
    /// alerts carried as a rustls error. A hang-up
    /// ([`Self::TlsConnectionClosed`]), a timeout or any other transport
    /// failure says nothing about pairing and is never classified as one,
    /// and neither is error text that merely mentions an alert.
    #[must_use]
    pub fn is_client_identity_rejected(&self) -> bool {
        let tls = match self {
            Self::TlsAuthRejected(_) => return true,
            Self::TLSError(error) => Some(error),
            Self::IOError(error) => error
                .get_ref()
                .and_then(|source| source.downcast_ref::<rustls::Error>()),
            _ => None,
        };
        matches!(
            tls,
            Some(rustls::Error::AlertReceived(alert))
                if crate::message_devices::tcp::is_untrusted_key_alert(*alert)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_identity_rejection_requires_a_typed_peer_alert() {
        assert!(
            RustADBError::TlsAuthRejected("TLS alert UnknownCA".into())
                .is_client_identity_rejected()
        );
        for alert in [
            rustls::AlertDescription::CertificateUnknown,
            rustls::AlertDescription::UnknownCA,
            rustls::AlertDescription::CertificateRequired,
            rustls::AlertDescription::AccessDenied,
            rustls::AlertDescription::BadCertificate,
        ] {
            assert!(
                RustADBError::TLSError(rustls::Error::AlertReceived(alert))
                    .is_client_identity_rejected()
            );
            assert!(
                RustADBError::IOError(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    rustls::Error::AlertReceived(alert),
                ))
                .is_client_identity_rejected()
            );
        }
        for error in [
            RustADBError::TLSError(rustls::Error::AlertReceived(
                rustls::AlertDescription::HandshakeFailure,
            )),
            RustADBError::TlsConnectionClosed(std::io::ErrorKind::UnexpectedEof.into()),
            RustADBError::TlsConnectionClosed(std::io::ErrorKind::ConnectionReset.into()),
            RustADBError::IOError(std::io::ErrorKind::TimedOut.into()),
            RustADBError::IOError(std::io::Error::other("CertificateUnknown")),
            RustADBError::ProtocolError("certificate_unknown".into()),
            RustADBError::ADBRequestFailed("not authorized".into()),
        ] {
            assert!(!error.is_client_identity_rejected(), "{error:?}");
        }
    }
}
