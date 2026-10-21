//! Pure Node filesystem error messages and metadata shared by filesystem callers.
//!
//! Raw errno names reuse the existing platform table; unproven descriptions retain
//! the previous std display text. This is not a complete libuv strerror port.

use rustix::io::Errno;
use std::io;

use crate::js_json::JsString;

/// Node's path argument validation error, without invented IO syscall/errno fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePathArgumentError {
    pub name: &'static str,
    pub code: &'static str,
    pub message: JsString,
}

/// Validate a native string path before entering a Node filesystem operation.
/// ERR_INVALID_ARG_VALUE inspects the value, then truncates at 128 UTF-16 units.
pub fn node_path_argument_error(path: &str) -> Option<NodePathArgumentError> {
    if !path.contains('\0') {
        return None;
    }
    let inspected = inspect_path_argument(path);
    let mut received: Vec<u16> = inspected.encode_utf16().take(128).collect();
    if inspected.encode_utf16().count() > 128 {
        received.extend("...".encode_utf16());
    }
    let mut message: Vec<u16> =
        "The argument 'path' must be a string, Uint8Array, or URL without null bytes. Received "
            .encode_utf16()
            .collect();
    message.extend(received);
    Some(NodePathArgumentError {
        name: "TypeError",
        code: "ERR_INVALID_ARG_VALUE",
        message: JsString::from_utf16(message),
    })
}

fn inspect_path_argument(path: &str) -> String {
    // util.inspect's default maxStringLength is 10000. A partial code point at
    // that budget's end cannot reach the 128-unit error excerpt or change quotes.
    let mut units = 0;
    let end = path.char_indices().find_map(|(index, ch)| {
        units += ch.len_utf16();
        (units > 10000).then_some(index)
    });
    let value = &path[..end.unwrap_or(path.len())];
    // Default breakLength 80, indentation 0: long strings split after newlines.
    if value.encode_utf16().count() > 76 {
        value
            .split_inclusive('\n')
            .map(inspect_path_string)
            .collect::<Vec<_>>()
            .join(" +\n  ")
    } else {
        inspect_path_string(value)
    }
}

fn inspect_path_string(value: &str) -> String {
    inspect_string_using(value, |ch, quote| {
        // Keep the existing IO formatter unchanged; Node argument errors use hex.
        if ch == '\u{b}' {
            "\\x0B".to_owned()
        } else {
            escape_inspect_char(ch, quote)
        }
    })
}

/// Node Error name, message and own filesystem metadata (stack is caller-owned).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeFsError {
    pub name: &'static str,
    pub errno: Option<i32>,
    pub code: &'static str,
    pub syscall: String,
    pub path: Option<String>,
    pub message: String,
}

impl NodeFsError {
    /// Node's uncaught-error line and own properties, excluding JS stack frames.
    /// `spawnargs` is present only for a `spawnSync` failure.
    pub fn console_message(&self, spawnargs: Option<&[String]>) -> String {
        let mut output = format!("{}: {} {{\n", self.name, self.message);
        if let Some(errno) = self.errno {
            output.push_str(&format!("  errno: {errno},\n"));
        }
        output.push_str(&format!(
            "  code: {},\n  syscall: {}",
            inspect_string(self.code),
            inspect_string(&self.syscall)
        ));
        if let Some(path) = &self.path {
            output.push_str(&format!(",\n  path: {}", inspect_string(path)));
        }
        if let Some(args) = spawnargs {
            output.push_str(&format!(",\n  spawnargs: {}", inspect_spawnargs(args)));
        }
        output.push_str("\n}");
        output
    }
}

// Node util.inspect's string quoting for native UTF-8 error properties.
fn inspect_string(value: &str) -> String {
    inspect_string_using(value, escape_inspect_char)
}

fn inspect_string_using(value: &str, escape: impl Fn(char, char) -> String) -> String {
    let quote = if !value.contains('\'') {
        '\''
    } else if !value.contains('"') {
        '"'
    } else if !value.contains('`') && !value.contains("${") {
        '`'
    } else {
        '\''
    };
    let escaped: String = value.chars().map(|ch| escape(ch, quote)).collect();
    format!("{quote}{escaped}{quote}")
}

fn escape_inspect_char(ch: char, quote: char) -> String {
    match ch {
        '\\' => "\\\\".into(),
        '\n' => "\\n".into(),
        '\r' => "\\r".into(),
        '\t' => "\\t".into(),
        '\u{8}' => "\\b".into(),
        '\u{c}' => "\\f".into(),
        '\u{b}' => "\\v".into(),
        ch if ch == quote => format!("\\{ch}"),
        ch if ch <= '\u{1f}' || ('\u{7f}'..='\u{9f}').contains(&ch) => format!("\\x{:02X}", ch as u32),
        ch => ch.to_string(),
    }
}

// The error is at depth zero: spawnargs has indentation 2, breakLength 80,
// compact 3 and maxArrayLength 100 in Node's default uncaught-error inspector.
fn inspect_spawnargs(args: &[String]) -> String {
    let mut entries: Vec<String> = args.iter().take(100).map(|arg| inspect_string(arg)).collect();
    let grouped = group_inspect_entries(&entries, args.len() > 100);
    let is_grouped = grouped.len() != entries.len();
    if args.len() > 100 {
        let remaining = args.len() - 100;
        let suffix = if remaining == 1 { "item" } else { "items" };
        entries.push(format!("... {remaining} more {suffix}"));
    }
    let single_line_length =
        entries.iter().map(|entry| entry.encode_utf16().count()).sum::<usize>() + entries.len() * 2 + 13;
    if !is_grouped && single_line_length <= 80 {
        return format!("[ {} ]", entries.join(", "));
    }
    let mut lines = grouped;
    if args.len() > 100 {
        lines.push(entries.pop().unwrap());
    }
    format!("[\n    {}\n  ]", lines.join(",\n    "))
}

// Port of util.inspect's compact string-array layout, with the fixed defaults
// used above. Padding is right-aligned only for numbers, never spawn strings.
fn group_inspect_entries(entries: &[String], has_remaining: bool) -> Vec<String> {
    let widths: Vec<usize> = entries.iter().map(|entry| entry.chars().count()).collect();
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

fn inspect_row(row: &[String], widths: &[usize]) -> String {
    row.iter()
        .enumerate()
        .map(|(column, entry)| {
            if column + 1 == row.len() {
                entry.clone()
            } else {
                let padding = widths[column] - entry.chars().count() - 2;
                format!("{entry}, {}", " ".repeat(padding))
            }
        })
        .collect()
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
pub fn node_fs_error_record(error: &io::Error, syscall: &str, path: &str) -> NodeFsError {
    format_fs_error_record(error, syscall, path, NodeReadApi::ReadFileSync)
}

fn format_fs_error_record(error: &io::Error, syscall: &str, path: &str, api: NodeReadApi) -> NodeFsError {
    let code = node_error_code(error);
    let description = match error.kind() {
        io::ErrorKind::NotFound => "no such file or directory".to_owned(),
        io::ErrorKind::PermissionDenied => "permission denied".to_owned(),
        io::ErrorKind::IsADirectory => "illegal operation on a directory".to_owned(),
        io::ErrorKind::NotADirectory => "not a directory".to_owned(),
        io::ErrorKind::AlreadyExists => "file already exists".to_owned(),
        io::ErrorKind::InvalidInput => "invalid argument".to_owned(),
        io::ErrorKind::DirectoryNotEmpty => "directory not empty".to_owned(),
        _ if code == "ELOOP" => "too many symbolic links encountered".to_owned(),
        _ => error.to_string(),
    };
    let omit_path = syscall == "uv_cwd"
        || (api == NodeReadApi::ReadFileSync
            && syscall == "read"
            && code == "EISDIR"
            && error.raw_os_error().is_some());
    let message = if syscall == "uv_cwd" && code == "ENOENT" {
        format!(
            "{code}: process.cwd failed with error {description}, the current working directory was likely removed without changing the working directory, uv_cwd"
        )
    } else if omit_path {
        format!("{code}: {description}, {syscall}")
    } else {
        format!("{code}: {description}, {syscall} '{path}'")
    };
    NodeFsError {
        name: "Error",
        errno: error.raw_os_error().and_then(i32::checked_neg),
        code,
        syscall: syscall.to_owned(),
        path: if omit_path { None } else { Some(path.to_owned()) },
        message,
    }
}

/// Node filesystem message; the record API additionally exposes metadata.
pub fn node_fs_error(error: &io::Error, syscall: &str, path: &str) -> String {
    node_fs_error_record(error, syscall, path).message
}

/// Whole-file read failure: directory reads use `read`, other failures use `open`.
pub fn node_read_error_record(error: &io::Error, path: &str, api: NodeReadApi) -> NodeFsError {
    let syscall = if error.kind() == io::ErrorKind::IsADirectory {
        "read"
    } else {
        "open"
    };
    format_fs_error_record(error, syscall, path, api)
}

/// Node `readFileSync` message, including its pathless raw EISDIR/read shape.
pub fn node_read_error(error: &io::Error, path: &str) -> String {
    node_read_error_record(error, path, NodeReadApi::ReadFileSync).message
}

/// Node `promises.readFile` message, retaining the path on EISDIR/read.
pub fn node_promises_read_error(error: &io::Error, path: &str) -> String {
    node_read_error_record(error, path, NodeReadApi::PromisesReadFile).message
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
