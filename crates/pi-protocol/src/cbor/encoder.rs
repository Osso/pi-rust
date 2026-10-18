use std::collections::HashSet;

use super::options::{CborCodecError, CborError, CborOptions, ResolvedCborOptions, resolve_options};
use super::{CborValue, is_negative_zero, is_safe_integer};

const MAX_UINT32: u64 = 0xffff_ffff;

/// Encodes the protocol's strict, definite-length RFC 8949 subset.
pub fn encode_cbor(value: &CborValue, options: Option<CborOptions>) -> Result<Vec<u8>, CborCodecError> {
    let resolved = resolve_options(options)?;
    let mut writer = CborWriter::new(resolved.max_byte_length());
    encode_value(&mut writer, value, resolved, 0)?;
    Ok(writer.finish())
}

struct CborWriter {
    buffer: Vec<u8>,
    max_byte_length: u32,
}

impl CborWriter {
    fn new(max_byte_length: u32) -> Self {
        Self {
            buffer: Vec::new(),
            max_byte_length,
        }
    }

    fn finish(self) -> Vec<u8> {
        self.buffer
    }

    fn write_byte(&mut self, value: u8) -> Result<(), CborError> {
        self.ensure_capacity(1)?;
        self.buffer.push(value);
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), CborError> {
        self.ensure_capacity(bytes.len())?;
        self.buffer.extend_from_slice(bytes);
        Ok(())
    }

    fn write_uint16(&mut self, value: u16) -> Result<(), CborError> {
        self.write_bytes(&value.to_be_bytes())
    }

    fn write_uint32(&mut self, value: u32) -> Result<(), CborError> {
        self.write_bytes(&value.to_be_bytes())
    }

    fn write_uint64(&mut self, value: u64) -> Result<(), CborError> {
        self.write_bytes(&value.to_be_bytes())
    }

    fn write_float64(&mut self, value: f64) -> Result<(), CborError> {
        self.write_byte(0xfb)?;
        self.write_bytes(&value.to_bits().to_be_bytes())
    }

    fn ensure_capacity(&self, additional_bytes: usize) -> Result<(), CborError> {
        let required = self.buffer.len().saturating_add(additional_bytes);
        if required > self.max_byte_length as usize {
            return Err(CborError::new(format!(
                "CBOR byte length exceeds configured limit of {}",
                self.max_byte_length
            )));
        }
        Ok(())
    }
}

fn write_argument(writer: &mut CborWriter, major_type: u8, value: u64) -> Result<(), CborError> {
    let prefix = major_type << 5;
    if value < 24 {
        writer.write_byte(prefix | value as u8)
    } else if value <= 0xff {
        writer.write_byte(prefix | 24)?;
        writer.write_byte(value as u8)
    } else if value <= 0xffff {
        writer.write_byte(prefix | 25)?;
        writer.write_uint16(value as u16)
    } else if value <= MAX_UINT32 {
        writer.write_byte(prefix | 26)?;
        writer.write_uint32(value as u32)
    } else {
        writer.write_byte(prefix | 27)?;
        writer.write_uint64(value)
    }
}

fn encode_text(writer: &mut CborWriter, value: &str, options: ResolvedCborOptions) -> Result<(), CborError> {
    let bytes = value.as_bytes();
    if bytes.len() > options.max_byte_length() as usize {
        return Err(CborError::new(format!(
            "CBOR text string length exceeds configured limit of {}",
            options.max_byte_length()
        )));
    }
    write_argument(writer, 3, bytes.len() as u64)?;
    writer.write_bytes(bytes)
}

fn encode_value(
    writer: &mut CborWriter,
    value: &CborValue,
    options: ResolvedCborOptions,
    depth: u16,
) -> Result<(), CborError> {
    if depth > options.max_depth() {
        return Err(CborError::new(format!(
            "CBOR nesting depth exceeds configured limit of {}",
            options.max_depth()
        )));
    }

    match value {
        CborValue::Null => writer.write_byte(0xf6),
        CborValue::Bool(value) => writer.write_byte(if *value { 0xf5 } else { 0xf4 }),
        CborValue::Number(value) => encode_number(writer, *value),
        CborValue::Text(value) => encode_text(writer, value, options),
        CborValue::Bytes(value) => encode_bytes(writer, value, options),
        CborValue::Array(values) => encode_array(writer, values, options, depth),
        CborValue::Map(entries) => encode_map(writer, entries, options, depth),
    }
}

fn encode_number(writer: &mut CborWriter, value: f64) -> Result<(), CborError> {
    if !value.is_finite() {
        return Err(CborError::new("CBOR numbers must be finite"));
    }
    if value.fract() != 0.0 || is_negative_zero(value) {
        return writer.write_float64(value);
    }
    if !is_safe_integer(value) {
        return Err(CborError::new("CBOR integers must be safe JavaScript integers"));
    }
    if value >= 0.0 {
        write_argument(writer, 0, value as u64)
    } else {
        write_argument(writer, 1, (-1.0 - value) as u64)
    }
}

fn encode_bytes(writer: &mut CborWriter, value: &[u8], options: ResolvedCborOptions) -> Result<(), CborError> {
    if value.len() > options.max_byte_length() as usize {
        return Err(CborError::new(format!(
            "CBOR byte string length exceeds configured limit of {}",
            options.max_byte_length()
        )));
    }
    write_argument(writer, 2, value.len() as u64)?;
    writer.write_bytes(value)
}

fn encode_array(
    writer: &mut CborWriter,
    values: &[CborValue],
    options: ResolvedCborOptions,
    depth: u16,
) -> Result<(), CborError> {
    if values.len() > options.max_container_length() as usize {
        return Err(CborError::new(format!(
            "CBOR array length exceeds configured limit of {}",
            options.max_container_length()
        )));
    }
    write_argument(writer, 4, values.len() as u64)?;
    for value in values {
        encode_value(writer, value, options, depth + 1)?;
    }
    Ok(())
}

fn encode_map(
    writer: &mut CborWriter,
    entries: &[(String, CborValue)],
    options: ResolvedCborOptions,
    depth: u16,
) -> Result<(), CborError> {
    if entries.len() > options.max_container_length() as usize {
        return Err(CborError::new(format!(
            "CBOR map length exceeds configured limit of {}",
            options.max_container_length()
        )));
    }
    reject_duplicate_keys(entries)?;
    write_argument(writer, 5, entries.len() as u64)?;
    for (key, value) in entries {
        encode_text(writer, key, options)?;
        encode_value(writer, value, options, depth + 1)?;
    }
    Ok(())
}

fn reject_duplicate_keys(entries: &[(String, CborValue)]) -> Result<(), CborError> {
    let mut keys = HashSet::new();
    for (key, _) in entries {
        if !keys.insert(key.as_str()) {
            return Err(CborError::new("CBOR map contains a duplicate key"));
        }
    }
    Ok(())
}
