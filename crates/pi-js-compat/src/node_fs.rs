//! Pure Node filesystem error messages and metadata shared by filesystem callers.
//!
//! Raw errno names reuse the existing platform table; unproven descriptions retain
//! the previous std display text. This is not a complete libuv strerror port.

use rustix::io::Errno;
use std::io;

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

/// Construct a Node filesystem error without touching the filesystem.
///
/// Pass the failing Node syscall, e.g. `open`, `read`, `mkdir`, or `scandir`.
/// Raw EISDIR from `read` and `uv_cwd` errors have no path. Deleted-CWD ENOENT
/// retains Node's contextual message. ErrorKind-only errors have no invented
/// errno. Negative errno parity is bounded to Unix.
pub fn node_fs_error_record(error: &io::Error, syscall: &str, path: &str) -> NodeFsError {
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
    let omit_path = syscall == "uv_cwd" || (syscall == "read" && code == "EISDIR" && error.raw_os_error().is_some());
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

/// `readFile` failure: directory reads use `read`, other failures use `open`.
pub fn node_read_error(error: &io::Error, path: &str) -> String {
    let syscall = if error.kind() == io::ErrorKind::IsADirectory {
        "read"
    } else {
        "open"
    };
    node_fs_error(error, syscall, path)
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
