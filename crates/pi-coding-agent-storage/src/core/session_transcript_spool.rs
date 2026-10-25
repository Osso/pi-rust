//! Frozen seekable transcript spool. Native subprocess replaces TS Worker so
//! synchronous callers retain bounded decoding, hard timeout and termination.
use super::session_transcript::{
    create_session_transcript_read_stream, read_session_transcript_path, session_transcript_read_error,
};
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8, parse_lossless, stringify_lossless};
use pi_js_compat::node_error::NativeNodeError;
use pi_js_compat::node_fs::{node_file_write_error_record, node_fs_error_record};
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
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
        _ => Err(Box::new(NativeNodeError::new(
            "Error",
            "Transcript worker requires source and target",
        ))),
    };
    Some(match result {
        Ok(()) => 0,
        Err(cause) => {
            // A lossless string on the private process boundary, like port.postMessage(String(error)).
            let error = cause.string_value();
            eprintln!("{}", stringify_lossless(&JsValue::String(error)));
            1
        }
    })
}
fn spool_worker(source: &str, target: &str) -> Result<(), Box<NativeNodeError>> {
    let source = JsString::from(source);
    let target = JsString::from(target);
    let mut input = create_session_transcript_read_stream(&source)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options
        .open(encode_node_utf8(&target))
        .map_err(|error| Box::new(NativeNodeError::from(node_fs_error_record(&error, "open", &target))))?;
    copy_spool_bytes(&source, &mut input, &mut output)
}
fn copy_spool_bytes(
    source: &JsString,
    input: &mut dyn Read,
    output: &mut impl Write,
) -> Result<(), Box<NativeNodeError>> {
    let mut bytes = [0; 64 * 1024];
    loop {
        let size = input
            .read(&mut bytes)
            .map_err(|error| session_transcript_read_error(source, error))?;
        if size == 0 {
            return Ok(());
        }
        output
            .write_all(&bytes[..size])
            .map_err(|error| Box::new(NativeNodeError::from(node_file_write_error_record(&error))))?;
    }
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
fn spool_archived_transcript(source: &JsString, target: &Path, errors: &Path) -> Result<(), CallbackError> {
    let executable = crate::utils::paths::node_join(
        crate::config::get_package_dir()?,
        if cfg!(windows) { "pi.exe" } else { "pi" },
    );
    let stderr = OpenOptions::new().write(true).create_new(true).open(errors)?;
    let child = Command::new(encode_node_utf8(&executable))
        .arg(SESSION_TRANSCRIPT_WORKER_MODE)
        .arg(encode_node_utf8(source))
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(stderr)
        .spawn()
        .map_err(|error| {
            CallbackError::new(pi_js_compat::js_format!(
                "Cannot start transcript worker {}: {}",
                executable,
                error.to_string()
            ))
        })?;
    let mut worker = Worker(child);
    let failure_prefix = JsString::from("Cannot decompress session transcript ") + source + ": ";
    match wait_worker(&mut worker.0, SPOOL_TIMEOUT) {
        Ok(true) => Ok(()),
        Ok(false) => {
            let wire = fs::read_to_string(errors)?;
            let error = parse_lossless(wire.trim())
                .map_err(|error| CallbackError::new(format!("Invalid transcript worker error: {error}")))?;
            let JsValue::String(error) = error else {
                return Err(CallbackError::new("Invalid transcript worker error: expected a string"));
            };
            Err(CallbackError::new(failure_prefix + error))
        }
        Err(cause) if cause.kind() == io::ErrorKind::TimedOut => Err(CallbackError::new(
            JsString::from("Timed out decompressing session transcript: ") + source,
        )),
        Err(cause) => Err(CallbackError::new(failure_prefix + cause.to_string())),
    }
}

pub fn with_seekable_session_transcript<T>(
    path: &JsString,
    read: impl FnOnce(&JsString) -> Result<T, CallbackError>,
) -> Result<T, CallbackError> {
    let source = read_session_transcript_path(path);
    if !source.ends_with(".jsonl.zst") {
        return read(&source);
    }
    let prefix = crate::utils::paths::node_join(pi_js_compat::node_os::tmpdir(), "pi-transcript-");
    let directory = pi_js_compat::node_fs::mkdtemp(&prefix).map_err(|error| CallbackError::from(*error))?;
    let plain = crate::utils::paths::node_join(directory.clone(), "transcript.jsonl");
    let errors = crate::utils::paths::node_join(directory.clone(), "worker-error");
    let result = spool_archived_transcript(
        &source,
        Path::new(&encode_node_utf8(&plain)),
        Path::new(&encode_node_utf8(&errors)),
    )
    .and_then(|()| read(&plain));
    match fs::remove_dir_all(encode_node_utf8(&directory)) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CallbackError::from(NativeNodeError::from(node_fs_error_record(
                &error, "rmdir", &directory,
            ))));
        }
    }
    result
}
