//! Base64 helpers matching the `atob`/`btoa` subset used by provider adapters.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Decode standard base64 with `atob` leniency: ASCII whitespace is ignored and
/// missing padding is accepted.
pub fn decode(input: &str) -> Result<Vec<u8>, Base64Error> {
    let mut symbols = Vec::with_capacity(input.len());
    let mut padding = 0usize;
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' => {
                if padding > 0 {
                    return Err(Base64Error);
                }
                symbols.push(byte);
            }
            b'=' => padding += 1,
            b' ' | b'\t' | b'\n' | b'\r' | b'\x0c' => {}
            _ => return Err(Base64Error),
        }
    }
    if padding > 2 || (padding > 0 && (symbols.len() + padding) % 4 != 0) {
        return Err(Base64Error);
    }
    if symbols.len() % 4 == 1 {
        return Err(Base64Error);
    }

    let mut output = Vec::with_capacity(symbols.len() * 3 / 4);
    for chunk in symbols.chunks(4) {
        let values = chunk
            .iter()
            .map(|byte| symbol_value(*byte))
            .collect::<Result<Vec<_>, _>>()?;
        output.push((values[0] << 2) | (values[1] >> 4));
        if values.len() > 2 {
            output.push((values[1] << 4) | (values[2] >> 2));
        }
        if values.len() > 3 {
            output.push((values[2] << 6) | values[3]);
        }
    }
    Ok(output)
}

/// Encode with the standard padded alphabet.
pub fn encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied();
        let third = chunk.get(2).copied();
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[(((first & 0x03) << 4) | (second.unwrap_or(0) >> 4)) as usize] as char);
        output.push(second.map_or('=', |second| {
            ALPHABET[(((second & 0x0f) << 2) | (third.unwrap_or(0) >> 6)) as usize] as char
        }));
        output.push(third.map_or('=', |third| ALPHABET[(third & 0x3f) as usize] as char));
    }
    output
}

fn symbol_value(byte: u8) -> Result<u8, Base64Error> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(Base64Error),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Base64Error;
