//! Frozen seekable transcript spool. Native subprocess replaces TS Worker so
//! synchronous callers retain bounded decoding, hard timeout and termination.
use super::session_manager::SessionManagerError;
use super::session_transcript::{create_session_transcript_read_stream, read_session_transcript_path};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const SESSION_TRANSCRIPT_WORKER_MODE: &str = "--internal-session-transcript-spool";
const SPOOL_TIMEOUT: Duration = Duration::from_secs(90);

/// Early hidden CLI dispatch, before provider/config initialization.
pub fn run_internal_session_transcript_worker(args: &[String]) -> Option<i32> {
    if args.first().map(String::as_str) != Some(SESSION_TRANSCRIPT_WORKER_MODE) {
        return None;
    }
    let result = match args.get(1..3) {
        Some(paths) if args.len() == 3 => spool_worker(&paths[0], &paths[1]),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Transcript worker requires source and target",
        )),
    };
    Some(match result {
        Ok(()) => 0,
        Err(cause) => {
            eprintln!("{cause}");
            1
        }
    })
}
fn spool_worker(source: &str, target: &str) -> io::Result<()> {
    let mut input = create_session_transcript_read_stream(source)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options.open(target)?;
    io::copy(&mut input, &mut output)?;
    output.flush()
}

struct Worker(Child);
impl Drop for Worker {
    fn drop(&mut self) {
        // Kill before reaping even on early error or timeout. No worker can keep
        // the temporary transcript alive after the caller has cleaned it up.
        match self.0.try_wait() {
            Ok(Some(_)) => {}
            _ => {
                if let Err(cause) = self.0.kill() {
                    eprintln!("Cannot terminate transcript worker: {cause}");
                }
            }
        }
        if let Err(cause) = self.0.wait() {
            eprintln!("Cannot reap transcript worker: {cause}");
        }
    }
}
fn wait_worker(child: &mut Child, timeout: Duration) -> io::Result<bool> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.success());
        }
        if start.elapsed() >= timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "transcript decompression timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn spool_archived_transcript(source: &str, target: &Path, errors: &Path) -> Result<(), SessionManagerError> {
    let executable = crate::config::get_package_dir().join(if cfg!(windows) { "pi.exe" } else { "pi" });
    let stderr = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(errors)
        .map_err(|e| SessionManagerError::new(e.to_string()))?;
    let child = Command::new(&executable)
        .arg(SESSION_TRANSCRIPT_WORKER_MODE)
        .arg(source)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .spawn()
        .map_err(|e| {
            SessionManagerError::new(format!("Cannot start transcript worker {}: {e}", executable.display()))
        })?;
    let mut worker = Worker(child);
    match wait_worker(&mut worker.0, SPOOL_TIMEOUT) {
        Ok(true) => Ok(()),
        Ok(false) => Err(SessionManagerError::new(format!(
            "Cannot decompress session transcript {source}: {}",
            fs::read_to_string(errors).map_err(|e| SessionManagerError::new(e.to_string()))?
        ))),
        Err(cause) if cause.kind() == io::ErrorKind::TimedOut => Err(SessionManagerError::new(format!(
            "Timed out decompressing session transcript: {source}"
        ))),
        Err(cause) => Err(SessionManagerError::new(format!(
            "Cannot decompress session transcript {source}: {cause}"
        ))),
    }
}

pub fn with_seekable_session_transcript<T>(
    path: &str,
    read: impl FnOnce(&str) -> Result<T, SessionManagerError>,
) -> Result<T, SessionManagerError> {
    let source = read_session_transcript_path(path);
    if !source.ends_with(".jsonl.zst") {
        return read(&source);
    }
    let directory = tempfile::Builder::new()
        .prefix("pi-transcript-")
        .tempdir()
        .map_err(|e| SessionManagerError::new(e.to_string()))?;
    let plain = directory.path().join("transcript.jsonl");
    let result = spool_archived_transcript(&source, &plain, &directory.path().join("worker-error"))
        .and_then(|()| read(&plain.to_string_lossy()));
    // Unlike TempDir's silent Drop, surface cleanup failure just as TS finally.
    directory.close().map_err(|e| SessionManagerError::new(e.to_string()))?;
    result
}
