//! Shared Node readline byte boundary: CR, LF, CRLF and lossy UTF-8 decoding.

#[derive(Default)]
pub struct LineBuffer {
    lines: Vec<String>,
    pending: Vec<u8>,
    skip_lf: bool,
}

impl LineBuffer {
    /// Append bytes until the configured line limit is reached, or consume the chunk.
    pub fn feed(&mut self, bytes: &[u8], max_lines: Option<f64>) -> bool {
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
        self.lines.push(String::from_utf8_lossy(&self.pending).into_owned());
        self.pending.clear();
    }

    /// Take completed lines without discarding a partial UTF-8 line or CRLF state.
    pub fn take_completed_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.lines)
    }

    /// Finish with a final nonempty unterminated line.
    pub fn finish(mut self) -> Vec<String> {
        if !self.pending.is_empty() {
            self.push_line();
        }
        self.lines
    }
}
