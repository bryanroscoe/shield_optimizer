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
