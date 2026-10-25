//! Frozen search-only session history traversal, including compacted ancestors.
use super::session_manager::{SessionEntry, SessionManagerError};
use super::session_transcript::{create_session_transcript_read_stream, read_session_transcript_path};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};

use pi_js_compat::js_json::{JsString, encode_node_utf8};
const READ_BUFFER_SIZE: usize = 64 * 1024;
fn error(cause: impl std::fmt::Display) -> SessionManagerError {
    SessionManagerError::new(cause.to_string())
}

struct ReverseEntries {
    file: File,
    position: u64,
    pending: Vec<u8>,
    directory: Option<tempfile::TempDir>,
}
impl ReverseEntries {
    fn open(path: &JsString) -> Result<Self, SessionManagerError> {
        let source = read_session_transcript_path(path);
        let (file, directory) = if source.ends_with(".jsonl.zst") {
            let directory = tempfile::Builder::new()
                .prefix("pi-history-")
                .tempdir()
                .map_err(error)?;
            let plain = directory.path().join("transcript.jsonl");
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&plain)
                .map_err(error)?;
            super::session_transcript_spool::copy_spool_bytes(
                &source,
                &mut create_session_transcript_read_stream(&source).map_err(SessionManagerError::from)?,
                &mut output,
            )
            .map_err(SessionManagerError::from)?;
            (File::open(plain).map_err(error)?, Some(directory))
        } else {
            (open_history_file(&source)?, None)
        };
        let position = file.metadata().map_err(error)?.len();
        Ok(Self {
            file,
            position,
            pending: Vec::new(),
            directory,
        })
    }
    fn next_entry(&mut self) -> Result<Option<SessionEntry>, SessionManagerError> {
        loop {
            if let Some(index) = self.pending.iter().rposition(|b| *b == b'\n') {
                let line = self.pending.split_off(index + 1);
                self.pending.pop();
                if let Some(entry) = parse_entry(&line) {
                    return Ok(Some(entry));
                }
                continue;
            }
            if self.position == 0 {
                let line = std::mem::take(&mut self.pending);
                let entry = parse_entry(&line);
                if entry.is_none()
                    && let Some(directory) = self.directory.take()
                {
                    directory.close().map_err(error)?;
                }
                return Ok(entry);
            }
            let size = (READ_BUFFER_SIZE as u64).min(self.position) as usize;
            self.position -= size as u64;
            self.file.seek(SeekFrom::Start(self.position)).map_err(error)?;
            let mut buffer = vec![0; size];
            let read = self.file.read(&mut buffer).map_err(error)?;
            buffer.truncate(read);
            buffer.append(&mut self.pending);
            self.pending = buffer;
        }
    }
}
fn open_history_file(path: &JsString) -> Result<File, SessionManagerError> {
    File::open(encode_node_utf8(path)).map_err(|cause| {
        let record = pi_js_compat::node_fs::node_fs_error_record(&cause, "open", path);
        SessionManagerError::from(Box::new(pi_js_compat::node_error::NativeNodeError::from(record)))
    })
}

fn parse_entry(line: &[u8]) -> Option<SessionEntry> {
    match super::session_manager::parse_session_file_entry(String::from_utf8_lossy(line).as_ref()).ok()? {
        super::session_manager::FileEntry::Header(_) => None,
        super::session_manager::FileEntry::Entry(entry) => Some(entry),
        // TS yields the raw non-record value, but its only consumer skips it: it has no `id`.
        super::session_manager::FileEntry::Json(_) => None,
    }
}

pub struct SessionHistoryReverse {
    branch: std::iter::Rev<std::vec::IntoIter<SessionEntry>>,
    parent: Option<JsString>,
    path: Option<JsString>,
    transcript: Option<ReverseEntries>,
    done: bool,
}
impl Iterator for SessionHistoryReverse {
    type Item = Result<SessionEntry, SessionManagerError>;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(entry) = self.branch.next() {
            return Some(Ok(entry));
        }
        if self.done || self.parent.is_none() || self.path.is_none() {
            self.done = true;
            if let Some(mut transcript) = self.transcript.take()
                && let Some(directory) = transcript.directory.take()
                && let Err(cause) = directory.close()
            {
                return Some(Err(error(cause)));
            }
            return None;
        }
        let result = self.next_ancestor();
        match result {
            Ok(Some(entry)) => Some(Ok(entry)),
            Ok(None) => {
                self.done = true;
                self.transcript = None;
                None
            }
            Err(cause) => {
                self.done = true;
                self.transcript = None;
                Some(Err(cause))
            }
        }
    }
}
impl SessionHistoryReverse {
    fn next_ancestor(&mut self) -> Result<Option<SessionEntry>, SessionManagerError> {
        if self.transcript.is_none() {
            self.transcript = Some(ReverseEntries::open(self.path.as_ref().expect("checked path"))?);
        }
        let transcript = self.transcript.as_mut().expect("opened transcript");
        while let Some(entry) = transcript.next_entry()? {
            if entry.id() != self.parent.as_ref() {
                continue;
            }
            self.parent = entry.parent_id().filter(|id| !id.is_empty()).cloned();
            return Ok(Some(entry));
        }
        Ok(None)
    }
}
/// Lazy newest-first traversal. Never installs historical entries into model context.
pub fn read_session_history_reverse(
    branch: Vec<SessionEntry>,
    session_file: Option<JsString>,
) -> SessionHistoryReverse {
    let parent = branch
        .first()
        .and_then(|e| e.parent_id())
        .filter(|id| !id.is_empty())
        .cloned();
    SessionHistoryReverse {
        branch: branch.into_iter().rev(),
        parent,
        path: session_file.filter(|path| !path.is_empty()),
        transcript: None,
        done: false,
    }
}
