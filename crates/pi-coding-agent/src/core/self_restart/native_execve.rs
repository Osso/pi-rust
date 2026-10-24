//! Node v26.11.1 `Execve` / `ThrowErrnoException` native boundary.
//! Provenance: docs/porting/node-execve-source-lock.json; Node MIT notice.

use super::RestartEnvironment;
use pi_ai::request::CallbackError;
use pi_js_compat::js_json::{JsString, JsValue, encode_node_utf8};
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use rustix::io::{FdFlags, fcntl_getfd, fcntl_setfd};
use std::ffi::CString;
use std::io;
use std::os::fd::{AsFd, BorrowedFd};
use std::sync::Arc;

pub(super) fn execve(file: &JsString, args: &[JsString], env: &RestartEnvironment) -> Result<(), CallbackError> {
    validate_exec_arguments(file, args)?;
    validate_exec_environment(env)?;
    let file = encode_node_utf8(file);
    let path = CString::new(file.as_str()).expect("validated exec filename");
    let args: Vec<_> = args
        .iter()
        .map(|arg| CString::new(encode_node_utf8(arg)).expect("validated exec argument"))
        .collect();
    let environment: Vec<_> = env
        .iter()
        .map(|(key, value)| {
            CString::new(format!("{}={}", encode_node_utf8(key), encode_node_utf8(value)))
                .expect("validated exec environment")
        })
        .collect();
    exec_with_inherited_stdio(&file, &path, &args, &environment)
}

fn validate_exec_arguments(file: &JsString, args: &[JsString]) -> Result<(), CallbackError> {
    use pi_js_compat::node_child_process::validate_string_argument;
    validate_string_argument("file", "must be a string without null bytes", file)
        .map_err(|error| CallbackError::from(*error))?;
    for (index, arg) in args.iter().enumerate() {
        validate_string_argument(&format!("args[{index}]"), "must be a string without null bytes", arg)
            .map_err(|error| CallbackError::from(*error))?;
    }
    Ok(())
}

fn validate_exec_environment(env: &RestartEnvironment) -> Result<(), CallbackError> {
    if !env
        .iter()
        .any(|(key, value)| key.contains('\0') || value.contains('\0'))
    {
        return Ok(());
    }
    let value = JsValue::Object(
        env.iter()
            .map(|(key, value)| (key.clone(), JsValue::String(value.clone())))
            .collect(),
    );
    let inspected = pi_js_compat::node_fs::inspect_console_value(&value);
    let mut received = inspected.slice(0, 128);
    if inspected.len() > 128 {
        received.push_str("...");
    }
    Err(NativeNodeError::from_node_constructor(
        NativeErrorClass::TypeError,
        "ERR_INVALID_ARG_VALUE",
        pi_js_compat::js_format!(
            "The argument 'env' must be an object with string keys and values without null bytes. Received {}",
            received
        ),
    )
    .into())
}

fn exec_with_inherited_stdio(
    file: &str,
    path: &CString,
    args: &[CString],
    env: &[CString],
) -> Result<(), CallbackError> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let stderr = io::stderr();
    let streams = [stdin.as_fd(), stdout.as_fd(), stderr.as_fd()];
    let saved = inherit_standard_streams(&streams)?;
    // Explicit environment never swaps global environ (CommandExt::exec does).
    match nix::unistd::execve(path, args, env) {
        Ok(never) => match never {},
        Err(errno) => {
            restore_standard_streams(&streams, &saved);
            Err(errno_exception(errno as i32, "execve", Some(file)))
        }
    }
}

fn inherit_standard_streams(streams: &[BorrowedFd<'_>; 3]) -> Result<[FdFlags; 3], CallbackError> {
    let mut saved = [FdFlags::empty(); 3];
    for (index, &fd) in streams.iter().enumerate() {
        match clear_close_on_exec(fd) {
            Ok(flags) => saved[index] = flags,
            Err(errno) => {
                restore_standard_streams(&streams[..index], &saved[..index]);
                return Err(errno_exception(errno.raw_os_error(), "fcntl", None));
            }
        }
    }
    Ok(saved)
}

fn clear_close_on_exec(fd: BorrowedFd<'_>) -> Result<FdFlags, rustix::io::Errno> {
    let flags = fcntl_getfd(fd)?;
    fcntl_setfd(fd, flags & !FdFlags::CLOEXEC)?;
    Ok(flags)
}

fn restore_standard_streams(streams: &[BorrowedFd<'_>], saved: &[FdFlags]) {
    for (&fd, &flags) in streams.iter().zip(saved) {
        // Node ignores restore failures and reports the original fcntl/exec errno.
        let _ = fcntl_setfd(fd, flags);
    }
}

fn errno_exception(errno: i32, syscall: &str, path: Option<&str>) -> CallbackError {
    let os_error = io::Error::from_raw_os_error(errno);
    let code = errno_string(errno);
    let display = os_error.to_string();
    let suffix = format!(" (os error {errno})");
    // The OS variant's Display adds only this Rust suffix to libc strerror.
    let description = display.strip_suffix(&suffix).expect("OS error numeric suffix");
    let message = match path {
        Some(path) => format!("{code}, {description} '{path}'"),
        None => format!("{code}, {description}"),
    };
    let mut error = NativeNodeError::new("Error", message)
        .with_property("errno", JsValue::Number(f64::from(errno)))
        .with_property("code", JsValue::String(code.into()));
    if let Some(path) = path {
        error = error.with_property("path", JsValue::String(path.into()));
    }
    error = error.with_property("syscall", JsValue::String(syscall.into()));
    CallbackError::from(error).with_original(Arc::new(os_error))
}

fn errno_string(errno: i32) -> &'static str {
    let errno = nix::errno::Errno::from_raw(errno);
    // Node selects ENOTSUP when EOPNOTSUPP aliases it on the current platform.
    if errno == nix::errno::Errno::ENOTSUP {
        return "ENOTSUP";
    }
    let name = format!("{errno:?}");
    NODE_ERRNO_NAMES
        .iter()
        .copied()
        .find(|candidate| *candidate == name)
        .unwrap_or("")
}

// Node's distinct POSIX errno_string switch, not libuv's error-name table.
// Its default is the empty string; E2BIG is intentionally absent.
const NODE_ERRNO_NAMES: &[&str] = &[
    "EACCES",
    "EADDRINUSE",
    "EADDRNOTAVAIL",
    "EAFNOSUPPORT",
    "EAGAIN",
    "EWOULDBLOCK",
    "EALREADY",
    "EBADF",
    "EBADMSG",
    "EBUSY",
    "ECANCELED",
    "ECHILD",
    "ECONNABORTED",
    "ECONNREFUSED",
    "ECONNRESET",
    "EDEADLK",
    "EDESTADDRREQ",
    "EDOM",
    "EDQUOT",
    "EEXIST",
    "EFAULT",
    "EFBIG",
    "EHOSTUNREACH",
    "EIDRM",
    "EILSEQ",
    "EINPROGRESS",
    "EINTR",
    "EINVAL",
    "EIO",
    "EISCONN",
    "EISDIR",
    "ELOOP",
    "EMFILE",
    "EMLINK",
    "EMSGSIZE",
    "EMULTIHOP",
    "ENAMETOOLONG",
    "ENETDOWN",
    "ENETRESET",
    "ENETUNREACH",
    "ENFILE",
    "ENOBUFS",
    "ENODATA",
    "ENODEV",
    "ENOENT",
    "ENOEXEC",
    "ENOLINK",
    "ENOLCK",
    "ENOMEM",
    "ENOMSG",
    "ENOPROTOOPT",
    "ENOSPC",
    "ENOSR",
    "ENOSTR",
    "ENOSYS",
    "ENOTCONN",
    "ENOTDIR",
    "ENOTEMPTY",
    "ENOTSOCK",
    "ENOTSUP",
    "EOPNOTSUPP",
    "ENOTTY",
    "ENXIO",
    "EOVERFLOW",
    "EPERM",
    "EPIPE",
    "EPROTO",
    "EPROTONOSUPPORT",
    "EPROTOTYPE",
    "ERANGE",
    "EROFS",
    "ESPIPE",
    "ESRCH",
    "ESTALE",
    "ETIME",
    "ETIMEDOUT",
    "ETXTBSY",
    "EXDEV",
];
