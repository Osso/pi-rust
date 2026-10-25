//! Frozen seekable transcript spool with bounded off-thread archive decoding.
use super::session_transcript::{
    create_session_transcript_read_stream, read_session_transcript_path, session_transcript_read_error,
};
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use pi_js_compat::node_error::NativeNodeError;
use pi_js_compat::node_fs::{node_file_write_error_record, node_fs_error_record, node_rm_recursive_error_record};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::time::Duration;

const SPOOL_TIMEOUT: Duration = Duration::from_secs(90);

fn spool_worker(source: &JsString, target: &JsString) -> Result<(), Box<NativeNodeError>> {
    let mut input = create_session_transcript_read_stream(source)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options
        .open(encode_node_utf8(target))
        .map_err(|error| Box::new(NativeNodeError::from(node_fs_error_record(&error, "open", target))))?;
    copy_spool_bytes(source, &mut input, &mut output)
}

pub(super) fn copy_spool_bytes(
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

fn spool_archived_transcript(source: &JsString, target: &JsString) -> Result<(), CallbackError> {
    let (sender, receiver) = mpsc::channel();
    let worker_source = source.clone();
    let worker_target = target.clone();
    std::thread::spawn(move || {
        // A timed-out caller has no receiver; its cleanup still owns the spool directory.
        let _ = sender.send(spool_worker(&worker_source, &worker_target));
    });
    match receiver.recv_timeout(SPOOL_TIMEOUT) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(CallbackError::new(pi_js_compat::js_format!(
            "Cannot decompress session transcript {}: {}",
            source,
            error.string_value()
        ))),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(CallbackError::new(
            JsString::from("Timed out decompressing session transcript: ") + source,
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(CallbackError::new(pi_js_compat::js_format!(
            "Cannot decompress session transcript {}: undefined",
            source
        ))),
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
    let result = spool_archived_transcript(&source, &plain).and_then(|()| read(&plain));
    remove_spool_directory(&directory)?;
    result
}

fn remove_spool_directory(directory: &JsString) -> Result<(), CallbackError> {
    let encoded = encode_node_utf8(directory);
    let result = fs::symlink_metadata(&encoded).and_then(|metadata| {
        if metadata.is_dir() {
            fs::remove_dir_all(&encoded)
        } else {
            fs::remove_file(&encoded)
        }
    });
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CallbackError::from(NativeNodeError::from(
            node_rm_recursive_error_record(&error, directory),
        ))),
    }
}
