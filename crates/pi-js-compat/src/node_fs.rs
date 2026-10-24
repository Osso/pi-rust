//! Node filesystem IO boundaries, error messages and metadata shared by callers.
//!
//! Raw errno names reuse the existing platform table; unproven descriptions retain
//! the previous std display text. This is not a complete libuv strerror port.

use rustix::io::Errno;
use std::io;

use crate::js_format;
use crate::js_json::{JsString, encode_node_utf8};
use crate::node_error::NativeNodeError;

/// Node fs.promises.realpath: validate before IO, retain the original native error.
pub async fn realpath(path: &JsString) -> Result<JsString, Box<NativeNodeError>> {
    let encoded = encode_realpath_argument(path)?;
    tokio::fs::canonicalize(encoded)
        .await
        .map(|path| decode_node_path(path.as_os_str()))
        .map_err(|error| Box::new(node_fs_error_record(&error, "realpath", path).into()))
}

/// Node realpathSync.native, not the default JavaScript realpathSync lstat walk.
/// Use for synchronous callers that consume the native binding or catch lookup failures.
pub fn realpath_sync_native(path: &JsString) -> Result<JsString, Box<NativeNodeError>> {
    let encoded = encode_realpath_argument(path)?;
    std::fs::canonicalize(encoded)
        .map(|path| decode_node_path(path.as_os_str()))
        .map_err(|error| Box::new(node_fs_error_record(&error, "realpath", path).into()))
}

fn encode_realpath_argument(path: &JsString) -> Result<String, Box<NativeNodeError>> {
    if let Some(error) = node_path_argument_error(path) {
        return Err(Box::new(error.into()));
    }
    Ok(encode_node_utf8(path))
}

/// Decode a POSIX OS path at Node's UTF-8 byte-ingress boundary.
/// Invalid sequences become U+FFFD; native JS strings must not use this decoder.
pub fn decode_node_path(path: &std::ffi::OsStr) -> JsString {
    JsString::from(String::from_utf8_lossy(path.as_encoded_bytes()).as_ref())
}

/// Node's path argument validation error, without invented IO syscall/errno fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePathArgumentError {
    pub name: &'static str,
    pub code: &'static str,
    pub message: JsString,
}

/// Validate a native string path before entering a Node filesystem operation.
/// ERR_INVALID_ARG_VALUE inspects the value, then truncates at 128 UTF-16 units.
pub fn node_path_argument_error(path: &JsString) -> Option<NodePathArgumentError> {
    let error = crate::node_child_process::validate_string_argument(
        "path",
        "must be a string, Uint8Array, or URL without null bytes",
        path,
    )
    .err()?;
    Some(NodePathArgumentError {
        name: "TypeError",
        code: "ERR_INVALID_ARG_VALUE",
        message: error.message,
    })
}

/// Shared ERR_INVALID_ARG_VALUE inspection/truncation for Node IO boundaries.
pub fn invalid_argument_value_message(name: &str, reason: &str, value: &JsString) -> JsString {
    let inspected = inspect_argument_value(value);
    let mut received: Vec<u16> = inspected.units().iter().copied().take(128).collect();
    if inspected.len() > 128 {
        received.extend("...".encode_utf16());
    }
    let kind = if name.contains('.') { "property" } else { "argument" };
    let mut message = js_format!("The {} '{}' {}. Received ", kind, name, reason);
    message.push_str(JsString::from_utf16(received));
    message
}

fn inspect_argument_value(argument: &JsString) -> JsString {
    // util.inspect's default maxStringLength is 10000 UTF-16 units; only its
    // first 128 inspected units can reach Node's path-validation diagnostic.
    let value = argument.slice(0, 10000);
    if value.len() > 76 {
        JsString::join(value.split_inclusive('\n').map(|part| inspect_string(&part)), " +\n  ")
    } else {
        inspect_string(&value)
    }
}

/// Node Error name, message and own filesystem metadata (stack is caller-owned).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeFsError {
    pub name: &'static str,
    pub errno: Option<i32>,
    pub code: String,
    pub syscall: JsString,
    pub path: Option<JsString>,
    pub message: JsString,
}

impl NodeFsError {
    /// Node's uncaught-error line and own properties, excluding JS stack frames.
    /// `spawnargs` is present only for a `spawnSync` failure.
    pub fn console_message(&self, spawnargs: Option<&[JsString]>) -> JsString {
        let mut output = js_format!("{}: {}", self.name, self.message) + " {\n";
        if let Some(errno) = self.errno {
            output.push_str(js_format!("  errno: {},\n", errno));
        }
        output.push_str(js_format!(
            "  code: {},\n  syscall: {}",
            inspect_string(&self.code.as_str().into()),
            inspect_string(&self.syscall)
        ));
        if let Some(path) = &self.path {
            output.push_str(js_format!(",\n  path: {}", inspect_string(path)));
        }
        if let Some(args) = spawnargs {
            output.push_str(js_format!(",\n  spawnargs: {}", inspect_spawnargs(args)));
        }
        output.push_str("\n}");
        output
    }
}

// Node util.inspect escapes unpaired units, but keeps valid surrogate pairs.
fn inspect_string(value: &JsString) -> JsString {
    let quote = if !value.contains('\'') {
        b'\''
    } else if !value.contains('"') {
        b'"'
    } else if !value.contains('`') && !value.contains("${") {
        b'`'
    } else {
        b'\''
    };
    let mut output = vec![u16::from(quote)];
    let mut units = value.units().iter().copied().peekable();
    while let Some(unit) = units.next() {
        if (0xd800..=0xdbff).contains(&unit) && units.peek().is_some_and(|next| (0xdc00..=0xdfff).contains(next)) {
            output.push(unit);
            output.push(units.next().expect("peeked low surrogate"));
        } else if (0xd800..=0xdfff).contains(&unit) {
            output.extend(format!("\\u{unit:04x}").encode_utf16());
        } else if let Some(escape) = inspect_unit_escape(unit, quote) {
            output.extend(escape.encode_utf16());
        } else {
            output.push(unit);
        }
    }
    output.push(u16::from(quote));
    JsString::from_utf16(output)
}

fn inspect_unit_escape(unit: u16, quote: u8) -> Option<String> {
    Some(match unit {
        92 => "\\\\".into(),
        10 => "\\n".into(),
        13 => "\\r".into(),
        9 => "\\t".into(),
        8 => "\\b".into(),
        12 => "\\f".into(),
        11 => "\\x0B".into(),
        unit if unit == u16::from(quote) => format!("\\{}", char::from(quote)),
        0..=31 | 127..=159 => format!("\\x{unit:02X}"),
        _ => return None,
    })
}

// The error is at depth zero: spawnargs has indentation 2, breakLength 80,
// compact 3 and maxArrayLength 100 in Node's default uncaught-error inspector.
fn inspect_spawnargs(args: &[JsString]) -> JsString {
    let values: Vec<_> = args.iter().cloned().map(crate::js_json::JsValue::String).collect();
    inspect_console_array(&values, 2, 1, "spawnargs".len())
}

// Port of util.inspect's compact string-array layout, with the fixed defaults
// used above. Padding is right-aligned only for numbers, never spawn strings.
fn group_inspect_entries(
    entries: &[JsString],
    has_remaining: bool,
    indentation: usize,
    numbers: bool,
) -> Vec<JsString> {
    let widths: Vec<usize> = entries.iter().map(inspect_entry_width).collect();
    let maximum = widths.iter().copied().max().unwrap_or(0);
    let actual_maximum = maximum + 2;
    let total: usize = widths.iter().map(|width| width + 2).sum();
    if entries.len() <= 6 || actual_maximum * 3 + indentation >= 80 {
        return entries.to_vec();
    }
    if total as f64 / actual_maximum as f64 <= 5.0 && maximum > 6 {
        return entries.to_vec();
    }
    let output_length = entries.len() + usize::from(has_remaining);
    let average_bias = (actual_maximum as f64 - total as f64 / output_length as f64).sqrt();
    let biased_maximum = (actual_maximum as f64 - 3.0 - average_bias).max(1.0);
    let columns = ((2.5 * biased_maximum * entries.len() as f64).sqrt() / biased_maximum).round() as usize;
    let columns = columns
        .min(80_usize.saturating_sub(indentation) / actual_maximum)
        .min(12);
    if columns <= 1 {
        return entries.to_vec();
    }
    let column_widths: Vec<usize> = (0..columns)
        .map(|column| widths.iter().skip(column).step_by(columns).max().unwrap() + 2)
        .collect();
    entries
        .chunks(columns)
        .map(|row| inspect_row(row, &column_widths, numbers))
        .collect()
}

fn inspect_row(row: &[JsString], widths: &[usize], numbers: bool) -> JsString {
    JsString::join(
        row.iter().enumerate().map(|(column, entry)| {
            let padding = widths[column] - inspect_entry_width(entry) - 2;
            let padded = if numbers {
                js_format!("{}{}", " ".repeat(padding), entry)
            } else {
                entry.clone()
            };
            if column + 1 == row.len() {
                padded
            } else if numbers {
                padded + ", "
            } else {
                js_format!("{}, {}", entry, " ".repeat(padding))
            }
        }),
        "",
    )
}

fn inspect_entry_width(entry: &JsString) -> usize {
    char::decode_utf16(entry.units().iter().copied()).count()
}

/// Default, non-colored util.inspect for JSON-shaped SDK console arguments.
/// Shares quoting and compact-array layout with Node IO/argument diagnostics.
pub fn inspect_console_value(value: &crate::js_json::JsValue) -> JsString {
    inspect_console_value_at(value, 0, 0)
}

fn inspect_console_value_at(value: &crate::js_json::JsValue, indentation: usize, depth: usize) -> JsString {
    use crate::js_json::JsValue;
    match value {
        JsValue::Null => "null".into(),
        JsValue::Bool(value) => value.to_string().into(),
        JsValue::Number(value) if *value == 0.0 && value.is_sign_negative() => "-0".into(),
        JsValue::Number(value) => crate::number::js_number_to_string(*value).into(),
        JsValue::String(value) => inspect_console_string(value),
        JsValue::Array(values) => inspect_console_array(values, indentation, depth, 0),
        JsValue::Object(values) => inspect_console_object(values, indentation, depth),
    }
}

fn inspect_console_string(value: &JsString) -> JsString {
    let inspected = inspect_string(&value.slice(0, 10000));
    if value.len() <= 10000 {
        return inspected;
    }
    let remaining = value.len() - 10000;
    let suffix = if remaining == 1 { "character" } else { "characters" };
    js_format!("{}... {} more {}", inspected, remaining, suffix)
}

fn inspect_console_array(
    values: &[crate::js_json::JsValue],
    indentation: usize,
    depth: usize,
    key_width: usize,
) -> JsString {
    use crate::js_json::JsValue;
    if values.is_empty() {
        return "[]".into();
    }
    if depth > 2 {
        return "[Array]".into();
    }
    let mut entries: Vec<_> = values
        .iter()
        .take(100)
        .map(|value| inspect_console_value_at(value, indentation + 2, depth + 1))
        .collect();
    let primitive = values
        .iter()
        .take(100)
        .all(|value| !matches!(value, JsValue::Object(_) | JsValue::Array(_)));
    let numbers = values.iter().take(100).all(|value| matches!(value, JsValue::Number(_)));
    let mut grouped = if primitive {
        group_inspect_entries(&entries, values.len() > 100, indentation, numbers)
    } else {
        entries.clone()
    };
    let grouped_rows = grouped.len() != entries.len();
    if values.len() > 100 {
        let remaining = values.len() - 100;
        let suffix = if remaining == 1 { "item" } else { "items" };
        let tail = js_format!("... {} more {}", remaining, suffix);
        grouped.push(tail.clone());
        entries.push(tail);
    }
    let inline = can_inspect_inline(&entries, indentation + key_width, grouped_rows);
    inspect_console_container(("[", "]"), &grouped, indentation, inline)
}

fn inspect_console_object(values: &crate::js_json::JsObject, indentation: usize, depth: usize) -> JsString {
    if values.is_empty() {
        return "{}".into();
    }
    if depth > 2 {
        return "[Object]".into();
    }
    let entries: Vec<_> = values
        .iter()
        .map(|(key, value)| {
            let key = if is_inspect_identifier(key) {
                key.clone()
            } else {
                inspect_string(key)
            };
            js_format!(
                "{}: {}",
                key,
                inspect_console_value_at(value, indentation + 2, depth + 1)
            )
        })
        .collect();
    let inline = can_inspect_inline(&entries, indentation, false);
    inspect_console_container(("{", "}"), &entries, indentation, inline)
}

fn is_inspect_identifier(value: &JsString) -> bool {
    value
        .units()
        .iter()
        .enumerate()
        .all(|(index, unit)| matches!(*unit, 95 | 65..=90 | 97..=122) || (index > 0 && matches!(*unit, 48..=57)))
        && !value.is_empty()
}

fn can_inspect_inline(entries: &[JsString], indentation: usize, grouped: bool) -> bool {
    let width = entries.iter().map(JsString::len).sum::<usize>() + entries.len() * 2 + 2 + indentation;
    !grouped && width <= 80 && entries.iter().all(|entry| !entry.contains('\n'))
}

fn inspect_console_container(braces: (&str, &str), rows: &[JsString], indentation: usize, inline: bool) -> JsString {
    let (open, close) = braces;
    if inline {
        return js_format!("{} {} {}", open, JsString::join(rows, ", "), close);
    }
    let inner_indent = " ".repeat(indentation + 2);
    js_format!(
        "{}\n{}{}\n{}{}",
        open,
        inner_indent,
        JsString::join(rows, &format!(",\n{inner_indent}")),
        " ".repeat(indentation),
        close
    )
}

/// Originating whole-file read API: Node gives these different EISDIR shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeReadApi {
    ReadFileSync,
    PromisesReadFile,
}

/// Construct a Node filesystem error without touching the filesystem.
///
/// Pass the failing Node syscall, e.g. `open`, `read`, `mkdir`, or `scandir`.
/// Raw EISDIR from `read` and `uv_cwd` errors have no path. Deleted-CWD ENOENT
/// retains Node's contextual message. ErrorKind-only errors have no invented
/// errno. Negative errno parity is bounded to Unix.
pub fn node_fs_error_record(error: &io::Error, syscall: &str, path: &JsString) -> NodeFsError {
    format_fs_error_record(error, syscall, Some(path), NodeReadApi::ReadFileSync)
}

fn node_error_description(error: &io::Error, code: &str) -> String {
    if let Some(errno) = error.raw_os_error() {
        return node_errno_description(errno);
    }
    match error.kind() {
        io::ErrorKind::NotFound
        | io::ErrorKind::PermissionDenied
        | io::ErrorKind::IsADirectory
        | io::ErrorKind::NotADirectory
        | io::ErrorKind::AlreadyExists
        | io::ErrorKind::InvalidInput
        | io::ErrorKind::DirectoryNotEmpty => LIBUV_ERRORS
            .iter()
            .find_map(|(_, name, description)| (*name == code).then_some(*description))
            .expect("mapped ErrorKind has a libuv description")
            .to_owned(),
        _ => error.to_string(),
    }
}

fn format_fs_error_record(error: &io::Error, syscall: &str, path: Option<&JsString>, api: NodeReadApi) -> NodeFsError {
    // Node's fs binding reports its UTF-8 path back after the OS boundary.
    // Other origins (spawn, sockets, argument validation) must not use this repair.
    let pathless = path.is_none();
    let path = path
        .map(|path| JsString::from(encode_node_utf8(path)))
        .unwrap_or_default();
    let code = node_error_code(error);
    let description = node_error_description(error, &code);
    let omit_path = pathless
        || syscall == "uv_cwd"
        || (matches!(syscall, "write" | "close" | "fsync") && path.is_empty())
        || (api == NodeReadApi::ReadFileSync
            && syscall == "read"
            && code == "EISDIR"
            && error.raw_os_error().is_some());
    let message = if syscall == "uv_cwd" && code == "ENOENT" {
        js_format!(
            "{}: process.cwd failed with error {}, the current working directory was likely removed without changing the working directory, uv_cwd",
            code,
            description
        )
    } else if omit_path {
        js_format!("{}: {}, {}", code, description, syscall)
    } else {
        js_format!("{}: {}, {} '{}'", code, description, syscall, path)
    };
    NodeFsError {
        name: "Error",
        errno: error.raw_os_error().and_then(i32::checked_neg),
        code,
        syscall: syscall.into(),
        path: if omit_path { None } else { Some(path.to_owned()) },
        message,
    }
}

/// Node filesystem message; the record API additionally exposes metadata.
pub fn node_fs_error(error: &io::Error, syscall: &str, path: &JsString) -> JsString {
    node_fs_error_record(error, syscall, path).message
}

/// Two-path syscalls (`rename`, `copyfile`) append the repaired destination as ` -> 'dest'`.
pub fn node_fs_dest_error(error: &io::Error, syscall: &str, path: &JsString, dest: &JsString) -> JsString {
    let dest = JsString::from(encode_node_utf8(dest));
    js_format!("{} -> '{}'", node_fs_error(error, syscall, path), dest)
}

/// Whole-file read failure: directory reads use `read`, other failures use `open`.
pub fn node_read_error_record(error: &io::Error, path: &JsString, api: NodeReadApi) -> NodeFsError {
    let syscall = if error.kind() == io::ErrorKind::IsADirectory {
        "read"
    } else {
        "open"
    };
    format_fs_error_record(error, syscall, Some(path), api)
}

/// Node `readFileSync` message, including its pathless raw EISDIR/read shape.
pub fn node_read_error(error: &io::Error, path: &JsString) -> JsString {
    node_read_error_record(error, path, NodeReadApi::ReadFileSync).message
}

/// Node `promises.readFile` message, retaining the path on EISDIR/read.
pub fn node_promises_read_error(error: &io::Error, path: &JsString) -> JsString {
    node_read_error_record(error, path, NodeReadApi::PromisesReadFile).message
}

/// ChildProcess spawn errors: deferred launch failures include the executable;
/// synchronous libuv failures (for example ENOTDIR) use the bare `spawn` syscall.
pub fn node_spawn_error_record(error: &io::Error, program: &JsString) -> NodeFsError {
    let code = node_error_code(error);
    let deferred = matches!(code.as_str(), "EACCES" | "EAGAIN" | "EMFILE" | "ENFILE" | "ENOENT");
    let syscall = if deferred {
        js_format!("spawn {}", program)
    } else {
        "spawn".into()
    };
    NodeFsError {
        name: "Error",
        errno: error.raw_os_error().and_then(i32::checked_neg),
        message: js_format!("{} {}", syscall, code),
        code,
        syscall,
        path: deferred.then(|| program.to_owned()),
    }
}

/// Unix pipe connection errors use Node's path-bearing stream diagnostic.
pub fn node_unix_connect_error(error: &io::Error, path: &JsString) -> JsString {
    js_format!("connect {} {}", node_error_code(error), path)
}

/// libuv reports an absent Unix bind parent as EACCES, not the native ENOENT.
pub fn node_unix_listen_error(error: &io::Error, path: &JsString) -> JsString {
    let missing_parent = io::Error::from_raw_os_error(Errno::ACCESS.raw_os_error());
    let error = if error.kind() == io::ErrorKind::NotFound {
        &missing_parent
    } else {
        error
    };
    let code = node_error_code(error);
    js_format!("listen {}: {} {}", code, node_error_description(error, &code), path)
}

/// A read from an already-open file reports `read` without the open path.
pub fn node_file_read_error_record(error: &io::Error) -> NodeFsError {
    format_fs_error_record(error, "read", None, NodeReadApi::ReadFileSync)
}

/// A write to an already-open file reports `write` without the open path.
pub fn node_file_write_error_record(error: &io::Error) -> NodeFsError {
    node_fs_error_record(error, "write", &JsString::default())
}

/// Writable pipe callback errors use `write EPIPE`, without a filesystem path.
pub fn node_pipe_write_error_record(error: &io::Error) -> NodeFsError {
    node_stream_error_record(error, "write")
}

/// Pipe/socket stream errors retain the originating read/write syscall.
pub fn node_stream_error_record(error: &io::Error, syscall: &str) -> NodeFsError {
    let code = node_error_code(error);
    NodeFsError {
        name: "Error",
        errno: error.raw_os_error().and_then(i32::checked_neg),
        message: js_format!("{} {}", syscall, code),
        code,
        syscall: syscall.into(),
        path: None,
    }
}

/// `rmSync(path)` rejects directories without recursive mode using SystemError's
/// message, unlike the ordinary `unlinkSync` EISDIR formatting.
pub fn node_rm_error(error: &io::Error, path: &JsString) -> JsString {
    if error.kind() == io::ErrorKind::IsADirectory {
        return js_format!("Path is a directory: rm returned EISDIR (is a directory) {}", path);
    }
    if error.kind() == io::ErrorKind::PermissionDenied {
        // Unlike rm's directory validation, this error comes back from the fs binding.
        let path = JsString::from(encode_node_utf8(path));
        return js_format!("{}, Permission denied: '{}'", node_error_code(error), path);
    }
    node_fs_error(error, "unlink", path)
}

/// Node `error.code` for an `io::Error`: the raw OS errno name first, then the
/// `ErrorKind` mapping for errors std constructs without an errno.
pub fn node_error_code(error: &io::Error) -> String {
    match error.raw_os_error() {
        Some(code) => node_errno_name(code)
            .map(str::to_owned)
            .unwrap_or_else(|| unknown_system_error(code)),
        None => error_kind_code(error.kind()).to_owned(),
    }
}

/// Node/libuv `uv_err_name` for a positive raw OS error number.
pub fn node_errno_name(code: i32) -> Option<&'static str> {
    LIBUV_ERRORS
        .iter()
        .find_map(|(errno, name, _)| (*errno == code).then_some(*name))
}

/// Node/libuv `uv_strerror`, including Node's exact unknown-number diagnostic.
pub fn node_errno_description(code: i32) -> String {
    LIBUV_ERRORS
        .iter()
        .find_map(|(errno, _, description)| (*errno == code).then_some(*description))
        .map(str::to_owned)
        .unwrap_or_else(|| unknown_system_error(code))
}

fn unknown_system_error(code: i32) -> String {
    format!("Unknown system error {}", -i64::from(code))
}

fn error_kind_code(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::NotFound => "ENOENT",
        io::ErrorKind::PermissionDenied => "EACCES",
        io::ErrorKind::IsADirectory => "EISDIR",
        io::ErrorKind::NotADirectory => "ENOTDIR",
        io::ErrorKind::AlreadyExists => "EEXIST",
        io::ErrorKind::InvalidInput => "EINVAL",
        io::ErrorKind::DirectoryNotEmpty => "ENOTEMPTY",
        _ => "EIO",
    }
}

// Node 26.11.1 / libuv 1.52.1, Linux x64: util.getSystemErrorMap().
// Positive magnitudes match io::Error::raw_os_error at the Rust boundary.
static LIBUV_ERRORS: &[(i32, &str, &str)] = &[
    (4095, "EOF", "end of file"),
    (4094, "UNKNOWN", "unknown error"),
    (4080, "ECHARSET", "invalid Unicode character"),
    (3014, "EAI_PROTOCOL", "resolved protocol is unknown"),
    (3013, "EAI_BADHINTS", "invalid value for hints"),
    (3011, "EAI_SOCKTYPE", "socket type not supported"),
    (3010, "EAI_SERVICE", "service not available for socket type"),
    (3009, "EAI_OVERFLOW", "argument buffer overflow"),
    (3008, "EAI_NONAME", "unknown node or service"),
    (3007, "EAI_NODATA", "no address"),
    (3006, "EAI_MEMORY", "out of memory"),
    (3005, "EAI_FAMILY", "ai_family not supported"),
    (3004, "EAI_FAIL", "permanent failure"),
    (3003, "EAI_CANCELED", "request canceled"),
    (3002, "EAI_BADFLAGS", "bad ai_flags value"),
    (3001, "EAI_AGAIN", "temporary failure"),
    (3000, "EAI_ADDRFAMILY", "address family not supported"),
    (134, "EFTYPE", "inappropriate file type or format"),
    (125, "ECANCELED", "operation canceled"),
    (121, "EREMOTEIO", "remote I/O error"),
    (114, "EALREADY", "connection already in progress"),
    (113, "EHOSTUNREACH", "host is unreachable"),
    (112, "EHOSTDOWN", "host is down"),
    (111, "ECONNREFUSED", "connection refused"),
    (110, "ETIMEDOUT", "connection timed out"),
    (108, "ESHUTDOWN", "cannot send after transport endpoint shutdown"),
    (107, "ENOTCONN", "socket is not connected"),
    (106, "EISCONN", "socket is already connected"),
    (105, "ENOBUFS", "no buffer space available"),
    (104, "ECONNRESET", "connection reset by peer"),
    (103, "ECONNABORTED", "software caused connection abort"),
    (101, "ENETUNREACH", "network is unreachable"),
    (100, "ENETDOWN", "network is down"),
    (99, "EADDRNOTAVAIL", "address not available"),
    (98, "EADDRINUSE", "address already in use"),
    (97, "EAFNOSUPPORT", "address family not supported"),
    (95, "ENOTSUP", "operation not supported on socket"),
    (94, "ESOCKTNOSUPPORT", "socket type not supported"),
    (93, "EPROTONOSUPPORT", "protocol not supported"),
    (92, "ENOPROTOOPT", "protocol not available"),
    (91, "EPROTOTYPE", "protocol wrong type for socket"),
    (90, "EMSGSIZE", "message too long"),
    (89, "EDESTADDRREQ", "destination address required"),
    (88, "ENOTSOCK", "socket operation on non-socket"),
    (84, "EILSEQ", "illegal byte sequence"),
    (75, "EOVERFLOW", "value too large for defined data type"),
    (71, "EPROTO", "protocol error"),
    (64, "ENONET", "machine is not on the network"),
    (61, "ENODATA", "no data available"),
    (49, "EUNATCH", "protocol driver not attached"),
    (40, "ELOOP", "too many symbolic links encountered"),
    (39, "ENOTEMPTY", "directory not empty"),
    (38, "ENOSYS", "function not implemented"),
    (36, "ENAMETOOLONG", "name too long"),
    (34, "ERANGE", "result too large"),
    (32, "EPIPE", "broken pipe"),
    (31, "EMLINK", "too many links"),
    (30, "EROFS", "read-only file system"),
    (29, "ESPIPE", "invalid seek"),
    (28, "ENOSPC", "no space left on device"),
    (27, "EFBIG", "file too large"),
    (26, "ETXTBSY", "text file is busy"),
    (25, "ENOTTY", "inappropriate ioctl for device"),
    (24, "EMFILE", "too many open files"),
    (23, "ENFILE", "file table overflow"),
    (22, "EINVAL", "invalid argument"),
    (21, "EISDIR", "illegal operation on a directory"),
    (20, "ENOTDIR", "not a directory"),
    (19, "ENODEV", "no such device"),
    (18, "EXDEV", "cross-device link not permitted"),
    (17, "EEXIST", "file already exists"),
    (16, "EBUSY", "resource busy or locked"),
    (14, "EFAULT", "bad address in system call argument"),
    (13, "EACCES", "permission denied"),
    (12, "ENOMEM", "not enough memory"),
    (11, "EAGAIN", "resource temporarily unavailable"),
    (9, "EBADF", "bad file descriptor"),
    (8, "ENOEXEC", "exec format error"),
    (7, "E2BIG", "argument list too long"),
    (6, "ENXIO", "no such device or address"),
    (5, "EIO", "i/o error"),
    (4, "EINTR", "interrupted system call"),
    (3, "ESRCH", "no such process"),
    (2, "ENOENT", "no such file or directory"),
    (1, "EPERM", "operation not permitted"),
];
