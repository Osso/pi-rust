//! Node's POSIX fileURLToPath path states. The caller validates/normalizes the
//! file host with its existing IDNA parser; no URL parser owns the pathname.

use crate::js_json::{JsString, JsValue, encode_node_utf8};
use crate::node_error::{NativeErrorClass, NativeNodeError};

pub struct FileUrl {
    host: String,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

impl FileUrl {
    /// Input has the literal file:// prefix, as checked by normalizePath.
    pub fn parse(input: &JsString) -> Self {
        let text = encode_node_utf8(input);
        let text = text.trim_matches(|character: char| character <= '\u{20}');
        let text = text.replace(['\t', '\r', '\n'], "");
        let tail = &text[7..];
        let end = tail.find(['/', '\\', '?', '#']).unwrap_or(tail.len());
        let authority = &tail[..end];
        let remainder = &tail[end..];
        let (host, path) = if is_drive_letter(authority) {
            (String::new(), format!("/{authority}{remainder}"))
        } else {
            (authority.to_owned(), remainder.to_owned())
        };
        let (path, fragment) = split_suffix(&path, '#');
        let (path, query) = split_suffix(path, '?');
        Self {
            host,
            path: normalize_file_path(path),
            query: query.map(str::to_owned),
            fragment: fragment.map(str::to_owned),
        }
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn into_posix_path(self, normalized_host: &str) -> Result<JsString, NativeNodeError> {
        if !matches!(normalized_host, "" | "localhost") {
            return Err(node_type_error(
                "ERR_INVALID_FILE_URL_HOST",
                format!(
                    "File URL host must be \"localhost\" or empty on {}",
                    std::env::consts::OS
                ),
            ));
        }
        decode_file_path(&self.path, &self.serialized_url())
    }

    fn serialized_url(&self) -> JsString {
        let mut output = format!("file://{}", encode_url_component(&self.path, UrlComponent::Path));
        if let Some(query) = &self.query {
            output.push('?');
            output.push_str(&encode_url_component(query, UrlComponent::Query));
        }
        if let Some(fragment) = &self.fragment {
            output.push('#');
            output.push_str(&encode_url_component(fragment, UrlComponent::Fragment));
        }
        output.into()
    }
}

fn split_suffix(input: &str, separator: char) -> (&str, Option<&str>) {
    input
        .split_once(separator)
        .map_or((input, None), |(head, tail)| (head, Some(tail)))
}

#[derive(Clone, Copy)]
enum UrlComponent {
    Path,
    Query,
    Fragment,
}

fn encode_url_component(input: &str, component: UrlComponent) -> String {
    let mut output = String::new();
    for byte in input.bytes() {
        let common = byte <= 0x20 || byte >= 0x7f || matches!(byte, b'\"' | b'<' | b'>');
        let encode = common
            || match component {
                UrlComponent::Path => matches!(byte, b'#' | b'?' | b'`' | b'{' | b'}'),
                UrlComponent::Query => matches!(byte, b'#' | b'\''),
                UrlComponent::Fragment => byte == b'`',
            };
        if encode {
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 15)]));
        } else {
            output.push(char::from(byte));
        }
    }
    output
}

fn normalize_file_path(input: &str) -> String {
    let input = input.replace('\\', "/");
    let input = input.strip_prefix('/').unwrap_or(&input);
    let mut segments: Vec<String> = Vec::new();
    let mut remaining = input.split('/').peekable();
    while let Some(segment) = remaining.next() {
        let lower = segment.to_ascii_lowercase();
        if matches!(lower.as_str(), ".." | ".%2e" | "%2e." | "%2e%2e") {
            if segments.len() != 1 || !is_drive_letter(&segments[0]) {
                segments.pop();
            }
            if remaining.peek().is_none() {
                segments.push(String::new());
            }
        } else if matches!(lower.as_str(), "." | "%2e") {
            if remaining.peek().is_none() {
                segments.push(String::new());
            }
        } else if segments.is_empty() && is_drive_letter(segment) {
            segments.push(format!("{}:", &segment[..1]));
        } else {
            segments.push(segment.to_owned());
        }
    }
    format!("/{}", segments.join("/"))
}

fn is_drive_letter(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && matches!(bytes[1], b':' | b'|')
}

fn decode_file_path(path: &str, input: &JsString) -> Result<JsString, NativeNodeError> {
    if path.to_ascii_lowercase().contains("%2f") {
        return Err(node_type_error(
            "ERR_INVALID_FILE_URL_PATH",
            "File URL path must not include encoded / characters",
        )
        .with_property("input", JsValue::String(input.clone())));
    }
    let malformed = || NativeNodeError::from_constructor(NativeErrorClass::URIError, "URI malformed");
    let mut bytes = Vec::with_capacity(path.len());
    let mut input = path.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = input.next().and_then(hex_value).ok_or_else(malformed)?;
            let low = input.next().and_then(hex_value).ok_or_else(malformed)?;
            bytes.push(high * 16 + low);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).map(JsString::from).map_err(|_| malformed())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn node_type_error(code: &str, message: impl Into<JsString>) -> NativeNodeError {
    NativeNodeError::from_node_constructor(NativeErrorClass::TypeError, code, message)
}
