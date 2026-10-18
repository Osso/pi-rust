//! Minimal Base64 support matching Node's `Buffer` semantics used by
//! `terminal-image.ts` (lenient decode, standard padded encode).

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn decode_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decode Base64 like `Buffer.from(value, "base64")`: invalid characters are
/// ignored and decoding stops at the first `=`.
pub(crate) fn decode_lenient(value: &str) -> Vec<u8> {
    let mut values = Vec::new();
    for byte in value.bytes() {
        if byte == b'=' {
            break;
        }
        if let Some(decoded) = decode_value(byte) {
            values.push(decoded);
        }
    }

    let mut output = Vec::with_capacity(values.len() * 3 / 4);
    for chunk in values.chunks(4) {
        let first = chunk[0];
        if chunk.len() >= 2 {
            output.push((first << 2) | (chunk[1] >> 4));
        }
        if chunk.len() >= 3 {
            output.push(((chunk[1] & 0x0f) << 4) | (chunk[2] >> 2));
        }
        if chunk.len() == 4 {
            output.push(((chunk[2] & 0x03) << 6) | chunk[3]);
        }
    }
    output
}

/// Decoded byte length of a Base64 payload.
pub(crate) fn decoded_len(value: &str) -> usize {
    let mut count = 0;
    for byte in value.bytes() {
        if byte == b'=' {
            break;
        }
        if decode_value(byte).is_some() {
            count += 1;
        }
    }
    count / 4 * 3
        + match count % 4 {
            2 => 1,
            3 => 2,
            _ => 0,
        }
}

/// Standard padded Base64 encoding.
pub(crate) fn encode(bytes: &[u8]) -> String {
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(ALPHABET[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(ALPHABET[(third & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{decode_lenient, decoded_len, encode};

    #[test]
    fn decodes_with_node_leniency() {
        assert_eq!(decode_lenient("AAAA"), vec![0, 0, 0]);
        assert_eq!(decode_lenient("AAA="), vec![0, 0]);
        assert_eq!(decode_lenient("AA=="), vec![0]);
        assert_eq!(decode_lenient("A"), Vec::<u8>::new());
        assert_eq!(decode_lenient("AAAAA"), vec![0, 0, 0]);
        assert_eq!(decode_lenient("AA==AAAA"), vec![0]);
        assert_eq!(decode_lenient("A A A A"), vec![0, 0, 0]);
        assert_eq!(decode_lenient("!!!!"), Vec::<u8>::new());
        assert_eq!(decode_lenient("AAA=AAAA"), vec![0, 0]);
    }

    #[test]
    fn reports_decoded_lengths() {
        for (value, expected) in [
            ("AAAA", 3),
            ("AAA=", 2),
            ("AA==", 1),
            ("A", 0),
            ("AAAAA", 3),
            ("AA==AAAA", 1),
            ("", 0),
        ] {
            assert_eq!(decoded_len(value), expected, "{value:?}");
        }
    }

    #[test]
    fn encodes_standard_padding() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"A"), "QQ==");
        assert_eq!(encode(b"AB"), "QUI=");
        assert_eq!(encode(b"ABC"), "QUJD");
        assert_eq!(encode(b"AAAA"), "QUFBQQ==");
    }
}
