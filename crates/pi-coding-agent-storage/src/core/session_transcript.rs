//! Frozen `session-transcript.ts`: archive-aware, non-mutating transcript reads.
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;

use pi_js_compat::js_json::{JsString, encode_node_utf8};

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
pub fn create_session_transcript_read_stream(path: impl Into<JsString>) -> io::Result<Box<dyn Read + Send>> {
    let source = read_session_transcript_path(path);
    let file = File::open(encode_node_utf8(&source))?;
    if source.ends_with(".jsonl.zst") {
        Ok(Box::new(SessionZstdDecoder::new(file)?))
    } else {
        Ok(Box::new(file))
    }
}

pub fn read_session_transcript_bytes(path: impl Into<JsString>) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    create_session_transcript_read_stream(path)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// `Buffer.toString("utf8")` decodes file bytes; that result is always scalar text.
pub fn read_session_transcript_text(path: impl Into<JsString>) -> io::Result<String> {
    Ok(String::from_utf8_lossy(&read_session_transcript_bytes(path)?).into_owned())
}

/// Node continues recognized concatenated frames, but ignores non-frame bytes
/// following a complete frame. Never suppress an error inside a frame.
struct SessionZstdDecoder<R: Read> {
    decoder: Option<zstd::stream::read::Decoder<'static, BufReader<R>>>,
}
impl<R: Read> SessionZstdDecoder<R> {
    fn new(reader: R) -> io::Result<Self> {
        Ok(Self {
            decoder: Some(zstd::stream::read::Decoder::new(reader)?.single_frame()),
        })
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
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            let Some(decoder) = self.decoder.as_mut() else {
                return Ok(0);
            };
            let count = decoder.read(output)?;
            if count > 0 {
                return Ok(count);
            }
            let mut reader = self.decoder.take().expect("decoder was present").finish();
            if !starts_zstd_frame(reader.fill_buf()?) {
                return Ok(0);
            }
            self.decoder = Some(zstd::stream::read::Decoder::with_buffer(reader)?.single_frame());
        }
    }
}
pub(crate) fn decode_session_zstd_bytes(bytes: &[u8]) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    SessionZstdDecoder::new(bytes)?.read_to_end(&mut output)?;
    Ok(output)
}
