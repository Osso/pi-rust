//! Frozen `session-transcript.ts`: archive-aware, non-mutating transcript reads.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;

use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::node_error::NativeNodeError;
use pi_js_compat::node_fs::{node_fs_error_record, node_path_argument_error};
mod errors;
use errors::ZstdReadFailure;

pub fn is_session_transcript_file(path: impl Into<JsString>) -> bool {
    let path = path.into();
    path.ends_with(".jsonl") || path.ends_with(".jsonl.zst")
}

/// Pure string selection; only the two existence probes cross the filesystem binding.
pub fn read_session_transcript_path(path: impl Into<JsString>) -> JsString {
    let path = path.into();
    let archived = path.clone() + ".zst";
    let plain_exists = path_exists(&path);
    if !plain_exists && path.ends_with(".jsonl") && path_exists(&archived) {
        archived
    } else {
        path
    }
}

fn path_exists(path: &JsString) -> bool {
    Path::new(&encode_node_utf8(path)).exists()
}

/// Byte stream; callers decode UTF-8 across read boundaries, never per chunk.
pub fn create_session_transcript_read_stream(
    path: impl Into<JsString>,
) -> Result<Box<dyn Read + Send>, Box<NativeNodeError>> {
    let source = read_session_transcript_path(path);
    if let Some(error) = node_path_argument_error(&source) {
        return Err(Box::new(error.into()));
    }
    let file = File::open(encode_node_utf8(&source))
        .map_err(|error| Box::new(node_fs_error_record(&error, "open", &source).into()))?;
    if source.ends_with(".jsonl.zst") {
        let decoder = SessionZstdDecoder::new(file).map_err(|error| session_transcript_read_error(&source, error))?;
        Ok(Box::new(decoder))
    } else {
        Ok(Box::new(file))
    }
}

/// Convert the required std::io::Read boundary once, retaining Node error data.
pub fn session_transcript_read_error(path: &JsString, error: io::Error) -> Box<NativeNodeError> {
    if let Some(failure) = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<ZstdReadFailure>())
    {
        return Box::new(failure.native_error());
    }
    Box::new(node_fs_error_record(&error, "read", path).into())
}

pub fn read_session_transcript_bytes(path: impl Into<JsString>) -> Result<Vec<u8>, Box<NativeNodeError>> {
    let path = path.into();
    let mut bytes = Vec::new();
    create_session_transcript_read_stream(&path)?
        .read_to_end(&mut bytes)
        .map_err(|error| session_transcript_read_error(&path, error))?;
    Ok(bytes)
}

/// `Buffer.toString("utf8")` decodes file bytes; that result is always scalar text.
pub fn read_session_transcript_text(path: impl Into<JsString>) -> Result<String, Box<NativeNodeError>> {
    Ok(String::from_utf8_lossy(&read_session_transcript_bytes(path)?).into_owned())
}

/// Node continues recognized concatenated frames, but ignores non-frame bytes
/// following a complete frame. Never suppress an error inside a frame.
struct SessionZstdDecoder<R: Read> {
    context: zstd::zstd_safe::DCtx<'static>,
    reader: BufReader<R>,
    frame_complete: bool,
    finished: bool,
}
impl<R: Read> SessionZstdDecoder<R> {
    fn new(reader: R) -> io::Result<Self> {
        let context = zstd::zstd_safe::DCtx::try_create()
            .ok_or_else(|| ZstdReadFailure::Library(0usize.wrapping_sub(64)).into_io())?;
        Ok(Self {
            context,
            reader: BufReader::new(reader),
            frame_complete: false,
            finished: false,
        })
    }

    fn decode_chunk(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let bytes = self.reader.fill_buf()?;
        let mut input = zstd::zstd_safe::InBuffer::around(bytes);
        let mut decoded = zstd::zstd_safe::OutBuffer::around(output);
        let remaining = self
            .context
            .decompress_stream(&mut decoded, &mut input)
            .map_err(|code| ZstdReadFailure::Library(code).into_io())?;
        self.frame_complete = remaining == 0;
        let consumed = input.pos();
        let count = decoded.pos();
        if bytes.is_empty() && count == 0 && !self.frame_complete {
            return Err(ZstdReadFailure::Incomplete.into_io());
        }
        self.reader.consume(consumed);
        Ok(count)
    }
}
fn starts_zstd_frame(bytes: &[u8]) -> bool {
    const MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
    let size = bytes.len().min(4);
    if size == 0 {
        return false;
    }
    if MAGIC.starts_with(&bytes[..size]) {
        return true;
    }
    (0x50..=0x5f).contains(&bytes[0]) && [0x2a, 0x4d, 0x18].starts_with(&bytes[1..size])
}
impl<R: Read> Read for SessionZstdDecoder<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() || self.finished {
            return Ok(0);
        }
        loop {
            if self.frame_complete && !starts_zstd_frame(self.reader.fill_buf()?) {
                self.finished = true;
                return Ok(0);
            }
            let count = self.decode_chunk(output)?;
            if count > 0 {
                return Ok(count);
            }
        }
    }
}
pub(crate) fn decode_session_zstd_bytes(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    SessionZstdDecoder::new(bytes)?.read_to_end(&mut output)?;
    Ok(output)
}
