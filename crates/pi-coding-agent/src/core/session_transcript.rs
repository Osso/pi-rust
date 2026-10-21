//! Frozen `session-transcript.ts`: archive-aware, non-mutating transcript reads.
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub fn is_session_transcript_file(path: &str) -> bool {
    path.ends_with(".jsonl") || path.ends_with(".jsonl.zst")
}

pub fn read_session_transcript_path(path: &str) -> String {
    let archived = format!("{path}.zst");
    if !Path::new(path).exists() && path.ends_with(".jsonl") && Path::new(&archived).exists() {
        archived
    } else {
        path.to_owned()
    }
}

/// Byte stream; callers decode UTF-8 across read boundaries, never per chunk.
pub fn create_session_transcript_read_stream(path: &str) -> io::Result<Box<dyn Read + Send>> {
    let source = read_session_transcript_path(path);
    let file = File::open(&source)?;
    if source.ends_with(".jsonl.zst") {
        Ok(Box::new(zstd::stream::read::Decoder::new(file)?))
    } else {
        Ok(Box::new(file))
    }
}

pub fn read_session_transcript_bytes(path: &str) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    create_session_transcript_read_stream(path)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub fn read_session_transcript_text(path: &str) -> io::Result<String> {
    Ok(String::from_utf8_lossy(&read_session_transcript_bytes(path)?).into_owned())
}
