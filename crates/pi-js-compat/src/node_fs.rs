//! Pure Node filesystem error messages and metadata shared by filesystem callers.
//!
//! Raw errno names reuse the existing platform table; unproven descriptions retain
//! the previous std display text. This is not a complete libuv strerror port.

use rustix::io::Errno;
use std::io;

use crate::js_format;
use crate::js_json::{JsString, encode_node_utf8};

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
    if !path.contains('\0') {
        return None;
    }
    Some(NodePathArgumentError {
        name: "TypeError",
        code: "ERR_INVALID_ARG_VALUE",
        message: invalid_argument_value_message(
            "path",
            "must be a string, Uint8Array, or URL without null bytes",
            path,
        ),
    })
}

/// Shared ERR_INVALID_ARG_VALUE inspection/truncation for Node IO boundaries.
pub fn invalid_argument_value_message(name: &str, reason: &str, value: &JsString) -> JsString {
    let inspected = inspect_argument_value(value);
    let mut received: Vec<u16> = inspected.units().iter().copied().take(128).collect();
    if inspected.len() > 128 {
        received.extend("...".encode_utf16());
    }
    let mut message = js_format!("The argument '{}' {}. Received ", name, reason);
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
    pub code: &'static str,
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
            inspect_string(&self.code.into()),
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
    let mut entries: Vec<JsString> = args.iter().take(100).map(|arg| inspect_string(arg)).collect();
    let grouped = group_inspect_entries(&entries, args.len() > 100);
    let is_grouped = grouped.len() != entries.len();
    if args.len() > 100 {
        let remaining = args.len() - 100;
        let suffix = if remaining == 1 { "item" } else { "items" };
        entries.push(js_format!("... {} more {}", remaining, suffix));
    }
    let single_line_length = entries.iter().map(JsString::len).sum::<usize>() + entries.len() * 2 + 13;
    if !is_grouped && single_line_length <= 80 {
        return js_format!("[ {} ]", JsString::join(&entries, ", "));
    }
    let mut lines = grouped;
    if args.len() > 100 {
        lines.push(entries.pop().unwrap());
    }
    js_format!("[\n    {}\n  ]", JsString::join(&lines, ",\n    "))
}

// Port of util.inspect's compact string-array layout, with the fixed defaults
// used above. Padding is right-aligned only for numbers, never spawn strings.
fn group_inspect_entries(entries: &[JsString], has_remaining: bool) -> Vec<JsString> {
    let widths: Vec<usize> = entries.iter().map(inspect_entry_width).collect();
    let maximum = widths.iter().copied().max().unwrap_or(0);
    let actual_maximum = maximum + 2;
    let total: usize = widths.iter().map(|width| width + 2).sum();
    if entries.len() <= 6 || actual_maximum * 3 + 2 >= 80 {
        return entries.to_vec();
    }
    if total as f64 / actual_maximum as f64 <= 5.0 && maximum > 6 {
        return entries.to_vec();
    }
    let output_length = entries.len() + usize::from(has_remaining);
    let average_bias = (actual_maximum as f64 - total as f64 / output_length as f64).sqrt();
    let biased_maximum = (actual_maximum as f64 - 3.0 - average_bias).max(1.0);
    let columns = ((2.5 * biased_maximum * entries.len() as f64).sqrt() / biased_maximum).round() as usize;
    let columns = columns.min(78 / actual_maximum).min(12);
    if columns <= 1 {
        return entries.to_vec();
    }
    let column_widths: Vec<usize> = (0..columns)
        .map(|column| widths.iter().skip(column).step_by(columns).max().unwrap() + 2)
        .collect();
    entries
        .chunks(columns)
        .map(|row| inspect_row(row, &column_widths))
        .collect()
}

fn inspect_row(row: &[JsString], widths: &[usize]) -> JsString {
    JsString::join(
        row.iter().enumerate().map(|(column, entry)| {
            if column + 1 == row.len() {
                entry.clone()
            } else {
                let padding = widths[column] - inspect_entry_width(entry) - 2;
                js_format!("{}, {}", entry, " ".repeat(padding))
            }
        }),
        "",
    )
}

fn inspect_entry_width(entry: &JsString) -> usize {
    char::decode_utf16(entry.units().iter().copied()).count()
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
    format_fs_error_record(error, syscall, path, NodeReadApi::ReadFileSync)
}

fn node_error_description(error: &io::Error, code: &str) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => "no such file or directory".to_owned(),
        io::ErrorKind::PermissionDenied => "permission denied".to_owned(),
        io::ErrorKind::IsADirectory => "illegal operation on a directory".to_owned(),
        io::ErrorKind::NotADirectory => "not a directory".to_owned(),
        io::ErrorKind::AlreadyExists => "file already exists".to_owned(),
        io::ErrorKind::InvalidInput => "invalid argument".to_owned(),
        io::ErrorKind::DirectoryNotEmpty => "directory not empty".to_owned(),
        _ if code == "ELOOP" => "too many symbolic links encountered".to_owned(),
        _ if code == "ENOSPC" => "no space left on device".to_owned(),
        _ if code == "EIO" && error.raw_os_error().is_some() => "i/o error".to_owned(),
        _ => error.to_string(),
    }
}

fn format_fs_error_record(error: &io::Error, syscall: &str, path: &JsString, api: NodeReadApi) -> NodeFsError {
    // Node's fs binding reports its UTF-8 path back after the OS boundary.
    // Other origins (spawn, sockets, argument validation) must not use this repair.
    let path = JsString::from(encode_node_utf8(path));
    let code = node_error_code(error);
    let description = node_error_description(error, code);
    let omit_path = syscall == "uv_cwd"
        || (matches!(syscall, "write" | "close") && path.is_empty())
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
    format_fs_error_record(error, syscall, path, api)
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
    let deferred = matches!(code, "EACCES" | "EAGAIN" | "EMFILE" | "ENFILE" | "ENOENT");
    let syscall = if deferred {
        js_format!("spawn {}", program)
    } else {
        "spawn".into()
    };
    NodeFsError {
        name: "Error",
        errno: error.raw_os_error().and_then(i32::checked_neg),
        code,
        message: js_format!("{} {}", syscall, code),
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
    js_format!("listen {}: {} {}", code, node_error_description(error, code), path)
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
        code,
        syscall: syscall.into(),
        path: None,
        message: js_format!("{} {}", syscall, code),
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
pub fn node_error_code(error: &io::Error) -> &'static str {
    error
        .raw_os_error()
        .and_then(node_errno_name)
        .unwrap_or_else(|| error_kind_code(error.kind()))
}

/// Node/libuv `uv_err_name` for a raw OS error number.
pub fn node_errno_name(code: i32) -> Option<&'static str> {
    if !(0..=4095).contains(&code) {
        return None;
    }
    let errno = Errno::from_raw_os_error(code);
    ERRNO_NAMES
        .iter()
        .find_map(|(candidate, name)| (*candidate == errno).then_some(*name))
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

static ERRNO_NAMES: &[(Errno, &str)] = &[
    (Errno::ACCESS, "EACCES"),
    (Errno::ADDRINUSE, "EADDRINUSE"),
    (Errno::ADDRNOTAVAIL, "EADDRNOTAVAIL"),
    (Errno::ADV, "EADV"),
    (Errno::AFNOSUPPORT, "EAFNOSUPPORT"),
    (Errno::AGAIN, "EAGAIN"),
    (Errno::ALREADY, "EALREADY"),
    (Errno::BADE, "EBADE"),
    (Errno::BADF, "EBADF"),
    (Errno::BADFD, "EBADFD"),
    (Errno::BADMSG, "EBADMSG"),
    (Errno::BADR, "EBADR"),
    (Errno::BADRQC, "EBADRQC"),
    (Errno::BADSLT, "EBADSLT"),
    (Errno::BFONT, "EBFONT"),
    (Errno::BUSY, "EBUSY"),
    (Errno::CANCELED, "ECANCELED"),
    (Errno::CHILD, "ECHILD"),
    (Errno::CHRNG, "ECHRNG"),
    (Errno::COMM, "ECOMM"),
    (Errno::CONNABORTED, "ECONNABORTED"),
    (Errno::CONNREFUSED, "ECONNREFUSED"),
    (Errno::CONNRESET, "ECONNRESET"),
    (Errno::DEADLK, "EDEADLK"),
    (Errno::DEADLOCK, "EDEADLOCK"),
    (Errno::DESTADDRREQ, "EDESTADDRREQ"),
    (Errno::DOM, "EDOM"),
    (Errno::DOTDOT, "EDOTDOT"),
    (Errno::DQUOT, "EDQUOT"),
    (Errno::EXIST, "EEXIST"),
    (Errno::FAULT, "EFAULT"),
    (Errno::FBIG, "EFBIG"),
    (Errno::HOSTDOWN, "EHOSTDOWN"),
    (Errno::HOSTUNREACH, "EHOSTUNREACH"),
    (Errno::HWPOISON, "EHWPOISON"),
    (Errno::IDRM, "EIDRM"),
    (Errno::ILSEQ, "EILSEQ"),
    (Errno::INPROGRESS, "EINPROGRESS"),
    (Errno::INVAL, "EINVAL"),
    (Errno::IO, "EIO"),
    (Errno::ISCONN, "EISCONN"),
    (Errno::ISDIR, "EISDIR"),
    (Errno::ISNAM, "EISNAM"),
    (Errno::KEYEXPIRED, "EKEYEXPIRED"),
    (Errno::KEYREJECTED, "EKEYREJECTED"),
    (Errno::KEYREVOKED, "EKEYREVOKED"),
    (Errno::L2HLT, "EL2HLT"),
    (Errno::L2NSYNC, "EL2NSYNC"),
    (Errno::L3HLT, "EL3HLT"),
    (Errno::L3RST, "EL3RST"),
    (Errno::LIBACC, "ELIBACC"),
    (Errno::LIBBAD, "ELIBBAD"),
    (Errno::LIBEXEC, "ELIBEXEC"),
    (Errno::LIBMAX, "ELIBMAX"),
    (Errno::LIBSCN, "ELIBSCN"),
    (Errno::LNRNG, "ELNRNG"),
    (Errno::LOOP, "ELOOP"),
    (Errno::MEDIUMTYPE, "EMEDIUMTYPE"),
    (Errno::MFILE, "EMFILE"),
    (Errno::MLINK, "EMLINK"),
    (Errno::MSGSIZE, "EMSGSIZE"),
    (Errno::MULTIHOP, "EMULTIHOP"),
    (Errno::NAMETOOLONG, "ENAMETOOLONG"),
    (Errno::NAVAIL, "ENAVAIL"),
    (Errno::NETDOWN, "ENETDOWN"),
    (Errno::NETRESET, "ENETRESET"),
    (Errno::NETUNREACH, "ENETUNREACH"),
    (Errno::NFILE, "ENFILE"),
    (Errno::NOANO, "ENOANO"),
    (Errno::NOBUFS, "ENOBUFS"),
    (Errno::NOCSI, "ENOCSI"),
    (Errno::NODATA, "ENODATA"),
    (Errno::NODEV, "ENODEV"),
    (Errno::NOENT, "ENOENT"),
    (Errno::NOEXEC, "ENOEXEC"),
    (Errno::NOKEY, "ENOKEY"),
    (Errno::NOLCK, "ENOLCK"),
    (Errno::NOLINK, "ENOLINK"),
    (Errno::NOMEDIUM, "ENOMEDIUM"),
    (Errno::NOMEM, "ENOMEM"),
    (Errno::NOMSG, "ENOMSG"),
    (Errno::NONET, "ENONET"),
    (Errno::NOPKG, "ENOPKG"),
    (Errno::NOPROTOOPT, "ENOPROTOOPT"),
    (Errno::NOSPC, "ENOSPC"),
    (Errno::NOSR, "ENOSR"),
    (Errno::NOSTR, "ENOSTR"),
    (Errno::NOSYS, "ENOSYS"),
    (Errno::NOTBLK, "ENOTBLK"),
    (Errno::NOTCONN, "ENOTCONN"),
    (Errno::NOTDIR, "ENOTDIR"),
    (Errno::NOTEMPTY, "ENOTEMPTY"),
    (Errno::NOTNAM, "ENOTNAM"),
    (Errno::NOTRECOVERABLE, "ENOTRECOVERABLE"),
    (Errno::NOTSOCK, "ENOTSOCK"),
    (Errno::NOTTY, "ENOTTY"),
    (Errno::NOTUNIQ, "ENOTUNIQ"),
    (Errno::NXIO, "ENXIO"),
    (Errno::OPNOTSUPP, "EOPNOTSUPP"),
    (Errno::OVERFLOW, "EOVERFLOW"),
    (Errno::OWNERDEAD, "EOWNERDEAD"),
    (Errno::PERM, "EPERM"),
    (Errno::PFNOSUPPORT, "EPFNOSUPPORT"),
    (Errno::PIPE, "EPIPE"),
    (Errno::PROTO, "EPROTO"),
    (Errno::PROTONOSUPPORT, "EPROTONOSUPPORT"),
    (Errno::PROTOTYPE, "EPROTOTYPE"),
    (Errno::RANGE, "ERANGE"),
    (Errno::REMCHG, "EREMCHG"),
    (Errno::REMOTE, "EREMOTE"),
    (Errno::REMOTEIO, "EREMOTEIO"),
    (Errno::RESTART, "ERESTART"),
    (Errno::RFKILL, "ERFKILL"),
    (Errno::ROFS, "EROFS"),
    (Errno::SHUTDOWN, "ESHUTDOWN"),
    (Errno::SOCKTNOSUPPORT, "ESOCKTNOSUPPORT"),
    (Errno::SPIPE, "ESPIPE"),
    (Errno::SRCH, "ESRCH"),
    (Errno::SRMNT, "ESRMNT"),
    (Errno::STALE, "ESTALE"),
    (Errno::STRPIPE, "ESTRPIPE"),
    (Errno::TIME, "ETIME"),
    (Errno::TIMEDOUT, "ETIMEDOUT"),
    (Errno::TOOBIG, "E2BIG"),
    (Errno::TOOMANYREFS, "ETOOMANYREFS"),
    (Errno::TXTBSY, "ETXTBSY"),
    (Errno::UCLEAN, "EUCLEAN"),
    (Errno::UNATCH, "EUNATCH"),
    (Errno::USERS, "EUSERS"),
    (Errno::WOULDBLOCK, "EWOULDBLOCK"),
    (Errno::XDEV, "EXDEV"),
    (Errno::XFULL, "EXFULL"),
];
