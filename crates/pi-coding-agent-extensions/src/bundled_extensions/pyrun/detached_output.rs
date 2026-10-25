//! Port of `extensions/pyrun/src/detached-output.ts`.

use std::collections::VecDeque;
use std::io::Write;

use pi_js_compat::{
    js_json as json,
    js_json::{JsString, JsValue, encode_node_utf8},
};

use super::runner::CanonicalPyrunProgressUpdate;

const OUTPUT_BYTE_LIMIT: usize = 50 * 1024 * 1024;
const HEAD_BYTE_LIMIT: usize = OUTPUT_BYTE_LIMIT / 2;
const TRUNCATION_MARKER_BYTE_RESERVE: usize = 256;

/// Append-only head while running; the bounded tail is committed on settlement.
pub struct PyrunArtifactWriter {
    path: JsString,
    head_bytes: usize,
    buffering: bool,
    tail: VecDeque<Vec<u8>>,
    tail_bytes: usize,
    omitted_bytes: usize,
    omitted_records: usize,
}

impl PyrunArtifactWriter {
    pub fn new(path: JsString) -> Self {
        Self {
            path,
            head_bytes: 0,
            buffering: false,
            tail: VecDeque::new(),
            tail_bytes: 0,
            omitted_bytes: 0,
            omitted_records: 0,
        }
    }

    pub fn append_progress(&mut self, update: &CanonicalPyrunProgressUpdate) -> Result<(), JsString> {
        let bytes = encode_record(&json!({"kind":"progress","update":update.raw}));
        if !self.buffering && self.head_bytes + bytes.len() <= HEAD_BYTE_LIMIT {
            self.append_bytes(&bytes)?;
            self.head_bytes += bytes.len();
            return Ok(());
        }
        self.buffering = true;
        self.tail_bytes += bytes.len();
        self.tail.push_back(bytes);
        self.drop_oldest_tail_records(OUTPUT_BYTE_LIMIT - self.head_bytes);
        Ok(())
    }

    pub fn finish(&mut self, record: &JsValue) -> Result<(), JsString> {
        let terminal = encode_record(record);
        let remaining = OUTPUT_BYTE_LIMIT as i128 - self.head_bytes as i128 - terminal.len() as i128;
        if self.omitted_records > 0 || self.tail_bytes as i128 > remaining {
            self.drop_oldest_tail_records((remaining - TRUNCATION_MARKER_BYTE_RESERVE as i128).max(0) as usize);
        }
        if self.omitted_records > 0 {
            let marker = json!({"kind":"truncation","message":"Middle progress records omitted from capped Pyrun output.","omittedBytes":self.omitted_bytes,"omittedRecords":self.omitted_records});
            self.append_bytes(&encode_record(&marker))?;
        }
        for bytes in &self.tail {
            self.append_bytes(bytes)?;
        }
        self.tail.clear();
        self.tail_bytes = 0;
        self.append_bytes(&terminal)
    }

    fn drop_oldest_tail_records(&mut self, byte_limit: usize) {
        while self.tail_bytes > byte_limit {
            let Some(bytes) = self.tail.pop_front() else {
                break;
            };
            self.tail_bytes -= bytes.len();
            self.omitted_bytes += bytes.len();
            self.omitted_records += 1;
        }
    }

    fn append_bytes(&self, bytes: &[u8]) -> Result<(), JsString> {
        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(encode_node_utf8(&self.path))
            .map_err(|error| JsString::from(error.to_string()))?;
        file.write_all(bytes).map_err(|error| JsString::from(error.to_string()))
    }
}

fn encode_record(record: &JsValue) -> Vec<u8> {
    let mut bytes = json::stringify_lossless(record).into_bytes();
    bytes.push(b'\n');
    bytes
}
