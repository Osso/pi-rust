use std::error::Error;
use std::fmt::{self, Display, Formatter};

const MAX_CONFIGURED_DEPTH: u16 = 512;

/// Default maximum encoded input/output bytes and maximum byte/text string length.
pub const DEFAULT_MAX_CBOR_BYTE_LENGTH: u32 = 16 * 1024 * 1024;
/// Default maximum number of array elements or map entries.
pub const DEFAULT_MAX_CBOR_CONTAINER_LENGTH: u32 = 1_000_000;
/// Default maximum recursive item depth.
pub const DEFAULT_MAX_CBOR_DEPTH: u16 = 64;

/// Caller-provided limits for untrusted CBOR payloads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CborOptions {
    pub max_byte_length: Option<u32>,
    pub max_container_length: Option<u32>,
    pub max_depth: Option<u16>,
}

/// Limits applied after validating caller-provided options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedCborOptions {
    max_byte_length: u32,
    max_container_length: u32,
    max_depth: u16,
}

impl ResolvedCborOptions {
    pub(crate) const fn max_byte_length(self) -> u32 {
        self.max_byte_length
    }

    pub(crate) const fn max_container_length(self) -> u32 {
        self.max_container_length
    }

    pub(crate) const fn max_depth(self) -> u16 {
        self.max_depth
    }
}

/// Failure while encoding or decoding CBOR data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CborError {
    message: String,
}

impl CborError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for CborError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for CborError {}

/// Returned when a configured CBOR limit is outside its accepted range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CborOptionsError;

impl Display for CborOptionsError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "maxDepth must be an integer between 0 and {MAX_CONFIGURED_DEPTH}"
        )
    }
}

impl Error for CborOptionsError {}

/// Failure from a CBOR codec operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CborCodecError {
    /// Data could not be encoded or decoded within the strict source subset.
    Cbor(CborError),
    /// A configured limit was invalid.
    Options(CborOptionsError),
}

impl Display for CborCodecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cbor(error) => Display::fmt(error, formatter),
            Self::Options(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for CborCodecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Cbor(error) => Some(error),
            Self::Options(error) => Some(error),
        }
    }
}

impl From<CborError> for CborCodecError {
    fn from(error: CborError) -> Self {
        Self::Cbor(error)
    }
}

impl From<CborOptionsError> for CborCodecError {
    fn from(error: CborOptionsError) -> Self {
        Self::Options(error)
    }
}

pub(crate) fn resolve_options(options: Option<CborOptions>) -> Result<ResolvedCborOptions, CborOptionsError> {
    let options = options.unwrap_or_default();
    let max_depth = options.max_depth.unwrap_or(DEFAULT_MAX_CBOR_DEPTH);
    if max_depth > MAX_CONFIGURED_DEPTH {
        return Err(CborOptionsError);
    }

    Ok(ResolvedCborOptions {
        max_byte_length: options.max_byte_length.unwrap_or(DEFAULT_MAX_CBOR_BYTE_LENGTH),
        max_container_length: options
            .max_container_length
            .unwrap_or(DEFAULT_MAX_CBOR_CONTAINER_LENGTH),
        max_depth,
    })
}
