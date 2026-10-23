//! Node readline file boundary: stream until the requested number of lines, not EOF.
use super::{abort_result, aborted_io_error, file_error};
use crate::harness::types::{FileError, ReadTextLinesOptions};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use tokio::io::AsyncReadExt;

const READ_BUFFER_BYTES: usize = 64 * 1024;

pub(super) async fn read_text_lines(path: &JsString, options: ReadTextLinesOptions) -> Result<Vec<String>, FileError> {
    let mut file = await_file_io(
        tokio::fs::File::open(encode_node_utf8(path)),
        path,
        "open",
        options.abort.as_ref(),
    )
    .await?;
    let mut decoded = LineBuffer::default();
    let mut bytes = vec![0; READ_BUFFER_BYTES];
    loop {
        let count = await_file_io(file.read(&mut bytes), path, "read", options.abort.as_ref()).await?;
        if count == 0 {
            break;
        }
        if let Some(error) = abort_result(options.abort.as_ref(), path) {
            return Err(error);
        }
        let reached_limit = decoded.feed(&bytes[..count], options.max_lines);
        if let Some(error) = abort_result(options.abort.as_ref(), path) {
            return Err(error);
        }
        if reached_limit {
            return Ok(decoded.lines);
        }
    }
    if let Some(error) = abort_result(options.abort.as_ref(), path) {
        return Err(error);
    }
    Ok(decoded.finish())
}
async fn await_file_io<T>(
    future: impl std::future::Future<Output = std::io::Result<T>>,
    path: &JsString,
    operation: &str,
    abort: Option<&AbortSignal>,
) -> Result<T, FileError> {
    let result = if let Some(abort) = abort {
        tokio::select! {
            biased;
            reason = abort.wait() => return Err(aborted_io_error(path, reason)),
            result = future => result,
        }
    } else {
        future.await
    };
    result.map_err(|error| file_error(&error, operation, path))
}
#[derive(Default)]
struct LineBuffer {
    lines: Vec<String>,
    pending: Vec<u8>,
    skip_lf: bool,
}
impl LineBuffer {
    fn feed(&mut self, bytes: &[u8], max_lines: Option<f64>) -> bool {
        for byte in bytes {
            if std::mem::take(&mut self.skip_lf) && *byte == b'\n' {
                continue;
            }
            if *byte == b'\n' || *byte == b'\r' {
                self.push_line();
                self.skip_lf = *byte == b'\r';
                if max_lines.is_some_and(|limit| self.lines.len() as f64 >= limit) {
                    return true;
                }
            } else {
                self.pending.push(*byte);
            }
        }
        false
    }
    fn push_line(&mut self) {
        // Node's UTF-8 decoder replaces malformed file bytes at this OS boundary.
        self.lines.push(String::from_utf8_lossy(&self.pending).into_owned());
        self.pending.clear();
    }
    fn finish(mut self) -> Vec<String> {
        if !self.pending.is_empty() {
            self.push_line();
        }
        self.lines
    }
}
