use std::collections::HashSet;
use std::str;

use super::options::{CborCodecError, CborError, CborOptions, ResolvedCborOptions, resolve_options};
use super::{CborValue, is_safe_integer};

const UINT32_BASE: u64 = 0x1_0000_0000;
const MAX_UINT64_ARGUMENT_HIGH: u64 = 0x1f_ffff;

/// Decodes exactly one item from the protocol's strict RFC 8949 subset.
pub fn decode_cbor(bytes: &[u8], options: Option<CborOptions>) -> Result<CborValue, CborCodecError> {
    let resolved = resolve_options(options)?;
    if bytes.len() > resolved.max_byte_length() as usize {
        return Err(CborError::new(format!(
            "CBOR byte length exceeds configured limit of {}",
            resolved.max_byte_length()
        ))
        .into());
    }
    CborReader::new(bytes, resolved).decode()
}

struct CborReader<'a> {
    bytes: &'a [u8],
    offset: usize,
    options: ResolvedCborOptions,
}

impl<'a> CborReader<'a> {
    const fn new(bytes: &'a [u8], options: ResolvedCborOptions) -> Self {
        Self {
            bytes,
            offset: 0,
            options,
        }
    }

    fn decode(mut self) -> Result<CborValue, CborCodecError> {
        let value = self.read_item(0)?;
        if self.offset != self.bytes.len() {
            return Err(CborError::new("CBOR payload contains trailing data").into());
        }
        Ok(value)
    }

    fn read_item(&mut self, depth: u16) -> Result<CborValue, CborError> {
        if depth > self.options.max_depth() {
            return Err(CborError::new(format!(
                "CBOR nesting depth exceeds configured limit of {}",
                self.options.max_depth()
            )));
        }

        let initial = self.read_byte()?;
        match initial >> 5 {
            0 => Ok(CborValue::Number(self.read_argument(initial & 0x1f)? as f64)),
            1 => self.read_negative_integer(initial & 0x1f),
            2 => self.read_byte_string(initial & 0x1f),
            3 => self.read_text_string(initial & 0x1f),
            4 => self.read_array(initial & 0x1f, depth),
            5 => self.read_map(initial & 0x1f, depth),
            6 => Err(CborError::new("CBOR tags are not supported")),
            _ => self.read_simple(initial & 0x1f),
        }
    }

    fn read_negative_integer(&mut self, additional_information: u8) -> Result<CborValue, CborError> {
        let argument = self.read_argument(additional_information)? as f64;
        let value = -1.0 - argument;
        if !is_safe_integer(value) {
            return Err(CborError::new("Decoded CBOR integer is outside the safe range"));
        }
        Ok(CborValue::Number(value))
    }

    fn read_byte_string(&mut self, additional_information: u8) -> Result<CborValue, CborError> {
        let length = self.read_length(additional_information, "byte string", self.options.max_byte_length())?;
        Ok(CborValue::Bytes(self.read_bytes(length)?))
    }

    fn read_text_string(&mut self, additional_information: u8) -> Result<CborValue, CborError> {
        let length = self.read_length(additional_information, "text string", self.options.max_byte_length())?;
        let bytes = self.read_bytes(length)?;
        let value = str::from_utf8(&bytes).map_err(|_| CborError::new("CBOR text string contains invalid UTF-8"))?;
        Ok(CborValue::Text(value.to_string()))
    }

    fn read_array(&mut self, additional_information: u8, depth: u16) -> Result<CborValue, CborError> {
        let length = self.read_length(additional_information, "array", self.options.max_container_length())?;
        let mut values = Vec::new();
        for _ in 0..length {
            values.push(self.read_item(depth + 1)?);
        }
        Ok(CborValue::Array(values))
    }

    fn read_map(&mut self, additional_information: u8, depth: u16) -> Result<CborValue, CborError> {
        let length = self.read_length(additional_information, "map", self.options.max_container_length())?;
        let mut entries = Vec::new();
        let mut keys = HashSet::new();
        for _ in 0..length {
            let key = match self.read_item(depth + 1)? {
                CborValue::Text(key) => key,
                _ => return Err(CborError::new("CBOR map keys must be strings")),
            };
            if !keys.insert(key.clone()) {
                return Err(CborError::new("CBOR map contains a duplicate key"));
            }
            let value = self.read_item(depth + 1)?;
            entries.push((key, value));
        }
        Ok(CborValue::Map(entries))
    }

    fn read_simple(&mut self, additional_information: u8) -> Result<CborValue, CborError> {
        match additional_information {
            20 => Ok(CborValue::Bool(false)),
            21 => Ok(CborValue::Bool(true)),
            22 => Ok(CborValue::Null),
            27 => self.read_float64(),
            31 => Err(CborError::new("CBOR break marker is not supported")),
            _ => Err(CborError::new("Unsupported CBOR simple value or floating-point width")),
        }
    }

    fn read_float64(&mut self) -> Result<CborValue, CborError> {
        let bytes = self.read_bytes(8)?;
        let value = f64::from_be_bytes(bytes.try_into().expect("eight bytes must convert to f64"));
        if !value.is_finite() {
            return Err(CborError::new("Decoded CBOR number must be finite"));
        }
        if value.fract() == 0.0 && !is_safe_integer(value) {
            return Err(CborError::new("Decoded CBOR integer is outside the safe range"));
        }
        Ok(CborValue::Number(value))
    }

    fn read_length(&mut self, additional_information: u8, kind: &str, limit: u32) -> Result<u64, CborError> {
        if additional_information == 31 {
            return Err(CborError::new(format!(
                "Indefinite-length CBOR {kind}s are not supported"
            )));
        }
        let length = self.read_argument(additional_information)?;
        if length > u64::from(limit) {
            return Err(CborError::new(format!(
                "CBOR {kind} length exceeds configured limit of {limit}"
            )));
        }
        Ok(length)
    }

    fn read_argument(&mut self, additional_information: u8) -> Result<u64, CborError> {
        if additional_information < 24 {
            return Ok(u64::from(additional_information));
        }
        match additional_information {
            24 => Ok(u64::from(self.read_byte()?)),
            25 => {
                let bytes = self.read_bytes(2)?;
                Ok(u64::from(bytes[0]) * 0x100 + u64::from(bytes[1]))
            }
            26 => {
                let bytes = self.read_bytes(4)?;
                Ok(u64::from(bytes[0]) * 0x1_000_000
                    + u64::from(bytes[1]) * 0x1_0000
                    + u64::from(bytes[2]) * 0x100
                    + u64::from(bytes[3]))
            }
            27 => self.read_uint64_argument(),
            31 => Err(CborError::new("Indefinite-length CBOR items are not supported")),
            _ => Err(CborError::new("Malformed CBOR additional information")),
        }
    }

    fn read_uint64_argument(&mut self) -> Result<u64, CborError> {
        let high = self.read_argument(26)?;
        let low = self.read_argument(26)?;
        if high > MAX_UINT64_ARGUMENT_HIGH {
            return Err(CborError::new(
                "Decoded CBOR integer or length is outside the safe range",
            ));
        }
        Ok(high * UINT32_BASE + low)
    }

    fn read_byte(&mut self) -> Result<u8, CborError> {
        if self.offset >= self.bytes.len() {
            return Err(CborError::new("Truncated CBOR payload"));
        }
        let value = self.bytes[self.offset];
        self.offset += 1;
        Ok(value)
    }

    fn read_bytes(&mut self, length: u64) -> Result<Vec<u8>, CborError> {
        let remaining = self.bytes.len() - self.offset;
        if length > remaining as u64 {
            return Err(CborError::new("Truncated CBOR payload"));
        }
        let length = length as usize;
        let value = self.bytes[self.offset..self.offset + length].to_vec();
        self.offset += length;
        Ok(value)
    }
}
