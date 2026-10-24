//! Node readline file boundary: stream until the requested number of lines, not EOF.
use super::{abort_result, aborted_io_error, file_error};
use crate::harness::types::{FileError, ReadTextLinesOptions};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::node_readline::LineBuffer;
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
            return Ok(decoded.finish());
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
