//! Shared process console writes, re-exported by the CLI entry module.
use std::io::Write;

use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, encode_node_utf8};

/// Writes text to stdout and flushes, like `process.stdout.write`.
pub fn write_stdout(text: impl Into<JsString>) {
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(encode_node_utf8(&text.into()).as_bytes());
    let _ = stdout.flush();
}

/// Writes text to stderr and flushes, like `process.stderr.write`.
pub fn write_stderr(text: impl Into<JsString>) {
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(encode_node_utf8(&text.into()).as_bytes());
    let _ = stderr.flush();
}

/// `console.log`.
pub fn console_log(text: impl Into<JsString>) {
    write_stdout(js_format!("{}\n", text.into()));
}

/// `console.error`.
pub fn console_error(text: impl Into<JsString>) {
    write_stderr(js_format!("{}\n", text.into()));
}
