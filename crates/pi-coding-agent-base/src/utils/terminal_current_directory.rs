use std::fmt::Write as _;

use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

use crate::utils::paths::{PathError, node_resolve};

/// OSC 7 announces the logical current directory to terminals so actions such
/// as opening a new tab can inherit the application-level cwd.
pub fn format_terminal_current_directory_sequence(cwd: &JsString) -> Result<JsString, PathError> {
    let path = resolve_file_url_path(cwd)?;
    // The URL serializer encodes the path as UTF-8 (lone surrogates as U+FFFD).
    Ok(js_format!(
        "\x1b]7;file://{}{}\x1b\\",
        pi_js_compat::node_fs::decode_node_path(gethostname::gethostname().as_os_str()),
        encode_file_url_path(&encode_node_utf8(&path))
    ))
}

/// `pathToFileURL`: `path.resolve(cwd)`, re-adding a trailing separator the input had.
fn resolve_file_url_path(cwd: &JsString) -> Result<JsString, PathError> {
    let mut resolved = node_resolve("", cwd.clone())?;
    if cwd.ends_with("/") && resolved != "/" {
        resolved.push_str("/");
    }
    Ok(resolved)
}

fn encode_file_url_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if is_unreserved_file_url_byte(byte) {
            encoded.push(char::from(byte));
            continue;
        }
        let _ = write!(encoded, "%{byte:02X}");
    }
    encoded
}

fn is_unreserved_file_url_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'-' | b'.'
                | b'_'
                | b'/'
                | b':'
                | b'@'
                | b'!'
                | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
        )
}
