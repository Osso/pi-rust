mod decoder;
mod encoder;
mod options;

pub use decoder::decode_cbor;
pub use encoder::encode_cbor;
pub use options::{
    CborCodecError, CborError, CborOptions, CborOptionsError, DEFAULT_MAX_CBOR_BYTE_LENGTH,
    DEFAULT_MAX_CBOR_CONTAINER_LENGTH, DEFAULT_MAX_CBOR_DEPTH,
};

const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// A value in the protocol's strict RFC 8949 subset.
///
/// Map entries retain wire order, and duplicate keys are rejected by both codecs.
#[derive(Debug, Clone, PartialEq)]
pub enum CborValue {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    Bytes(Vec<u8>),
    Array(Vec<CborValue>),
    Map(Vec<(String, CborValue)>),
}

pub(crate) fn is_safe_integer(value: f64) -> bool {
    value.is_finite() && value.fract() == 0.0 && value.abs() <= MAX_SAFE_INTEGER
}

pub(crate) fn is_negative_zero(value: f64) -> bool {
    value == 0.0 && value.is_sign_negative()
}
