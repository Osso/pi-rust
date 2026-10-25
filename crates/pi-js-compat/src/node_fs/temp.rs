//! Node mkdtemp: an exclusive private directory, with the original error template.
use super::{node_fs_error_record, node_path_argument_error};
use crate::js_json::{JsString, encode_node_utf8};
use crate::node_error::NativeNodeError;
use std::fs::DirBuilder;
use std::io;

const SUFFIX_ALPHABET: &[u8; 62] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const SUFFIX_SIZE: usize = 6;
const MAX_COLLISION_ATTEMPTS: usize = 256;
const UNBIASED_RANDOM_LIMIT: u8 = 248; // Four complete62-character alphabet ranges.

/// Append six random alphanumeric characters and create the directory with mode0700.
/// Failures carry Node's unexpanded prefix+XXXXXX template, not the random candidate.
pub fn mkdtemp(prefix: &JsString) -> Result<JsString, Box<NativeNodeError>> {
    if node_path_argument_error(prefix).is_some() {
        return Err(Box::new(NativeNodeError::invalid_argument_value(
            "prefix",
            "must be a string, Uint8Array, or URL without null bytes",
            prefix,
        )));
    }
    let template = prefix.clone() + "XXXXXX";
    for _ in 0..MAX_COLLISION_ATTEMPTS {
        let suffix = read_random_suffix().map_err(|error| mkdtemp_error(error, &template))?;
        let path = prefix.clone() + suffix;
        match create_private_directory(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(mkdtemp_error(error, &template)),
        }
    }
    Err(mkdtemp_error(
        io::Error::from_raw_os_error(rustix::io::Errno::EXIST.raw_os_error()),
        &template,
    ))
}

fn mkdtemp_error(error: io::Error, template: &JsString) -> Box<NativeNodeError> {
    Box::new(node_fs_error_record(&error, "mkdtemp", template).into())
}

fn create_private_directory(path: &JsString) -> io::Result<()> {
    let mut builder = DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(encode_node_utf8(path))
}

fn read_random_suffix() -> io::Result<String> {
    let mut suffix = String::with_capacity(SUFFIX_SIZE);
    while suffix.len() < SUFFIX_SIZE {
        let mut bytes = [0; SUFFIX_SIZE];
        let size = rustix::rand::getrandom(&mut bytes[..], rustix::rand::GetRandomFlags::empty())?;
        for byte in &bytes[..size] {
            // Reject the incomplete alphabet range so all62 symbols are equiprobable.
            if *byte < UNBIASED_RANDOM_LIMIT && suffix.len() < SUFFIX_SIZE {
                suffix.push(char::from(SUFFIX_ALPHABET[usize::from(*byte) % SUFFIX_ALPHABET.len()]));
            }
        }
    }
    Ok(suffix)
}
