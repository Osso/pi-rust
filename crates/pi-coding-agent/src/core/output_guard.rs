//! Raw stdout writer and takeover flag (`core/output-guard.ts`).
//!
//! Approved deviation (user decision 2026-10-18): the port keeps the writer,
//! backpressure, and the takeover flag, but does not redirect file descriptor 1.
//! Stray stdout writes are prevented statically by `#![deny(clippy::print_stdout)]`
//! at the crate root instead of being intercepted at runtime.

use std::io::Write;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use tokio::sync::{oneshot, watch};

const RAW_STDOUT_RETRY_DELAY: Duration = Duration::from_millis(10);
const RAW_STDOUT_MAX_CHUNK_LENGTH: usize = 1024;

static TAKEN_OVER: AtomicBool = AtomicBool::new(false);

struct WriteJob {
    chunk: String,
    done: Option<oneshot::Sender<()>>,
}

struct RawStdout {
    sender: Sender<WriteJob>,
    pending: watch::Sender<usize>,
}

fn raw_stdout() -> &'static RawStdout {
    static WRITER: OnceLock<RawStdout> = OnceLock::new();
    WRITER.get_or_init(|| {
        let (sender, receiver) = std::sync::mpsc::channel::<WriteJob>();
        let (pending, _pending_rx) = watch::channel(0_usize);
        let writer_pending = pending.clone();
        std::thread::spawn(move || {
            while let Ok(job) = receiver.recv() {
                write_chunk_with_retry(&job.chunk);
                writer_pending.send_modify(|count| *count = count.saturating_sub(1));
                if let Some(done) = job.done {
                    let _ = done.send(());
                }
            }
        });
        RawStdout { sender, pending }
    })
}

pub fn take_over_stdout() {
    TAKEN_OVER.store(true, Ordering::SeqCst);
}

pub fn restore_stdout() {
    TAKEN_OVER.store(false, Ordering::SeqCst);
}

pub fn is_stdout_taken_over() -> bool {
    TAKEN_OVER.load(Ordering::SeqCst)
}

/// Queue `text` for raw stdout delivery, split exactly like the source.
pub fn write_raw_stdout(text: &str) {
    if text.is_empty() {
        return;
    }
    let writer = raw_stdout();
    for chunk in split_raw_stdout_text(text) {
        writer.pending.send_modify(|count| *count += 1);
        let _ = writer.sender.send(WriteJob { chunk, done: None });
    }
}

/// Wait until every queued chunk has been written.
pub async fn wait_for_raw_stdout_backpressure() {
    let mut pending = raw_stdout().pending.subscribe();
    let _ = pending.wait_for(|count| *count == 0).await;
}

/// Wait for the queue, then force a flush.
pub async fn flush_raw_stdout() {
    wait_for_raw_stdout_backpressure().await;
    let (done, finished) = oneshot::channel();
    let writer = raw_stdout();
    writer.pending.send_modify(|count| *count += 1);
    let _ = writer.sender.send(WriteJob {
        chunk: String::new(),
        done: Some(done),
    });
    let _ = finished.await;
}

/// Split on newlines, capping each chunk at 1024 UTF-16 code units like the source.
///
/// Astral characters can be cut mid-surrogate exactly as `String.prototype.slice`
/// does; `from_utf16_lossy` then substitutes U+FFFD, matching Node's encoding.
fn split_raw_stdout_text(text: &str) -> Vec<String> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let newline = u16::from(b'\n');
    let mut chunks = Vec::new();
    let mut offset = 0;
    while offset < units.len() {
        let max_end = (offset + RAW_STDOUT_MAX_CHUNK_LENGTH).min(units.len());
        let end = units[offset..max_end]
            .iter()
            .position(|unit| *unit == newline)
            .map_or(max_end, |position| offset + position + 1);
        chunks.push(String::from_utf16_lossy(&units[offset..end]));
        offset = end;
    }
    chunks
}

/// `ENOBUFS`, `EAGAIN`, and `EWOULDBLOCK` are retried; anything else exits like
/// the source's `process.exit(1)`.
fn write_chunk_with_retry(chunk: &str) {
    let bytes = chunk.as_bytes();
    loop {
        let mut stdout = std::io::stdout().lock();
        let result = stdout.write_all(bytes).and_then(|()| stdout.flush());
        drop(stdout);
        match result {
            Ok(()) => return,
            Err(error) if is_retryable(&error) => std::thread::sleep(RAW_STDOUT_RETRY_DELAY),
            Err(_) => std::process::exit(1),
        }
    }
}

fn is_retryable(error: &std::io::Error) -> bool {
    if error.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }
    matches!(error.raw_os_error(), Some(11 | 105))
}

#[cfg(test)]
mod tests {
    use super::split_raw_stdout_text;

    #[test]
    fn splits_complete_lines_as_separate_chunks() {
        assert_eq!(
            split_raw_stdout_text("one\ntwo\nthree"),
            vec!["one\n".to_owned(), "two\n".to_owned(), "three".to_owned()]
        );
    }

    #[test]
    fn splits_long_lines_at_1024_units() {
        let text = format!("{}\n", "a".repeat(2050));
        let chunks = split_raw_stdout_text(&text);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], "a".repeat(1024));
        assert_eq!(chunks[1], "a".repeat(1024));
        assert_eq!(chunks[2], format!("{}\n", "a".repeat(2)));
    }

    #[test]
    fn prefers_a_newline_inside_the_chunk_window() {
        let text = format!("{}\nrest", "a".repeat(1000));
        assert_eq!(
            split_raw_stdout_text(&text),
            vec![format!("{}\n", "a".repeat(1000)), "rest".to_owned()]
        );
    }

    #[test]
    fn cuts_astral_characters_at_the_utf16_boundary_like_slice() {
        let text = format!("{}\u{1F600}", "a".repeat(1023));
        let chunks = split_raw_stdout_text(&text);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], format!("{}\u{fffd}", "a".repeat(1023)));
        assert_eq!(chunks[1], "\u{fffd}");
    }

    #[test]
    fn empty_text_has_no_chunks() {
        assert!(split_raw_stdout_text("").is_empty());
    }
}
