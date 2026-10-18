//! Raw frame comparison helpers for wire differentials.
//!
//! Frames are compared byte for byte, in emission order. Only values that the source derives from
//! clocks or randomness are replaced, and each replacement happens in place: the rest of the CBOR
//! payload (map key order, field presence, encoding widths) stays untouched.

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn decode_hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).expect("hex digits"))
        .collect()
}

const DYNAMIC_MARKER: u8 = 0xff;

const DYNAMIC_NUMBER_KEYS: [&str; 2] = ["createdAt", "updatedAt"];

/// Normalizes one framed payload: dynamic values are replaced with a fixed marker and every other
/// byte is preserved.
pub fn normalize_frame(payload: &[u8]) -> String {
    let mut normalized = Vec::with_capacity(payload.len());
    let mut cursor = 0;
    normalize_item(payload, &mut cursor, None, &mut normalized);
    assert_eq!(cursor, payload.len(), "payload must be consumed exactly");
    hex(&normalized)
}

/// Returns the complete frame hex: the deterministic length prefix plus the normalized payload.
pub fn frame_hex(payload: &[u8]) -> String {
    let mut frame = hex(&(payload.len() as u32).to_be_bytes());
    frame.push_str(&normalize_frame(payload));
    frame
}

fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

fn read_argument(bytes: &[u8], cursor: &mut usize, additional: u8) -> u64 {
    let (width, value) = match additional {
        0..=23 => (0, additional as u64),
        24 => (1, bytes[*cursor] as u64),
        25 => (2, u16::from_be_bytes([bytes[*cursor], bytes[*cursor + 1]]) as u64),
        26 => (
            4,
            u32::from_be_bytes([
                bytes[*cursor],
                bytes[*cursor + 1],
                bytes[*cursor + 2],
                bytes[*cursor + 3],
            ]) as u64,
        ),
        27 => (
            8,
            u64::from_be_bytes([
                bytes[*cursor],
                bytes[*cursor + 1],
                bytes[*cursor + 2],
                bytes[*cursor + 3],
                bytes[*cursor + 4],
                bytes[*cursor + 5],
                bytes[*cursor + 6],
                bytes[*cursor + 7],
            ]),
        ),
        _ => panic!("indefinite CBOR lengths are outside the protocol subset"),
    };
    *cursor += width;
    value
}

fn copy_item(bytes: &[u8], start: usize, cursor: usize, output: &mut Vec<u8>) {
    output.extend_from_slice(&bytes[start..cursor]);
}

fn normalize_item(bytes: &[u8], cursor: &mut usize, key: Option<&str>, output: &mut Vec<u8>) {
    let start = *cursor;
    let head = bytes[*cursor];
    *cursor += 1;
    let major = head >> 5;
    let additional = head & 0x1f;
    let argument = read_argument(bytes, cursor, additional);

    match major {
        0 | 1 => {
            if key.is_some_and(|key| DYNAMIC_NUMBER_KEYS.contains(&key)) {
                output.push(DYNAMIC_MARKER);
            } else {
                copy_item(bytes, start, *cursor, output);
            }
        }
        2 => {
            let length = argument as usize;
            *cursor += length;
            copy_item(bytes, start, *cursor, output);
        }
        3 => {
            let length = argument as usize;
            let value = std::str::from_utf8(&bytes[*cursor..*cursor + length]).expect("CBOR text must be UTF-8");
            let dynamic = key == Some("connectionId") || is_uuid(value);
            *cursor += length;
            if dynamic {
                output.push(DYNAMIC_MARKER);
            } else {
                copy_item(bytes, start, *cursor, output);
            }
        }
        4 => {
            copy_item(bytes, start, *cursor, output);
            for _ in 0..argument {
                normalize_item(bytes, cursor, None, output);
            }
        }
        5 => {
            copy_item(bytes, start, *cursor, output);
            for _ in 0..argument {
                let entry_key = read_map_key(bytes, cursor, output);
                normalize_item(bytes, cursor, Some(&entry_key), output);
            }
        }
        7 => copy_item(bytes, start, *cursor, output),
        _ => panic!("unsupported CBOR major type {major} in protocol frame"),
    }
}

fn read_map_key(bytes: &[u8], cursor: &mut usize, output: &mut Vec<u8>) -> String {
    let start = *cursor;
    let head = bytes[*cursor];
    *cursor += 1;
    assert_eq!(head >> 5, 3, "protocol map keys must be text");
    let length = read_argument(bytes, cursor, head & 0x1f) as usize;
    let value = std::str::from_utf8(&bytes[*cursor..*cursor + length])
        .expect("CBOR map keys must be UTF-8")
        .to_string();
    *cursor += length;
    copy_item(bytes, start, *cursor, output);
    value
}
