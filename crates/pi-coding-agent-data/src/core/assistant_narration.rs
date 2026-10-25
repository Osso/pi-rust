//! Frozen `core/assistant-narration.ts`: public prose inside signed thinking.

use pi_ai::types::AssistantContent;
use pi_js_compat::{base64::buffer_from_base64, js_json::JsString};

/// Return public prose without modifying signed blocks used for provider replay.
pub fn get_assistant_content_text(content: &AssistantContent) -> Option<JsString> {
    match content {
        AssistantContent::Text(content) => Some(content.text.clone()),
        AssistantContent::Thinking(content) if is_server_narration_signature(content.thinking_signature.as_deref()) => {
            Some(content.thinking.clone())
        }
        _ => None,
    }
}

/// Anthropic narration marker at protobuf envelope field 2 -> 1 -> 8.
/// Signatures use the approved Unicode-scalar opaque-ID boundary.
pub fn is_server_narration_signature(signature: Option<&str>) -> bool {
    let Some(signature) = signature.filter(|signature| !signature.is_empty()) else {
        return false;
    };
    let bytes = buffer_from_base64(signature);
    let Some(envelope) = read_protobuf_field(&bytes, 2) else {
        return false;
    };
    let Some(header) = read_protobuf_field(envelope, 1) else {
        return false;
    };
    // Node's UTF-8 decoding equals this ASCII literal iff these bytes match.
    read_protobuf_field(header, 8) == Some(b"narration".as_slice())
}

fn read_protobuf_varint(bytes: &[u8], mut offset: usize) -> Option<(u64, usize)> {
    let mut value = 0;
    for shift in (0..35).step_by(7) {
        let byte = *bytes.get(offset)?;
        offset += 1;
        value += u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return Some((value, offset));
        }
    }
    None
}

fn read_protobuf_field(bytes: &[u8], field: u32) -> Option<&[u8]> {
    let mut offset = 0;
    while offset < bytes.len() {
        let (key, end) = read_protobuf_varint(bytes, offset)?;
        offset = end;
        // Frozen JS bitwise operations narrow the accepted 35-bit key to u32.
        let key = key as u32;
        match key & 7 {
            0 => offset = read_protobuf_varint(bytes, offset)?.1,
            1 => offset = offset.checked_add(8)?,
            5 => offset = offset.checked_add(4)?,
            2 => {
                let (length, start) = read_protobuf_varint(bytes, offset)?;
                let end = start.checked_add(usize::try_from(length).ok()?)?;
                let value = bytes.get(start..end)?;
                if key >> 3 == field {
                    return Some(value);
                }
                offset = end;
            }
            _ => return None,
        }
    }
    None
}
